//! Launch at login via a LaunchAgent in ~/Library/LaunchAgents.

use std::path::PathBuf;

const LABEL: &str = "local.macpilot";

fn plist_path() -> PathBuf {
    macpilot::home().join("Library/LaunchAgents").join(format!("{LABEL}.plist"))
}

/// Path to MacPilot.app: the bundle we run from, otherwise /Applications.
fn app_path() -> PathBuf {
    std::env::current_exe().ok().and_then(|e| macpilot::procs::outer_app(&e)).unwrap_or_else(|| PathBuf::from("/Applications/MacPilot.app"))
}

pub fn enabled() -> bool {
    plist_path().exists()
}

pub fn set(on: bool) -> Result<(), String> {
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
