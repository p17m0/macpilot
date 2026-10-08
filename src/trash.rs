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

/// Why something could not be put back.
#[derive(Debug, PartialEq, Eq)]
pub enum PutBackError {
    /// Something else is at the original place now.
    Occupied,
    /// Not in the Trash any more (emptied, or put back by hand).
    NotInTrash,
    /// Several items with this name are in the Trash — MacPilot cannot tell which one.
    Ambiguous(usize),
    Other(String),
}

/// Move an item from the Trash back to where it was. Finder does it (the Trash itself is off
/// limits to apps without Full Disk Access), finding the item by its name.
pub fn put_back(original: &Path) -> Result<(), PutBackError> {
    if std::fs::symlink_metadata(original).is_ok() {
        return Err(PutBackError::Occupied);
    }
    let (Some(name), Some(parent)) = (original.file_name(), original.parent()) else {
        return Err(PutBackError::Other("not a file path".into()));
    };
    std::fs::create_dir_all(parent).map_err(|e| PutBackError::Other(e.to_string()))?;
    let script = format!(
        "tell application \"Finder\"\nset found to (every item of trash whose name is \"{}\")\nset n to count of found\nif n is 1 then\nmove (item 1 of found) to (POSIX file \"{}\" as alias)\nend if\nreturn n\nend tell",
        applescript_str(Path::new(name)),
        applescript_str(parent)
    );
    let out = std::process::Command::new("osascript").arg("-e").arg(script).output().map_err(|e| PutBackError::Other(e.to_string()))?;
    if !out.status.success() {
        return Err(PutBackError::Other(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    match String::from_utf8_lossy(&out.stdout).trim().parse::<usize>() {
        Ok(1) if std::fs::symlink_metadata(original).is_ok() => Ok(()),
        Ok(0) => Err(PutBackError::NotInTrash),
        Ok(1) => Err(PutBackError::Other("Finder did not move the item".into())),
        Ok(n) => Err(PutBackError::Ambiguous(n)),
        Err(_) => Err(PutBackError::Other(String::from_utf8_lossy(&out.stdout).trim().to_string())),
    }
}

/// Show the Trash in Finder.
pub fn open_trash() {
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"Finder\"\nopen trash\nactivate\nend tell")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

pub fn reveal_in_finder(p: &Path) {
    let _ = std::process::Command::new("open").arg("-R").arg(p).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

#[cfg(test)]
mod put_back_tests {
    use super::*;

    /// Talks to Finder (macOS asks for permission once): `cargo test live_put_back -- --ignored`.
    #[test]
    #[ignore]
    fn live_put_back() {
        let dir = crate::home().join(format!("macpilot-put-back-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(format!("note-{}.txt", std::process::id()));
        std::fs::write(&file, "hello").unwrap();
        assert!(matches!(put_back(&file), Err(PutBackError::Occupied)));
        move_to_trash(std::slice::from_ref(&file)).unwrap();
        assert!(!file.exists());
        put_back(&file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello");
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(put_back(&file), Err(PutBackError::NotInTrash)));
    }
}
