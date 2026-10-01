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

/// Leftover files and folders of one installed app (for a full uninstall), with sizes.
pub fn leftovers(app: &AppInfo) -> Vec<(PathBuf, u64)> {
    if app.bundle_id.is_empty() {
        return Vec::new();
    }
    let found = traces_in(&home().join("Library"), &app.bundle_id, &[app.name.clone(), name_of_id(&app.bundle_id)]);
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

/// Everything in `lib` (a Library folder) that belongs to the app `id`: entries named by its bundle id
/// or a helper's ("com.foo.app.ShipIt"), and folders named like the app in the places that use names.
fn traces_in(lib: &Path, id: &str, names: &[String]) -> Vec<PathBuf> {
    let id = id.to_lowercase();
    if id.len() < 4 {
        return Vec::new();
    }
    let names: Vec<String> = names.iter().map(|n| loose(n)).filter(|n| n.len() >= 3).collect();
    let mut found = Vec::new();
    for (place, by_name) in PLACES {
        let Ok(rd) = std::fs::read_dir(lib.join(place)) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            let eid = id_of_entry(&n).to_lowercase();
            let by_id = eid == id || eid.starts_with(&format!("{id}."));
            let by_nm = *by_name && !n.contains('.') && names.contains(&loose(&n));
            if by_id || by_nm {
                found.push(e.path());
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Everything one removed app left behind, possibly in several places of ~/Library.
#[derive(Clone, Debug)]
pub struct Orphan {
    /// Bundle id ("com.hnc.Discord").
    pub id: String,
    /// Readable name ("Discord").
    pub name: String,
    /// The files and folders, with their size (`None`: cannot be measured without Full Disk Access).
    pub items: Vec<(PathBuf, Option<u64>)>,
    /// Sum of the sizes that are known.
    pub size: u64,
}

impl Orphan {
    /// Sandboxed apps keep their documents inside their container.
    pub fn may_hold_documents(&self) -> bool {
        self.items.iter().any(|(p, _)| p.components().any(|c| c.as_os_str() == "Containers" || c.as_os_str() == "Group Containers"))
    }

    /// Some sizes are unknown (containers without Full Disk Access).
    pub fn size_unknown(&self) -> bool {
        self.items.iter().any(|(_, s)| s.is_none())
    }

    /// Identifies the group in a selection.
    pub fn key(&self) -> &Path {
        &self.items[0].0
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.items.iter().map(|(p, _)| p.clone()).collect()
    }
}

/// Looks like a reverse-DNS bundle id: com.vendor.app
fn looks_like_bundle_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
}

/// The bundle id in the name of an entry of ~/Library: "com.foo.app.plist", "com.foo.app.savedState",
/// "ABCDE12345.com.foo.app" (group container with a team id), "group.com.foo", "com.foo.app.<UUID>.plist".
fn id_of_entry(name: &str) -> String {
    let mut n = name;
    for suffix in [".plist", ".savedState", ".binarycookies"] {
        n = n.strip_suffix(suffix).unwrap_or(n);
    }
    // ByHost preferences end with the hardware UUID.
    if let Some((head, tail)) = n.rsplit_once('.') {
        if tail.len() == 36 && tail.matches('-').count() == 4 {
            n = head;
        }
    }
    let n = n.strip_prefix("group.").or_else(|| n.strip_prefix("groups.")).or_else(|| n.strip_prefix("systemgroup.")).unwrap_or(n);
    // Team id prefix: ten uppercase letters and digits.
    match n.split_once('.') {
        Some((team, rest)) if team.len() == 10 && team.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) => rest.to_string(),
        _ => n.to_string(),
    }
}

/// A readable app name from a bundle id: the last part that says something.
/// "com.hnc.Discord" → "Discord", "dev.kiro.desktop" → "Kiro", "app.hiddify.com" → "Hiddify".
fn name_of_id(id: &str) -> String {
    const VAGUE: &[&str] = &[
        "com",
        "org",
        "net",
        "io",
        "app",
        "dev",
        "co",
        "ru",
        "de",
        "nl",
        "fr",
        "uk",
        "us",
        "me",
        "ai",
        "macos",
        "mac",
        "osx",
        "desktop",
        "client",
        "ide",
        "helper",
        "launcher",
        "agent",
        "steam",
        "app-store",
        "appstore",
        "release",
        "beta",
        "pro",
    ];
    let parts: Vec<&str> = id.split('.').collect();
    let pick = parts.iter().rev().find(|p| p.len() > 1 && !VAGUE.contains(&p.to_lowercase().as_str())).or(parts.last()).copied().unwrap_or(id);
    let mut c = pick.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Folder names compared loosely: case, spaces, dashes and underscores do not matter.
fn loose(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Where apps leave data, in ~/Library, and whether apps also use their plain name there
/// ("Application Support/Discord") instead of their bundle id.
const PLACES: &[(&str, bool)] = &[
    ("Containers", false),
    ("Group Containers", false),
    ("Application Support", true),
    ("Application Scripts", false),
    ("Caches", true),
    ("Logs", true),
    ("Preferences", false),
    ("Preferences/ByHost", false),
    ("Saved Application State", false),
    ("HTTPStorages", false),
    ("WebKit", false),
    ("Cookies", false),
    ("Caches/com.apple.nsurlsessiond/Downloads", false),
    // Services: a removed app's agent is removed with it, but in general an agent may belong to a
    // command-line tool (Homebrew services and the like) — the Startup page judges those.
    ("LaunchAgents", false),
];

/// Data of Apple's own apps, some of which use unusual ids (Shortcuts kept Workflow's "is.workflow").
fn is_apple(id: &str) -> bool {
    let l = id.to_lowercase();
    ["com.apple.", "apple.", "is.workflow.", "swift-playgrounds"].iter().any(|p| l.starts_with(p))
}

/// Places not searched for leftovers of removed apps (see `PLACES`).
const NOT_FOR_LEFTOVERS: &[&str] = &["LaunchAgents"];

/// Updaters shared by all apps of a vendor: (id prefix, vendor prefix of an installed app).
const SHARED_UPDATERS: &[(&str, &str)] = &[
    ("com.google.keystone", "com.google."),
    ("com.google.googleupdater", "com.google."),
    ("com.google.softwareupdate", "com.google."),
    ("com.microsoft.autoupdate", "com.microsoft."),
];

/// Data left behind by apps that are no longer installed, grouped by app. Found by bundle id in
/// every place of ~/Library where apps keep data, and by the app's name in Application Support,
/// Caches and Logs. Apple's own data, installed apps and their helpers are skipped.
pub fn orphans(apps: &[AppInfo]) -> Vec<Orphan> {
    let installed: HashSet<String> = apps.iter().map(|a| a.bundle_id.to_lowercase()).filter(|s| !s.is_empty()).collect();
    let installed_names: Vec<String> = apps.iter().map(|a| loose(&a.name)).filter(|n| n.len() >= 4).collect();
    // "Docker Desktop" belongs to the installed "Docker": names that contain an installed app's name are kept.
    let like_installed = |name: &str| {
        let n = loose(name);
        n.len() >= 4 && installed_names.iter().any(|i| n.contains(i.as_str()) || i.contains(n.as_str()))
    };
    // The same vendor ("com.zoom.*") as an installed app means it is probably that app's helper
    // (updaters, agents and extensions often have their own ids) — not a leftover.
    let vendor = |id: &str| id.split('.').take(2).collect::<Vec<_>>().join(".");
    let vendors: HashSet<String> = installed.iter().map(|i| vendor(i)).filter(|v| v.len() > 4 && !GENERIC_VENDORS.contains(&v.as_str())).collect();
    let is_installed = |id: &str| {
        let id = id.to_lowercase();
        let updater = SHARED_UPDATERS.iter().any(|(u, v)| id.starts_with(u) && installed.iter().any(|i| i.starts_with(v)));
        updater
            || installed.iter().any(|i| id == *i || id.starts_with(&format!("{i}.")) || i.starts_with(&format!("{id}.")))
            || vendors.contains(&vendor(&id))
    };
    let groups = group_entries(
        &home().join("Library"),
        &|id: &str| {
            let lower = id.to_lowercase();
            is_apple(id)
                || is_installed(id)
                || like_installed(&name_of_id(id))
                || like_installed(id.rsplit('.').next().unwrap_or(id))
                || has_running_or_cli(&lower)
        },
        &like_installed,
    );

    let mut out: Vec<Orphan> = crate::disk::apps_pool().install(|| {
        groups
            .into_par_iter()
            .map(|(_, (id, paths))| {
                // Inside other apps' containers sizes need Full Disk Access; unknown is shown as such.
                let items: Vec<(PathBuf, Option<u64>)> =
                    paths.into_iter().map(|p| (p.clone(), crate::disk::reachable(&p.join("x")).then(|| crate::disk::measure(&p).size))).collect();
                let size = items.iter().filter_map(|(_, s)| *s).sum();
                Orphan { name: name_of_id(&id), id, items, size }
            })
            .collect()
    });
    for o in &mut out {
        o.items.sort_by_key(|(_, s)| std::cmp::Reverse(s.unwrap_or(0)));
    }
    out.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

/// Library entries of apps, grouped by bundle id: (lowercased id → (id, paths)). Helpers
/// ("com.foo.app.ShipIt") join their app, and folders named like the app join too.
/// `skip_id` drops ids that are not leftovers; `skip_name` drops names that belong to installed apps.
type Groups = HashMap<String, (String, Vec<PathBuf>)>;

fn group_entries(lib: &Path, skip_id: &dyn Fn(&str) -> bool, skip_name: &dyn Fn(&str) -> bool) -> Groups {
    // 1. Entries named by bundle id.
    let mut groups: Groups = HashMap::new();
    for (place, _) in PLACES.iter().filter(|(p, _)| !NOT_FOR_LEFTOVERS.contains(p)) {
        let Ok(rd) = std::fs::read_dir(lib.join(place)) else { continue };
        for e in rd.flatten() {
            let id = id_of_entry(&e.file_name().to_string_lossy());
            if !looks_like_bundle_id(&id) || skip_id(&id) {
                continue;
            }
            groups.entry(id.to_lowercase()).or_insert_with(|| (id.clone(), Vec::new())).1.push(e.path());
        }
    }

    // Helpers of an app ("dev.kiro.desktop.ShipIt") join the app's group.
    let mut keys: Vec<String> = groups.keys().cloned().collect();
    keys.sort_by_key(|k| k.len());
    for k in keys {
        let parent = groups.keys().filter(|p| k.starts_with(&format!("{p}."))).min_by_key(|p| p.len()).cloned();
        if let Some(parent) = parent {
            if let Some((_, paths)) = groups.remove(&k) {
                if let Some(g) = groups.get_mut(&parent) {
                    g.1.extend(paths);
                }
            }
        }
    }

    // 2. Folders named like one of those apps ("Application Support/Discord" next to "com.hnc.Discord").
    let mut by_name: HashMap<String, String> = HashMap::new();
    for (key, (id, _)) in &groups {
        for n in [name_of_id(id), id.rsplit('.').next().unwrap_or(id).to_string()] {
            if loose(&n).len() >= 4 && !skip_name(&n) {
                by_name.insert(loose(&n), key.clone());
            }
        }
    }
    for (place, named) in PLACES {
        if !named {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(lib.join(place)) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.contains('.') || !e.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            if let Some(key) = by_name.get(&loose(&n)) {
                if let Some(g) = groups.get_mut(key) {
                    g.1.push(e.path());
                }
            }
        }
    }
    groups
}

/// Vendor prefixes shared by unrelated apps.
const GENERIC_VENDORS: &[&str] =
    &["com.example", "com.github", "org.mozilla", "com.google", "io.github", "com.microsoft", "com.electron", "org.webkit"];

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

#[cfg(test)]
mod orphan_tests {
    use super::*;

    /// A small fake Library with an app's traces in many places, plus things that must not count.
    fn fake_library(tag: &str) -> PathBuf {
        let lib = std::env::temp_dir().join(format!("macpilot-lib-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&lib);
        for d in [
            "Application Support/com.foo.Bard",
            "Application Support/Bard",
            "Application Support/com.apple.Notes",
            "Caches/Docker Desktop",
            "Containers/com.foo.Bard",
            "Group Containers/ABCDE12345.com.foo.Bard",
            "Preferences/ByHost",
            "LaunchAgents",
        ] {
            std::fs::create_dir_all(lib.join(d)).unwrap();
        }
        std::fs::write(lib.join("Application Support/Bard/data.db"), b"x").unwrap();
        for f in [
            "Preferences/com.foo.Bard.plist",
            "Preferences/ByHost/com.foo.Bard.ShipIt.0A1B2C3D-1111-2222-3333-444455556666.plist",
            "Preferences/com.electron.dockerdesktop.plist",
            "LaunchAgents/com.foo.Bard.agent.plist",
            "LaunchAgents/homebrew.mxcl.redis.plist",
            "Preferences/com.foo.bardista.plist",
        ] {
            std::fs::write(lib.join(f), b"x").unwrap();
        }
        lib
    }

    #[test]
    fn leftovers_of_removed_apps() {
        let lib = fake_library("orphans");
        // "Docker" is installed, so "Docker Desktop" and com.electron.dockerdesktop are its own data.
        let installed = ["docker".to_string()];
        let like_installed = |n: &str| installed.iter().any(|i| loose(n).contains(i.as_str()));
        let groups = group_entries(&lib, &|id: &str| is_apple(id) || like_installed(&name_of_id(id)), &like_installed);
        let bar = &groups["com.foo.bard"].1;
        let rel: Vec<String> = bar.iter().map(|p| p.strip_prefix(&lib).unwrap().display().to_string()).collect();
        for want in [
            "Application Support/com.foo.Bard",
            "Application Support/Bard",
            "Containers/com.foo.Bard",
            "Group Containers/ABCDE12345.com.foo.Bard",
            "Preferences/com.foo.Bard.plist",
            "Preferences/ByHost/com.foo.Bard.ShipIt.0A1B2C3D-1111-2222-3333-444455556666.plist",
        ] {
            assert!(rel.iter().any(|r| r == want), "missing {want} in {rel:?}");
        }
        // Services are judged on the Startup page, not here.
        assert!(!rel.iter().any(|r| r.starts_with("LaunchAgents")), "{rel:?}");
        assert!(!rel.iter().any(|r| r.contains("bardista")), "another app with a longer id: {rel:?}");
        assert_eq!(groups.len(), 2, "Bard and Bardista are leftovers: {:?}", groups.keys().collect::<Vec<_>>());
        let _ = std::fs::remove_dir_all(&lib);
    }

    #[test]
    fn traces_of_an_installed_app() {
        let lib = fake_library("traces");
        let t = traces_in(&lib, "com.foo.Bard", &["Bard".into()]);
        let rel: Vec<String> = t.iter().map(|p| p.strip_prefix(&lib).unwrap().display().to_string()).collect();
        assert_eq!(rel.len(), 7, "{rel:?}"); // six above + its launch agent
        assert!(rel.iter().any(|r| r == "LaunchAgents/com.foo.Bard.agent.plist"));
        assert!(!rel.iter().any(|r| r.contains("redis") || r.contains("Docker") || r.contains("bardista")), "{rel:?}");
        let _ = std::fs::remove_dir_all(&lib);
    }

    #[test]
    fn ids_from_library_entries() {
        assert_eq!(id_of_entry("com.hnc.Discord.plist"), "com.hnc.Discord");
        assert_eq!(id_of_entry("com.hnc.Discord.savedState"), "com.hnc.Discord");
        assert_eq!(id_of_entry("com.hnc.Discord.binarycookies"), "com.hnc.Discord");
        assert_eq!(id_of_entry("com.foo.app.0A1B2C3D-1111-2222-3333-444455556666.plist"), "com.foo.app");
        assert_eq!(id_of_entry("ABCDE12345.com.foo.app"), "com.foo.app");
        assert_eq!(id_of_entry("group.com.foo.shared"), "com.foo.shared");
        assert_eq!(id_of_entry("systemgroup.com.apple.icloud.plist"), "com.apple.icloud");
        assert!(is_apple(&id_of_entry("groups.com.apple.podcasts")));
        assert!(is_apple("is.workflow.shortcuts"));
        assert!(!is_apple("com.hnc.Discord"));
        assert_eq!(name_of_id("com.operasoftware.OperaGX"), "OperaGX");
        assert_eq!(name_of_id("dev.kiro.desktop"), "Kiro");
        assert_eq!(name_of_id("app.hiddify.com"), "Hiddify");
        assert_eq!(name_of_id("com.aspyr.civ6.steam"), "Civ6");
        assert_eq!(loose("Opera GX"), loose("OperaGX"));
        assert_eq!(loose("visual-studio_code"), "visualstudiocode");
    }
}
