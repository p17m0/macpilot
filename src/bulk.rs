//! Fast folder listing for scans: `getattrlistbulk` returns names together with sizes and dates,
//! dozens of entries per system call, instead of a `readdir` and an `lstat` for every entry.

use std::ffi::{CString, OsStr, OsString};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// What a scan needs to know about one directory entry (symlinks are not followed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: OsString,
    pub is_dir: bool,
    /// Space used on disk, as `st_blocks * 512`.
    pub alloc: u64,
    pub mtime: i64,
    pub atime: i64,
    pub nlink: u32,
    pub dev: u64,
    pub ino: u64,
}

const VDIR: u32 = 2;
// Not in the libc crate (sys/attr.h).
const ATTR_CMN_ERROR: libc::attrgroup_t = 0x2000_0000;

struct Fd(libc::c_int);

impl Drop for Fd {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}

fn read_at<T: Copy>(buf: &[u8], at: usize) -> Option<T> {
    let end = at.checked_add(std::mem::size_of::<T>())?;
    (end <= buf.len()).then(|| unsafe { std::ptr::read_unaligned(buf[at..].as_ptr().cast::<T>()) })
}

/// Entries of the folder `p`, without "." and "..".
pub fn read(p: &Path) -> io::Result<Vec<Entry>> {
    let c = CString::new(p.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = Fd(fd);
    let mut list = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS
            | ATTR_CMN_ERROR
            | libc::ATTR_CMN_NAME
            | libc::ATTR_CMN_DEVID
            | libc::ATTR_CMN_OBJTYPE
            | libc::ATTR_CMN_MODTIME
            | libc::ATTR_CMN_ACCTIME
            | libc::ATTR_CMN_FILEID,
        volattr: 0,
        dirattr: 0,
        fileattr: libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE,
        forkattr: 0,
    };
    let mut buf = vec![0u8; 64 * 1024];
    let mut out = Vec::new();
    loop {
        let n = unsafe {
            libc::getattrlistbulk(fd.0, (&raw mut list).cast(), buf.as_mut_ptr().cast(), buf.len(), u64::from(libc::FSOPT_PACK_INVAL_ATTRS))
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n == 0 {
            return Ok(out);
        }
        let mut at = 0usize;
        for _ in 0..n {
            let Some(len) = read_at::<u32>(&buf, at) else { break };
            let rec = &buf[at..(at + len as usize).min(buf.len())];
            at += len as usize;
            if let Some(e) = parse(rec) {
                out.push(e);
            }
        }
    }
}

/// One record: length, the returned attribute set, then the attributes in the order of their bits.
/// With FSOPT_PACK_INVAL_ATTRS the common attributes always have their place, unsupported ones are zero.
fn parse(rec: &[u8]) -> Option<Entry> {
    let returned: libc::attribute_set_t = read_at(rec, 4)?;
    let mut at = 4 + std::mem::size_of::<libc::attribute_set_t>();
    // The error comes right after the attribute set, ahead of the order of bits.
    let error: u32 = read_at(rec, at)?;
    at += 4;
    let needed = libc::ATTR_CMN_NAME | libc::ATTR_CMN_OBJTYPE | libc::ATTR_CMN_MODTIME;
    if error != 0 || returned.commonattr & needed != needed {
        return None;
    }
    let name_ref: libc::attrreference_t = read_at(rec, at)?;
    let start = at.checked_add_signed(name_ref.attr_dataoffset as isize)?;
    // The length counts the terminating NUL.
    let bytes = rec.get(start..start + (name_ref.attr_length as usize).saturating_sub(1))?;
    at += std::mem::size_of::<libc::attrreference_t>();
    let dev: i32 = read_at(rec, at)?;
    at += 4;
    let kind: u32 = read_at(rec, at)?;
    at += 4;
    let mtime: libc::timespec = read_at(rec, at)?;
    at += std::mem::size_of::<libc::timespec>();
    let atime: libc::timespec = read_at(rec, at)?;
    at += std::mem::size_of::<libc::timespec>();
    let ino: u64 = read_at(rec, at)?;
    at += 8;
    // Folders come without the file attributes (the record simply ends here).
    let is_dir = kind == VDIR;
    let (nlink, alloc) = if is_dir { (0, 0) } else { (read_at::<u32>(rec, at)?, read_at::<i64>(rec, at + 4)?) };
    Some(Entry {
        name: OsStr::from_bytes(bytes).to_os_string(),
        is_dir,
        alloc: alloc.max(0) as u64,
        mtime: mtime.tv_sec,
        atime: atime.tv_sec,
        nlink,
        dev: dev as u64,
        ino,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn by_lstat(p: &Path) -> Vec<Entry> {
        let mut v: Vec<Entry> = std::fs::read_dir(p)
            .unwrap()
            .flatten()
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                Some(Entry {
                    name: e.file_name(),
                    is_dir: m.is_dir(),
                    alloc: m.blocks() * 512,
                    mtime: m.mtime(),
                    atime: m.atime(),
                    nlink: m.nlink() as u32,
                    dev: m.dev(),
                    ino: m.ino(),
                })
            })
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    fn compare(p: &Path) {
        // Folders are compared by name and type: for the firmlinks of the system volume (/usr/local,
        // /System/Library/Assets…) lstat describes the target and bulk listing the link itself.
        let folders_alike = |v: &mut Vec<Entry>| {
            for e in v.iter_mut().filter(|e| e.is_dir) {
                (e.alloc, e.nlink, e.ino, e.mtime, e.atime) = (0, 0, 0, 0, 0);
            }
        };
        let mut fast = read(p).unwrap();
        fast.sort_by(|a, b| a.name.cmp(&b.name));
        folders_alike(&mut fast);
        let mut slow = by_lstat(p);
        folders_alike(&mut slow);
        // Bulk listing also shows entries that lstat may not look at (protected system data).
        let seen: std::collections::HashSet<_> = slow.iter().map(|e| e.name.clone()).collect();
        fast.retain(|e| seen.contains(&e.name));
        assert_eq!(fast, slow, "{}", p.display());
    }

    #[test]
    fn matches_lstat() {
        let dir = std::env::temp_dir().join(format!("macpilot-bulk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), vec![7u8; 10_000]).unwrap();
        std::fs::write(dir.join("пустой"), b"").unwrap();
        std::fs::hard_link(dir.join("a.txt"), dir.join("b.txt")).unwrap();
        std::os::unix::fs::symlink("a.txt", dir.join("link")).unwrap();
        // Enough long names to need several calls.
        for i in 0..1500 {
            std::fs::write(dir.join(format!("{i:0>200}")), b"x").unwrap();
        }
        compare(&dir);
        assert_eq!(read(&dir).unwrap().len(), 1505);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_folder() {
        assert!(read(Path::new("/nonexistent-macpilot")).is_err());
        assert!(read(Path::new("/etc/hosts")).is_err());
    }

    /// `cargo test --release --lib real_folders -- --ignored --nocapture`: system folders two levels
    /// deep (no privacy-protected ones), compared with readdir + lstat and timed.
    #[test]
    #[ignore]
    fn real_folders() {
        let mut dirs: Vec<std::path::PathBuf> = ["/Applications", "/Library", "/System/Library", "/opt", "/usr"].iter().map(Into::into).collect();
        for d in dirs.clone() {
            dirs.extend(read(&d).unwrap_or_default().into_iter().filter(|e| e.is_dir).map(|e| d.join(e.name)));
        }
        for d in &dirs {
            if read(d).is_ok() {
                compare(d);
            }
        }
        let t = std::time::Instant::now();
        let n: usize = dirs.iter().map(|d| read(d).map(|v| v.len()).unwrap_or(0)).sum();
        let fast = t.elapsed();
        let t = std::time::Instant::now();
        let m: usize = dirs.iter().map(|d| std::fs::read_dir(d).map(|r| r.flatten().filter(|e| e.metadata().is_ok()).count()).unwrap_or(0)).sum();
        let slow = t.elapsed();
        println!("{} folders, {n}/{m} entries: bulk {fast:?}, readdir+lstat {slow:?}", dirs.len());
    }
}
