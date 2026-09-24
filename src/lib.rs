//! MacPilot core: processes, disk analysis, cleanup, apps and startup items.
//! Shared by the window app (`macpilot-gui`) and the terminal app (`macpilot`).

pub mod apps;
pub mod clean;
pub mod devjunk;
pub mod disk;
pub mod dupes;
pub mod fmt;
pub mod i18n;
pub mod plist;
pub mod procs;
pub mod settings;
pub mod startup;
pub mod trash;

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

/// Open System Settings → Privacy & Security → Full Disk Access.
pub fn open_full_disk_access_settings() {
    let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles").spawn();
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
