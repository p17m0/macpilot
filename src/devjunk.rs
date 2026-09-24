//! Developer junk: build output and dependency folders that can always be recreated
//! (node_modules, target, build, DerivedData, Pods, virtualenvs…). What matters is not when the
//! build folder was touched but when the *project* was last changed — an old project's build is junk.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::disk::{DirStat, Scan};

#[derive(Clone, Debug)]
pub struct Junk {
    pub path: PathBuf,
    /// The project folder the artifact belongs to.
    pub project: PathBuf,
    pub kind: &'static str,
    pub size: u64,
    pub files: u64,
    /// Latest change in the project's own files (build folders excluded).
    pub project_modified: i64,
}

fn has(dir: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| dir.join(n).exists())
}

/// If `p` is a recreatable build/dependency folder, what kind it is.
pub fn artifact_kind(p: &Path) -> Option<&'static str> {
    let name = p.file_name()?.to_str()?;
    let parent = p.parent()?;
    let parent_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let js = || has(parent, &["package.json"]);
    Some(match name {
        "node_modules" if js() => "node_modules (npm)",
        "target" if has(parent, &["Cargo.toml"]) => "target (Rust)",
        "target" if has(parent, &["pom.xml"]) => "target (Maven)",
        ".gradle" if has(parent, &["settings.gradle", "settings.gradle.kts", "build.gradle", "build.gradle.kts"]) => ".gradle (Gradle)",
        "build" if has(parent, &["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"]) => "build (Gradle)",
        "build" if has(parent, &["pubspec.yaml"]) => "build (Flutter)",
        "build" if has(parent, &["CMakeLists.txt"]) => "build (CMake)",
        "build" | "dist" if js() => "build output (JS)",
        ".dart_tool" if has(parent, &["pubspec.yaml"]) => ".dart_tool (Dart)",
        "Pods" if has(parent, &["Podfile"]) => "Pods (CocoaPods)",
        ".build" if has(parent, &["Package.swift"]) => ".build (SwiftPM)",
        "DerivedData" => "DerivedData (Xcode)",
        _ if parent_name == "DerivedData" => "DerivedData (Xcode)",
        ".venv" | "venv" | "env" if p.join("pyvenv.cfg").exists() => "virtualenv (Python)",
        ".next" | ".nuxt" | ".svelte-kit" | ".turbo" | ".parcel-cache" | ".angular" | ".expo" if js() => "framework cache (JS)",
        "bundle" if parent_name == "vendor" && parent.parent().is_some_and(|g| has(g, &["Gemfile"])) => "vendor/bundle (Ruby)",
        "_build" | "deps" if has(parent, &["mix.exs"]) => "_build (Elixir)",
        ".stack-work" => ".stack-work (Haskell)",
        "zig-cache" | ".zig-cache" | "zig-out" if has(parent, &["build.zig"]) => "zig-cache (Zig)",
        _ if name.starts_with("cmake-build-") => "cmake-build (CLion)",
        _ => return None,
    })
}

fn project_of(path: &Path, kind: &str) -> PathBuf {
    let parent = path.parent().unwrap_or(path);
    if kind.starts_with("DerivedData") {
        return path.to_path_buf();
    }
    if kind.starts_with("vendor/bundle") {
        return parent.parent().unwrap_or(parent).to_path_buf();
    }
    parent.to_path_buf()
}

/// Latest change among the project's own entries, ignoring build folders and `.git`.
fn project_modified(project: &Path, lookup: &dyn Fn(&Path) -> Option<DirStat>) -> i64 {
    let mut latest = std::fs::symlink_metadata(project).map(|m| m.mtime()).unwrap_or(0);
    let Ok(rd) = std::fs::read_dir(project) else { return latest };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(md) = e.metadata() else { continue };
        if md.is_dir() {
            let n = e.file_name();
            if n == ".git" || artifact_kind(&p).is_some() {
                continue;
            }
            latest = latest.max(lookup(&p).map(|s| s.modified).unwrap_or_else(|| md.mtime()));
        } else {
            latest = latest.max(md.mtime());
        }
    }
    latest
}

/// Only the user's own projects: not inside apps, ~/Library (except Xcode DerivedData) or hidden
/// tool folders like ~/.vscode/extensions — there build folders belong to installed software.
pub fn in_user_project(p: &Path) -> bool {
    let home = crate::home();
    let Ok(rel) = p.strip_prefix(&home) else { return false };
    if rel.starts_with("Library/Developer/Xcode/DerivedData") {
        return true;
    }
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    let Some((_, parents)) = parts.split_last() else { return false };
    if parents.first().is_some_and(|f| f == "Library") {
        return false;
    }
    !parents.iter().any(|c| c.starts_with('.') || c.ends_with(".app") || c == "node_modules" || c == "site-packages")
}

/// All build/dependency folders found by the scan, largest first.
pub fn find(scan: &Scan) -> Vec<Junk> {
    let dirs = scan.shared.dirs.lock().unwrap();
    let mut cands: Vec<(PathBuf, &'static str, DirStat)> = dirs
        .iter()
        .filter(|(p, st)| st.size >= 1_000_000 && in_user_project(p))
        .filter_map(|(p, st)| artifact_kind(p).map(|k| (p.clone(), k, *st)))
        .collect();
    drop(dirs);
    // Top-most only: node_modules inside node_modules, build inside target, etc. are covered by the outer one.
    cands.sort_by_key(|(p, ..)| p.components().count());
    let mut out: Vec<Junk> = Vec::new();
    for (path, kind, st) in cands {
        if out.iter().any(|j| path.starts_with(&j.path)) {
            continue;
        }
        let project = project_of(&path, kind);
        let project_modified = if kind.starts_with("DerivedData") { st.modified } else { project_modified(&project, &|q| scan.dir(q)) };
        out.push(Junk { path, project, kind, size: st.size, files: st.files, project_modified });
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.size));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_artifacts_by_marker_files() {
        let tmp = std::env::temp_dir().join(format!("macpilot-junk-{}", std::process::id()));
        let proj = tmp.join("app");
        std::fs::create_dir_all(proj.join("node_modules")).unwrap();
        std::fs::create_dir_all(proj.join("target")).unwrap();
        std::fs::create_dir_all(proj.join("build")).unwrap();
        std::fs::write(proj.join("package.json"), "{}").unwrap();
        assert_eq!(artifact_kind(&proj.join("node_modules")), Some("node_modules (npm)"));
        // No Cargo.toml → "target" is just a folder.
        assert_eq!(artifact_kind(&proj.join("target")), None);
        std::fs::write(proj.join("Cargo.toml"), "").unwrap();
        assert_eq!(artifact_kind(&proj.join("target")), Some("target (Rust)"));
        assert_eq!(artifact_kind(&proj.join("build")), Some("build output (JS)"));
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn only_user_projects() {
        let h = crate::home();
        assert!(in_user_project(&h.join("Dev/app/node_modules")));
        assert!(in_user_project(&h.join("Dev/app/.venv")));
        assert!(in_user_project(&h.join("Library/Developer/Xcode/DerivedData/App-abc")));
        assert!(!in_user_project(&h.join(".vscode/extensions/x/node_modules")));
        assert!(!in_user_project(&h.join("Library/Caches/x/Foo.app/Contents/node_modules")));
        assert!(!in_user_project(&h.join("Library/Containers/x/venv")));
        assert!(!in_user_project(&h.join("Dev/app/.local/tool/.venv")));
    }
}
