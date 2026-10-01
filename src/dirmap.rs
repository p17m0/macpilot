//! Compact map from folder path to its [`DirStat`] for scans with hundreds of thousands of folders.
//!
//! A `HashMap<PathBuf, DirStat>` costs ~200 bytes per folder (a heap string with the full path, a
//! 64-byte entry, hash-table slack). Here each folder is a tree node — its own name, a link to its
//! parent and packed numbers — plus a 12-byte index entry: about 60 bytes, a third of the memory.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::disk::DirStat;

const NONE: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct Node {
    parent: u32,
    name_off: u32,
    name_len: u32,
    /// Seconds fit in u32 until 2106; `u32::MAX` in `files` marks a folder not measured (only an ancestor).
    files: u32,
    modified: u32,
    used: u32,
    size: u64,
    media: u64,
}

impl Node {
    fn stat(&self) -> Option<DirStat> {
        (self.files != u32::MAX).then_some(DirStat {
            size: self.size,
            files: self.files as u64,
            modified: self.modified as i64,
            used: self.used as i64,
            media: self.media,
        })
    }

    fn set(&mut self, s: DirStat) {
        self.size = s.size;
        self.media = s.media;
        self.files = s.files.min(u32::MAX as u64 - 1) as u32;
        self.modified = s.modified.clamp(0, u32::MAX as i64) as u32;
        self.used = s.used.clamp(0, u32::MAX as i64) as u32;
    }
}

/// FNV-1a over the path bytes. It can be continued: the hash of "a/b" is the hash of "a" fed with "/b".
fn hash_more(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn hash(p: &Path) -> u64 {
    // "/a/b/" and "/a/b" are the same folder.
    let mut b = p.as_os_str().as_bytes();
    while b.len() > 1 && b.ends_with(b"/") {
        b = &b[..b.len() - 1];
    }
    hash_more(0xcbf2_9ce4_8422_2325, b)
}

#[derive(Default)]
pub struct DirMap {
    nodes: Vec<Node>,
    names: Vec<u8>,
    index: HashMap<u64, u32>,
    /// Paths whose hash collides with another path's (practically never, but correctness first).
    overflow: HashMap<PathBuf, u32>,
    measured: usize,
}

impl DirMap {
    pub fn new() -> DirMap {
        DirMap::default()
    }

    /// Number of measured folders.
    pub fn len(&self) -> usize {
        self.measured
    }

    pub fn is_empty(&self) -> bool {
        self.measured == 0
    }

    fn name(&self, i: u32) -> &[u8] {
        let n = &self.nodes[i as usize];
        &self.names[n.name_off as usize..(n.name_off + n.name_len) as usize]
    }

    /// Full path of a node.
    pub fn path(&self, i: u32) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = i;
        while cur != NONE {
            parts.push(self.name(cur));
            cur = self.nodes[cur as usize].parent;
        }
        let mut bytes = Vec::with_capacity(parts.iter().map(|p| p.len() + 1).sum());
        for (k, part) in parts.iter().rev().enumerate() {
            if k > 0 && bytes.last() != Some(&b'/') {
                bytes.push(b'/');
            }
            bytes.extend_from_slice(part);
        }
        PathBuf::from(OsStr::from_bytes(&bytes))
    }

    /// The node of `p`, if present, checked against the actual names (not just the hash).
    fn find(&self, p: &Path) -> Option<u32> {
        if let Some(&i) = self.overflow.get(p) {
            return Some(i);
        }
        let i = *self.index.get(&hash(p))?;
        self.same(i, p).then_some(i)
    }

    fn same(&self, i: u32, p: &Path) -> bool {
        let mut cur = i;
        let mut at = Some(p);
        while cur != NONE {
            let Some(q) = at else { return false };
            let name = q.file_name().map(|n| n.as_bytes()).unwrap_or(q.as_os_str().as_bytes());
            if self.name(cur) != name {
                return false;
            }
            cur = self.nodes[cur as usize].parent;
            at = q.parent();
        }
        at.is_none()
    }

    /// Node of `p`, creating it (and its missing ancestors) as "not measured".
    fn ensure(&mut self, p: &Path) -> u32 {
        if let Some(i) = self.find(p) {
            return i;
        }
        let parent = match p.parent() {
            Some(pp) => self.ensure(pp),
            None => NONE,
        };
        let name = p.file_name().map(|n| n.as_bytes()).unwrap_or(p.as_os_str().as_bytes());
        let i = self.nodes.len() as u32;
        self.nodes.push(Node {
            parent,
            name_off: self.names.len() as u32,
            name_len: name.len() as u32,
            files: u32::MAX,
            modified: 0,
            used: 0,
            size: 0,
            media: 0,
        });
        self.names.extend_from_slice(name);
        match self.index.entry(hash(p)) {
            std::collections::hash_map::Entry::Occupied(_) => {
                self.overflow.insert(p.to_path_buf(), i);
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(i);
            }
        }
        i
    }

    pub fn insert(&mut self, p: &Path, s: DirStat) {
        let i = self.ensure(p) as usize;
        if self.nodes[i].files == u32::MAX {
            self.measured += 1;
        }
        self.nodes[i].set(s);
    }

    pub fn get(&self, p: &Path) -> Option<DirStat> {
        self.nodes[self.find(p)? as usize].stat()
    }

    pub fn get_mut_apply(&mut self, p: &Path, f: impl FnOnce(&mut DirStat)) {
        if let Some(i) = self.find(p) {
            let n = &mut self.nodes[i as usize];
            if let Some(mut s) = n.stat() {
                f(&mut s);
                n.set(s);
            }
        }
    }

    /// Forget `p` and everything inside it.
    pub fn remove_tree(&mut self, p: &Path) {
        let Some(top) = self.find(p) else { return };
        let inside = |map: &DirMap, mut i: u32| {
            while i != NONE {
                if i == top {
                    return true;
                }
                i = map.nodes[i as usize].parent;
            }
            false
        };
        for i in 0..self.nodes.len() as u32 {
            if self.nodes[i as usize].files != u32::MAX && inside(self, i) {
                self.nodes[i as usize].files = u32::MAX;
                self.measured -= 1;
            }
        }
    }

    /// Measured folders as (node, stat); get the path with [`DirMap::path`] only for the ones you keep.
    pub fn iter(&self) -> impl Iterator<Item = (u32, DirStat)> + '_ {
        self.nodes.iter().enumerate().filter_map(|(i, n)| n.stat().map(|s| (i as u32, s)))
    }

    /// Approximate heap memory, bytes.
    pub fn heap_bytes(&self) -> usize {
        self.nodes.capacity() * std::mem::size_of::<Node>() + self.names.capacity() + self.index.capacity() * 13
    }

    // -----------------------------------------------------------------------
    // Cache format: nodes in order (parents always come first), numbers as varints.
    // -----------------------------------------------------------------------

    pub fn encode(&self, out: &mut Vec<u8>) {
        put(out, self.nodes.len() as u64);
        for (i, n) in self.nodes.iter().enumerate() {
            put(out, if n.parent == NONE { 0 } else { n.parent as u64 + 1 });
            let name = self.name(i as u32);
            put(out, name.len() as u64);
            out.extend_from_slice(name);
            match n.stat() {
                Some(_) => {
                    put(out, n.files as u64 + 1);
                    for v in [n.size, n.modified as u64, n.used as u64, n.media] {
                        put(out, v);
                    }
                }
                None => put(out, 0),
            }
        }
    }

    pub fn decode(r: &mut Reader) -> Option<DirMap> {
        let n = r.num()? as usize;
        let mut m = DirMap { nodes: Vec::with_capacity(n.min(10_000_000)), names: Vec::with_capacity(n.min(10_000_000) * 12), ..Default::default() };
        m.index.reserve(n.min(10_000_000));
        // Hashes are rebuilt from the parents' hashes as the tree is read.
        let mut hashes: Vec<u64> = Vec::with_capacity(n.min(10_000_000));
        for i in 0..n {
            let parent = match r.num()? {
                0 => NONE,
                v if (v as usize) <= i => v as u32 - 1,
                _ => return None,
            };
            let len = r.num()? as usize;
            let name = r.bytes(len)?;
            let h = if parent == NONE {
                hash(Path::new(OsStr::from_bytes(name)))
            } else {
                let ph = hashes[parent as usize];
                let pname_is_root = m.nodes[parent as usize].parent == NONE && m.name(parent) == b"/";
                hash_more(if pname_is_root { ph } else { hash_more(ph, b"/") }, name)
            };
            let mut node =
                Node { parent, name_off: m.names.len() as u32, name_len: len as u32, files: u32::MAX, modified: 0, used: 0, size: 0, media: 0 };
            m.names.extend_from_slice(name);
            let files = r.num()?;
            if files > 0 {
                node.files = (files - 1).min(u32::MAX as u64 - 1) as u32;
                node.size = r.num()?;
                node.modified = r.num()?.min(u32::MAX as u64) as u32;
                node.used = r.num()?.min(u32::MAX as u64) as u32;
                node.media = r.num()?;
                m.measured += 1;
            }
            m.nodes.push(node);
            hashes.push(h);
            // On a collision the first path keeps the slot and this one goes to the overflow map.
            if let std::collections::hash_map::Entry::Vacant(e) = m.index.entry(h) {
                e.insert(i as u32);
            } else {
                let p = m.path(i as u32);
                m.overflow.insert(p, i as u32);
            }
        }
        Some(m)
    }
}

pub fn put(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub struct Reader<'a> {
    pub b: &'a [u8],
    pub i: usize,
}

impl Reader<'_> {
    pub fn num(&mut self) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.b.get(self.i)?;
            self.i += 1;
            v |= u64::from(byte & 0x7f) << shift;
            if byte < 0x80 {
                return Some(v);
            }
        }
        None
    }

    pub fn bytes(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.i..self.i.checked_add(n)?)?;
        self.i += n;
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(size: u64) -> DirStat {
        DirStat { size, files: 1, modified: 10, used: 20, media: 0 }
    }

    #[test]
    fn insert_get_and_paths() {
        let mut m = DirMap::new();
        // Children are measured before their parents, as the scanner does.
        m.insert(Path::new("/Users/me/Dev/app"), st(5));
        m.insert(Path::new("/Users/me/Dev"), st(7));
        m.insert(Path::new("/Users/me/Фото"), st(3));
        m.insert(Path::new("/Users/me"), st(15));
        assert_eq!(m.len(), 4);
        assert_eq!(m.get(Path::new("/Users/me/Dev")).unwrap().size, 7);
        assert_eq!(m.get(Path::new("/Users/me/Фото")).unwrap().size, 3);
        assert!(m.get(Path::new("/Users")).is_none(), "an ancestor that was not measured");
        assert!(m.get(Path::new("/Users/me/Dev/other")).is_none());
        assert_eq!(m.get(Path::new("/Users/me/")).unwrap().size, 15, "a trailing slash is the same folder");
        let mut paths: Vec<PathBuf> = m.iter().map(|(i, _)| m.path(i)).collect();
        paths.sort();
        assert_eq!(paths[0], PathBuf::from("/Users/me"));
        assert!(paths.contains(&PathBuf::from("/Users/me/Dev/app")));
        m.remove_tree(Path::new("/Users/me/Dev"));
        assert_eq!(m.len(), 2);
        assert!(m.get(Path::new("/Users/me/Dev/app")).is_none());
        m.get_mut_apply(Path::new("/Users/me"), |s| s.size -= 12);
        assert_eq!(m.get(Path::new("/Users/me")).unwrap().size, 3);
    }

    #[test]
    fn encode_decode() {
        let mut m = DirMap::new();
        for p in ["/a/b/c", "/a/b", "/a/x y", "/a", "/"] {
            m.insert(Path::new(p), st(p.len() as u64));
        }
        let mut buf = Vec::new();
        m.encode(&mut buf);
        let d = DirMap::decode(&mut Reader { b: &buf, i: 0 }).unwrap();
        assert_eq!(d.len(), 5);
        for p in ["/a/b/c", "/a/b", "/a/x y", "/a", "/"] {
            assert_eq!(d.get(Path::new(p)).unwrap().size, p.len() as u64, "{p}");
        }
        assert!(DirMap::decode(&mut Reader { b: &buf[..buf.len() - 2], i: 0 }).is_none());
    }
}
