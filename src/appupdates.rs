//! Which installed apps have a newer version: the Mac App Store (Apple's public lookup), Homebrew
//! casks, and apps with their own Sparkle update feed. Runs only when the user asks for it.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::apps::AppInfo;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Mac App Store; `id` opens the app's page there.
    AppStore { id: u64 },
    /// Installed with `brew install --cask TOKEN`.
    Homebrew { token: String },
    /// The app updates itself (Sparkle): opening it offers the update.
    Sparkle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub app: PathBuf,
    pub name: String,
    pub installed: String,
    pub latest: String,
    pub source: Source,
}

/// What a check found: the updates and how many apps could be checked at all.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub updates: Vec<Update>,
    pub checked: usize,
    pub total: usize,
}

/// Numbers of a version: "12.10 (beta 2)" → [12, 10]. Only the leading dotted part counts.
fn numbers(v: &str) -> Vec<u64> {
    let lead: String = v.trim().trim_start_matches(['v', 'V']).chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    lead.split('.').filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(0)).collect()
}

/// `latest` is a higher version than `installed`. Versions that do not start with a number never count.
pub fn newer(latest: &str, installed: &str) -> bool {
    let (mut a, mut b) = (numbers(latest), numbers(installed));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

/// Where the value of `"key":` starts in a JSON text (the first real key, not one inside a string).
fn json_value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\"");
    let mut from = 0;
    while let Some(i) = json[from..].find(&pat) {
        let at = from + i;
        from = at + pat.len();
        if json[..at].ends_with('\\') {
            continue;
        }
        if let Some(rest) = json[from..].trim_start().strip_prefix(':') {
            return Some(rest.trim_start());
        }
    }
    None
}

fn json_string(json: &str, key: &str) -> Option<String> {
    let rest = json_value(json, key)?.strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

fn json_number(json: &str, key: &str) -> Option<u64> {
    let rest = json_value(json, key)?;
    rest[..rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len())].parse().ok()
}

fn curl(url: &str) -> Option<String> {
    let out = std::process::Command::new("/usr/bin/curl").args(["-fsSL", "--max-time", "12", url]).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Version and store id from Apple's lookup answer for one app.
fn parse_lookup(json: &str) -> Option<(String, u64)> {
    if json_number(json, "resultCount")? == 0 {
        return None;
    }
    Some((json_string(json, "version")?, json_number(json, "trackId")?))
}

/// The user's region ("ru" from "en_US@rg=ruzzzz" or "ru_RU"), for the right App Store.
fn region() -> String {
    let loc = crate::run_output("defaults", &["read", "-g", "AppleLocale"]);
    let loc = loc.trim();
    let r = match loc.split_once("@rg=") {
        Some((_, rg)) => rg.chars().take(2).collect::<String>(),
        None => loc.rsplit('_').next().unwrap_or("").chars().take(2).collect(),
    };
    if r.len() == 2 && r.chars().all(|c| c.is_ascii_alphabetic()) { r.to_lowercase() } else { "us".into() }
}

fn app_store(app: &AppInfo, region: &str) -> Option<(String, Source)> {
    if app.bundle_id.is_empty() || !app.bundle_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return None;
    }
    let ask = |country: &str| {
        curl(&format!("https://itunes.apple.com/lookup?bundleId={}&country={country}&entity=macSoftware", app.bundle_id))
            .and_then(|j| parse_lookup(&j))
    };
    let (version, id) = ask(region).or_else(|| (region != "us").then(|| ask("us")).flatten())?;
    Some((version, Source::AppStore { id }))
}

/// The highest version in a Sparkle appcast (items of other channels, such as betas, are skipped).
fn parse_appcast(xml: &str) -> Option<String> {
    let mut best: Option<String> = None;
    for item in xml.split("<item>").skip(1) {
        let item = &item[..item.find("</item>").unwrap_or(item.len())];
        if item.contains("<sparkle:channel>") {
            continue;
        }
        let tag = item.find("<sparkle:shortVersionString>").and_then(|at| {
            let rest = &item[at + 28..];
            Some(rest[..rest.find('<')?].trim().to_string())
        });
        let attr = item.find("sparkle:shortVersionString=\"").and_then(|at| {
            let rest = &item[at + 28..];
            Some(rest[..rest.find('"')?].trim().to_string())
        });
        let Some(v) = tag.or(attr) else { continue };
        if best.as_ref().is_none_or(|b| newer(&v, b)) && !numbers(&v).is_empty() {
            best = Some(v);
        }
    }
    best
}

fn sparkle(app: &AppInfo) -> Option<(String, Source)> {
    let xml = crate::plist::read_xml(&app.path.join("Contents/Info.plist"))?;
    let feed = crate::plist::string(&xml, "SUFeedURL").filter(|u| u.starts_with("https://"))?;
    Some((parse_appcast(&curl(&feed)?)?, Source::Sparkle))
}

fn brew() -> Option<PathBuf> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"].iter().map(PathBuf::from).find(|p| p.exists())
}

/// Installed casks from `brew info --cask --json=v2 --installed`: (token, latest version, app names).
fn parse_casks(json: &str) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for chunk in json.split("\"token\"").skip(1) {
        let chunk = format!("\"token\"{chunk}");
        let (Some(token), Some(version)) = (json_string(&chunk, "token"), json_string(&chunk, "version")) else { continue };
        // "app": ["Name.app", …] inside the artifacts.
        let mut apps = Vec::new();
        for part in chunk.split("\"app\"").skip(1) {
            let Some(list) = part.trim_start().strip_prefix(':').and_then(|r| r.trim_start().strip_prefix('[')) else { continue };
            let list = &list[..list.find(']').unwrap_or(list.len())];
            apps.extend(list.split('"').filter(|s| s.ends_with(".app")).map(|s| s.rsplit('/').next().unwrap_or(s).to_string()));
        }
        // Cask versions may carry a build after a comma: "4.2.1,1234".
        out.push((token, version.split(',').next().unwrap_or("").to_string(), apps));
    }
    out
}

fn casks() -> Vec<(String, String, Vec<String>)> {
    let Some(brew) = brew() else { return Vec::new() };
    let out = std::process::Command::new(brew)
        .args(["info", "--cask", "--json=v2", "--installed"])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .stdin(std::process::Stdio::null())
        .output();
    out.ok().filter(|o| o.status.success()).map(|o| parse_casks(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
}

fn from_store(app: &Path) -> bool {
    app.join("Contents/_MASReceipt").exists()
}

/// Ask every source. Slow (network): call it from a background thread.
pub fn check(apps: &[AppInfo]) -> Report {
    let casks = casks();
    let region = region();
    let found: Vec<Option<(&AppInfo, String, Source)>> = apps
        .par_iter()
        .filter(|a| !a.protected && !a.version.is_empty())
        .map(|a| {
            let file = a.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let got = if from_store(&a.path) {
                app_store(a, &region)
            } else if let Some((token, version, _)) = casks.iter().find(|(_, _, names)| names.contains(&file)) {
                Some((version.clone(), Source::Homebrew { token: token.clone() }))
            } else {
                sparkle(a)
            };
            got.map(|(latest, source)| (a, latest, source))
        })
        .collect();
    let total = found.len();
    let known: Vec<_> = found.into_iter().flatten().collect();
    let checked = known.len();
    let mut updates: Vec<Update> = known
        .into_iter()
        .filter(|(a, latest, _)| newer(latest, &a.version))
        .map(|(a, latest, source)| Update { app: a.path.clone(), name: a.name.clone(), installed: a.version.clone(), latest, source })
        .collect();
    updates.sort_by_key(|u| u.name.to_lowercase());
    Report { updates, checked, total }
}

/// Start the update the way its source does it. Homebrew runs in the background and may take minutes.
pub fn start(u: &Update) -> Result<(), String> {
    let open = |arg: &std::ffi::OsStr| {
        std::process::Command::new("open")
            .arg(arg)
            .status()
            .map_err(|e| e.to_string())
            .and_then(|s| if s.success() { Ok(()) } else { Err("open failed".into()) })
    };
    match &u.source {
        Source::AppStore { id } => open(format!("macappstore://apps.apple.com/app/id{id}").as_ref()),
        Source::Sparkle => open(u.app.as_os_str()),
        Source::Homebrew { token } => {
            let brew = brew().ok_or("Homebrew is not installed")?;
            let out = std::process::Command::new(brew)
                .args(["upgrade", "--cask", token])
                .stdin(std::process::Stdio::null())
                .output()
                .map_err(|e| e.to_string())?;
            if out.status.success() {
                Ok(())
            } else {
                let err = String::from_utf8_lossy(&out.stderr);
                Err(format!("{} — brew upgrade --cask {token}", err.lines().last().unwrap_or("").trim()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("12.10", "12.9"));
        assert!(newer("2.0", "1.9.9"));
        assert!(newer("1.2.1", "1.2"));
        assert!(!newer("1.2", "1.2.0"));
        assert!(!newer("1.2", "1.3"));
        assert!(newer("v3.1 (beta)", "3.0.4"));
        // Not comparable: never reported as an update.
        assert!(!newer("latest", "1.0"));
        assert!(!newer("2.0", ""));
    }

    #[test]
    fn reads_the_store_answer() {
        let json = r#"{"resultCount":1, "results": [{"description":"say \"version\": hi", "trackId":747648890, "minimumOsVersion":"10.13", "version":"12.10"}]}"#;
        assert_eq!(parse_lookup(json), Some(("12.10".into(), 747648890)));
        assert_eq!(parse_lookup(r#"{"resultCount":0, "results": []}"#), None);
    }

    #[test]
    fn reads_an_appcast() {
        let xml = r#"<rss><channel>
            <item><title>1.4</title><sparkle:shortVersionString>1.4</sparkle:shortVersionString></item>
            <item><enclosure url="https://x/app.zip" sparkle:version="150" sparkle:shortVersionString="1.5.0" /></item>
            <item><sparkle:channel>beta</sparkle:channel><sparkle:shortVersionString>2.0</sparkle:shortVersionString></item>
        </channel></rss>"#;
        assert_eq!(parse_appcast(xml), Some("1.5.0".into()));
        assert_eq!(parse_appcast("<rss></rss>"), None);
    }

    #[test]
    fn reads_casks() {
        let json = r#"{"formulae":[],"casks":[
            {"token":"firefox","full_token":"firefox","name":["Mozilla Firefox"],"version":"131.0","installed":"130.0","artifacts":[{"app":["Firefox.app"]},{"zap":[{"trash":["~/Library/x"]}]}]},
            {"token":"some-tool","version":"4.2.1,1234","artifacts":[{"app":["Sub/Some Tool.app"]}]}]}"#;
        let c = parse_casks(json);
        assert_eq!(c[0], ("firefox".into(), "131.0".into(), vec!["Firefox.app".to_string()]));
        assert_eq!(c[1], ("some-tool".into(), "4.2.1".into(), vec!["Some Tool.app".to_string()]));
    }

    /// Asks Apple and the update feeds of the installed apps: `cargo test live_app_updates -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_app_updates() {
        let r = check(&crate::apps::list());
        println!("checked {} of {}", r.checked, r.total);
        for u in r.updates {
            println!("{} {} → {} {:?}", u.name, u.installed, u.latest, u.source);
        }
    }
}
