//! Installed apps: size, last opened, leftovers in ~/Library, and leftovers of apps already removed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::home;

#[derive(Clone, Debug)]
pub struct AppInfo {
    pub path: PathBuf,
    pub name: String,
    pub bundle_id: String,
    pub version: String,
    pub size: u64,
    /// Last opened (Spotlight's "Last opened" or the executable's last read), unix seconds.
    pub last_used: Option<i64>,
    /// Built into macOS or otherwise not removable.
    pub protected: bool,
}

/// Convert "2026-09-23 16:51:08 +0000" (UTC) to unix seconds.
fn parse_mdls_date(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, rest) = s.split_once(' ')?;
    let time = rest.split(' ').next()?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|x| x.parse::<i64>().ok());
    let (hh, mm, ss) = (t.next()??, t.next()??, t.next()??);
    // Days from civil (Howard Hinnant's algorithm).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// "Last opened" dates that Spotlight knows, for all apps in one query.
fn spotlight_last_used() -> HashMap<PathBuf, i64> {
    let out = crate::run_output("mdfind", &["-attr", "kMDItemLastUsedDate", "kMDItemContentType == 'com.apple.application-bundle'"]);
    let mut m = HashMap::new();
    for line in out.lines() {
        let Some((path, attr)) = line.split_once("   kMDItemLastUsedDate = ") else { continue };
        if let Some(t) = parse_mdls_date(attr) {
            m.insert(PathBuf::from(path.trim()), t);
        }
    }
    m
}

fn app_dirs() -> Vec<PathBuf> {
    vec![PathBuf::from("/Applications"), home().join("Applications")]
}

/// Details of one app bundle. `spot_used` is the last use Spotlight knows of, if any.
pub fn info(p: &Path, spot_used: Option<i64>) -> AppInfo {
    let xml = crate::plist::read_xml(&p.join("Contents/Info.plist")).unwrap_or_default();
    let bundle_id = crate::plist::string(&xml, "CFBundleIdentifier").unwrap_or_default();
    let version = crate::plist::string(&xml, "CFBundleShortVersionString").unwrap_or_default();
    let exe = crate::plist::string(&xml, "CFBundleExecutable");
    let name = p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let exe_used = exe.and_then(|e| std::fs::metadata(p.join("Contents/MacOS").join(e)).ok()).map(|m| crate::disk::used_time(&m));
    let last_used = match (spot_used, exe_used) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    let protected = bundle_id == "com.apple.Safari" || p.starts_with("/System");
    AppInfo { path: p.to_path_buf(), name, bundle_id, version, size: crate::disk::measure(p).size, last_used, protected }
}

/// All installed apps (in /Applications and ~/Applications, one folder level deep).
pub fn list() -> Vec<AppInfo> {
    let mut bundles = Vec::new();
    for d in app_dirs() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "app") {
                bundles.push(p);
            } else if e.file_type().is_ok_and(|t| t.is_dir()) {
                // Vendor folders like /Applications/Adobe Photoshop/…app
                if let Ok(rd2) = std::fs::read_dir(&p) {
                    bundles.extend(rd2.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "app")));
                }
            }
        }
    }
    let spot = spotlight_last_used();
    let mut apps: Vec<AppInfo> = crate::disk::apps_pool().install(|| bundles.par_iter().map(|p| info(p, spot.get(p).copied())).collect());
    apps.sort_by_key(|a| std::cmp::Reverse(a.size));
    apps
}

/// Where apps keep their data, and whether a match there is by bundle id or also by name.
fn leftover_places() -> Vec<(PathBuf, bool)> {
    let l = home().join("Library");
    vec![
        (l.join("Application Support"), true),
        (l.join("Caches"), true),
        (l.join("Containers"), false),
        (l.join("Group Containers"), false),
        (l.join("Preferences"), false),
        (l.join("Preferences/ByHost"), false),
        (l.join("Saved Application State"), false),
        (l.join("Logs"), true),
        (l.join("HTTPStorages"), false),
        (l.join("WebKit"), false),
        (l.join("Cookies"), false),
        (l.join("Application Scripts"), false),
        (l.join("LaunchAgents"), false),
        (l.join("Caches/com.apple.nsurlsessiond/Downloads"), false),
    ]
}

/// Does a Library entry belong to `id` (or to `name`, where names are used)?
fn belongs(entry: &str, id: &str, name: Option<&str>) -> bool {
    let e = entry.to_lowercase();
    let id = id.to_lowercase();
    if id.len() < 4 {
        return false;
    }
    let stem = e.trim_end_matches(".plist").trim_end_matches(".savedstate").trim_end_matches(".binarycookies");
    // Exact id, id with suffixes (com.foo.app.helper), ByHost (com.foo.app.UUID), group (TEAMID.com.foo.app).
    let exact = stem == id || stem.starts_with(&format!("{id}.")) || stem.ends_with(&format!(".{id}")) || stem.contains(&format!(".{id}."));
    exact || name.is_some_and(|n| n.len() >= 3 && e == n.to_lowercase())
}

/// Leftover files and folders of one app, with sizes.
pub fn leftovers(app: &AppInfo) -> Vec<(PathBuf, u64)> {
    if app.bundle_id.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    for (dir, by_name) in leftover_places() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if belongs(&n, &app.bundle_id, by_name.then_some(app.name.as_str())) {
                found.push(e.path());
            }
        }
    }
    found.sort();
    found.dedup();
    crate::disk::apps_pool().install(|| {
        found
            .into_par_iter()
            .map(|p| {
                let s = crate::disk::measure(&p).size;
                (p, s)
            })
            .collect()
    })
}

#[derive(Clone, Debug)]
pub struct Orphan {
    pub path: PathBuf,
    pub id: String,
    pub size: u64,
}

impl Orphan {
    /// Sandboxed apps keep their documents inside their container.
    pub fn may_hold_documents(&self) -> bool {
        self.path.components().any(|c| c.as_os_str() == "Containers" || c.as_os_str() == "Group Containers")
    }
}

/// Looks like a reverse-DNS bundle id: com.vendor.app
fn looks_like_bundle_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
}

/// Data left behind by apps that are no longer installed (by bundle id, in Containers,
/// Application Support, Caches, Saved Application State and HTTPStorages). Apple's own ids are ignored.
pub fn orphans(apps: &[AppInfo]) -> Vec<Orphan> {
    let installed: HashSet<String> = apps.iter().map(|a| a.bundle_id.to_lowercase()).filter(|s| !s.is_empty()).collect();
    // Also treat apps outside /Applications (e.g. inside other apps or in /System) as installed.
    // The same vendor ("com.zoom.*") as an installed app means it is probably that app's helper
    // (updaters, agents and extensions often have their own ids) — not a leftover.
    let vendor = |id: &str| id.split('.').take(2).collect::<Vec<_>>().join(".");
    let vendors: HashSet<String> = installed.iter().map(|i| vendor(i)).filter(|v| v.len() > 4 && !GENERIC_VENDORS.contains(&v.as_str())).collect();
    let is_installed = |id: &str| {
        let id = id.to_lowercase();
        installed.iter().any(|i| id == *i || id.starts_with(&format!("{i}.")) || i.starts_with(&format!("{id}."))) || vendors.contains(&vendor(&id))
    };
    let l = home().join("Library");
    let dirs = [l.join("Containers"), l.join("Application Support"), l.join("Caches"), l.join("Saved Application State"), l.join("HTTPStorages")];
    let mut cands = Vec::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            let id = n.trim_end_matches(".savedState").trim_end_matches(".binarycookies").to_string();
            if !looks_like_bundle_id(&id) {
                continue;
            }
            let lower = id.to_lowercase();
            if lower.starts_with("com.apple.") || lower.starts_with("group.com.apple") || is_installed(&id) {
                continue;
            }
            // A process with this id may still be around (menu bar helpers, CLI tools).
            if has_running_or_cli(&lower) {
                continue;
            }
            cands.push((e.path(), id));
        }
    }
    let mut out: Vec<Orphan> = crate::disk::apps_pool().install(|| {
        cands
            .into_par_iter()
            .map(|(path, id)| {
                let size = crate::disk::measure(&path).size;
                Orphan { path, id, size }
            })
            .filter(|o| o.size >= 100_000)
            .collect()
    });
    out.sort_by_key(|a| std::cmp::Reverse(a.size));
    out
}

/// Vendor prefixes shared by unrelated apps.
const GENERIC_VENDORS: &[&str] = &["com.example", "com.github", "org.mozilla", "com.google", "io.github", "com.microsoft"];

/// Bundle ids of apps that live elsewhere (inside other apps, Homebrew casks, system extensions).
fn has_running_or_cli(id: &str) -> bool {
    static EXTRA: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();
    let extra = EXTRA.get_or_init(|| {
        let out = crate::run_output("mdfind", &["-attr", "kMDItemCFBundleIdentifier", "kMDItemContentType == 'com.apple.application-bundle'"]);
        out.lines()
            .filter_map(|l| l.split_once("kMDItemCFBundleIdentifier = ").map(|(_, id)| id.trim().to_lowercase()))
            .filter(|s| s != "(null)")
            .collect()
    });
    extra.iter().any(|i| id == i || id.starts_with(&format!("{i}.")) || i.starts_with(&format!("{id}.")))
}

pub fn is_running(app: &Path, procs: &crate::procs::Snapshot) -> bool {
    procs.procs.iter().any(|p| p.app.as_deref() == Some(app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_leftovers_by_id() {
        assert!(belongs("com.foo.Bar", "com.foo.bar", None));
        assert!(belongs("com.foo.bar.plist", "com.foo.bar", None));
        assert!(belongs("com.foo.bar.ABCD-1234.plist", "com.foo.bar", None));
        assert!(belongs("TEAM123.com.foo.bar", "com.foo.bar", None));
        assert!(belongs("com.foo.bar.savedState", "com.foo.bar", None));
        assert!(belongs("Bar", "com.foo.bar", Some("Bar")));
        assert!(!belongs("com.foo.barista", "com.foo.bar", None));
        assert!(!belongs("Bar", "com.foo.bar", None));
    }

    #[test]
    fn parses_spotlight_dates() {
        assert_eq!(parse_mdls_date("1970-01-02 00:00:10 +0000"), Some(86_410));
        assert_eq!(parse_mdls_date("2026-09-23 16:51:08 +0000"), Some(1_790_182_268));
    }

    #[test]
    fn bundle_id_shape() {
        assert!(looks_like_bundle_id("com.docker.docker"));
        assert!(!looks_like_bundle_id("Google"));
        assert!(!looks_like_bundle_id("some folder.v2"));
    }
}
