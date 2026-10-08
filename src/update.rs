//! Checks GitHub for a newer release: one small request, at most once a day, and only in builds
//! that know their repository. Release builds made by GitHub Actions do; set `MACPILOT_REPO=owner/name`
//! to enable it in your own builds.
//!
//! The update itself is one click: the release archive is downloaded, checked against the
//! release's SHA-256 list and its code signature, and swapped in when MacPilot quits.

use std::path::{Path, PathBuf};

/// "owner/name" of the GitHub repository the releases are published in.
pub fn repo() -> Option<&'static str> {
    // Set by the release build; local builds fall back to `repository` in Cargo.toml.
    option_env!("MACPILOT_REPO")
        .or(option_env!("GITHUB_REPOSITORY"))
        .or(env!("CARGO_PKG_REPOSITORY").strip_prefix("https://github.com/"))
        .filter(|r| r.contains('/'))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Version without the "v", e.g. "0.2.0".
    pub version: String,
    /// Release page with the downloads.
    pub url: String,
    /// Git tag, e.g. "v0.2.0".
    pub tag: String,
}

/// The latest release if it is newer than this build; `Ok(None)` when up to date.
pub fn check() -> Result<Option<Release>, String> {
    let repo = repo().ok_or("this build does not know its GitHub repository")?;
    let out = std::process::Command::new("/usr/bin/curl")
        .args(["-fsSL", "--max-time", "15", "-H", "Accept: application/vnd.github+json"])
        .arg(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let body = String::from_utf8_lossy(&out.stdout);
    let tag = json_string(&body, "tag_name").ok_or("unexpected answer from GitHub")?;
    let version = tag.trim_start_matches('v').to_string();
    Ok(newer(&version, env!("CARGO_PKG_VERSION")).then(|| Release { url: format!("https://github.com/{repo}/releases/tag/{tag}"), version, tag }))
}

/// Value of the first `"key": "value"` pair in a JSON text (enough for GitHub's flat release fields).
fn json_string(json: &str, key: &str) -> Option<String> {
    let at = json.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = json[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new(cmd).args(args).output().map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(format!("{cmd}: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// The SHA-256 of `file` in a `shasum` list ("<hash>  <name>" per line).
fn listed_hash(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

/// Download a release and check it. Returns the new `MacPilot.app`, ready to be swapped in.
pub fn download(release: &Release) -> Result<PathBuf, String> {
    let repo = repo().ok_or("this build does not know its GitHub repository")?;
    let dir = crate::home().join("Library/Caches/MacPilot/update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let zip_name = format!("MacPilot-{}-macos-universal.zip", release.tag);
    let base = format!("https://github.com/{repo}/releases/download/{}", release.tag);
    let zip = dir.join(&zip_name);
    let sums = run("/usr/bin/curl", &["-fsSL", "--max-time", "30", &format!("{base}/SHA256SUMS.txt")])?;
    run("/usr/bin/curl", &["-fsSL", "--max-time", "600", "-o", &zip.to_string_lossy(), &format!("{base}/{zip_name}")])?;
    // The archive must be the one the release lists.
    let want = listed_hash(&sums, &zip_name).ok_or("the release has no checksum for the app")?;
    let got = run("/usr/bin/shasum", &["-a", "256", &zip.to_string_lossy()])?;
    if got.split_whitespace().next().map(str::to_lowercase).as_deref() != Some(want.as_str()) {
        return Err("the download does not match the release checksum".into());
    }
    run("/usr/bin/ditto", &["-x", "-k", &zip.to_string_lossy(), &dir.to_string_lossy()])?;
    let app = dir.join("MacPilot.app");
    // An intact bundle of the expected version.
    run("/usr/bin/codesign", &["--verify", "--deep", "--strict", &app.to_string_lossy()])?;
    let version = crate::plist::read_xml(&app.join("Contents/Info.plist")).and_then(|x| crate::plist::string(&x, "CFBundleShortVersionString"));
    if version.as_deref() != Some(release.version.as_str()) {
        return Err(format!("expected version {}, the download is {}", release.version, version.unwrap_or_default()));
    }
    let _ = std::fs::remove_file(&zip);
    Ok(app)
}

/// The app bundle MacPilot runs from, if it can be replaced (it is a bundle, in a folder we may write to).
pub fn replaceable_app() -> Option<PathBuf> {
    let app = crate::procs::outer_app(&std::env::current_exe().ok()?)?;
    let parent = std::ffi::CString::new(app.parent()?.to_string_lossy().as_bytes()).ok()?;
    (unsafe { libc::access(parent.as_ptr(), libc::W_OK) } == 0).then_some(app)
}

fn sh_quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"))
}

/// Shell script that waits for process `pid` to end, replaces `current` with `new` (putting the
/// old app back if that fails) and then runs `after` (opening the app).
fn swap_script(pid: u32, current: &Path, new: &Path, after: &str) -> String {
    let (cur, new) = (sh_quote(current), sh_quote(new));
    let old = sh_quote(&current.with_extension("app.old"));
    format!(
        "while kill -0 {pid} 2>/dev/null; do sleep 0.2; done\n\
         rm -rf {old}\n\
         if mv {cur} {old} && mv {new} {cur}; then rm -rf {old}; else [ -e {cur} ] || mv {old} {cur}; fi\n\
         {after}\n"
    )
}

/// Replace the running app with `new_app` as soon as this process quits, then open it again.
/// The caller quits right after.
pub fn install_on_quit(new_app: &Path) -> Result<(), String> {
    let current = replaceable_app().ok_or("MacPilot cannot replace itself here; download the new version instead")?;
    let script = swap_script(std::process::id(), &current, new_app, &format!("open {}", sh_quote(&current)));
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// `a` is a newer version than `b` ("0.10.0" > "0.9.1"; "1.0.0" > "1.0.0-beta").
pub fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| {
        let (core, pre) = v.split_once('-').map_or((v, None), |(c, p)| (c, Some(p.to_string())));
        let nums: Vec<u64> = core.split('.').map(|x| x.parse().unwrap_or(0)).collect();
        (nums, pre)
    };
    let (an, ap) = parse(a);
    let (bn, bp) = parse(b);
    for i in 0..an.len().max(bn.len()) {
        let (x, y) = (an.get(i).copied().unwrap_or(0), bn.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    // Same numbers: a release beats a pre-release.
    matches!((ap, bp), (None, Some(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(newer("0.2.0", "0.1.0"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("1.0", "0.99.1"));
        assert!(newer("1.0.0", "1.0.0-beta"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0-rc1", "0.1.0"));
        assert!(!newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn reads_tag() {
        let j = r#"{"url": "x", "html_url": "https://github.com/a/b/releases/tag/v0.2.0", "tag_name" : "v0.2.0", "name": "MacPilot 0.2.0"}"#;
        assert_eq!(json_string(j, "tag_name").as_deref(), Some("v0.2.0"));
        assert_eq!(json_string(j, "missing"), None);
    }

    #[test]
    fn finds_the_checksum() {
        let sums = "aaaa  other.dmg\n0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef  MacPilot-v1.0.0-macos-universal.zip\n";
        assert_eq!(
            listed_hash(sums, "MacPilot-v1.0.0-macos-universal.zip").unwrap(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert_eq!(listed_hash(sums, "other.dmg"), None, "a short hash is not a hash");
        assert_eq!(listed_hash(sums, "missing.zip"), None);
    }

    /// The swap replaces the app, and puts the old one back when the new one is missing.
    #[test]
    fn swaps_the_app() {
        let dir = std::env::temp_dir().join(format!("macpilot-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (cur, new) = (dir.join("It's Mac Pilot.app"), dir.join("update/MacPilot.app"));
        for (d, v) in [(&cur, "old"), (&new, "new")] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("version"), v).unwrap();
        }
        let sh = |script: String| std::process::Command::new("/bin/sh").arg("-c").arg(script).status().unwrap().success();
        // 999999 is not a running process, so the script does not wait.
        assert!(sh(swap_script(999_999, &cur, &new, "true")));
        assert_eq!(std::fs::read_to_string(cur.join("version")).unwrap(), "new");
        assert!(!new.exists() && !cur.with_extension("app.old").exists());
        // The new app vanished: the current one stays in place.
        sh(swap_script(999_999, &cur, &dir.join("update/nothing.app"), "true"));
        assert_eq!(std::fs::read_to_string(cur.join("version")).unwrap(), "new");
        assert!(!cur.with_extension("app.old").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real download: `MACPILOT_REPO=owner/name cargo test --lib live_download -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_download() {
        let tag = std::env::var("MACPILOT_TAG").unwrap_or("v0.6.1".into());
        let r = Release { version: tag.trim_start_matches('v').to_string(), url: String::new(), tag };
        println!("{:?}", download(&r));
    }

    /// Real request: `MACPILOT_REPO=owner/name cargo test --lib live_check -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_check() {
        println!("{:?} → {:?}", repo(), check());
    }
}
