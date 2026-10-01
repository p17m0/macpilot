//! Where the disk space goes beyond the scanned folders, and how the scanned folders change over time.
//!
//! * [`volumes`]: what APFS reports per volume — macOS itself, swap and the Data volume with all files.
//! * [`outside_home`]: big folders outside the home folder (measured on request).
//! * History: after every full scan of the home folder a small summary (folders of 50 MB and more,
//!   six levels deep) is saved per day; [`changes`] compares the current scan with one of them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::dirmap::DirMap;
use crate::disk::{self, Scan};

// ---------------------------------------------------------------------------
// Volumes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Volumes {
    /// Size and free space of the whole APFS container (what Finder shows for the disk).
    pub total: u64,
    pub free: u64,
    /// The sealed macOS system volume plus Preboot and Recovery.
    pub macos: u64,
    /// Swap and the sleep image.
    pub vm: u64,
    /// All files: users, apps, /Library, /opt…
    pub data: u64,
}

impl Volumes {
    /// Space that is used but belongs to none of the volumes above (snapshots, other volumes).
    pub fn other(&self) -> u64 {
        self.total.saturating_sub(self.free + self.macos + self.vm + self.data)
    }
}

pub fn volumes() -> Option<Volumes> {
    // The container that holds the startup disk, e.g. "disk3".
    let info = crate::run_output("diskutil", &["info", "-plist", "/"]);
    let container = plist_str(&info, "APFSContainerReference")?;
    let total = plist_int(&info, "APFSContainerSize")?;
    let free = plist_int(&info, "APFSContainerFree")?;
    parse_volumes(&crate::run_output("diskutil", &["apfs", "list", "-plist"]), &container, total, free)
}

/// Per-volume usage from `diskutil apfs list -plist` (statfs on APFS reports the whole container).
fn parse_volumes(xml: &str, container: &str, total: u64, free: u64) -> Option<Volumes> {
    let mut v = Volumes { total, free, ..Default::default() };
    let mut found = false;
    // Each volume's dictionary starts with its UUID (keys are sorted).
    for chunk in xml.split("<key>APFSVolumeUUID</key>").skip(1) {
        let Some(dev) = plist_str(chunk, "DeviceIdentifier") else { continue };
        if !dev.strip_prefix(container).is_some_and(|rest| rest.starts_with('s')) {
            continue;
        }
        let used = plist_int(chunk, "CapacityInUse").unwrap_or(0);
        let roles = chunk.find("<key>Roles</key>").map(|at| &chunk[at..chunk[at..].find("</array>").map_or(chunk.len(), |e| at + e)]).unwrap_or("");
        let has = |r: &str| roles.contains(&format!("<string>{r}</string>"));
        if has("Data") {
            v.data += used;
            found = true;
        } else if has("VM") {
            v.vm += used;
        } else if has("System") || has("Preboot") || has("Recovery") || has("Update") {
            v.macos += used;
        }
    }
    found.then_some(v)
}

fn plist_str(xml: &str, key: &str) -> Option<String> {
    let at = xml.find(&format!("<key>{key}</key>"))?;
    let rest = &xml[at..];
    let s = rest.find("<string>")? + 8;
    Some(rest[s..s + rest[s..].find("</string>")?].to_string())
}

fn plist_int(xml: &str, key: &str) -> Option<u64> {
    let at = xml.find(&format!("<key>{key}</key>"))?;
    let rest = &xml[at..];
    let s = rest.find("<integer>")? + 9;
    rest[s..s + rest[s..].find("</integer>")?].trim().parse().ok()
}

/// Local Time Machine snapshots (they hold on to deleted files until macOS removes them).
pub fn local_snapshots() -> Vec<String> {
    crate::run_output("tmutil", &["listlocalsnapshots", "/"])
        .lines()
        .filter_map(|l| l.trim().strip_prefix("com.apple.TimeMachine.").map(|s| s.trim_end_matches(".local").to_string()))
        .collect()
}

/// Big folders outside the home folder, with a short explanation key.
pub fn outside_home() -> Vec<(PathBuf, &'static str)> {
    let mut v = vec![
        (PathBuf::from("/Applications"), "Applications"),
        (PathBuf::from("/Library"), "Library"),
        (PathBuf::from("/opt"), "opt"),
        (PathBuf::from("/usr/local"), "usrlocal"),
        (PathBuf::from("/Users/Shared"), "shared"),
    ];
    // This user's temporary files and caches (/private/var/folders/…).
    let tmp = crate::run_output("getconf", &["DARWIN_USER_CACHE_DIR"]);
    if let Some(parent) = Path::new(tmp.trim()).parent() {
        if parent.as_os_str().len() > 1 {
            v.push((parent.to_path_buf(), "temp"));
        }
    }
    v.retain(|(p, _)| p.exists());
    v
}

// ---------------------------------------------------------------------------
// History of the home folder
// ---------------------------------------------------------------------------

/// Folders at least this big go into the daily summary.
const SUMMARY_MIN: u64 = 50 * 1000 * 1000;
const SUMMARY_DEPTH: usize = 6;
const KEEP_DAYS: i64 = 35;

fn history_dir() -> PathBuf {
    crate::settings::dir().join("disk-history")
}

/// Local date "20260930".
fn day(ts: i64) -> String {
    let t = ts as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    format!("{:04}{:02}{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday)
}

/// Sizes of the folders of a scan: relative path → bytes ("" is the root).
pub fn summary(dirs: &DirMap, root: &Path) -> HashMap<String, u64> {
    dirs.iter()
        .filter(|(_, s)| s.size >= SUMMARY_MIN)
        .filter_map(|(i, s)| {
            let p = dirs.path(i);
            let rel = p.strip_prefix(root).ok()?;
            (rel.components().count() <= SUMMARY_DEPTH).then(|| (rel.to_string_lossy().to_string(), s.size))
        })
        .collect()
}

/// Save the day's summary of a finished home-folder scan (replacing an earlier one from that day).
pub fn save_summary(scan: &Scan) {
    // A scan loaded from the cache counts for the day it was made.
    let at = scan.cached_at.unwrap_or_else(disk::now_unix);
    let text = {
        let dirs = scan.shared.dirs.lock().unwrap();
        let mut rows: Vec<(String, u64)> = summary(&dirs, &scan.root).into_iter().collect();
        rows.sort();
        let mut t = format!("#{at}\n");
        for (rel, size) in rows {
            t += &format!("{size}\t{rel}\n");
        }
        t
    };
    let dir = history_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{}.tsv", day(at))), text);
    // Old days go.
    let cutoff = day(at - KEEP_DAYS * 86_400);
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".tsv") && name[..name.len() - 4] < *cutoff {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// A saved summary: when, and the sizes.
pub struct Summary {
    pub at: i64,
    pub sizes: HashMap<String, u64>,
}

fn parse_summary(text: &str) -> Option<Summary> {
    let mut lines = text.lines();
    let at = lines.next()?.strip_prefix('#')?.parse().ok()?;
    let sizes = lines.filter_map(|l| l.split_once('\t')).filter_map(|(s, rel)| Some((rel.to_string(), s.parse().ok()?))).collect();
    Some(Summary { at, sizes })
}

/// The saved summary closest to `days` ago, but at least most of a day older than `now`.
pub fn baseline(days: i64, now: i64) -> Option<Summary> {
    let target = now - days * 86_400;
    let mut best: Option<Summary> = None;
    for e in std::fs::read_dir(history_dir()).ok()?.flatten() {
        let Some(s) = std::fs::read_to_string(e.path()).ok().and_then(|t| parse_summary(&t)) else { continue };
        if now - s.at < 20 * 3600 {
            continue;
        }
        if best.as_ref().is_none_or(|b| (s.at - target).abs() < (b.at - target).abs()) {
            best = Some(s);
        }
    }
    best
}

/// Days for which a summary exists (for the "history starts on…" note).
pub fn first_summary() -> Option<i64> {
    std::fs::read_dir(history_dir())
        .ok()?
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok().and_then(|t| parse_summary(&t)))
        .map(|s| s.at)
        .min()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub path: PathBuf,
    pub old: u64,
    pub new: u64,
}

impl Change {
    pub fn delta(&self) -> i64 {
        self.new as i64 - self.old as i64
    }
}

/// Folders that explain how the scanned space changed since `base`: the most specific folder
/// that accounts for most of a change, largest changes first.
pub fn changes(now: &HashMap<String, u64>, base: &HashMap<String, u64>, root: &Path, min: u64) -> Vec<Change> {
    let mut rels: Vec<&String> = now.keys().chain(base.keys()).collect();
    rels.sort();
    rels.dedup();
    let mut cands: Vec<(String, i64)> = rels
        .into_iter()
        .map(|r| (r.clone(), *now.get(r).unwrap_or(&0) as i64 - *base.get(r).unwrap_or(&0) as i64))
        .filter(|(r, d)| !r.is_empty() && d.unsigned_abs() >= min)
        .collect();
    // Deepest first: a folder is reported only if its reported subfolders do not already explain it.
    cands.sort_by_key(|(r, _)| std::cmp::Reverse(r.matches('/').count()));
    let mut reported: Vec<(String, i64)> = Vec::new();
    for (rel, d) in cands {
        let prefix = format!("{rel}/");
        let explained: i64 = reported.iter().filter(|(r, rd)| r.starts_with(&prefix) && rd.signum() == d.signum()).map(|(_, rd)| rd).sum();
        if explained.unsigned_abs() * 10 < d.unsigned_abs() * 6 {
            reported.push((rel, d - explained));
        }
    }
    let mut out: Vec<Change> = reported
        .into_iter()
        .filter(|(_, d)| d.unsigned_abs() >= min)
        .map(|(rel, _)| Change { path: root.join(&rel), old: *base.get(&rel).unwrap_or(&0), new: *now.get(&rel).unwrap_or(&0) })
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.delta().unsigned_abs()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(v: &[(&str, u64)]) -> HashMap<String, u64> {
        v.iter().map(|(k, s)| (k.to_string(), *s)).collect()
    }
    const G: u64 = 1_000_000_000;

    #[test]
    fn finds_the_folder_that_grew() {
        let base = m(&[("", 100 * G), ("Library", 50 * G), ("Library/Containers", 20 * G), ("Library/Containers/docker", 15 * G), ("Dev", 10 * G)]);
        let now = m(&[("", 140 * G), ("Library", 88 * G), ("Library/Containers", 58 * G), ("Library/Containers/docker", 53 * G), ("Dev", 12 * G)]);
        let c = changes(&now, &base, Path::new("/Users/me"), G / 2);
        // Docker explains the growth of Containers and Library; Dev grew on its own.
        assert_eq!(c[0].path, Path::new("/Users/me/Library/Containers/docker"));
        assert_eq!(c[0].delta(), 38 * G as i64);
        assert!(c.iter().any(|x| x.path == Path::new("/Users/me/Dev")));
        assert!(!c.iter().any(|x| x.path.ends_with("Library") || x.path.ends_with("Containers")), "{c:?}");
    }

    #[test]
    fn two_children_and_removals() {
        let base = m(&[("", 50 * G), ("A", 10 * G), ("A/x", G), ("A/y", G), ("Old", 5 * G)]);
        let now = m(&[("", 51 * G), ("A", 16 * G), ("A/x", 4 * G), ("A/y", 4 * G)]);
        let c = changes(&now, &base, Path::new("/h"), G / 2);
        let paths: Vec<_> = c.iter().map(|x| x.path.clone()).collect();
        assert!(paths.iter().any(|p| p == Path::new("/h/A/x")) && paths.iter().any(|p| p == Path::new("/h/A/y")));
        assert!(!paths.iter().any(|p| p == Path::new("/h/A")), "explained by its two subfolders");
        assert!(c.iter().any(|x| x.path == Path::new("/h/Old") && x.delta() == -(5 * G as i64)));
    }

    #[test]
    fn summary_files() {
        let s = parse_summary("#1700000000\n5000\t\n300\tDev\n").unwrap();
        assert_eq!(s.at, 1_700_000_000);
        assert_eq!(s.sizes.get("Dev"), Some(&300));
        assert_eq!(s.sizes.get(""), Some(&5000));
        assert_eq!(plist_int("<key>APFSContainerFree</key>\n<integer>23548051456</integer>", "APFSContainerFree"), Some(23_548_051_456));
        let xml = "<dict><key>APFSVolumeUUID</key><string>a</string><key>CapacityInUse</key><integer>12</integer>\
<key>DeviceIdentifier</key><string>disk3s1</string><key>Roles</key><array><string>System</string></array></dict>\
<dict><key>APFSVolumeUUID</key><string>b</string><key>CapacityInUse</key><integer>400</integer>\
<key>DeviceIdentifier</key><string>disk3s5</string><key>Roles</key><array><string>Data</string></array></dict>\
<dict><key>APFSVolumeUUID</key><string>c</string><key>CapacityInUse</key><integer>7</integer>\
<key>DeviceIdentifier</key><string>disk2s1</string><key>Roles</key><array><string>Recovery</string></array></dict>\
<dict><key>APFSVolumeUUID</key><string>d</string><key>CapacityInUse</key><integer>4</integer>\
<key>DeviceIdentifier</key><string>disk3s6</string><key>Roles</key><array><string>VM</string></array></dict>";
        let v = parse_volumes(xml, "disk3", 500, 50).unwrap();
        assert_eq!((v.macos, v.data, v.vm, v.other()), (12, 400, 4, 34), "disk2 belongs to another container");
    }

    /// `cargo test --lib live_volumes -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_volumes() {
        println!("{:?}\nsnapshots {:?}\noutside {:?}", volumes(), local_snapshots(), outside_home());
    }
}
