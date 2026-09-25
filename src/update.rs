//! Checks GitHub for a newer release: one small request, at most once a day, and only in builds
//! that know their repository. Release builds made by GitHub Actions do; set `MACPILOT_REPO=owner/name`
//! to enable it in your own builds.

/// "owner/name" of the GitHub repository this build comes from.
pub fn repo() -> Option<&'static str> {
    option_env!("MACPILOT_REPO").or(option_env!("GITHUB_REPOSITORY")).filter(|r| r.contains('/'))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Version without the "v", e.g. "0.2.0".
    pub version: String,
    /// Release page with the downloads.
    pub url: String,
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
    Ok(newer(&version, env!("CARGO_PKG_VERSION")).then(|| Release { url: format!("https://github.com/{repo}/releases/tag/{tag}"), version }))
}

/// Value of the first `"key": "value"` pair in a JSON text (enough for GitHub's flat release fields).
fn json_string(json: &str, key: &str) -> Option<String> {
    let at = json.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = json[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
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

    /// Real request: `MACPILOT_REPO=owner/name cargo test --lib live_check -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_check() {
        println!("{:?} → {:?}", repo(), check());
    }
}
