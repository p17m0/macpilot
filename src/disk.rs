//! Disk usage analysis and deletion safety rules. Nothing is ever deleted permanently:
//! removal always goes through the Trash (see `trash.rs`).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use rayon::prelude::*;

use crate::{home, tr, trf};

/// Files at least this big appear in the "Large files" list.
const BIG_FILE: u64 = 50 * 1000 * 1000;
/// Files at least this big are remembered for the "Not used lately" analysis.
const TRACK_FILE: u64 = 20 * 1000 * 1000;
const TRACK_KEEP: usize = 4000;

#[derive(Clone, Copy, Default, Debug)]
pub struct DirStat {
    pub size: u64,
    pub files: u64,
    /// Latest modification inside (unix seconds).
    pub modified: i64,
    /// Latest use inside: max(last read, last modification).
    pub used: i64,
    /// Bytes taken by photos, video and music.
    pub media: u64,
}

impl DirStat {
    fn merge(self, b: DirStat) -> DirStat {
        DirStat {
            size: self.size + b.size,
            files: self.files + b.files,
            modified: self.modified.max(b.modified),
            used: self.used.max(b.used),
            media: self.media + b.media,
        }
    }
}

/// A remembered large file.
#[derive(Clone, Debug)]
pub struct FileRec {
    pub size: u64,
    pub path: PathBuf,
    pub modified: i64,
    pub used: i64,
    pub media: bool,
}

/// Scan results, filled in the background.
pub struct ScanShared {
    pub dirs: Mutex<HashMap<PathBuf, DirStat>>,
    pub big: Mutex<Vec<FileRec>>,
    inodes: Mutex<HashSet<(u64, u64)>>,
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    pub done: AtomicBool,
    pub cancel: AtomicBool,
}

impl ScanShared {
    fn new() -> ScanShared {
        ScanShared {
            dirs: Mutex::new(HashMap::new()),
            big: Mutex::new(Vec::new()),
            inodes: Mutex::new(HashSet::new()),
            files: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            done: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
        }
    }
}

pub struct Scan {
    pub root: PathBuf,
    pub shared: Arc<ScanShared>,
    pub started: std::time::Instant,
    pub finished_in: Option<std::time::Duration>,
}

fn make_pool(name: &'static str, background: bool) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .thread_name(move |i| format!("macpilot-{name}-{i}"))
        .start_handler(move |_| {
            if background {
                crate::background_qos();
            }
        })
        .build()
        .expect("thread pool")
}

/// Low-priority thread pools. Each kind of work gets its own pool: while macOS shows a permission
/// dialog, file access blocks, and one stalled job must not freeze the others.
pub fn pool() -> &'static rayon::ThreadPool {
    static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
    POOL.get_or_init(|| make_pool("measure", true))
}

pub fn apps_pool() -> &'static rayon::ThreadPool {
    static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
    POOL.get_or_init(|| make_pool("apps", true))
}

impl Scan {
    pub fn start(root: PathBuf) -> Scan {
        let shared = Arc::new(ScanShared::new());
        let s = shared.clone();
        let r = root.clone();
        std::thread::spawn(move || {
            let mt = fs::symlink_metadata(&r).map(|m| m.mtime()).unwrap_or(0);
            // A pool per scan, so a scan stuck on a permission dialog never blocks a new one.
            // Normal priority: the user is usually waiting for the result.
            make_pool("scan", false).install(|| walk(&s, &r, mt));
            s.done.store(true, Ordering::SeqCst);
        });
        Scan { root, shared, started: std::time::Instant::now(), finished_in: None }
    }

    pub fn done(&self) -> bool {
        self.shared.done.load(Ordering::Relaxed)
    }

    pub fn dir(&self, p: &Path) -> Option<DirStat> {
        self.shared.dirs.lock().unwrap().get(p).copied()
    }

    pub fn covers(&self, p: &Path) -> bool {
        p.starts_with(&self.root)
    }

    /// After moving something to the Trash, subtract it from all parent folders.
    pub fn forget(&self, p: &Path, size: u64, files: u64) {
        let mut dirs = self.shared.dirs.lock().unwrap();
        dirs.retain(|k, _| !k.starts_with(p));
        let mut cur = p.parent();
        while let Some(d) = cur {
            if let Some(st) = dirs.get_mut(d) {
                st.size = st.size.saturating_sub(size);
                st.files = st.files.saturating_sub(files);
            }
            if d == self.root {
                break;
            }
            cur = d.parent();
        }
        drop(dirs);
        self.shared.big.lock().unwrap().retain(|f| !f.path.starts_with(p));
    }

    /// Size of a path from the scan (folder or tracked file), if known.
    pub fn size_of(&self, p: &Path) -> Option<DirStat> {
        self.dir(p).or_else(|| {
            self.shared.big.lock().unwrap().iter().find(|f| f.path == p).map(|f| DirStat {
                size: f.size,
                files: 1,
                modified: f.modified,
                used: f.used,
                media: if f.media { f.size } else { 0 },
            })
        })
    }

    pub fn big_files(&self) -> Vec<(u64, PathBuf)> {
        let mut v: Vec<(u64, PathBuf)> =
            self.shared.big.lock().unwrap().iter().filter(|f| f.size >= BIG_FILE).map(|f| (f.size, f.path.clone())).collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.0));
        v
    }
}

impl Drop for Scan {
    fn drop(&mut self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
    }
}

/// Paths we never walk: other volumes, virtual file systems and APFS firmlink duplicates.
pub fn skip_dir(p: &Path) -> bool {
    matches!(p.to_str(), Some("/System/Volumes" | "/Volumes" | "/dev" | "/net" | "/home" | "/cores"))
}

/// Space actually used on disk (accounts for APFS compression and sparse files).
pub fn alloc_size(md: &fs::Metadata) -> u64 {
    md.blocks() * 512
}

/// Last use of a file: max(last read, last modification).
pub fn used_time(md: &fs::Metadata) -> i64 {
    md.atime().max(md.mtime())
}

pub fn now_unix() -> i64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Photos, video and music.
pub fn is_media(p: &Path) -> bool {
    let Some(ext) = p.extension().map(|e| e.to_string_lossy().to_lowercase()) else { return false };
    matches!(
        ext.as_str(),
        "jpg"
            | "jpeg"
            | "png"
            | "heic"
            | "heif"
            | "gif"
            | "tif"
            | "tiff"
            | "webp"
            | "bmp"
            | "raw"
            | "dng"
            | "cr2"
            | "cr3"
            | "nef"
            | "arw"
            | "orf"
            | "rw2"
            | "raf"
            | "psd"
            | "mov"
            | "mp4"
            | "m4v"
            | "avi"
            | "mkv"
            | "mts"
            | "m2ts"
            | "3gp"
            | "webm"
            | "mp3"
            | "m4a"
            | "flac"
            | "wav"
            | "aac"
            | "aiff"
            | "aif"
            | "ogg"
            | "photoslibrary"
            | "aplibrary"
            | "fcpbundle"
            | "imovielibrary"
    )
}

/// Folders of other apps' data (sandbox containers). Since macOS 14 opening some of them waits
/// for a privacy decision that may never come, which would hang the calling thread forever.
fn needs_guard(p: &Path) -> bool {
    let home = home();
    ["Library/Containers", "Library/Group Containers"]
        .iter()
        .any(|base| p.strip_prefix(home.join(base)).is_ok_and(|rest| (1..=2).contains(&rest.components().count())))
}

/// Directory entries with metadata (symlinks not followed). Reading guarded folders happens on a
/// helper thread with a timeout; if macOS blocks, the folder is treated as not accessible.
pub fn read_entries(p: &Path) -> std::io::Result<Vec<(PathBuf, std::ffi::OsString, fs::Metadata)>> {
    fn read(p: &Path) -> std::io::Result<Vec<(PathBuf, std::ffi::OsString, fs::Metadata)>> {
        Ok(fs::read_dir(p)?.flatten().filter_map(|e| e.metadata().ok().map(|m| (e.path(), e.file_name(), m))).collect())
    }
    if !needs_guard(p) {
        return read(p);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let q = p.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(read(&q));
    });
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .unwrap_or_else(|_| Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "blocked by macOS privacy protection")))
}

fn walk(s: &ScanShared, path: &Path, dir_mtime: i64) -> DirStat {
    if s.cancel.load(Ordering::Relaxed) {
        return DirStat::default();
    }
    let rd = match read_entries(path) {
        Ok(rd) => rd,
        Err(_) => {
            s.errors.fetch_add(1, Ordering::Relaxed);
            return DirStat::default();
        }
    };
    // Adding or removing an entry changes the folder itself — that counts as activity too.
    let mut st = DirStat { modified: dir_mtime, used: dir_mtime, ..Default::default() };
    let mut subdirs = Vec::new();
    // Metadata of directory entries does not follow symlinks, so nothing is counted twice.
    for (path, _, md) in rd {
        if md.is_dir() {
            if !skip_dir(&path) {
                subdirs.push((path, md.mtime()));
            }
            continue;
        }
        let size = alloc_size(&md);
        if md.nlink() > 1 && !s.inodes.lock().unwrap().insert((md.dev(), md.ino())) {
            continue; // another hard link to a file we already counted
        }
        let used = used_time(&md);
        st.size += size;
        st.files += 1;
        st.modified = st.modified.max(md.mtime());
        st.used = st.used.max(used);
        let media = is_media(&path);
        if media {
            st.media += size;
        }
        if size >= TRACK_FILE {
            let mut big = s.big.lock().unwrap();
            big.push(FileRec { size, path, modified: md.mtime(), used, media });
            if big.len() > TRACK_KEEP * 2 {
                big.sort_by_key(|a| std::cmp::Reverse(a.size));
                big.truncate(TRACK_KEEP);
            }
        }
    }
    s.files.fetch_add(st.files, Ordering::Relaxed);
    s.bytes.fetch_add(st.size, Ordering::Relaxed);
    let sub = subdirs.par_iter().map(|(d, mt)| walk(s, d, *mt)).reduce(DirStat::default, DirStat::merge);
    st = st.merge(sub);
    if !s.cancel.load(Ordering::Relaxed) {
        s.dirs.lock().unwrap().insert(path.to_path_buf(), st);
    }
    st
}

/// Measure one path synchronously. Inside a thread pool it runs on that pool;
/// otherwise on the shared low-priority "measure" pool.
pub fn measure(path: &Path) -> DirStat {
    let s = ScanShared::new();
    match fs::symlink_metadata(path) {
        Ok(md) if md.is_dir() && rayon::current_thread_index().is_some() => walk(&s, path, md.mtime()),
        Ok(md) if md.is_dir() => pool().install(|| walk(&s, path, md.mtime())),
        Ok(md) => DirStat {
            size: alloc_size(&md),
            files: 1,
            modified: md.mtime(),
            used: used_time(&md),
            media: if is_media(path) { alloc_size(&md) } else { 0 },
        },
        Err(_) => DirStat::default(),
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: Option<u64>,
    pub files: Option<u64>,
    /// Last modification (for folders: latest inside), unix seconds.
    pub mtime: Option<i64>,
    /// Last use (read or modification), unix seconds.
    pub used: Option<i64>,
}

/// List a folder; folder sizes come from the scan results.
pub fn list_dir(dir: &Path, scan: Option<&Scan>) -> Result<Vec<Entry>, String> {
    let rd = read_entries(dir).map_err(|e| perm_hint(&e))?;
    let dirs = scan.map(|s| s.shared.dirs.lock().unwrap());
    let mut out = Vec::new();
    for (path, name, md) in rd {
        let is_dir = md.is_dir();
        let (size, files, mtime, used) = if is_dir {
            match dirs.as_ref().and_then(|d| d.get(&path)) {
                Some(st) => (Some(st.size), Some(st.files), Some(st.modified), Some(st.used)),
                None if skip_dir(&path) => (Some(0), None, None, None),
                None => (None, None, None, None),
            }
        } else {
            (Some(alloc_size(&md)), None, Some(md.mtime()), Some(used_time(&md)))
        };
        out.push(Entry { name: name.to_string_lossy().to_string(), path, is_dir, is_link: md.file_type().is_symlink(), size, files, mtime, used });
    }
    out.sort_by(|a, b| b.size.unwrap_or(0).cmp(&a.size.unwrap_or(0)).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

pub fn perm_hint(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        tr("No access. Give MacPilot Full Disk Access: System Settings → Privacy & Security → Full Disk Access.").into()
    } else {
        e.to_string()
    }
}

// ---------------------------------------------------------------------------
// Not used lately
// ---------------------------------------------------------------------------

/// Folders with nothing used for a while are listed from this size.
pub const STALE_DIR_MIN: u64 = 100 * 1000 * 1000;
/// Single files — from this size.
pub const STALE_FILE_MIN: u64 = TRACK_FILE;

#[derive(Clone, Debug)]
pub struct StaleItem {
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub files: u64,
    pub modified: i64,
    pub used: i64,
    pub safety: DelSafety,
    pub why: String,
}

/// Never offered as "not used lately": media libraries, app data, cloud folders, system and protected places.
fn stale_excluded(p: &Path) -> bool {
    let home = home();
    let Ok(rel) = p.strip_prefix(&home) else { return true };
    const EXCL: &[&str] = &[
        "Pictures",
        "Movies",
        "Music",
        "Library/Application Support",
        "Library/Containers",
        "Library/Group Containers",
        "Library/Mobile Documents",
        "Library/CloudStorage",
        "Library/Mail",
        "Library/Messages",
        "Library/Photos",
        "Library/Keychains",
        "Library/Preferences",
        "Library/Accounts",
        "Library/Calendars",
        "Library/Safari",
        "Library/Metadata",
        ".Trash",
        ".ssh",
        ".gnupg",
    ];
    EXCL.iter().any(|x| rel.starts_with(x)) || is_media(p) || in_media_folder(rel)
}

/// Folders whose name says photos/video/music ("Photos", "Camera", "Видео", …).
fn in_media_folder(rel: &Path) -> bool {
    const WORDS: &[&str] = &[
        "photo",
        "фото",
        "фотк",
        "camera",
        "dcim",
        "video",
        "видео",
        "movie",
        "фильм",
        "film",
        "music",
        "музык",
        "musique",
        "musik",
        "música",
        "wedding",
        "свадьб",
        "bilder",
        "fotos",
        "vidéo",
        "vídeo",
    ];
    rel.components().any(|c| {
        let n = c.as_os_str().to_string_lossy().to_lowercase();
        WORDS.iter().any(|w| n.contains(w))
    })
}

/// Containers that hold whole tool versions (safe to remove one version entirely).
const VERSION_CONTAINERS: &[&str] = &[
    "versions",
    "toolchains",
    "dists",
    "system-images",
    "iOS DeviceSupport",
    "watchOS DeviceSupport",
    "tvOS DeviceSupport",
    "sdks",
    "installations",
    "envs",
    "ndk",
    "avd",
];

/// In regular folders any file or folder can be offered. In service areas (~/.something, ~/Library)
/// only whole units are: a cache, a whole tool (~/.flet) or a whole version (~/.rbenv/versions/3.1.2).
/// Removing a single file from inside a tool would break it.
fn stale_eligible(p: &Path, is_dir: bool) -> bool {
    let Ok(rel) = p.strip_prefix(home()) else { return false };
    let first = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_string()).unwrap_or_default();
    let service = first.starts_with('.') || first == "Library";
    if !service {
        return true;
    }
    if !is_dir {
        return false;
    }
    if deletion_safety(p).0 == DelSafety::Safe {
        return true;
    }
    if first.starts_with('.') && rel.components().count() == 1 {
        return true;
    }
    let name_of = |q: Option<&Path>| q.and_then(|x| x.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let parent = name_of(p.parent());
    let grand = name_of(p.parent().and_then(|x| x.parent()));
    VERSION_CONTAINERS.contains(&parent.as_str()) || (VERSION_CONTAINERS.contains(&grand.as_str()) && !parent.starts_with('.'))
}

/// Things not used for a while: the top-most such folders plus separate large files.
pub fn stale_items(scan: &Scan, older_than_secs: i64) -> Vec<StaleItem> {
    let cutoff = now_unix() - older_than_secs;
    let dirs = scan.shared.dirs.lock().unwrap();
    let mut cands: Vec<(PathBuf, DirStat)> = dirs
        .iter()
        .filter(|(p, st)| {
            **p != scan.root
                && st.used > 0
                && st.used < cutoff
                && st.size >= STALE_DIR_MIN
                && st.media * 2 < st.size
                && !stale_excluded(p)
                && stale_eligible(p, true)
        })
        .map(|(p, st)| (p.clone(), *st))
        .collect();
    drop(dirs);
    // Shallow folders first, so nested ones are not listed twice.
    cands.sort_by_key(|(p, _)| p.components().count());
    let mut chosen: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::new();
    for (p, st) in cands {
        if p.ancestors().skip(1).any(|a| chosen.contains(a)) {
            continue;
        }
        let (safety, why) = deletion_safety(&p);
        if safety == DelSafety::Blocked {
            continue;
        }
        chosen.insert(p.clone());
        out.push(StaleItem { path: p, is_dir: true, size: st.size, files: st.files, modified: st.modified, used: st.used, safety, why });
    }
    let files: Vec<FileRec> = scan.shared.big.lock().unwrap().clone();
    for f in files {
        if f.media || f.used >= cutoff || f.size < STALE_FILE_MIN || stale_excluded(&f.path) || !stale_eligible(&f.path, false) {
            continue;
        }
        if f.path.ancestors().skip(1).any(|a| chosen.contains(a)) {
            continue;
        }
        let (safety, why) = deletion_safety(&f.path);
        if safety == DelSafety::Blocked {
            continue;
        }
        out.push(StaleItem { path: f.path, is_dir: false, size: f.size, files: 1, modified: f.modified, used: f.used, safety, why });
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.size));
    out
}

// ---------------------------------------------------------------------------
// Deletion safety
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelSafety {
    /// Caches, build output and the like — recreated automatically.
    Safe,
    /// User data — can be removed, but make sure it is not needed.
    Careful,
    /// System or important — removal is blocked.
    Blocked,
}

impl DelSafety {
    pub fn label(self) -> &'static str {
        match self {
            DelSafety::Safe => tr("safe"),
            DelSafety::Careful => tr("careful"),
            DelSafety::Blocked => tr("protected"),
        }
    }
}

/// Decide whether a path can be removed safely, and explain why.
pub fn deletion_safety(p: &Path) -> (DelSafety, String) {
    use DelSafety::*;
    let home = home();
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

    if p.components().any(|c| c.as_os_str() == ".git") {
        return (Blocked, tr("Git history of a project. Removing it loses all commits.").into());
    }
    // The inside of an .app bundle — removing parts breaks the app.
    if let Some(app) = crate::procs::outer_app(p) {
        if app != p {
            return (Blocked, tr("Part of an app. Remove the whole app instead (see Apps).").into());
        }
    }
    if let Ok(rest) = p.strip_prefix("/Volumes") {
        if rest.components().count() >= 2 {
            return (Careful, tr("On an external or other volume. Make sure you do not need it.").into());
        }
        return (Blocked, tr("The root of a volume cannot be removed.").into());
    }
    if let Ok(rest) = p.strip_prefix("/Applications") {
        let n = rest.components().count();
        if n == 1 && name.ends_with(".app") {
            if name == "Safari.app" {
                return (Blocked, tr("Built-in Apple app.").into());
            }
            return (Careful, tr("An app. Use Apps → Uninstall to remove its leftovers too.").into());
        }
        if n == 0 {
            return (Blocked, tr("The Applications folder is part of the system.").into());
        }
        return (Careful, tr("An item in the Applications folder.").into());
    }
    let Ok(rel) = p.strip_prefix(&home) else {
        return (Blocked, tr("System area outside your home folder. Removing things here can break macOS or installed apps.").into());
    };
    let rel_s = rel.to_string_lossy();
    if rel_s.is_empty() {
        return (Blocked, tr("This is your home folder.").into());
    }

    const PROTECTED_EXACT: &[&str] = &[
        "Library",
        "Library/Application Support",
        "Library/Preferences",
        "Library/Containers",
        "Library/Group Containers",
        "Library/Caches",
        "Library/Logs",
        "Library/Mobile Documents",
        "Library/CloudStorage",
        "Library/Developer",
        "Library/LaunchAgents",
        "Desktop",
        "Documents",
        "Downloads",
        "Pictures",
        "Movies",
        "Music",
        "Public",
        "Applications",
        ".Trash",
        ".config",
        ".local",
        ".cargo",
        ".rustup",
    ];
    if PROTECTED_EXACT.contains(&rel_s.as_ref()) {
        return (Blocked, tr("A standard macOS folder. The folder itself stays; you can remove what is inside.").into());
    }
    const PROTECTED_TREE: &[&str] = &[
        "Library/Keychains",
        "Library/Accounts",
        "Library/Cookies",
        "Library/Preferences",
        "Library/Autosave Information",
        "Library/Photos",
        "Library/Mail",
        "Library/Messages",
        "Library/Calendars",
        "Library/Safari",
        "Library/Metadata",
        "Library/Sharing",
        "Library/IdentityServices",
        ".ssh",
        ".gnupg",
        ".rustup/toolchains",
    ];
    for t in PROTECTED_TREE {
        if rel.starts_with(t) {
            return (Blocked, trf("Important data (~/{0}): keys, passwords, mail, messages or settings. Removal is blocked.", &[t]));
        }
    }
    if name.ends_with(".photoslibrary") {
        return (Blocked, tr("Photos library. Removing it removes all photos — use the Photos app.").into());
    }
    if rel.starts_with("Library/Mobile Documents") || rel.starts_with("Library/CloudStorage") {
        return (Careful, tr("Cloud folder (iCloud, Dropbox…): removal syncs and deletes it on ALL your devices.").into());
    }

    let safe_tree: [(&str, &str); 16] = [
        ("Library/Caches", tr("App cache — recreated automatically.")),
        ("Library/Logs", tr("Logs — safe to remove.")),
        ("Library/Developer/Xcode/DerivedData", tr("Xcode build files — recreated on the next build.")),
        ("Library/Developer/Xcode/iOS DeviceSupport", tr("iPhone debug symbols — downloaded again when a device connects.")),
        ("Library/Developer/Xcode/watchOS DeviceSupport", tr("Apple Watch debug symbols — downloaded again when needed.")),
        ("Library/Developer/CoreSimulator/Caches", tr("Simulator cache — recreated automatically.")),
        (".Trash", tr("Already in the Trash.")),
        (".npm/_cacache", tr("npm cache — downloaded again when needed.")),
        (".npm/_npx", tr("npx cache.")),
        (".cache", tr("Command-line tools cache.")),
        (".gradle/caches", tr("Gradle cache — downloaded again when needed.")),
        (".cargo/registry", tr("Cargo crate cache — downloaded again when needed.")),
        (".cargo/git", tr("Cargo git dependency cache.")),
        (".bun/install/cache", tr("Bun cache.")),
        ("Library/pnpm/store", tr("pnpm store — downloaded again when needed.")),
        (".pnpm-store", tr("pnpm store — downloaded again when needed.")),
    ];
    for (t, why) in safe_tree {
        if rel.starts_with(t) {
            return (Safe, why.into());
        }
    }
    if let Some(kind) = crate::devjunk::artifact_kind(p).filter(|_| crate::devjunk::in_user_project(p)) {
        return (Safe, trf("Build output ({0}) — recreated by the next build.", &[&kind]));
    }
    match name.as_str() {
        "__pycache__" | ".pytest_cache" | ".mypy_cache" | ".ruff_cache" => return (Safe, tr("Python cache.").into()),
        ".DS_Store" => return (Safe, tr("Finder service file.").into()),
        _ => {}
    }
    if rel.starts_with("Library/Developer/Xcode/Archives") {
        return (Careful, tr("Xcode archives — needed to symbolicate crash reports of released builds.").into());
    }
    if rel.starts_with("Library/Developer/CoreSimulator/Devices") {
        return (Careful, tr("iOS simulator data. Better: `xcrun simctl delete unavailable`.").into());
    }
    if rel.starts_with("Library/Containers") || rel.starts_with("Library/Group Containers") || rel.starts_with("Library/Application Support") {
        return (Careful, tr("App data (settings, databases, cache). If the app is still used it may lose data.").into());
    }
    if rel.starts_with("Library") {
        return (Careful, tr("Service data in ~/Library. Remove only if you know what it is.").into());
    }
    if rel.starts_with("Downloads") {
        return (Careful, tr("Downloads — usually fine to remove once you no longer need the file.").into());
    }
    (Careful, tr("Your data. It goes to the Trash, so you can restore it.").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(rel: &str) -> PathBuf {
        home().join(rel)
    }

    #[test]
    fn blocks_system_and_important() {
        for p in ["/System/Library", "/usr/bin/ls", "/Library/Preferences", "/private/etc/hosts", "/", "/Applications"] {
            assert_eq!(deletion_safety(Path::new(p)).0, DelSafety::Blocked, "{p}");
        }
        for r in ["", "Library", "Documents", "Library/Keychains/login.keychain-db", ".ssh/id_ed25519", "Library/Caches", "Dev/app/.git"] {
            assert_eq!(deletion_safety(&h(r)).0, DelSafety::Blocked, "~/{r}");
        }
        assert_eq!(deletion_safety(Path::new("/Applications/Foo.app/Contents/MacOS/Foo")).0, DelSafety::Blocked);
    }

    #[test]
    fn safe_and_careful() {
        assert_eq!(deletion_safety(&h("Library/Caches/com.foo")).0, DelSafety::Safe);
        assert_eq!(deletion_safety(&h("Library/Developer/Xcode/DerivedData/X")).0, DelSafety::Safe);
        assert_eq!(deletion_safety(&h("Documents/report.pdf")).0, DelSafety::Careful);
        assert_eq!(deletion_safety(&h("Library/Mobile Documents/x.txt")).0, DelSafety::Careful);
        assert_eq!(deletion_safety(Path::new("/Applications/Foo.app")).0, DelSafety::Careful);
    }

    #[test]
    fn stale_rules() {
        // Media and app data are never offered.
        assert!(stale_excluded(&h("Pictures/old")));
        assert!(stale_excluded(&h("Docs/ФОТКИ/archive.zip")));
        assert!(stale_excluded(&h("Library/Application Support/Foo")));
        assert!(stale_excluded(&h("Docs/clip.mov")));
        assert!(!stale_excluded(&h("Dev/old-project")));
        // Inside tools only whole units.
        assert!(stale_eligible(&h(".flet"), true));
        assert!(stale_eligible(&h(".rbenv/versions/3.1.2"), true));
        assert!(!stale_eligible(&h(".rbenv/versions/3.1.2/lib/x.gem"), false));
        assert!(!stale_eligible(&h("Library/Android/sdk/emulator/qemu"), true));
        assert!(stale_eligible(&h("Dev/old/dump.sql"), false));
    }
}
