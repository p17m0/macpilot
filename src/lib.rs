//! MacPilot core: processes, disk analysis, cleanup, apps and startup items.
//! Shared by the window app (`macpilot-gui`) and the terminal app (`macpilot`).

pub mod actionlog;
pub mod apps;
pub mod appupdates;
pub mod battery;
pub mod clean;
pub mod devjunk;
pub mod dirmap;
pub mod disk;
pub mod dupes;
pub mod fmt;
pub mod i18n;
pub mod net;
pub mod plist;
pub mod procs;
pub mod sensors;
pub mod settings;
pub mod space;
pub mod startup;
pub mod trash;
pub mod trashlog;
pub mod update;

pub use i18n::{tr, trf};

use std::path::PathBuf;

/// The user's home folder.
pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// Whether this app has Full Disk Access. The TCC database can only be opened with it,
/// and the check never triggers a permission dialog.
pub fn has_full_disk_access() -> bool {
    std::fs::File::open(home().join("Library/Application Support/com.apple.TCC/TCC.db")).is_ok()
}

/// [`has_full_disk_access`], re-checked at most every 10 seconds (it is asked for every guarded folder).
pub fn full_disk_access_cached() -> bool {
    static CACHE: std::sync::Mutex<Option<(std::time::Instant, bool)>> = std::sync::Mutex::new(None);
    let mut c = CACHE.lock().unwrap();
    match *c {
        Some((t, v)) if t.elapsed() < std::time::Duration::from_secs(10) => v,
        _ => {
            let v = has_full_disk_access();
            *c = Some((std::time::Instant::now(), v));
            v
        }
    }
}

/// Open System Settings → Privacy & Security → Full Disk Access.
pub fn open_full_disk_access_settings() {
    let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles").spawn();
}

/// Open System Settings → Privacy & Security → Files and Folders.
pub fn open_files_and_folders_settings() {
    let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_FilesAndFolders").spawn();
}

/// Run a command and return its stdout (empty on failure).
pub fn run_output(cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Lower the current thread's priority so background scans stay out of the way
/// (macOS schedules "utility" work on efficiency cores).
pub fn background_qos() {
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
    }
}
