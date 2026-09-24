//! Startup items: launch agents and daemons that start with the Mac or at login.
//! Disabling uses `launchctl disable` + `bootout` (reversible, files are not touched).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::home;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// ~/Library/LaunchAgents — only for you, no password needed.
    User,
    /// /Library/LaunchAgents — for every user, at login.
    AllUsers,
    /// /Library/LaunchDaemons — system-wide, at boot, as root.
    System,
}

#[derive(Clone, Debug)]
pub struct StartupItem {
    pub path: PathBuf,
    pub label: String,
    /// Program that is started.
    pub program: String,
    pub scope: Scope,
    pub run_at_load: bool,
    pub keep_alive: bool,
    pub disabled: bool,
    /// The program no longer exists (leftover of a removed app).
    pub broken: bool,
    /// Known unwanted software (adware, "cleaners" that nag).
    pub unwanted: bool,
    pub vendor: String,
}

/// Labels of well-known unwanted software.
fn is_unwanted(label: &str) -> bool {
    let l = label.to_lowercase();
    [
        "mackeeper",
        "zeobit",
        "com.pcv.",
        "advancedmaccleaner",
        "com.mplayerx",
        "com.genieo",
        "com.installcore",
        "com.spigot",
        "searchbaron",
        "com.shlp.",
    ]
    .iter()
    .any(|k| l.contains(k))
}

/// Vendor shown to the user: the app a startup item belongs to, or a readable name from its label.
fn vendor_of(label: &str, program: &str) -> String {
    if let Some(app) = crate::procs::outer_app(Path::new(program)) {
        return crate::procs::bundle_name(&app);
    }
    const KNOWN: &[(&str, &str)] = &[
        ("mackeeper", "MacKeeper"),
        ("epicgames", "Epic Games"),
        ("xk72", "Charles Proxy"),
        ("carriez", "RustDesk"),
        ("github.facebook", "Watchman (Meta)"),
        ("valvesoftware", "Valve (Steam)"),
        ("google", "Google"),
        ("microsoft", "Microsoft"),
        ("adobe", "Adobe"),
        ("docker", "Docker"),
        ("cloudflare", "Cloudflare"),
        ("zoom", "Zoom"),
        ("browsec", "Browsec"),
        ("jetbrains", "JetBrains"),
    ];
    let l = label.to_lowercase();
    if let Some((_, name)) = KNOWN.iter().find(|(k, _)| l.contains(k)) {
        return (*name).to_string();
    }
    let parts: Vec<&str> = label.split('.').collect();
    let raw = match parts.as_slice() {
        ["homebrew", "mxcl", name, ..] => return format!("Homebrew ({name})"),
        [tld, v, ..] if matches!(*tld, "com" | "org" | "net" | "io" | "us" | "ru" | "de" | "app" | "co" | "dev") => *v,
        [v, ..] => *v,
        [] => "",
    };
    let mut c = raw.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

/// Labels disabled through `launchctl disable` in a domain.
fn disabled_labels(domain: &str) -> HashSet<String> {
    let out = crate::run_output("launchctl", &["print-disabled", domain]);
    out.lines()
        .filter_map(|l| {
            let l = l.trim();
            let (label, state) = l.split_once("=>")?;
            state.trim().starts_with("disabled").then(|| label.trim().trim_matches('"').to_string())
        })
        .collect()
}

pub fn list() -> Vec<StartupItem> {
    let dirs = [
        (home().join("Library/LaunchAgents"), Scope::User),
        (PathBuf::from("/Library/LaunchAgents"), Scope::AllUsers),
        (PathBuf::from("/Library/LaunchDaemons"), Scope::System),
    ];
    let user_disabled = disabled_labels(&format!("gui/{}", uid()));
    let system_disabled = disabled_labels("system");
    let mut out = Vec::new();
    for (dir, scope) in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "plist") {
                continue;
            }
            let Some(xml) = crate::plist::read_xml(&path) else { continue };
            let label = crate::plist::string(&xml, "Label").unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string());
            let program = crate::plist::string(&xml, "Program")
                .or_else(|| crate::plist::array(&xml, "ProgramArguments").into_iter().next())
                .unwrap_or_default();
            let broken = program.starts_with('/') && !Path::new(&program).exists();
            let disabled = crate::plist::bool(&xml, "Disabled").unwrap_or(false)
                || match scope {
                    Scope::System => system_disabled.contains(&label),
                    _ => user_disabled.contains(&label),
                };
            out.push(StartupItem {
                vendor: vendor_of(&label, &program),
                unwanted: is_unwanted(&label),
                run_at_load: crate::plist::bool(&xml, "RunAtLoad").unwrap_or(false),
                keep_alive: crate::plist::bool(&xml, "KeepAlive").unwrap_or(false),
                path,
                label,
                program,
                scope,
                disabled,
                broken,
            });
        }
    }
    out.sort_by(|a, b| (b.unwanted, b.broken, a.vendor.to_lowercase()).cmp(&(a.unwanted, a.broken, b.vendor.to_lowercase())));
    out
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Enable or disable an item. User items need no password; others show the macOS admin prompt.
pub fn set_enabled(item: &StartupItem, on: bool) -> Result<(), String> {
    let (domain, target) = match item.scope {
        Scope::User | Scope::AllUsers => (format!("gui/{}", uid()), format!("gui/{}/{}", uid(), item.label)),
        Scope::System => ("system".to_string(), format!("system/{}", item.label)),
    };
    let plist = item.path.to_string_lossy().to_string();
    let cmds = if on {
        vec![
            format!("launchctl enable {}", sh_quote(&target)),
            format!("launchctl bootstrap {} {} 2>/dev/null || true", sh_quote(&domain), sh_quote(&plist)),
        ]
    } else {
        vec![format!("launchctl disable {}", sh_quote(&target)), format!("launchctl bootout {} 2>/dev/null || true", sh_quote(&target))]
    };
    let script = cmds.join("; ");
    let needs_admin = item.scope == Scope::System;
    let out = if needs_admin {
        let esc = script.replace('\\', "\\\\").replace('"', "\\\"");
        std::process::Command::new("osascript").arg("-e").arg(format!("do shell script \"{esc}\" with administrator privileges")).output()
    } else {
        std::process::Command::new("/bin/sh").arg("-c").arg(&script).output()
    }
    .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.contains("-128") { crate::tr("Cancelled.").to_string() } else { err })
    }
}

/// Open System Settings → General → Login Items (for "Open at Login" apps).
pub fn open_login_items_settings() {
    let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.LoginItems-Settings.extension").spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendors_and_flags() {
        assert_eq!(vendor_of("com.google.keystone.agent", ""), "Google");
        assert_eq!(vendor_of("homebrew.mxcl.mysql", "/opt/homebrew/bin/mysqld"), "Homebrew (mysql)");
        assert_eq!(vendor_of("AmneziaVPN-service", "/Applications/AmneziaVPN.app/Contents/MacOS/svc"), "AmneziaVPN");
        assert!(is_unwanted("com.mackeeper.MacKeeperAgent"));
        assert!(!is_unwanted("com.docker.vmnetd"));
    }
}
