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
            let st = if p.exists() { disk::measure(&p) } else { DirStat::default() };
            if tx.send((i, st)).is_err() {
                break;
            }
        }
    });
}
