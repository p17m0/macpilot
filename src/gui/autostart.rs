//! Launch at login. On macOS 13+ MacPilot registers itself with `SMAppService`, so it appears in
//! System Settings → General → Login Items like any other app. On macOS 12, or when not running
//! from an app bundle, it falls back to a LaunchAgent in ~/Library/LaunchAgents.

use std::path::PathBuf;

use crate::mac;

const LABEL: &str = "local.macpilot";

fn plist_path() -> PathBuf {
    macpilot::home().join("Library/LaunchAgents").join(format!("{LABEL}.plist"))
}

/// Path to MacPilot.app: the bundle we run from, otherwise /Applications.
fn app_path() -> PathBuf {
    std::env::current_exe().ok().and_then(|e| macpilot::procs::outer_app(&e)).unwrap_or_else(|| PathBuf::from("/Applications/MacPilot.app"))
}

pub fn enabled() -> bool {
    if mac::login_item_supported() { mac::login_item() != mac::LoginItem::Off } else { legacy_enabled() }
}

/// Registered, but switched off by the user in System Settings → Login Items.
pub fn needs_approval() -> bool {
    mac::login_item_supported() && mac::login_item() == mac::LoginItem::NeedsApproval
}

pub fn set(on: bool) -> Result<(), String> {
    if mac::login_item_supported() {
        set_legacy(false)?;
        return mac::set_login_item(on);
    }
    set_legacy(on)
}

/// Move a LaunchAgent made by older versions to a proper login item.
pub fn migrate() {
    if mac::login_item_supported() && legacy_enabled() && mac::set_login_item(true).is_ok() {
        let _ = set_legacy(false);
    }
}

fn legacy_enabled() -> bool {
    plist_path().exists()
}

fn set_legacy(on: bool) -> Result<(), String> {
    let p = plist_path();
    if !on {
        return match std::fs::remove_file(&p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    }
    let app = app_path();
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/bin/open</string>
    <string>-a</string>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>LimitLoadToSessionType</key><string>Aqua</string>
</dict>
</plist>
"#,
        app.display().to_string().replace('&', "&amp;").replace('<', "&lt;")
    );
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, xml).map_err(|e| e.to_string())
}
