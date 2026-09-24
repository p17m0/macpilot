//! Terminal UI state and key handling.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sysinfo::Disks;

use crate::clean::{self, Target};
use crate::disk::{self, DelSafety, DirStat, Entry, Scan};
use crate::procs::{self, AppGroup, Monitor, Safety};
use crate::{fmt, tr, trf};

pub enum Msg {
    Trashed { paths: Vec<PathBuf>, size: u64, result: Result<(), String> },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Procs,
    Disk,
    Clean,
    Help,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProcView {
    Flat,
    Tree,
    Apps,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Cpu,
    Mem,
    Pid,
    Name,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DiskView {
    Browse,
    BigFiles,
    Stale,
}

pub enum Action {
    Signal { pids: Vec<u32>, sig: i32 },
    QuitApp { app: PathBuf, pids: Vec<u32> },
    Trash { paths: Vec<PathBuf>, size: u64 },
}

pub struct Confirm {
    pub title: String,
    pub lines: Vec<(String, Level)>,
    pub action: Action,
    /// When set, the user must type this word.
    pub require: Option<&'static str>,
    pub input: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Ok,
    Warn,
    Danger,
}

pub enum Mode {
    Normal,
    Filter,
    GotoPath(String),
    Confirm(Confirm),
}

pub enum ProcRow {
    Proc { pid: u32, depth: usize },
    Group(AppGroup),
}

pub struct App {
    pub tab: Tab,
    pub mode: Mode,
    pub quit: bool,
    pub status: Option<(String, Level, Instant)>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    mtx: Sender<(usize, DirStat)>,
    mrx: Receiver<(usize, DirStat)>,

    pub mon: Monitor,
    pub view: ProcView,
    pub sort: SortKey,
    pub sort_desc: bool,
    pub filter: String,
    pub only_mine: bool,
    pub rows: Vec<ProcRow>,
    pub sel: usize,
    sel_key: Option<String>,
    last_refresh: Instant,
    pending: Vec<(u32, String, Instant)>,

    pub disks: Disks,
    pub scan: Option<Scan>,
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub entries_err: Option<String>,
    pub dsel: usize,
    pub disk_view: DiskView,
    pub big: Vec<(u64, PathBuf)>,
    pub bsel: usize,
    pub stale: Vec<disk::StaleItem>,
    pub ssel: usize,
    pub stale_days: i64,
    last_list: Instant,

    pub targets: Vec<Target>,
    pub csel: usize,
}

impl App {
    pub fn new(stale_days: i64) -> App {
        let (tx, rx) = channel();
        let (mtx, mrx) = channel();
        let targets = clean::targets();
        clean::measure_all(&targets, mtx.clone());
        let mut app = App {
            tab: Tab::Procs,
            mode: Mode::Normal,
            quit: false,
            status: None,
            tx,
            rx,
            mtx,
            mrx,
            mon: Monitor::new(),
            view: ProcView::Flat,
            sort: SortKey::Cpu,
            sort_desc: true,
            filter: String::new(),
            only_mine: false,
            rows: Vec::new(),
            sel: 0,
            sel_key: None,
            last_refresh: Instant::now(),
            pending: Vec::new(),
            disks: Disks::new_with_refreshed_list(),
            scan: None,
            cwd: macpilot::home(),
            entries: Vec::new(),
            entries_err: None,
            dsel: 0,
            disk_view: DiskView::Browse,
            big: Vec::new(),
            bsel: 0,
            stale: Vec::new(),
            ssel: 0,
            stale_days,
            last_list: Instant::now() - Duration::from_secs(10),
            targets,
            csel: 0,
        };
        app.rebuild_rows();
        app
    }

    pub fn set_status(&mut self, s: impl Into<String>, lvl: Level) {
        self.status = Some((s.into(), lvl, Instant::now()));
    }

    pub fn tick(&mut self) {
        while let Ok(Msg::Trashed { paths, size, result }) = self.rx.try_recv() {
            match result {
                Ok(()) => {
                    let what = if paths.len() == 1 { fmt::path(&paths[0]) } else { fmt::n(paths.len() as u64, fmt::Noun::Item) };
                    self.set_status(
                        trf("Moved to the Trash: {0}. {1} will be freed when you empty the Trash.", &[&what, &fmt::bytes(size)]),
                        Level::Ok,
                    );
                }
                Err(e) => self.set_status(trf("Some items could not be moved to the Trash: {0}", &[&e]), Level::Danger),
            }
            if let Some(scan) = &self.scan {
                for p in &paths {
                    let st = scan.size_of(p).unwrap_or_default();
                    scan.forget(p, st.size, st.files);
                }
            }
            for i in 0..self.targets.len() {
                if paths.iter().any(|p| p.starts_with(&self.targets[i].path)) {
                    self.remeasure(i);
                }
            }
            self.stale.retain(|i| !paths.contains(&i.path));
            self.ssel = self.ssel.min(self.stale.len().saturating_sub(1));
            self.reload_dir();
        }
        while let Ok((i, st)) = self.mrx.try_recv() {
            if let Some(t) = self.targets.get_mut(i) {
                t.stat = Some(st);
            }
        }
        if self.last_refresh.elapsed() >= Duration::from_millis(1500) {
            self.mon.refresh();
            self.disks.refresh(true);
            self.last_refresh = Instant::now();
            self.check_pending();
            self.rebuild_rows();
        }
        if self.tab == Tab::Disk {
            let scanning = self.scan.as_ref().is_some_and(|s| !s.done());
            let period = if scanning { 700 } else { 5000 };
            if self.last_list.elapsed() >= Duration::from_millis(period) {
                self.reload_dir();
            }
        }
        if let Some(s) = &mut self.scan {
            if s.done() && s.finished_in.is_none() {
                s.finished_in = Some(s.started.elapsed());
                self.reload_dir();
            }
        }
        if self.status.as_ref().is_some_and(|(_, _, t)| t.elapsed() > Duration::from_secs(12)) {
            self.status = None;
        }
    }

    fn remeasure(&mut self, i: usize) {
        let Some(t) = self.targets.get_mut(i) else { return };
        t.stat = None;
        let p = t.path.clone();
        let tx = self.mtx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((i, disk::measure(&p)));
        });
    }

    fn check_pending(&mut self) {
        let mut still = Vec::new();
        let mut msg = None;
        for (pid, name, t) in std::mem::take(&mut self.pending) {
            if !self.mon.alive(pid) {
                msg = Some((trf("“{0}” (PID {1}) has quit.", &[&name, &pid]), Level::Ok));
            } else if t.elapsed() > Duration::from_secs(8) {
                msg = Some((
                    trf("“{0}” did not quit within 8 s — it may be waiting for an answer in a dialog. Press K to force quit.", &[&name]),
                    Level::Warn,
                ));
            } else {
                still.push((pid, name, t));
            }
        }
        self.pending = still;
        if let Some((m, l)) = msg {
            self.set_status(m, l);
        }
    }

    // ------------------------------------------------------------------
    // Processes
    // ------------------------------------------------------------------

    fn proc_cmp(&self) -> impl Fn(&procs::ProcInfo, &procs::ProcInfo) -> std::cmp::Ordering + '_ {
        let (key, desc) = (self.sort, self.sort_desc);
        move |a, b| {
            let o = match key {
                SortKey::Cpu => a.cpu.total_cmp(&b.cpu).then(a.mem.cmp(&b.mem)),
                SortKey::Mem => a.mem.cmp(&b.mem),
                SortKey::Pid => a.pid.cmp(&b.pid),
                SortKey::Name => b.name.to_lowercase().cmp(&a.name.to_lowercase()),
            };
            if desc { o.reverse() } else { o }
        }
    }

    fn matches(&self, p: &procs::ProcInfo) -> bool {
        if self.only_mine && p.uid != Some(self.mon.my_uid) {
            return false;
        }
        if self.filter.is_empty() {
            return true;
        }
        let f = self.filter.to_lowercase();
        p.name.to_lowercase().contains(&f)
            || p.pid.to_string() == f
            || p.app_name().is_some_and(|a| a.to_lowercase().contains(&f))
            || p.cmd.to_lowercase().contains(&f)
    }

    pub fn rebuild_rows(&mut self) {
        if let Some(k) = self.row_key(self.sel) {
            self.sel_key = Some(k);
        }
        let cmp = self.proc_cmp();
        let rows: Vec<ProcRow> = match self.view {
            ProcView::Flat => {
                let mut v: Vec<&procs::ProcInfo> = self.mon.procs.iter().filter(|p| self.matches(p)).collect();
                v.sort_by(|a, b| cmp(a, b));
                v.into_iter().map(|p| ProcRow::Proc { pid: p.pid, depth: 0 }).collect()
            }
            ProcView::Tree => {
                let mut visible: HashSet<u32> = HashSet::new();
                for p in self.mon.procs.iter().filter(|p| self.matches(p)) {
                    let mut cur = Some(p.pid);
                    while let Some(pid) = cur {
                        if !visible.insert(pid) {
                            break;
                        }
                        cur = self.mon.get(pid).and_then(|x| x.ppid).filter(|pp| *pp != pid);
                    }
                }
                self.mon.tree_order(&visible, &cmp).into_iter().map(|(pid, depth)| ProcRow::Proc { pid, depth }).collect()
            }
            ProcView::Apps => {
                let mut g: Vec<AppGroup> =
                    self.mon.groups().into_iter().filter(|g| g.pids.iter().any(|pid| self.mon.get(*pid).is_some_and(|p| self.matches(p)))).collect();
                let (key, desc) = (self.sort, self.sort_desc);
                g.sort_by(|a, b| {
                    let o = match key {
                        SortKey::Cpu => a.cpu.total_cmp(&b.cpu),
                        SortKey::Mem => a.mem.cmp(&b.mem),
                        SortKey::Pid => a.pids.len().cmp(&b.pids.len()),
                        SortKey::Name => b.label.to_lowercase().cmp(&a.label.to_lowercase()),
                    };
                    if desc { o.reverse() } else { o }
                });
                g.into_iter().map(ProcRow::Group).collect()
            }
        };
        drop(cmp);
        self.rows = rows;
        if let Some(k) = &self.sel_key {
            if let Some(i) = (0..self.rows.len()).find(|i| self.row_key(*i).as_ref() == Some(k)) {
                self.sel = i;
            }
        }
        self.sel = self.sel.min(self.rows.len().saturating_sub(1));
    }

    fn row_key(&self, i: usize) -> Option<String> {
        match self.rows.get(i)? {
            ProcRow::Proc { pid, .. } => Some(format!("p{pid}")),
            ProcRow::Group(g) => Some(format!("g{}", g.key)),
        }
    }

    pub fn selected_proc(&self) -> Option<&procs::ProcInfo> {
        match self.rows.get(self.sel)? {
            ProcRow::Proc { pid, .. } => self.mon.get(*pid),
            ProcRow::Group(g) => g.pids.iter().filter_map(|p| self.mon.get(*p)).max_by_key(|p| (p.is_main_app, p.mem)),
        }
    }

    pub fn selected_group(&self) -> Option<&AppGroup> {
        match self.rows.get(self.sel)? {
            ProcRow::Group(g) => Some(g),
            _ => None,
        }
    }

    fn ask_stop(&mut self, force: bool) {
        let (pids, name, safety, app, is_app_quit): (Vec<u32>, String, Safety, Option<PathBuf>, bool) = if let Some(g) = self.selected_group() {
            let main = g.app.is_some() && g.pids.iter().any(|p| self.mon.get(*p).is_some_and(|x| x.is_main_app));
            (g.pids.clone(), g.label.clone(), g.safety, g.app.clone(), main)
        } else if let Some(p) = self.selected_proc() {
            (vec![p.pid], p.name.clone(), p.safety, p.app.clone(), p.is_main_app)
        } else {
            return;
        };
        if safety.blocked() {
            let why = pids
                .iter()
                .filter_map(|p| self.mon.get(*p))
                .find(|p| p.safety.blocked())
                .map(|p| {
                    if p.pid == self.mon.my_pid {
                        tr("this is MacPilot itself — press q to exit").to_string()
                    } else {
                        trf("“{0}” (PID {1}) is vital to macOS", &[&p.name, &p.pid])
                    }
                })
                .unwrap_or_default();
            self.set_status(trf("Blocked: {0}.", &[&why]), Level::Danger);
            return;
        }
        let what = if pids.len() > 1 {
            format!("“{name}” — {}", fmt::n(pids.len() as u64, fmt::Noun::Process))
        } else {
            format!("“{name}” (PID {})", pids[0])
        };
        let mut lines = vec![(what, Level::Info)];
        let (title, action) = if force {
            lines.push((tr("The process will be killed immediately (SIGKILL).").into(), Level::Warn));
            lines.push((tr("Unsaved work will be lost. Use only if a normal stop did not help.").into(), Level::Danger));
            (tr("Force quit?"), Action::Signal { pids: pids.clone(), sig: libc::SIGKILL })
        } else if let (true, Some(app)) = (is_app_quit, app) {
            lines.push((tr("The app quits as if you pressed ⌘Q: it saves its state and asks about unsaved documents.").into(), Level::Ok));
            (tr("Quit the app?"), Action::QuitApp { app, pids: pids.clone() })
        } else {
            lines.push((tr("The process is asked to quit (SIGTERM) and can close its files and connections properly.").into(), Level::Ok));
            (tr("Stop the process?"), Action::Signal { pids: pids.clone(), sig: libc::SIGTERM })
        };
        let require = if safety == Safety::System {
            lines.push((Safety::System.explain().into(), Level::Danger));
            Some(tr("yes"))
        } else {
            None
        };
        self.mode = Mode::Confirm(Confirm { title: title.into(), lines, action, require, input: String::new() });
    }

    fn toggle_pause(&mut self) {
        if self.selected_group().is_some() {
            self.set_status(tr("Pause works on single processes (switch the view with v)."), Level::Warn);
            return;
        }
        let Some(p) = self.selected_proc() else { return };
        let (pid, name, safety, stopped) = (p.pid, p.name.clone(), p.safety, p.stopped);
        if safety != Safety::User {
            self.set_status(tr("Only your own processes can be paused: freezing a system one can hang the Mac."), Level::Danger);
            return;
        }
        let sig = if stopped { libc::SIGCONT } else { libc::SIGSTOP };
        match procs::send_signal(pid, sig) {
            Ok(()) => self.set_status(if stopped { trf("“{0}” resumed", &[&name]) } else { trf("“{0}” paused", &[&name]) }, Level::Ok),
            Err(e) => self.set_status(trf("Failed: {0}", &[&e]), Level::Danger),
        }
        self.mon.refresh();
        self.rebuild_rows();
    }

    // ------------------------------------------------------------------
    // Disk
    // ------------------------------------------------------------------

    pub fn start_scan(&mut self, root: PathBuf) {
        self.scan = Some(Scan::start(root.clone()));
        self.cwd = root;
        self.dsel = 0;
        self.reload_dir();
    }

    pub fn reload_dir(&mut self) {
        self.last_list = Instant::now();
        let keep = self.entries.get(self.dsel).map(|e| e.path.clone());
        match disk::list_dir(&self.cwd, self.scan.as_ref()) {
            Ok(v) => {
                self.entries = v;
                self.entries_err = None;
            }
            Err(e) => {
                self.entries.clear();
                self.entries_err = Some(e);
            }
        }
        if let Some(k) = keep {
            if let Some(i) = self.entries.iter().position(|e| e.path == k) {
                self.dsel = i;
            }
        }
        self.dsel = self.dsel.min(self.entries.len().saturating_sub(1));
        if let Some(s) = &self.scan {
            self.big = s.big_files();
            self.bsel = self.bsel.min(self.big.len().saturating_sub(1));
        }
    }

    fn open_dir(&mut self, p: PathBuf) {
        let from = self.cwd.clone();
        self.cwd = p;
        self.dsel = 0;
        self.entries.clear();
        self.reload_dir();
        if let Some(i) = self.entries.iter().position(|e| e.path == from) {
            self.dsel = i;
        }
        if self.scan.as_ref().is_none_or(|s| !s.covers(&self.cwd)) {
            self.set_status(tr("This folder is outside the scanned area — press r to scan it."), Level::Info);
        }
    }

    pub fn dir_total(&self) -> Option<DirStat> {
        self.scan.as_ref().and_then(|s| s.dir(&self.cwd))
    }

    fn selected_disk_path(&self) -> Option<(PathBuf, Option<u64>)> {
        match self.disk_view {
            DiskView::Browse => self.entries.get(self.dsel).map(|e| (e.path.clone(), e.size)),
            DiskView::BigFiles => self.big.get(self.bsel).map(|(s, p)| (p.clone(), Some(*s))),
            DiskView::Stale => self.stale.get(self.ssel).map(|i| (i.path.clone(), Some(i.size))),
        }
    }

    fn ask_trash(&mut self) {
        let Some((path, size)) = self.selected_disk_path() else { return };
        let (safety, why) = disk::deletion_safety(&path);
        if safety == DelSafety::Blocked {
            self.set_status(format!("{}: {why}", fmt::path(&path)), Level::Danger);
            return;
        }
        let size = size.unwrap_or_else(|| disk::measure(&path).size);
        let mut lines = vec![
            (fmt::path(&path), Level::Info),
            (trf("Size: {0}", &[&fmt::bytes(size)]), Level::Info),
            (format!("{} — {why}", safety.label()), if safety == DelSafety::Safe { Level::Ok } else { Level::Warn }),
            (tr("It goes to the Trash and can be restored until the Trash is emptied.").into(), Level::Ok),
        ];
        let running = self.running_in(&path);
        if !running.is_empty() {
            lines.push((trf("Running from here right now: {0} — quit them first.", &[&running.join(", ")]), Level::Warn));
        }
        self.mode = Mode::Confirm(Confirm {
            title: tr("Move to the Trash?").into(),
            lines,
            action: Action::Trash { paths: vec![path], size },
            require: None,
            input: String::new(),
        });
    }

    fn running_in(&self, path: &Path) -> Vec<String> {
        let mut v: Vec<String> = self
            .mon
            .procs
            .iter()
            .filter(|p| p.exe.as_deref().is_some_and(|e| e.starts_with(path)) || p.cwd.as_deref().is_some_and(|c| c.starts_with(path)))
            .map(|p| format!("{} ({})", p.name, p.pid))
            .collect();
        v.sort();
        v.dedup();
        v.truncate(5);
        v
    }

    fn ask_clean(&mut self) {
        let Some(t) = self.targets.get(self.csel).cloned() else { return };
        if !t.cleanable() {
            self.set_status(trf("“{0}” is not cleaned automatically: {1}", &[&t.label, &t.hint]), Level::Warn);
            return;
        }
        let children: Vec<PathBuf> = match std::fs::read_dir(&t.path) {
            Ok(rd) => rd.flatten().map(|e| e.path()).filter(|c| disk::deletion_safety(c).0 != DelSafety::Blocked).collect(),
            Err(e) => {
                self.set_status(format!("{}: {}", fmt::path(&t.path), disk::perm_hint(&e)), Level::Danger);
                return;
            }
        };
        if children.is_empty() {
            self.set_status(trf("“{0}” is already empty.", &[&t.label]), Level::Info);
            return;
        }
        let st = t.stat.unwrap_or_default();
        let lines = vec![
            (format!("{} — {}, {}", fmt::path(&t.path), fmt::n(children.len() as u64, fmt::Noun::Item), fmt::bytes(st.size)), Level::Info),
            (t.hint.to_string(), Level::Info),
            (tr("The contents go to the Trash (the folder itself stays).").into(), Level::Ok),
        ];
        self.mode = Mode::Confirm(Confirm {
            title: trf("Clean “{0}”?", &[&t.label]),
            lines,
            action: Action::Trash { paths: children, size: st.size },
            require: None,
            input: String::new(),
        });
    }

    fn execute(&mut self, a: Action) {
        match a {
            Action::Signal { pids, sig } => {
                let mut errs = Vec::new();
                for pid in &pids {
                    let name = self.mon.get(*pid).map(|p| p.name.clone()).unwrap_or_default();
                    match procs::send_signal(*pid, sig) {
                        Ok(()) if sig == libc::SIGTERM => self.pending.push((*pid, name, Instant::now())),
                        Ok(()) => {}
                        Err(e) => errs.push(format!("{name} ({pid}): {e}")),
                    }
                }
                if errs.is_empty() {
                    let n = pids.len();
                    let msg = if sig == libc::SIGKILL {
                        trf("Force quit: {0}.", &[&fmt::n(n as u64, fmt::Noun::Process)])
                    } else {
                        trf("Asked to quit: {0}.", &[&fmt::n(n as u64, fmt::Noun::Process)])
                    };
                    self.set_status(msg, Level::Ok);
                } else {
                    self.set_status(trf("Failed: {0}", &[&errs.join("; ")]), Level::Danger);
                }
                self.last_refresh = Instant::now() - Duration::from_secs(1);
            }
            Action::QuitApp { app, pids } => {
                procs::quit_app(&app);
                let name = procs::bundle_name(&app);
                if let Some(main) = pids.iter().find(|p| self.mon.get(**p).is_some_and(|x| x.is_main_app)) {
                    self.pending.push((*main, name.clone(), Instant::now()));
                }
                self.set_status(trf("Asked “{0}” to quit. If it asks about saving, answer in the app.", &[&name]), Level::Info);
            }
            Action::Trash { paths, size } => {
                let tx = self.tx.clone();
                self.set_status(tr("Moving to the Trash…"), Level::Info);
                std::thread::spawn(move || {
                    let result = macpilot::trash::move_to_trash(&paths);
                    let _ = tx.send(Msg::Trashed { paths, size, result });
                });
            }
        }
    }

    // ------------------------------------------------------------------
    // Keys
    // ------------------------------------------------------------------

    pub fn on_key(&mut self, k: KeyEvent) {
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Normal => self.on_key_normal(k),
            Mode::Filter => {
                match k.code {
                    KeyCode::Esc => self.filter.clear(),
                    KeyCode::Enter => {}
                    KeyCode::Backspace => {
                        self.filter.pop();
                        self.mode = Mode::Filter;
                    }
                    KeyCode::Char(c) => {
                        self.filter.push(c);
                        self.mode = Mode::Filter;
                    }
                    KeyCode::Up | KeyCode::Down => {
                        self.mode = Mode::Filter;
                        self.on_key_normal(k);
                    }
                    _ => self.mode = Mode::Filter,
                }
                self.sel = 0;
                self.sel_key = None;
                self.rebuild_rows();
            }
            Mode::GotoPath(mut s) => match k.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    let p = expand(&s);
                    if p.is_dir() {
                        self.disk_view = DiskView::Browse;
                        self.open_dir(p);
                    } else {
                        self.set_status(trf("No such folder: {0}", &[&p.display()]), Level::Warn);
                    }
                }
                KeyCode::Backspace => {
                    s.pop();
                    self.mode = Mode::GotoPath(s);
                }
                KeyCode::Char(c) => {
                    s.push(c);
                    self.mode = Mode::GotoPath(s);
                }
                _ => self.mode = Mode::GotoPath(s),
            },
            Mode::Confirm(mut c) => {
                if let Some(word) = c.require {
                    match k.code {
                        KeyCode::Esc => self.set_status(tr("Cancelled."), Level::Info),
                        KeyCode::Enter if c.input.trim().eq_ignore_ascii_case(word) => self.execute(c.action),
                        KeyCode::Enter => self.set_status(trf("Cancelled: you needed to type “{0}”.", &[&word]), Level::Info),
                        KeyCode::Backspace => {
                            c.input.pop();
                            self.mode = Mode::Confirm(c);
                        }
                        KeyCode::Char(ch) => {
                            c.input.push(ch);
                            self.mode = Mode::Confirm(c);
                        }
                        _ => self.mode = Mode::Confirm(c),
                    }
                } else {
                    match ru_to_en(k.code) {
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => self.execute(c.action),
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => self.set_status(tr("Cancelled."), Level::Info),
                        _ => self.mode = Mode::Confirm(c),
                    }
                }
            }
        }
    }

    fn on_key_normal(&mut self, k: KeyEvent) {
        match ru_to_en(k.code) {
            KeyCode::Char('q') => {
                self.quit = true;
                return;
            }
            KeyCode::Char('1') => return self.tab = Tab::Procs,
            KeyCode::Char('2') => return self.switch_disk(),
            KeyCode::Char('3') => return self.tab = Tab::Clean,
            KeyCode::Char('?') | KeyCode::F(1) => return self.tab = Tab::Help,
            KeyCode::Tab => {
                match self.tab {
                    Tab::Procs => self.switch_disk(),
                    Tab::Disk => self.tab = Tab::Clean,
                    Tab::Clean => self.tab = Tab::Help,
                    Tab::Help => self.tab = Tab::Procs,
                }
                return;
            }
            _ => {}
        }
        match self.tab {
            Tab::Procs => self.key_procs(k),
            Tab::Disk => self.key_disk(k),
            Tab::Clean => self.key_clean(k),
            Tab::Help => {
                if k.code == KeyCode::Esc {
                    self.tab = Tab::Procs;
                }
            }
        }
    }

    fn switch_disk(&mut self) {
        self.tab = Tab::Disk;
        if self.scan.is_none() {
            self.start_scan(macpilot::home());
        }
    }

    fn key_procs(&mut self, k: KeyEvent) {
        let n = self.rows.len();
        match ru_to_en(k.code) {
            KeyCode::Up | KeyCode::Char('k') => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            KeyCode::PageUp => self.sel = self.sel.saturating_sub(20),
            KeyCode::PageDown => self.sel = (self.sel + 20).min(n.saturating_sub(1)),
            KeyCode::Home | KeyCode::Char('g') => self.sel = 0,
            KeyCode::End | KeyCode::Char('G') => self.sel = n.saturating_sub(1),
            KeyCode::Char('/') | KeyCode::Char('f') => self.mode = Mode::Filter,
            KeyCode::Esc => {
                self.filter.clear();
                self.rebuild_rows();
            }
            KeyCode::Char(ch @ ('c' | 'm' | 'p' | 'n')) => {
                let key = match ch {
                    'c' => SortKey::Cpu,
                    'm' => SortKey::Mem,
                    'p' => SortKey::Pid,
                    _ => SortKey::Name,
                };
                if self.sort == key {
                    self.sort_desc = !self.sort_desc;
                } else {
                    self.sort = key;
                    self.sort_desc = key != SortKey::Pid && key != SortKey::Name;
                }
                self.rebuild_rows();
            }
            KeyCode::Char('v') | KeyCode::Char('t') => {
                self.view = match self.view {
                    ProcView::Flat => ProcView::Apps,
                    ProcView::Apps => ProcView::Tree,
                    ProcView::Tree => ProcView::Flat,
                };
                self.sel = 0;
                self.sel_key = None;
                self.rebuild_rows();
            }
            KeyCode::Char('u') => {
                self.only_mine = !self.only_mine;
                self.rebuild_rows();
            }
            KeyCode::Char('x') | KeyCode::Delete => self.ask_stop(false),
            KeyCode::Char('K') => self.ask_stop(true),
            KeyCode::Char('s') => self.toggle_pause(),
            KeyCode::Char('o') => {
                if let Some(e) = self.selected_proc().and_then(|p| p.app.clone().or(p.exe.clone())) {
                    macpilot::trash::reveal_in_finder(&e);
                }
            }
            KeyCode::Char('d') => {
                if let Some(e) = self.selected_proc().and_then(|p| p.app.clone().or(p.exe.clone())) {
                    if let Some(parent) = e.parent().map(|p| p.to_path_buf()) {
                        self.switch_disk();
                        self.disk_view = DiskView::Browse;
                        self.open_dir(parent);
                        if let Some(i) = self.entries.iter().position(|x| x.path == e) {
                            self.dsel = i;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn key_disk(&mut self, k: KeyEvent) {
        let (sel, n) = match self.disk_view {
            DiskView::Browse => (&mut self.dsel, self.entries.len()),
            DiskView::BigFiles => (&mut self.bsel, self.big.len()),
            DiskView::Stale => (&mut self.ssel, self.stale.len()),
        };
        match ru_to_en(k.code) {
            KeyCode::Up | KeyCode::Char('k') => *sel = sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *sel = (*sel + 1).min(n.saturating_sub(1)),
            KeyCode::PageUp => *sel = sel.saturating_sub(20),
            KeyCode::PageDown => *sel = (*sel + 20).min(n.saturating_sub(1)),
            KeyCode::Home | KeyCode::Char('g') => *sel = 0,
            KeyCode::End | KeyCode::Char('G') => *sel = n.saturating_sub(1),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => match self.disk_view {
                DiskView::Browse => {
                    if let Some(e) = self.entries.get(self.dsel) {
                        if e.is_dir && !e.is_link {
                            let p = e.path.clone();
                            self.open_dir(p);
                        }
                    }
                }
                DiskView::BigFiles | DiskView::Stale => {
                    if let Some((p, _)) = self.selected_disk_path() {
                        self.disk_view = DiskView::Browse;
                        if let Some(parent) = p.parent() {
                            self.open_dir(parent.to_path_buf());
                        }
                        if let Some(i) = self.entries.iter().position(|x| x.path == p) {
                            self.dsel = i;
                        }
                    }
                }
            },
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('-') => {
                if self.disk_view != DiskView::Browse {
                    self.disk_view = DiskView::Browse;
                } else if let Some(p) = self.cwd.parent().map(|p| p.to_path_buf()) {
                    self.open_dir(p);
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => self.ask_trash(),
            KeyCode::Char('o') => {
                if let Some((p, _)) = self.selected_disk_path() {
                    macpilot::trash::reveal_in_finder(&p);
                }
            }
            KeyCode::Char('r') => {
                let p = self.cwd.clone();
                self.start_scan(p);
                self.set_status(trf("Scanning {0}…", &[&fmt::path(&self.cwd)]), Level::Info);
            }
            KeyCode::Char('b') | KeyCode::Char('f') => {
                self.disk_view = if self.disk_view == DiskView::BigFiles { DiskView::Browse } else { DiskView::BigFiles };
            }
            KeyCode::Char('a') => {
                if self.disk_view == DiskView::Stale {
                    self.disk_view = DiskView::Browse;
                } else if let Some(scan) = self.scan.as_ref().filter(|s| s.done()) {
                    self.stale = disk::stale_items(scan, self.stale_days * 86_400);
                    self.ssel = 0;
                    self.disk_view = DiskView::Stale;
                } else {
                    self.set_status(tr("Wait for the scan to finish."), Level::Info);
                }
            }
            KeyCode::Char('~') => self.open_dir(macpilot::home()),
            KeyCode::Char('/') | KeyCode::Char(':') => self.mode = Mode::GotoPath(fmt::path(&self.cwd) + "/"),
            KeyCode::Char('i') => {
                if let Some((p, _)) = self.selected_disk_path() {
                    let (s, why) = disk::deletion_safety(&p);
                    self.set_status(format!("{}: [{}] {why}", fmt::path(&p), s.label()), level_of(s));
                }
            }
            _ => {}
        }
    }

    fn key_clean(&mut self, k: KeyEvent) {
        let n = self.targets.len();
        match ru_to_en(k.code) {
            KeyCode::Up | KeyCode::Char('k') => self.csel = self.csel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.csel = (self.csel + 1).min(n.saturating_sub(1)),
            KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Delete => self.ask_clean(),
            KeyCode::Enter | KeyCode::Right => {
                if let Some(p) = self.targets.get(self.csel).map(|t| t.path.clone()) {
                    if p.is_dir() {
                        self.switch_disk();
                        self.disk_view = DiskView::Browse;
                        self.open_dir(p);
                    } else {
                        self.set_status(tr("This folder does not exist — nothing to clean."), Level::Info);
                    }
                }
            }
            KeyCode::Char('o') => {
                if let Some(t) = self.targets.get(self.csel) {
                    macpilot::trash::reveal_in_finder(&t.path);
                }
            }
            KeyCode::Char('r') => {
                for i in 0..n {
                    self.remeasure(i);
                }
            }
            _ => {}
        }
    }
}

pub fn level_of(s: DelSafety) -> Level {
    match s {
        DelSafety::Safe => Level::Ok,
        DelSafety::Careful => Level::Warn,
        DelSafety::Blocked => Level::Danger,
    }
}

fn expand(s: &str) -> PathBuf {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('~') {
        return macpilot::home().join(rest.trim_start_matches('/'));
    }
    PathBuf::from(s)
}

/// Keys also work with the Russian keyboard layout.
fn ru_to_en(c: KeyCode) -> KeyCode {
    let KeyCode::Char(ch) = c else { return c };
    const RU: &str = "йцукенгшщзфывапролдячсмитьЛП";
    const EN: &str = "qwertyuiopasdfghjklzxcvbnmKG";
    match RU.chars().position(|r| r == ch) {
        Some(i) => KeyCode::Char(EN.chars().nth(i).unwrap_or(ch)),
        None => c,
    }
}
