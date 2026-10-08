//! A journal of what MacPilot moved to the Trash: when, from where, how big — so that a week
//! later you can still see what went and put it back. Kept in
//! `~/Library/Application Support/MacPilot/trash-log.tsv` for 90 days.

use std::path::{Path, PathBuf};

const KEEP_DAYS: i64 = 90;
const KEEP_ENTRIES: usize = 3000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Still in the Trash, as far as MacPilot knows.
    InTrash,
    /// Put back to where it was.
    Restored,
    /// The Trash was emptied afterwards: gone for good.
    Emptied,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// When it was moved to the Trash (unix seconds).
    pub at: i64,
    /// Where it was.
    pub path: PathBuf,
    pub size: u64,
    pub state: State,
}

fn file() -> PathBuf {
    crate::settings::dir().join("trash-log.tsv")
}

fn parse(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.splitn(4, '\t');
            let at = f.next()?.parse().ok()?;
            let size = f.next()?.parse().ok()?;
            let state = match f.next()? {
                "restored" => State::Restored,
                "emptied" => State::Emptied,
                _ => State::InTrash,
            };
            Some(Entry { at, size, state, path: PathBuf::from(f.next()?) })
        })
        .collect()
}

fn render(entries: &[Entry]) -> String {
    entries
        .iter()
        .map(|e| {
            let state = match e.state {
                State::InTrash => "trash",
                State::Restored => "restored",
                State::Emptied => "emptied",
            };
            format!("{}\t{}\t{state}\t{}\n", e.at, e.size, e.path.display())
        })
        .collect()
}

/// The journal, oldest first.
pub fn load() -> Vec<Entry> {
    parse(&std::fs::read_to_string(file()).unwrap_or_default())
}

pub fn save(entries: &[Entry]) {
    let _ = std::fs::create_dir_all(crate::settings::dir());
    let _ = std::fs::write(file(), render(entries));
}

/// Drop what is too old, and keep the journal a reasonable size.
fn trim(entries: &mut Vec<Entry>, now: i64) {
    entries.retain(|e| now - e.at <= KEEP_DAYS * 86_400);
    if entries.len() > KEEP_ENTRIES {
        entries.drain(..entries.len() - KEEP_ENTRIES);
    }
}

/// Record items that just went to the Trash.
pub fn record(entries: &mut Vec<Entry>, items: Vec<(PathBuf, u64)>, now: i64) {
    entries.extend(items.into_iter().map(|(path, size)| Entry { at: now, path, size, state: State::InTrash }));
    trim(entries, now);
    save(entries);
}

/// The Trash was emptied: nothing recorded so far can be put back any more.
pub fn mark_emptied(entries: &mut [Entry]) {
    for e in entries.iter_mut().filter(|e| e.state == State::InTrash) {
        e.state = State::Emptied;
    }
    save(entries);
}

pub fn mark_restored(entries: &mut [Entry], path: &Path, at: i64) {
    if let Some(e) = entries.iter_mut().find(|e| e.path == path && e.at == at) {
        e.state = State::Restored;
    }
    save(entries);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_round_trip_and_trim() {
        let mut v = vec![
            Entry { at: 100, path: "/Users/me/old".into(), size: 5, state: State::Emptied },
            Entry { at: 9_000_000, path: "/Users/me/a\tb, c.txt".into(), size: 7, state: State::InTrash },
            Entry { at: 9_000_100, path: "/Users/me/Dev/target".into(), size: 1 << 33, state: State::Restored },
        ];
        assert_eq!(parse(&render(&v)), v, "paths with tabs and commas survive");
        trim(&mut v, 9_000_200);
        assert_eq!(v.len(), 2, "entries older than 90 days go");
        for e in v.iter_mut().filter(|e| e.state == State::InTrash) {
            e.state = State::Emptied;
        }
        assert!(v.iter().all(|e| e.state != State::InTrash));
        assert!(parse("garbage\n12\tx\n").is_empty());
    }
}
