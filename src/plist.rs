//! Minimal property-list reading via `plutil` (handles binary and XML plists) — no extra dependencies.

use std::path::Path;

/// Read a plist as XML text.
pub fn read_xml(path: &Path) -> Option<String> {
    let out = std::process::Command::new("plutil").args(["-convert", "xml1", "-o", "-"]).arg(path).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Top-level `<key>K</key><string>V</string>`.
pub fn string(xml: &str, key: &str) -> Option<String> {
    let pat = format!("<key>{key}</key>");
    let rest = &xml[xml.find(&pat)? + pat.len()..];
    let rest = rest.trim_start();
    let body = rest.strip_prefix("<string>")?;
    Some(unescape(&body[..body.find("</string>")?]))
}

/// Top-level `<key>K</key><true/>`.
pub fn bool(xml: &str, key: &str) -> Option<bool> {
    let pat = format!("<key>{key}</key>");
    let rest = xml[xml.find(&pat)? + pat.len()..].trim_start();
    if rest.starts_with("<true/>") {
        Some(true)
    } else if rest.starts_with("<false/>") {
        Some(false)
    } else if rest.starts_with("<dict>") {
        Some(true) // e.g. KeepAlive with conditions
    } else {
        None
    }
}

/// Strings of a top-level `<key>K</key><array>…</array>`.
pub fn array(xml: &str, key: &str) -> Vec<String> {
    let pat = format!("<key>{key}</key>");
    let Some(i) = xml.find(&pat) else { return Vec::new() };
    let rest = xml[i + pat.len()..].trim_start();
    let Some(body) = rest.strip_prefix("<array>") else { return Vec::new() };
    let body = &body[..body.find("</array>").unwrap_or(body.len())];
    body.split("<string>").skip(1).filter_map(|s| s.split("</string>").next()).map(unescape).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_keys() {
        let xml = "<dict><key>Label</key><string>com.a&amp;b</string><key>RunAtLoad</key><true/>\
                   <key>ProgramArguments</key><array><string>/bin/x</string><string>-y</string></array></dict>";
        assert_eq!(super::string(xml, "Label").as_deref(), Some("com.a&b"));
        assert_eq!(super::bool(xml, "RunAtLoad"), Some(true));
        assert_eq!(super::array(xml, "ProgramArguments"), vec!["/bin/x", "-y"]);
    }
}
