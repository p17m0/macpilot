//! Moving files to the Trash and emptying it.

use std::path::{Path, PathBuf};

fn applescript_str(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"")
}

/// Move items to the Trash. Uses Finder first (so "Put Back" works),
/// then falls back to renaming into `~/.Trash` for anything left over.
pub fn move_to_trash(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    // Finder handles big batches fine, but keep AppleScript lines reasonable.
    for chunk in paths.chunks(200) {
        let list = chunk.iter().map(|p| format!("POSIX file \"{}\"", applescript_str(p))).collect::<Vec<_>>().join(", ");
        let script = format!("tell application \"Finder\" to delete {{{list}}}");
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let trash = crate::home().join(".Trash");
    let mut errors = Vec::new();
    for p in paths {
        if std::fs::symlink_metadata(p).is_err() {
            continue; // already moved by Finder
        }
        if let Err(e) = rename_into(p, &trash) {
            errors.push(format!("{}: {e}", crate::fmt::path(p)));
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
}

fn rename_into(p: &Path, trash: &Path) -> std::io::Result<()> {
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "item".into());
    let mut target = trash.join(&name);
    let mut i = 2;
    while target.exists() {
        target = trash.join(format!("{name} {i}"));
        i += 1;
    }
    std::fs::rename(p, target)
}

/// Permanently delete everything in the Trash (via Finder, like "Empty Trash").
pub fn empty_trash() -> Result<(), String> {
    let out =
        std::process::Command::new("osascript").arg("-e").arg("tell application \"Finder\" to empty trash").output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

pub fn reveal_in_finder(p: &Path) {
    let _ = std::process::Command::new("open").arg("-R").arg(p).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}
