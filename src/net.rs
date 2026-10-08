//! Network traffic per process, from `nettop` (the tool behind Activity Monitor's Network tab).
//! It reports the bytes each process has received and sent so far; two readings give a rate.

use std::collections::HashMap;
use std::time::Instant;

/// pid → (bytes received, bytes sent) since the process started using the network.
pub type Totals = HashMap<u32, (u64, u64)>;

/// Parse `nettop -P -L 1 -x -n -J bytes_in,bytes_out`: lines like `Google Chrome H.1234,5120,880,`.
pub fn parse(text: &str) -> Totals {
    let mut out = Totals::new();
    for line in text.lines() {
        let mut f = line.split(',');
        let (Some(who), Some(rx), Some(tx)) = (f.next(), f.next(), f.next()) else { continue };
        // The name may contain dots; the pid is after the last one.
        let Some(pid) = who.rsplit_once('.').and_then(|(_, p)| p.parse::<u32>().ok()) else { continue };
        let (Ok(rx), Ok(tx)) = (rx.parse::<u64>(), tx.parse::<u64>()) else { continue };
        let e = out.entry(pid).or_insert((0, 0));
        e.0 += rx;
        e.1 += tx;
    }
    out
}

pub fn read() -> Totals {
    parse(&crate::run_output("nettop", &["-P", "-L", "1", "-x", "-n", "-J", "bytes_in,bytes_out"]))
}

/// What a process does on the network right now.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rate {
    /// Bytes per second.
    pub down: f64,
    pub up: f64,
    /// Bytes so far.
    pub received: u64,
    pub sent: u64,
}

impl Rate {
    pub fn total(&self) -> f64 {
        self.down + self.up
    }
}

/// Turns successive readings into rates.
pub struct Meter {
    prev: Totals,
    at: Instant,
}

impl Default for Meter {
    fn default() -> Self {
        Meter { prev: Totals::new(), at: Instant::now() }
    }
}

impl Meter {
    /// Rates since the previous call (zero on the first one).
    pub fn update(&mut self, now: Totals, at: Instant) -> HashMap<u32, Rate> {
        let dt = at.duration_since(self.at).as_secs_f64();
        let out = now
            .iter()
            .map(|(pid, (rx, tx))| {
                // A new process, or a pid that was reused, starts from zero.
                let (down, up) = match self.prev.get(pid) {
                    Some((prx, ptx)) if dt > 0.2 && rx >= prx && tx >= ptx => ((rx - prx) as f64 / dt, (tx - ptx) as f64 / dt),
                    _ => (0.0, 0.0),
                };
                (*pid, Rate { down, up, received: *rx, sent: *tx })
            })
            .collect();
        self.prev = now;
        self.at = at;
        out
    }

    pub fn sample(&mut self) -> HashMap<u32, Rate> {
        self.update(read(), Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parses_nettop() {
        let t = parse(",bytes_in,bytes_out,\nlaunchd.1,0,0,\nGoogle Chrome H.1234,5120,880,\nmDNSResponder.700,27574913,10902909,\nbroken line\n");
        assert_eq!(t.len(), 3);
        assert_eq!(t[&1234], (5120, 880));
        assert_eq!(t[&700].0, 27_574_913);
    }

    #[test]
    fn rates_between_readings() {
        let t0 = Instant::now();
        let mut m = Meter { prev: Totals::new(), at: t0 };
        let first = m.update(Totals::from([(10, (1000, 100))]), t0 + Duration::from_secs(1));
        assert_eq!(first[&10].total(), 0.0, "nothing to compare with yet");
        let r = m.update(Totals::from([(10, (5000, 300)), (11, (50, 50))]), t0 + Duration::from_secs(3));
        assert_eq!((r[&10].down, r[&10].up), (2000.0, 100.0));
        assert_eq!(r[&10].received, 5000);
        assert_eq!(r[&11].total(), 0.0, "a new process starts from zero");
        // A reused pid (counters went down) does not give a negative or huge rate.
        let r = m.update(Totals::from([(10, (10, 10))]), t0 + Duration::from_secs(5));
        assert_eq!(r[&10].total(), 0.0);
    }
}
