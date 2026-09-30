//! Battery: charge, power draw, health, what keeps the Mac awake, and a small history log.
//!
//! Live values come from the `AppleSmartBattery` registry entry (`ioreg`), health from
//! `system_profiler` (the same numbers as System Settings → Battery), and sleep blockers from
//! `pmset -g assertions`. Nothing here needs administrator rights.

use std::io::Write as _;
use std::path::PathBuf;

/// Apple rates the batteries of current MacBooks for this many charge cycles.
pub const RATED_CYCLES: u32 = 1000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Battery {
    /// Charge, 0–100.
    pub percent: u32,
    pub charging: bool,
    /// Connected to a power adapter (it may still not charge, e.g. when full or held at 80%).
    pub plugged: bool,
    pub fully_charged: bool,
    /// Minutes, as estimated by macOS.
    pub time_to_empty: Option<u32>,
    pub time_to_full: Option<u32>,
    /// Power flowing out of (negative) or into (positive) the battery, watts.
    pub watts: f32,
    pub voltage: f32,
    pub temperature: f32,
    pub cycles: u32,
    pub design_mah: u32,
    pub max_mah: u32,
    /// Maximum capacity relative to new, %. From System Settings when available.
    pub health: Option<u32>,
    /// "Good", "Service Recommended"… as macOS says it.
    pub condition: Option<String>,
    pub adapter_watts: Option<u32>,
    pub low_power_mode: bool,
}

impl Battery {
    /// Health from the raw capacities when System Settings does not say.
    pub fn health_pct(&self) -> Option<u32> {
        self.health.or_else(|| (self.design_mah > 0 && self.max_mah > 0).then(|| (self.max_mah * 100 / self.design_mah).min(100)))
    }

    pub fn needs_service(&self) -> bool {
        self.condition.as_deref().is_some_and(|c| !matches!(c, "Good" | "Normal")) || self.health_pct().is_some_and(|h| h < 80)
    }
}

/// `"Key" = value` in the text output of `ioreg`.
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\" = ");
    let at = text.find(&pat)? + pat.len();
    Some(text[at..].lines().next()?.trim())
}

fn num(text: &str, key: &str) -> Option<i64> {
    // Negative currents are printed as unsigned 64-bit numbers.
    field(text, key)?.parse::<u64>().ok().map(|v| v as i64)
}

fn yes(text: &str, key: &str) -> bool {
    field(text, key) == Some("Yes")
}

/// Parse `ioreg -rn AppleSmartBattery -w0`.
pub fn parse_ioreg(text: &str) -> Option<Battery> {
    if !text.contains("AppleSmartBattery") || field(text, "BatteryInstalled") == Some("No") {
        return None;
    }
    let voltage = num(text, "Voltage").unwrap_or(0) as f32 / 1000.0;
    let amps = num(text, "InstantAmperage").or_else(|| num(text, "Amperage")).unwrap_or(0) as f32 / 1000.0;
    let minutes = |k: &str| num(text, k).filter(|m| *m > 0 && *m < 65535).map(|m| m as u32);
    let charging = yes(text, "IsCharging");
    let plugged = yes(text, "ExternalConnected");
    let adapter_watts = field(text, "AdapterDetails").and_then(|d| {
        let at = d.find("\"Watts\"=")? + 8;
        d[at..].split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
    });
    // Apple Silicon reports MaxCapacity as a percentage; the raw values are in mAh.
    let max_mah = num(text, "AppleRawMaxCapacity").or_else(|| num(text, "MaxCapacity").filter(|v| *v > 100)).unwrap_or(0) as u32;
    Some(Battery {
        percent: num(text, "CurrentCapacity").unwrap_or(0).clamp(0, 100) as u32,
        charging,
        plugged,
        fully_charged: yes(text, "FullyCharged"),
        time_to_empty: if plugged { None } else { minutes("AvgTimeToEmpty").or_else(|| minutes("TimeRemaining")) },
        time_to_full: if charging { minutes("AvgTimeToFull") } else { None },
        watts: voltage * amps,
        voltage,
        temperature: num(text, "Temperature").unwrap_or(0) as f32 / 100.0,
        cycles: num(text, "CycleCount").unwrap_or(0) as u32,
        design_mah: num(text, "DesignCapacity").unwrap_or(0) as u32,
        max_mah,
        health: None,
        condition: None,
        adapter_watts: if plugged { adapter_watts } else { None },
        low_power_mode: false,
    })
}

/// Health and condition as System Settings shows them: `system_profiler SPPowerDataType -json`.
pub fn parse_profiler(json: &str) -> (Option<u32>, Option<String>) {
    let s = |key: &str| {
        let at = json.find(&format!("\"{key}\""))? + key.len() + 2;
        let rest = json[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
        Some(rest[..rest.find('"')?].to_string())
    };
    // "85 %" — the space may be a no-break one, so take the leading digits.
    let health =
        s("sppower_battery_health_maximum_capacity").and_then(|v| v.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok());
    (health, s("sppower_battery_health"))
}

/// Current battery state; `None` on Macs without a battery.
pub fn read() -> Option<Battery> {
    let mut b = parse_ioreg(&crate::run_output("ioreg", &["-rn", "AppleSmartBattery", "-w0"]))?;
    b.low_power_mode = crate::run_output("pmset", &["-g"]).lines().any(|l| {
        let mut w = l.split_whitespace();
        matches!((w.next(), w.next()), (Some("lowpowermode" | "powermode"), Some("1")))
    });
    Some(b)
}

/// Adds health and condition (a slower call — once every few minutes is plenty).
pub fn read_health(b: &mut Battery) {
    let (h, c) = parse_profiler(&crate::run_output("system_profiler", &["SPPowerDataType", "-json"]));
    b.health = h;
    b.condition = c;
}

pub fn open_battery_settings() {
    let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.Battery-Settings.extension").spawn();
}

// ---------------------------------------------------------------------------
// What keeps the Mac awake
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct SleepBlocker {
    /// The process the assertion is for (an app playing audio, not `coreaudiod`).
    pub pid: u32,
    pub name: String,
    /// Keeps the display on too.
    pub display: bool,
    pub reason: String,
    pub seconds: u64,
}

/// Parse `pmset -g assertions`: apps that stop the Mac (or its display) from sleeping.
pub fn parse_assertions(text: &str) -> Vec<SleepBlocker> {
    const SYSTEM: &[&str] = &["PreventUserIdleSystemSleep", "NoIdleSleepAssertion", "PreventSystemSleep"];
    const DISPLAY: &[&str] = &["PreventUserIdleDisplaySleep", "NoDisplaySleepAssertion"];
    let Some(start) = text.find("Listed by owning process:") else { return Vec::new() };
    let mut out: Vec<SleepBlocker> = Vec::new();
    let lines: Vec<&str> = text[start..].lines().skip(1).collect();
    for (i, line) in lines.iter().enumerate() {
        let l = line.trim();
        let Some(rest) = l.strip_prefix("pid ") else { continue };
        let Some((pid, rest)) = rest.split_once('(') else { continue };
        let Some((owner, rest)) = rest.split_once("):") else { continue };
        let mut words = rest.split_whitespace().skip(1); // [0x…]
        let (Some(time), Some(kind)) = (words.next(), words.next()) else { continue };
        let display = DISPLAY.contains(&kind);
        if !display && !SYSTEM.contains(&kind) {
            continue;
        }
        let reason = rest.split_once("named: ").map(|(_, n)| n.trim().trim_matches('"').to_string()).unwrap_or_default();
        // powerd keeps the Mac awake while the display is on — that is not an app's doing.
        if owner == "powerd" && reason.contains("display is on") {
            continue;
        }
        let seconds = time.split(':').filter_map(|x| x.parse::<u64>().ok()).fold(0, |acc, x| acc * 60 + x);
        // Daemons act for apps: "Created for PID: 48045."
        let for_pid = lines.get(i + 1).and_then(|n| n.trim().strip_prefix("Created for PID: ")).and_then(|p| p.trim_end_matches('.').parse().ok());
        let (pid, name) = match for_pid {
            Some(p) => (p, String::new()),
            None => (pid.parse().unwrap_or(0), owner.to_string()),
        };
        match out.iter_mut().find(|b| b.pid == pid) {
            Some(b) => {
                b.display |= display;
                b.seconds = b.seconds.max(seconds);
            }
            None => out.push(SleepBlocker { pid, name, display, reason, seconds }),
        }
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.seconds));
    out
}

pub fn sleep_blockers() -> Vec<SleepBlocker> {
    parse_assertions(&crate::run_output("pmset", &["-g", "assertions"]))
}

// ---------------------------------------------------------------------------
// History: one line every few minutes in ~/Library/Application Support/MacPilot/battery.csv
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub ts: i64,
    pub percent: u32,
    pub plugged: bool,
    pub watts: f32,
}

/// How long the log is kept.
const KEEP_DAYS: i64 = 14;

fn history_file() -> PathBuf {
    crate::settings::dir().join("battery.csv")
}

pub fn load_history() -> Vec<Sample> {
    let cutoff = crate::disk::now_unix() - KEEP_DAYS * 86_400;
    let text = std::fs::read_to_string(history_file()).unwrap_or_default();
    let mut v: Vec<Sample> = text
        .lines()
        .filter_map(|l| {
            let mut f = l.split(',');
            Some(Sample { ts: f.next()?.parse().ok()?, percent: f.next()?.parse().ok()?, plugged: f.next()? == "1", watts: f.next()?.parse().ok()? })
        })
        .filter(|s| s.ts >= cutoff)
        .collect();
    v.sort_by_key(|s| s.ts);
    v
}

/// Append a sample; the file is rewritten without old lines about once a day.
pub fn append_history(s: Sample, all: &[Sample]) {
    let path = history_file();
    let _ = std::fs::create_dir_all(crate::settings::dir());
    let line = |s: &Sample| format!("{},{},{},{:.2}\n", s.ts, s.percent, u8::from(s.plugged), s.watts);
    let first = all.first().map(|f| f.ts).unwrap_or(s.ts);
    if s.ts - first > (KEEP_DAYS + 1) * 86_400 {
        let cutoff = s.ts - KEEP_DAYS * 86_400;
        let text: String = all.iter().filter(|x| x.ts >= cutoff).chain(std::iter::once(&s)).map(line).collect();
        let _ = std::fs::write(&path, text);
    } else if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line(&s).as_bytes());
    }
}

/// Average drain on battery power, % per hour, from pairs of close samples.
pub fn drain_per_hour(h: &[Sample]) -> Option<f32> {
    let (mut drop, mut secs) = (0.0f32, 0i64);
    for w in h.windows(2) {
        let (a, b) = (w[0], w[1]);
        let dt = b.ts - a.ts;
        if a.plugged || b.plugged || dt <= 0 || dt > 20 * 60 || b.percent > a.percent {
            continue;
        }
        drop += (a.percent - b.percent) as f32;
        secs += dt;
    }
    // At least half an hour on battery, or the number means nothing.
    (secs >= 1800).then(|| drop / (secs as f32 / 3600.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOREG: &str = r#"+-o AppleSmartBattery  <class AppleSmartBattery>
    {
      "CurrentCapacity" = 27
      "TimeRemaining" = 68
      "Amperage" = 18446744073709549661
      "InstantAmperage" = 18446744073709549661
      "AvgTimeToFull" = 65535
      "ExternalConnected" = No
      "IsCharging" = No
      "FullyCharged" = No
      "MaxCapacity" = 100
      "AppleRawMaxCapacity" = 7151
      "DesignCapacity" = 8579
      "Temperature" = 3044
      "AvgTimeToEmpty" = 68
      "Voltage" = 11134
      "CycleCount" = 364
      "AdapterDetails" = {"Watts"=96,"Name"="96W USB-C Power Adapter"}
      "BatteryInstalled" = Yes
    }"#;

    #[test]
    fn reads_ioreg() {
        let b = parse_ioreg(IOREG).unwrap();
        assert_eq!((b.percent, b.charging, b.plugged, b.cycles), (27, false, false, 364));
        assert_eq!(b.time_to_empty, Some(68));
        assert_eq!(b.time_to_full, None);
        assert!((b.watts + 21.77).abs() < 0.05, "{}", b.watts); // -1955 mA × 11.134 V
        assert!((b.temperature - 30.44).abs() < 0.01);
        assert_eq!(b.health_pct(), Some(83)); // 7151 / 8579
        assert_eq!(b.adapter_watts, None, "only shown while plugged in");
        let plugged = IOREG.replace("\"ExternalConnected\" = No", "\"ExternalConnected\" = Yes");
        assert_eq!(parse_ioreg(&plugged).unwrap().adapter_watts, Some(96));
        assert!(parse_ioreg("").is_none());
    }

    #[test]
    fn reads_profiler() {
        let j = r#"{"sppower_battery_health_info": {"sppower_battery_cycle_count": 364, "sppower_battery_health": "Good", "sppower_battery_health_maximum_capacity" : "85 %"}}"#; // a no-break space, as macOS writes it
        assert_eq!(parse_profiler(j), (Some(85), Some("Good".into())));
        let b = Battery { health: Some(78), ..Default::default() };
        assert!(b.needs_service());
    }

    #[test]
    fn reads_assertions() {
        let t = "Assertion status system-wide:\n   PreventUserIdleSystemSleep     1\nListed by owning process:\n   \
pid 13766(Claude): [0x0000034200018534] 03:40:52 NoIdleSleepAssertion named: \"Electron\"  \n   \
pid 359(powerd): [0x00000f1600018834] 00:41:33 PreventUserIdleSystemSleep named: \"Powerd - Prevent sleep while display is on\"  \n   \
pid 359(powerd): [0x00000f1600098831] 00:41:34 UserIsActive named: \"com.apple.powermanagement.lidopen\"  \n   \
pid 428(coreaudiod): [0x0000184d0001867f] 00:02:15 PreventUserIdleSystemSleep named: \"com.apple.audio.context.preventuseridlesleep\"  \n\
\tCreated for PID: 48045. \n\tResources: audio-out BuiltInSpeakerDevice \n   \
pid 900(zoom.us): [0x1] 00:10:00 PreventUserIdleDisplaySleep named: \"Meeting\"  \nNo kernel assertions.\n";
        let b = parse_assertions(t);
        assert_eq!(b.len(), 3, "{b:?}");
        assert_eq!((b[0].pid, b[0].name.as_str(), b[0].seconds), (13766, "Claude", 3 * 3600 + 40 * 60 + 52));
        assert!(b.iter().any(|x| x.pid == 48045 && x.name.is_empty()), "audio is attributed to the app");
        assert!(b.iter().any(|x| x.pid == 900 && x.display));
    }

    #[test]
    fn drain() {
        let s = |ts, percent, plugged| Sample { ts, percent, plugged, watts: 0.0 };
        // 12% per hour on battery (3% every 15 minutes); the plugged-in part and the long gap are ignored.
        let h = [s(0, 90, false), s(900, 87, false), s(1800, 84, false), s(2000, 84, true), s(90_000, 50, false), s(90_900, 47, false)];
        let d = drain_per_hour(&h).unwrap();
        assert!((d - 12.0).abs() < 0.01, "{d}");
        assert_eq!(drain_per_hour(&h[..3]), Some(12.0));
        assert_eq!(drain_per_hour(&[s(0, 90, false), s(600, 89, false)]), None, "too short to say");
    }
}
