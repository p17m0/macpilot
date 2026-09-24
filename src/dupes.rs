//! Duplicate file finder: group by size, then by a hash of the first and last 64 KB,
//! then by a full-content hash. Hard links are not duplicates and are skipped.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::hash::Hasher;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;

#[derive(Clone, Debug)]
pub struct DupFile {
    pub path: PathBuf,
    pub modified: i64,
}

#[derive(Clone, Debug)]
pub struct DupGroup {
    /// Size of one copy.
    pub size: u64,
    pub files: Vec<DupFile>,
    /// Some copies live inside a git project — removing them could break it.
    pub in_project: bool,
}

/// Is the path inside a git working tree?
pub fn in_git_repo(p: &Path) -> bool {
    let home = crate::home();
    p.ancestors().skip(1).take_while(|a| *a != home && a.parent().is_some()).any(|a| a.join(".git").exists())
}

impl DupGroup {
    /// Space freed by keeping one copy.
    pub fn wasted(&self) -> u64 {
        self.size * (self.files.len() as u64).saturating_sub(1)
    }
}

pub struct DupScan {
    pub root: PathBuf,
    pub files_seen: AtomicU64,
    pub bytes_hashed: AtomicU64,
    pub stage: AtomicU64, // 0 = listing, 1 = comparing, 2 = done
    pub cancel: AtomicBool,
    pub groups: Mutex<Vec<DupGroup>>,
}

impl DupScan {
    pub fn start(root: PathBuf, min_size: u64) -> Arc<DupScan> {
        let s = Arc::new(DupScan {
            root: root.clone(),
            files_seen: AtomicU64::new(0),
            bytes_hashed: AtomicU64::new(0),
            stage: AtomicU64::new(0),
            cancel: AtomicBool::new(false),
            groups: Mutex::new(Vec::new()),
        });
        let sc = s.clone();
        std::thread::spawn(move || {
            crate::background_qos();
            let pool = rayon::ThreadPoolBuilder::new().start_handler(|_| crate::background_qos()).build().expect("thread pool");
            let groups = pool.install(|| find(&sc, &root, min_size));
            *sc.groups.lock().unwrap() = groups;
            sc.stage.store(2, Ordering::SeqCst);
        });
        s
    }

    pub fn done(&self) -> bool {
        self.stage.load(Ordering::Relaxed) == 2
    }

    pub fn remove_paths(&self, removed: &[PathBuf]) {
        let set: HashSet<&PathBuf> = removed.iter().collect();
        let mut g = self.groups.lock().unwrap();
        for grp in g.iter_mut() {
            grp.files.retain(|f| !set.contains(&f.path));
        }
        g.retain(|grp| grp.files.len() > 1);
    }
}

/// Folders not worth comparing: app data, package internals, dependency trees, VCS.
fn skip(p: &Path, home: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.starts_with('.') || crate::disk::skip_dir(p) {
        return true;
    }
    if matches!(name, "node_modules" | "Pods" | "target" | "DerivedData" | "vendor") {
        return true;
    }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    if matches!(ext, "app" | "photoslibrary" | "fcpbundle" | "bundle" | "framework" | "xcodeproj" | "xcworkspace" | "musiclibrary" | "tvlibrary") {
        return true;
    }
    p == home.join("Library")
}

/// (size, path, modified, (device, inode)) of every candidate file.
type Listing = Mutex<Vec<(u64, PathBuf, i64, (u64, u64))>>;

fn list(s: &DupScan, dir: &Path, min: u64, home: &Path, out: &Listing) {
    if s.cancel.load(Ordering::Relaxed) {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut subdirs = Vec::new();
    let mut local = Vec::new();
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else { continue };
        let p = e.path();
        if md.is_dir() {
            if !skip(&p, home) {
                subdirs.push(p);
            }
        } else if md.is_file() && md.len() >= min {
            local.push((md.len(), p, md.mtime(), (md.dev(), md.ino())));
        }
    }
    s.files_seen.fetch_add(local.len() as u64, Ordering::Relaxed);
    out.lock().unwrap().extend(local);
    subdirs.par_iter().for_each(|d| list(s, d, min, home, out));
}

fn hash_part(p: &Path, size: u64) -> Option<u64> {
    const PART: u64 = 64 * 1024;
    let mut f = File::open(p).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut buf = vec![0u8; PART as usize];
    let n = f.read(&mut buf).ok()?;
    h.write(&buf[..n]);
    if size > PART * 2 {
        f.seek(SeekFrom::End(-(PART as i64))).ok()?;
        let n = f.read(&mut buf).ok()?;
        h.write(&buf[..n]);
    }
    Some(h.finish())
}

fn hash_full(s: &DupScan, p: &Path) -> Option<u64> {
    let mut f = File::open(p).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if s.cancel.load(Ordering::Relaxed) {
            return None;
        }
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.write(&buf[..n]);
        s.bytes_hashed.fetch_add(n as u64, Ordering::Relaxed);
    }
    Some(h.finish())
}

/// Split `files` into groups that share the same key.
fn regroup<K: std::hash::Hash + Eq + Send>(files: Vec<(PathBuf, i64)>, key: impl Fn(&Path) -> Option<K> + Sync) -> Vec<Vec<(PathBuf, i64)>> {
    let keyed: Vec<(K, (PathBuf, i64))> = files.into_par_iter().filter_map(|f| key(&f.0).map(|k| (k, f))).collect();
    let mut m: HashMap<K, Vec<(PathBuf, i64)>> = HashMap::new();
    for (k, f) in keyed {
        m.entry(k).or_default().push(f);
    }
    m.into_values().filter(|v| v.len() > 1).collect()
}

fn find(s: &DupScan, root: &Path, min: u64) -> Vec<DupGroup> {
    let home = crate::home();
    let all = Mutex::new(Vec::new());
    list(s, root, min.max(1), &home, &all);
    let all = all.into_inner().unwrap();
    s.stage.store(1, Ordering::SeqCst);

    // Group by size, dropping extra hard links to the same inode.
    let mut by_size: HashMap<u64, Vec<(PathBuf, i64)>> = HashMap::new();
    let mut seen_inodes = HashSet::new();
    for (size, p, mt, ino) in all {
        if seen_inodes.insert(ino) {
            by_size.entry(size).or_default().push((p, mt));
        }
    }
    let size_groups: Vec<(u64, Vec<(PathBuf, i64)>)> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();

    let mut out: Vec<DupGroup> = size_groups
        .into_par_iter()
        .flat_map(|(size, files)| {
            let mut result = Vec::new();
            for g in regroup(files, |p| hash_part(p, size)) {
                let g = if size > 128 * 1024 { regroup(g, |p| hash_full(s, p)) } else { vec![g] };
                for files in g {
                    let mut files: Vec<DupFile> = files.into_iter().map(|(path, modified)| DupFile { path, modified }).collect();
                    // Oldest first: it is usually the original and the default copy to keep.
                    files.sort_by(|a, b| a.modified.cmp(&b.modified).then(a.path.as_os_str().len().cmp(&b.path.as_os_str().len())));
                    let in_project = files.iter().any(|f| in_git_repo(&f.path));
                    result.push(DupGroup { size, files, in_project });
                }
            }
            result
        })
        .collect();
    out.sort_by_key(|g| std::cmp::Reverse(g.wasted()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_identical_files_only() {
        let tmp = std::env::temp_dir().join(format!("macpilot-dupes-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("sub")).unwrap();
        let data = vec![7u8; 300_000];
        std::fs::write(tmp.join("a.bin"), &data).unwrap();
        std::fs::write(tmp.join("sub/b.bin"), &data).unwrap();
        let mut other = data.clone();
        other[150_000] = 8; // same size, same head and tail, different middle
        std::fs::write(tmp.join("c.bin"), &other).unwrap();
        std::fs::hard_link(tmp.join("a.bin"), tmp.join("hard.bin")).unwrap();
        let s = DupScan::start(tmp.clone(), 1);
        while !s.done() {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let groups = s.groups.lock().unwrap().clone();
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(groups[0].files.len(), 2);
        assert_eq!(groups[0].wasted(), 300_000);
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
