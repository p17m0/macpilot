//! Temperatures and fans from the System Management Controller (SMC). macOS has no public API for
//! them; like iStat Menus and Stats, MacPilot asks the `AppleSMC` service. Reading needs no
//! administrator rights. Key names differ between chips, so the temperature keys are discovered
//! once by listing all keys, and the CPU temperature is the median of its many sensors.

use std::ffi::c_void;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fan {
    pub rpm: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sensors {
    /// °C; `None` when this Mac does not report it.
    pub cpu: Option<f32>,
    /// The hottest CPU sensor.
    pub cpu_max: Option<f32>,
    pub gpu: Option<f32>,
    pub fans: Vec<Fan>,
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const libc::c_char) -> *mut c_void;
    fn IOServiceGetMatchingService(port: u32, matching: *mut c_void) -> u32;
    fn IOServiceOpen(service: u32, task: u32, kind: u32, conn: *mut u32) -> i32;
    fn IOServiceClose(conn: u32) -> i32;
    fn IOObjectRelease(obj: u32) -> i32;
    fn IOConnectCallStructMethod(conn: u32, selector: u32, input: *const c_void, in_size: usize, output: *mut c_void, out_size: *mut usize) -> i32;
}

unsafe extern "C" {
    fn mach_task_self() -> u32;
}

// The request/response structure of the SMC user client (80 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
struct Version {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PowerLimit {
    version: u16,
    length: u16,
    cpu: u32,
    gpu: u32,
    mem: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KeyInfo {
    size: u32,
    kind: u32,
    attributes: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KeyData {
    key: u32,
    version: Version,
    limit: PowerLimit,
    info: KeyInfo,
    result: u8,
    status: u8,
    command: u8,
    index: u32,
    bytes: [u8; 32],
}

const _: () = assert!(std::mem::size_of::<KeyData>() == 80);

const CMD_READ: u8 = 5;
const CMD_KEY_BY_INDEX: u8 = 8;
const CMD_KEY_INFO: u8 = 9;

fn fourcc(s: &str) -> u32 {
    s.bytes().take(4).fold(0, |a, b| (a << 8) | b as u32)
}

fn key_name(k: u32) -> String {
    k.to_be_bytes().iter().map(|b| *b as char).collect()
}

/// Decode an SMC value by its type code.
fn decode(kind: &str, b: &[u8]) -> Option<f32> {
    match kind {
        "flt " if b.len() >= 4 => Some(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        "sp78" if b.len() >= 2 => Some(i16::from_be_bytes([b[0], b[1]]) as f32 / 256.0),
        "fpe2" if b.len() >= 2 => Some(u16::from_be_bytes([b[0], b[1]]) as f32 / 4.0),
        "ui8 " if !b.is_empty() => Some(b[0] as f32),
        "ui16" if b.len() >= 2 => Some(u16::from_be_bytes([b[0], b[1]]) as f32),
        "ui32" if b.len() >= 4 => Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f32),
        _ => None,
    }
}

/// CPU core sensors: "Tp…" and "Te…" on Apple Silicon, "TC…" on Intel.
fn is_cpu_key(k: &str) -> bool {
    k.starts_with("Tp") || k.starts_with("Te") || k.starts_with("TC")
}

fn is_gpu_key(k: &str) -> bool {
    k.starts_with("Tg") || k.starts_with("TG")
}

/// Middle value of the believable readings (sensors that are off report 0 or nonsense).
fn median(mut v: Vec<f32>) -> Option<f32> {
    v.retain(|t| *t > 10.0 && *t < 125.0);
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    Some(v[v.len() / 2])
}

/// An open connection to the SMC with the sensor keys of this Mac.
pub struct Smc {
    conn: u32,
    cpu_keys: Vec<u32>,
    gpu_keys: Vec<u32>,
    fans: usize,
}

// The connection is a Mach port, used from one thread at a time.
unsafe impl Send for Smc {}

impl Smc {
    /// `None` when the SMC is not available (virtual machines).
    pub fn open() -> Option<Smc> {
        let service = unsafe { IOServiceGetMatchingService(0, IOServiceMatching(c"AppleSMC".as_ptr())) };
        if service == 0 {
            return None;
        }
        let mut conn = 0;
        let r = unsafe { IOServiceOpen(service, mach_task_self(), 0, &mut conn) };
        unsafe { IOObjectRelease(service) };
        if r != 0 {
            return None;
        }
        let mut smc = Smc { conn, cpu_keys: Vec::new(), gpu_keys: Vec::new(), fans: 0 };
        let count = smc.value(fourcc("#KEY")).unwrap_or(0.0) as u32;
        for i in 0..count.min(5000) {
            let mut q = blank();
            q.command = CMD_KEY_BY_INDEX;
            q.index = i;
            let Some(a) = smc.call(&q) else { continue };
            let name = key_name(a.key);
            if is_cpu_key(&name) {
                smc.cpu_keys.push(a.key);
            } else if is_gpu_key(&name) {
                smc.gpu_keys.push(a.key);
            }
        }
        smc.fans = smc.value(fourcc("FNum")).unwrap_or(0.0) as usize;
        Some(smc)
    }

    fn call(&self, input: &KeyData) -> Option<KeyData> {
        let mut out = blank();
        let mut size = std::mem::size_of::<KeyData>();
        let r = unsafe {
            IOConnectCallStructMethod(
                self.conn,
                2,
                input as *const KeyData as *const c_void,
                size,
                &mut out as *mut KeyData as *mut c_void,
                &mut size,
            )
        };
        (r == 0 && out.result == 0).then_some(out)
    }

    fn value(&self, key: u32) -> Option<f32> {
        let mut q = blank();
        q.key = key;
        q.command = CMD_KEY_INFO;
        let info = self.call(&q)?.info;
        let mut q = blank();
        q.key = key;
        q.info.size = info.size;
        q.command = CMD_READ;
        let a = self.call(&q)?;
        decode(&key_name(info.kind), &a.bytes[..(info.size as usize).min(32)])
    }

    pub fn read(&self) -> Sensors {
        let cpu: Vec<f32> = self.cpu_keys.iter().filter_map(|k| self.value(*k)).collect();
        let cpu_max = cpu.iter().copied().filter(|t| *t > 10.0 && *t < 125.0).reduce(f32::max);
        let gpu = median(self.gpu_keys.iter().filter_map(|k| self.value(*k)).collect());
        let fans = (0..self.fans.min(8))
            .filter_map(|i| {
                let v = |suffix: &str| self.value(fourcc(&format!("F{i}{suffix}")));
                Some(Fan { rpm: v("Ac")?, min: v("Mn").unwrap_or(0.0), max: v("Mx").unwrap_or(0.0) })
            })
            .collect();
        Sensors { cpu: median(cpu), cpu_max, gpu, fans }
    }
}

impl Drop for Smc {
    fn drop(&mut self) {
        unsafe { IOServiceClose(self.conn) };
    }
}

fn blank() -> KeyData {
    unsafe { std::mem::zeroed() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_smc_types() {
        assert_eq!(decode("flt ", &42.5f32.to_le_bytes()), Some(42.5));
        assert_eq!(decode("sp78", &[0x2A, 0x80]), Some(42.5));
        assert_eq!(decode("fpe2", &[0x15, 0x18]), Some(1350.0));
        assert_eq!(decode("ui8 ", &[2]), Some(2.0));
        assert_eq!(decode("ch8*", &[1, 2, 3]), None);
        assert_eq!(decode("flt ", &[1]), None, "too short");
        assert_eq!(key_name(fourcc("F0Ac")), "F0Ac");
    }

    #[test]
    fn median_ignores_dead_sensors() {
        assert_eq!(median(vec![0.0, 41.0, 45.0, 72.0, -3.0, 200.0]), Some(45.0));
        assert_eq!(median(vec![0.0, 0.0]), None);
        assert!(is_cpu_key("Tp0D") && is_cpu_key("TC0P") && !is_cpu_key("Tg05") && is_gpu_key("Tg05"));
    }

    /// `cargo test --lib live_sensors -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_sensors() {
        let t = std::time::Instant::now();
        let smc = Smc::open().expect("SMC");
        println!("opened in {:?}: {} cpu keys, {} gpu keys", t.elapsed(), smc.cpu_keys.len(), smc.gpu_keys.len());
        let t = std::time::Instant::now();
        println!("{:?} in {:?}", smc.read(), t.elapsed());
    }
}
