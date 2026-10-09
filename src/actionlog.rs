//! A journal of what MacPilot did on this Mac: what it moved to the Trash, which processes it
//! stopped, which startup items it switched — so that a week later you can still see what
//! happened and when. Kept in `~/Library/Application Support/MacPilot/actions.tsv` for 90 days.

use std::path::PathBuf;

const KEEP_DAYS: i64 = 90;
const KEEP_ENTRIES: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Trash,
    /// The cleanup on a schedule.
    AutoClean,
    EmptyTrash,
    PutBack,
    /// A process was asked to quit (SIGTERM).
    Quit,
    ForceQuit,
    QuitApp,
    Pause,
    Resume,
    StartupOn,
    StartupOff,
    /// Local Time Machine snapshots were deleted.
    Snapshots,
    AppUpdate,
}

impl Kind {
    const ALL: [(Kind, &'static str); 13] = [
        (Kind::Trash, "trash"),
        (Kind::AutoClean, "autoclean"),
        (Kind::EmptyTrash, "emptytrash"),
        (Kind::PutBack, "putback"),
        (Kind::Quit, "quit"),
        (Kind::ForceQuit, "forcequit"),
        (Kind::QuitApp, "quitapp"),
        (Kind::Pause, "pause"),
        (Kind::Resume, "resume"),
        (Kind::StartupOn, "startupon"),
        (Kind::StartupOff, "startupoff"),
        (Kind::Snapshots, "snapshots"),
        (Kind::AppUpdate, "appupdate"),
    ];

    fn code(self) -> &'static str {
        Kind::ALL.iter().find(|k| k.0 == self).map_or("", |k| k.1)
    }

    fn from_code(s: &str) -> Option<Kind> {
        Kind::ALL.iter().find(|k| k.1 == s).map(|k| k.0)
    }

    /// The action in words.
    pub fn label(self) -> &'static str {
        use crate::tr;
        match self {
            Kind::Trash => tr("Moved to the Trash"),
            Kind::AutoClean => tr("Scheduled cleanup"),
            Kind::EmptyTrash => tr("Emptied the Trash"),
            Kind::PutBack => tr("Put back from the Trash"),
            Kind::Quit => tr("Asked a process to quit"),
            Kind::ForceQuit => tr("Force quit a process"),
            Kind::QuitApp => tr("Quit an app"),
            Kind::Pause => tr("Paused a process"),
            Kind::Resume => tr("Resumed a process"),
            Kind::StartupOn => tr("Turned on a startup item"),
            Kind::StartupOff => tr("Turned off a startup item"),
            Kind::Snapshots => tr("Deleted Time Machine snapshots"),
            Kind::AppUpdate => tr("Updated an app"),
        }
    }

    /// Something that touched files (as opposed to processes, startup items and apps).
    pub fn is_cleanup(self) -> bool {
        matches!(self, Kind::Trash | Kind::AutoClean | Kind::EmptyTrash | Kind::PutBack | Kind::Snapshots)
    }

    pub fn is_process(self) -> bool {
        matches!(self, Kind::Quit | Kind::ForceQuit | Kind::QuitApp | Kind::Pause | Kind::Resume)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// When it happened (unix seconds).
    pub at: i64,
    pub kind: Kind,
    /// How many things it was about (files, processes, snapshots); 0 when that says nothing.
    pub count: u64,
    /// Bytes, when the action is about space; otherwise 0.
    pub size: u64,
    /// What it was about: a path, a process, an app — in a line.
    pub what: String,
}

fn file() -> PathBuf {
    crate::settings::dir().join("actions.tsv")
}

fn parse(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.splitn(5, '\t');
            let at = f.next()?.parse().ok()?;
            let kind = Kind::from_code(f.next()?)?;
            let count = f.next()?.parse().ok()?;
            let size = f.next()?.parse().ok()?;
            Some(Entry { at, kind, count, size, what: f.next()?.to_string() })
        })
        .collect()
}

fn render(entries: &[Entry]) -> String {
    entries.iter().map(|e| format!("{}\t{}\t{}\t{}\t{}\n", e.at, e.kind.code(), e.count, e.size, e.what.replace(['\t', '\n', '\r'], " "))).collect()
}

/// The journal, oldest first.
pub fn load() -> Vec<Entry> {
    parse(&std::fs::read_to_string(file()).unwrap_or_default())
}

fn save(entries: &[Entry]) {
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

pub fn record(entries: &mut Vec<Entry>, entry: Entry) {
    let now = entry.at;
    entries.push(entry);
    trim(entries, now);
    save(entries);
}

pub fn clear(entries: &mut Vec<Entry>) {
    entries.clear();
    save(entries);
}

/// "a.txt, b.txt, c.txt…" for a list of names: the first few, so that a line stays a line.
pub fn names<S: AsRef<str>>(all: &[S]) -> String {
    let mut s = all.iter().take(3).map(|n| n.as_ref()).collect::<Vec<_>>().join(", ");
    if all.len() > 3 {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_round_trip_and_trim() {
        let mut v: Vec<Entry> = Kind::ALL
            .iter()
            .enumerate()
            .map(|(i, k)| Entry { at: 9_000_000 + i as i64, kind: k.0, count: i as u64, size: 1 << i, what: format!("~/Library/Caches, item {i}") })
            .collect();
        assert_eq!(parse(&render(&v)), v, "every kind survives");
        v.insert(0, Entry { at: 100, kind: Kind::Trash, count: 1, size: 5, what: "a\tb\nc".into() });
        assert_eq!(parse(&render(&v))[0].what, "a b c", "an entry stays on one line");
        trim(&mut v, 9_000_100);
        assert_eq!(v.len(), Kind::ALL.len(), "entries older than 90 days go");
        assert!(parse("garbage\n12\tnonsense\t1\t1\tx\n").is_empty());
        assert_eq!(names(&["a", "b", "c", "d"]), "a, b, c…");
        assert_eq!(names(&["a"]), "a");
    }
}
