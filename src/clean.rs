//! Cleanup: well-known places where junk piles up.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use crate::disk::{self, DelSafety, DirStat};
use crate::{home, tr};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Contents can be moved to the Trash with one click.
    Clean,
    /// Shown for information; clean by hand or with the tool mentioned in the hint.
    Manual,
    /// The Trash itself (can be emptied).
    Trash,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub id: &'static str,
    pub label: &'static str,
    pub path: PathBuf,
    pub hint: &'static str,
    pub kind: Kind,
    pub stat: Option<DirStat>,
}

impl Target {
    pub fn cleanable(&self) -> bool {
        self.kind == Kind::Clean
    }
}

pub fn targets() -> Vec<Target> {
    use Kind::*;
    let h = home();
    let t = |id, label, rel: &str, hint, kind| Target { id, label, path: h.join(rel), hint, kind, stat: None };
    vec![
        t(
            "caches",
            tr("App caches"),
            "Library/Caches",
            tr("Cache of all apps. Recreated automatically; apps may start a bit slower the first time. Quit apps before cleaning."),
            Clean,
        ),
        t("logs", tr("Logs"), "Library/Logs", tr("App logs and crash reports."), Clean),
        t("derived", tr("Xcode DerivedData"), "Library/Developer/Xcode/DerivedData", tr("Xcode build files. Safe — projects rebuild."), Clean),
        t(
            "devsupport",
            tr("Xcode device support"),
            "Library/Developer/Xcode/iOS DeviceSupport",
            tr("Debug symbols for iPhones. Downloaded again when a device connects."),
            Clean,
        ),
        t(
            "previews",
            tr("Xcode Previews"),
            "Library/Developer/Xcode/UserData/Previews",
            tr("SwiftUI preview simulators. Recreated when needed."),
            Clean,
        ),
        t("simcache", tr("Simulator caches"), "Library/Developer/CoreSimulator/Caches", tr("iOS simulator cache."), Clean),
        t(
            "simulators",
            tr("iOS simulators"),
            "Library/Developer/CoreSimulator/Devices",
            tr("Installed simulators with their data. Do not clean by hand — run `xcrun simctl delete unavailable`."),
            Manual,
        ),
        t(
            "archives",
            tr("Xcode archives"),
            "Library/Developer/Xcode/Archives",
            tr("Archives of released builds — needed to read crash reports. Remove old ones by hand."),
            Manual,
        ),
        t(
            "ipsw",
            tr("iOS updates"),
            "Library/iTunes/iPhone Software Updates",
            tr("Downloaded iPhone/iPad firmware. Downloaded again when needed."),
            Clean,
        ),
        t(
            "mobilebackup",
            tr("iPhone backups"),
            "Library/Application Support/MobileSync/Backup",
            tr("Local iPhone/iPad backups. Manage them in Finder → your device → Manage Backups."),
            Manual,
        ),
        t(
            "maildl",
            tr("Mail downloads"),
            "Library/Containers/com.apple.mail/Data/Library/Mail Downloads",
            tr("Attachments you opened from Mail. The originals stay in the messages."),
            Clean,
        ),
        t("npm", tr("npm cache"), ".npm/_cacache", tr("npm package cache. Downloaded again when needed."), Clean),
        t("pnpm", tr("pnpm store"), "Library/pnpm/store", tr("pnpm package store."), Clean),
        t("yarn", tr("Yarn cache"), "Library/Caches/Yarn", tr("Yarn package cache."), Clean),
        t("cargo", tr("Cargo cache"), ".cargo/registry", tr("Rust crate sources. Downloaded again on the next build."), Clean),
        t("gradle", tr("Gradle cache"), ".gradle/caches", tr("Gradle/Android dependency cache."), Clean),
        t(
            "cli",
            tr("Command-line caches (~/.cache)"),
            ".cache",
            tr("Caches of various tools (pip, Hugging Face, pre-commit…). AI models can take tens of GB."),
            Clean,
        ),
        t("brew", tr("Homebrew cache"), "Library/Caches/Homebrew", tr("Downloaded Homebrew packages. You can also run `brew cleanup`."), Clean),
        t(
            "jetbrains",
            tr("JetBrains caches"),
            "Library/Caches/JetBrains",
            tr("Indexes and caches of IntelliJ, PyCharm, WebStorm… Rebuilt on the next launch."),
            Clean,
        ),
        t(
            "docker",
            tr("Docker"),
            "Library/Containers/com.docker.docker",
            tr("Docker images and containers. Clean with `docker system prune`, not by hand."),
            Manual,
        ),
        t("downloads", tr("Downloads"), "Downloads", tr("Downloaded files. Open it and remove what you do not need."), Manual),
        t("trash", tr("Trash"), ".Trash", tr("Files you already deleted. Empty the Trash to actually free the space."), Trash),
    ]
}

pub fn target_safety(t: &Target) -> DelSafety {
    match t.kind {
        Kind::Clean => disk::deletion_safety(&t.path.join("x")).0,
        _ => DelSafety::Careful,
    }
}

/// Measure all targets in the background; results arrive as (index, size).
pub fn measure_all(targets: &[Target], tx: Sender<(usize, DirStat)>) {
    let mut paths: Vec<(usize, PathBuf)> = targets.iter().enumerate().map(|(i, t)| (i, t.path.clone())).collect();
    // Other apps' containers may trigger a macOS permission dialog — measure them last.
    paths.sort_by_key(|(_, p)| p.components().any(|c| c.as_os_str() == "Containers"));
    std::thread::spawn(move || {
        crate::background_qos();
        for (i, p) in paths {
            let st = if disk::present(&p) { disk::measure(&p) } else { DirStat::default() };
            if tx.send((i, st)).is_err() {
                break;
            }
        }
    });
}

/// What the scheduled cleanup touches: only things that are recreated by themselves and that
/// nobody misses — app caches, logs, preview and simulator caches, the npm download cache.
pub const AUTO_IDS: &[&str] = &["caches", "logs", "previews", "simcache", "npm"];

/// Which entries of a target the scheduled cleanup takes. In the app caches it leaves alone
/// Apple's own caches and those of running apps (`busy` says whether a name belongs to one):
/// removing a cache under a running app can make it misbehave until restarted.
fn auto_pick(target_id: &str, names: Vec<String>, busy: &dyn Fn(&str) -> bool) -> Vec<String> {
    names
        .into_iter()
        .filter(|n| {
            if target_id != "caches" {
                return true;
            }
            let l = n.to_lowercase();
            !(l.starts_with("com.apple.") || l == "macpilot" || l == "cloudkit" || busy(n))
        })
        .collect()
}

/// Everything the scheduled cleanup would move to the Trash right now.
pub fn auto_paths(targets: &[Target], busy: &dyn Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for t in targets.iter().filter(|t| AUTO_IDS.contains(&t.id) && t.kind == Kind::Clean && disk::present(&t.path)) {
        let Ok(entries) = disk::read_entries(&t.path) else { continue };
        let names = entries.into_iter().map(|(_, name, _)| name.to_string_lossy().to_string()).collect();
        for n in auto_pick(t.id, names, busy) {
            let p = t.path.join(&n);
            if disk::deletion_safety(&p).0 == DelSafety::Safe {
                out.push(p);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_cleanup_spares_running_apps() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let busy = |n: &str| n.eq_ignore_ascii_case("com.hnc.Discord") || n == "Firefox";
        let picked = auto_pick("caches", names(&["com.hnc.Discord", "Firefox", "com.apple.Safari", "MacPilot", "com.old.app", "Yarn"]), &busy);
        assert_eq!(picked, names(&["com.old.app", "Yarn"]));
        // Logs and the like are taken whole.
        assert_eq!(auto_pick("logs", names(&["Firefox", "DiagnosticReports"]), &busy).len(), 2);
        // Only targets that are safe to clean without asking are on the list.
        let all = targets();
        for id in AUTO_IDS {
            let t = all.iter().find(|t| t.id == *id).expect(id);
            assert_eq!((t.kind, target_safety(t)), (Kind::Clean, DelSafety::Safe), "{id}");
        }
    }
}
