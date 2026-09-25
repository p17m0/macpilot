//! MacPilot — the window app.

mod apps_view;
mod autostart;
mod clean_view;
mod disk_view;
mod mac;
mod overview;
mod procs_view;
mod settings_view;
mod startup_view;
mod widgets;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};
use macpilot::apps::{AppInfo, Orphan};
use macpilot::clean::{self, Target};
use macpilot::devjunk::Junk;
use macpilot::disk::{self, DirStat, Entry, Scan, StaleItem};
use macpilot::dupes::DupScan;
use macpilot::procs::{self, Monitor, Snapshot};
use macpilot::settings::{Settings, Theme};
use macpilot::startup::StartupItem;
use macpilot::{fmt, tr, trf};

use widgets::{C, Level};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Overview,
    Procs,
    Disk,
    Clean,
    Apps,
    Startup,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProcView {
    Apps,
    Flat,
    Tree,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Pid,
    Count,
    Cpu,
    Mem,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DiskMode {
    List,
    Map,
    Big,
    Stale,
    Dupes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CleanMode {
    System,
    Dev,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppsMode {
    Installed,
    Leftovers,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Sel {
    Pid(u32),
    Group(String),
}

pub enum Action {
    Signal { pids: Vec<u32>, sig: i32, admin: bool },
    QuitApp { app: PathBuf, main_pid: Option<u32> },
    Trash { paths: Vec<PathBuf>, size: u64 },
    EmptyTrash,
    Startup { item: StartupItem, on: bool },
}

pub struct Confirm {
    pub title: String,
    pub lines: Vec<(String, Level)>,
    pub action: Action,
    pub button: String,
    pub danger: bool,
    /// Word the user must type to confirm a dangerous action.
    pub require: Option<&'static str>,
    pub input: String,
}

impl Confirm {
    pub fn new(title: impl Into<String>, lines: Vec<(String, Level)>, action: Action, button: impl Into<String>) -> Confirm {
        Confirm { title: title.into(), lines, action, button: button.into(), danger: false, require: None, input: String::new() }
    }
}

pub enum Msg {
    Trashed { paths: Vec<PathBuf>, size: u64, result: Result<(), String> },
    Signalled { result: Result<(), String>, pids: Vec<u32>, sig: i32 },
    Measured(usize, DirStat),
    Apps(Vec<AppInfo>),
    Orphans(Vec<Orphan>),
    Leftovers(PathBuf, Vec<(PathBuf, u64)>),
    Startup(Vec<StartupItem>),
    StartupChanged(Result<(), String>),
    TrashEmptied(Result<(), String>),
    Update(Result<Option<macpilot::update::Release>, String>),
}

pub struct Gui {
    pub page: Page,
    pub settings: Settings,
    pub snap: Arc<Snapshot>,
    shared_snap: Arc<Mutex<Arc<Snapshot>>>,
    pub ports: Arc<Mutex<HashMap<u32, Vec<String>>>>,
    pub cpu_hist: VecDeque<f32>,
    pub mem_hist: VecDeque<f32>,
    last_hist: Instant,
    pub disks: sysinfo::Disks,
    last_disks: Instant,
    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub ctx: egui::Context,
    pub confirm: Option<Confirm>,
    pub toast: Option<(String, Level, Instant)>,
    pub pending: Vec<(u32, String, Instant)>,

    // Processes
    pub view: ProcView,
    pub sort: SortKey,
    pub sort_desc: bool,
    pub filter: String,
    pub only_mine: bool,
    pub sel: Option<Sel>,
    pub focus_search: bool,

    // Disk
    pub scan: Option<Scan>,
    /// Fresh scan running in the background while `scan` shows cached results.
    pub rescan: Option<Scan>,
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub entries_err: Option<String>,
    pub last_list: Instant,
    pub disk_sel: Option<PathBuf>,
    pub disk_mode: DiskMode,
    pub big: Vec<(u64, PathBuf)>,
    pub path_input: String,
    pub stale_days: i64,
    pub stale: Vec<StaleItem>,
    pub stale_dirty: bool,
    pub stale_checked: HashSet<PathBuf>,
    pub dupes: Option<Arc<DupScan>>,
    /// For each duplicate group (keyed by its first path): the copy to keep.
    pub dupes_keep: HashMap<PathBuf, PathBuf>,
    /// Duplicate groups (by first path) the user chose to clean up. Nothing is selected by default.
    pub dupes_selected: HashSet<PathBuf>,

    // Cleanup
    pub clean_mode: CleanMode,
    pub targets: Vec<Target>,
    pub junk: Vec<Junk>,
    pub junk_dirty: bool,
    pub junk_checked: HashSet<PathBuf>,

    // Apps
    pub apps_mode: AppsMode,
    pub apps: Option<Vec<AppInfo>>,
    pub orphans: Option<Vec<Orphan>>,
    pub orphans_checked: HashSet<PathBuf>,
    pub app_sel: Option<PathBuf>,
    pub leftovers: HashMap<PathBuf, Vec<(PathBuf, u64)>>,
    pub leftover_checked: HashSet<PathBuf>,
    pub apps_filter: String,

    // Startup
    pub startup: Option<Vec<StartupItem>>,

    pub autostart: bool,
    pub full_disk_access: bool,
    /// Menu bar item: when it was last updated.
    last_status: Option<Instant>,
    /// Update check: a newer release, the last error, and when it last ran.
    pub update: Option<macpilot::update::Release>,
    pub update_error: Option<String>,
    pub update_checking: bool,
    update_checked: Option<Instant>,
    started: Instant,
    /// Scan progress watchdog: macOS blocks file access while a permission dialog is open.
    scan_files_seen: u64,
    scan_last_progress: Instant,
    pub scan_stalled: bool,
    #[allow(dead_code)]
    shot: Option<Shot>,
}

/// Developer screenshot mode: MACPILOT_SHOT=path.png (feature "dev-tools").
#[allow(dead_code)]
struct Shot {
    path: PathBuf,
    started: Instant,
    requested: bool,
    sent: bool,
}

impl Gui {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        match std::env::var("MACPILOT_LANG").ok().and_then(|l| macpilot::i18n::Lang::from_code(&l)) {
            Some(l) => macpilot::i18n::set_lang(l),
            None => settings.apply_lang(),
        }
        widgets::setup_style(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx, settings.theme);
        let ctx = cc.egui_ctx.clone();

        // Processes are collected in the background so the window never stutters.
        let first = Monitor::new();
        let shared_snap = Arc::new(Mutex::new(Arc::new(first.snap.clone())));
        {
            let shared = shared_snap.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let mut mon = first;
                loop {
                    std::thread::sleep(Duration::from_millis(1500));
                    mon.refresh();
                    *shared.lock().unwrap() = Arc::new(mon.snap.clone());
                    ctx.request_repaint();
                }
            });
        }
        // Listening ports, refreshed every 10 seconds.
        let ports = Arc::new(Mutex::new(HashMap::new()));
        {
            let ports = ports.clone();
            std::thread::spawn(move || {
                macpilot::background_qos();
                loop {
                    let p = procs::listening_ports();
                    *ports.lock().unwrap() = p;
                    std::thread::sleep(Duration::from_secs(10));
                }
            });
        }

        let (tx, rx) = channel();
        let targets = clean::targets();
        {
            let (mtx, mrx) = channel();
            clean::measure_all(&targets, mtx);
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                for (i, st) in mrx {
                    let _ = tx.send(Msg::Measured(i, st));
                    ctx.request_repaint();
                }
            });
        }
        // Startup items, apps and leftovers of removed apps — in the background.
        {
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                macpilot::background_qos();
                let _ = tx.send(Msg::Startup(macpilot::startup::list()));
                ctx.request_repaint();
                let apps = macpilot::apps::list();
                let orphans = macpilot::apps::orphans(&apps);
                let _ = tx.send(Msg::Apps(apps));
                let _ = tx.send(Msg::Orphans(orphans));
                ctx.request_repaint();
            });
        }

        let home = macpilot::home();
        let snap = shared_snap.lock().unwrap().clone();
        let stale_days = settings.stale_days;
        let mut g = Gui {
            page: Page::Overview,
            snap,
            shared_snap,
            ports,
            cpu_hist: VecDeque::new(),
            mem_hist: VecDeque::new(),
            last_hist: Instant::now() - Duration::from_secs(5),
            disks: sysinfo::Disks::new_with_refreshed_list(),
            last_disks: Instant::now(),
            tx,
            rx,
            ctx,
            confirm: None,
            toast: None,
            pending: Vec::new(),
            view: ProcView::Apps,
            sort: SortKey::Mem,
            sort_desc: true,
            filter: String::new(),
            only_mine: false,
            sel: None,
            focus_search: false,
            scan: None,
            rescan: None,
            cwd: home.clone(),
            entries: Vec::new(),
            entries_err: None,
            last_list: Instant::now() - Duration::from_secs(60),
            disk_sel: None,
            disk_mode: DiskMode::List,
            big: Vec::new(),
            path_input: fmt::path(&home),
            stale_days,
            stale: Vec::new(),
            stale_dirty: true,
            stale_checked: HashSet::new(),
            dupes: None,
            dupes_keep: HashMap::new(),
            dupes_selected: HashSet::new(),
            clean_mode: CleanMode::System,
            targets,
            junk: Vec::new(),
            junk_dirty: true,
            junk_checked: HashSet::new(),
            apps_mode: AppsMode::Installed,
            apps: None,
            orphans: None,
            orphans_checked: HashSet::new(),
            app_sel: None,
            leftovers: HashMap::new(),
            leftover_checked: HashSet::new(),
            apps_filter: String::new(),
            startup: None,
            autostart: {
                autostart::migrate();
                autostart::enabled()
            },
            last_status: None,
            update: None,
            update_error: None,
            update_checking: false,
            update_checked: None,
            started: Instant::now(),
            full_disk_access: macpilot::has_full_disk_access(),
            scan_files_seen: 0,
            scan_last_progress: Instant::now(),
            scan_stalled: false,
            shot: None,
            settings,
        };
        if g.settings.scan_on_start {
            g.start_home_scan();
        }
        g.dev_setup();
        g
    }

    /// Developer options for automated UI checks: MACPILOT_PAGE=disk:map etc.
    fn dev_setup(&mut self) {
        if let Ok(p) = std::env::var("MACPILOT_SHOT") {
            self.shot = Some(Shot { path: PathBuf::from(p), started: Instant::now(), requested: false, sent: false });
        }
        match std::env::var("MACPILOT_THEME").as_deref() {
            Ok("dark") => apply_theme(&self.ctx, Theme::Dark),
            Ok("light") => apply_theme(&self.ctx, Theme::Light),
            _ => {}
        }
        let Ok(page) = std::env::var("MACPILOT_PAGE") else { return };
        let (page, sub) = page.split_once(':').unwrap_or((page.as_str(), ""));
        let p = match page {
            "procs" => Page::Procs,
            "disk" => Page::Disk,
            "clean" => Page::Clean,
            "apps" => Page::Apps,
            "startup" => Page::Startup,
            "settings" => Page::Settings,
            _ => Page::Overview,
        };
        self.go_page(p);
        match sub {
            "map" => self.disk_mode = DiskMode::Map,
            "big" => self.disk_mode = DiskMode::Big,
            "stale" => self.disk_mode = DiskMode::Stale,
            "dupes" => {
                self.disk_mode = DiskMode::Dupes;
                self.start_dupes();
            }
            "dev" => self.clean_mode = CleanMode::Dev,
            "leftovers" => self.apps_mode = AppsMode::Leftovers,
            "flat" => self.view = ProcView::Flat,
            _ => {}
        }
    }

    pub fn toast(&mut self, s: impl Into<String>, l: Level) {
        self.toast = Some((s.into(), l, Instant::now()));
    }

    pub fn go_page(&mut self, p: Page) {
        self.page = p;
        if matches!(p, Page::Disk | Page::Clean | Page::Overview) && self.scan.is_none() {
            self.start_home_scan();
        }
    }

    pub fn save_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.toast(trf("Could not save settings: {0}", &[&e]), Level::Danger);
        }
    }

    /// Re-translate everything that stores translated text.
    pub fn language_changed(&mut self) {
        self.settings.apply_lang();
        let old: HashMap<&str, DirStat> = self.targets.iter().filter_map(|t| t.stat.map(|s| (t.id, s))).collect();
        let mut fresh = clean::targets();
        for t in &mut fresh {
            t.stat = old.get(t.id).copied();
        }
        self.targets = fresh;
        self.stale_dirty = true;
        self.junk_dirty = true;
        self.save_settings();
    }

    fn logic(&mut self, ctx: &egui::Context) {
        let latest = self.shared_snap.lock().unwrap().clone();
        if !Arc::ptr_eq(&latest, &self.snap) {
            self.snap = latest;
            self.check_pending();
        }
        if self.last_hist.elapsed() >= Duration::from_millis(1500) {
            self.last_hist = Instant::now();
            push_hist(&mut self.cpu_hist, self.snap.cpu_total);
            let m = self.snap.mem_used as f32 / self.snap.mem_total.max(1) as f32 * 100.0;
            push_hist(&mut self.mem_hist, m);
        }
        if self.last_disks.elapsed() >= Duration::from_secs(5) {
            self.disks.refresh(true);
            self.last_disks = Instant::now();
        }
        while let Ok(m) = self.rx.try_recv() {
            self.on_msg(m);
        }
        let scanning = self.scan.as_ref().is_some_and(|s| !s.done());
        if let Some(s) = self.scan.as_ref().filter(|_| scanning) {
            let files = s.shared.files.load(std::sync::atomic::Ordering::Relaxed);
            if files != self.scan_files_seen {
                self.scan_files_seen = files;
                self.scan_last_progress = Instant::now();
            }
            let stalled = self.scan_last_progress.elapsed() > Duration::from_secs(5);
            if stalled && !self.scan_stalled {
                // Show partial results now; they are rebuilt when the scan finishes.
                self.stale_dirty = true;
                self.junk_dirty = true;
            }
            self.scan_stalled = stalled;
        } else {
            self.scan_stalled = false;
            self.scan_last_progress = Instant::now();
        }
        if self.page == Page::Disk {
            let period = if scanning { 600 } else { 5000 };
            if self.last_list.elapsed() >= Duration::from_millis(period) {
                self.reload_dir();
            }
        }
        if scanning || self.dupes.as_ref().is_some_and(|d| !d.done()) {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        if let Some(s) = &mut self.scan {
            if s.done() && s.finished_in.is_none() {
                s.finished_in = Some(s.started.elapsed());
                if s.root == macpilot::home() {
                    s.save_cache();
                }
                self.stale_dirty = true;
                self.junk_dirty = true;
                self.reload_dir();
            }
        }
        // The fresh scan replaces the cached results once it is complete.
        if self.rescan.as_ref().is_some_and(|s| s.done()) {
            if let Some(mut fresh) = self.rescan.take() {
                fresh.finished_in = Some(fresh.started.elapsed());
                fresh.save_cache();
                self.scan = Some(fresh);
                self.stale_dirty = true;
                self.junk_dirty = true;
                self.reload_dir();
            }
        }
        if self.rescan.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        self.refresh_stale();
        self.refresh_junk();
        if let Some((_, _, t)) = &self.toast {
            if t.elapsed() > Duration::from_secs(10) {
                self.toast = None;
            }
        }
        self.update_menu_bar();
        // Once a day (the app may run for weeks in the menu bar); the first check a little after launch.
        let due = match self.update_checked {
            None => self.started.elapsed() > Duration::from_secs(20),
            Some(t) => t.elapsed() > Duration::from_secs(24 * 3600),
        };
        if due && self.settings.check_updates && macpilot::update::repo().is_some() {
            self.check_updates();
        }
        ctx.request_repaint_after(Duration::from_millis(1000));
    }

    fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Update(r) => {
                self.update_checking = false;
                match r {
                    Ok(u) => {
                        self.update = u;
                        self.update_error = None;
                    }
                    Err(e) => self.update_error = Some(e),
                }
            }
            Msg::Measured(i, st) => {
                if let Some(t) = self.targets.get_mut(i) {
                    t.stat = Some(st);
                }
            }
            Msg::Apps(mut a) => {
                // Keep an app that was dropped on the window before the list was ready.
                if let Some(sel) = self.app_sel.as_ref().filter(|p| p.is_dir() && !a.iter().any(|x| &x.path == *p)) {
                    a.insert(0, macpilot::apps::info(sel, None));
                }
                if let Some(x) = self.app_sel.as_ref().and_then(|p| a.iter().find(|x| &x.path == p)).cloned() {
                    self.load_leftovers(&x);
                }
                self.apps = Some(a);
            }
            Msg::Orphans(o) => self.orphans = Some(o),
            Msg::Leftovers(app, l) => {
                // Everything found is pre-selected, except very large folders (games, documents).
                for (p, size) in &l {
                    if *size < 1_000_000_000 {
                        self.leftover_checked.insert(p.clone());
                    }
                }
                self.leftovers.insert(app, l);
            }
            Msg::Startup(s) => self.startup = Some(s),
            Msg::StartupChanged(r) => {
                match r {
                    Ok(()) => self.toast(tr("Startup item updated."), Level::Ok),
                    Err(e) => self.toast(trf("Could not change the startup item: {0}", &[&e]), Level::Danger),
                }
                self.reload_startup();
            }
            Msg::TrashEmptied(r) => {
                match r {
                    Ok(()) => self.toast(tr("The Trash is empty."), Level::Ok),
                    Err(e) => self.toast(trf("Could not empty the Trash: {0}", &[&e]), Level::Danger),
                }
                self.remeasure_by_id("trash");
            }
            Msg::Signalled { result, pids, sig } => match result {
                Ok(()) => {
                    if sig == libc::SIGTERM {
                        for p in &pids {
                            let name = self.snap.get(*p).map(|x| x.name.clone()).unwrap_or_default();
                            self.pending.push((*p, name, Instant::now()));
                        }
                    }
                    let n = pids.len();
                    let msg = if sig == libc::SIGKILL {
                        trf("Force quit: {0}.", &[&fmt::n(n as u64, fmt::Noun::Process)])
                    } else {
                        trf("Asked to quit: {0}.", &[&fmt::n(n as u64, fmt::Noun::Process)])
                    };
                    self.toast(msg, Level::Ok);
                }
                Err(e) => self.toast(trf("Failed: {0}", &[&e]), Level::Danger),
            },
            Msg::Trashed { paths, size, result } => {
                match result {
                    Ok(()) => {
                        let what = if paths.len() == 1 { fmt::path(&paths[0]) } else { fmt::n(paths.len() as u64, fmt::Noun::Item) };
                        self.toast(
                            trf("Moved to the Trash: {0}. {1} will be freed when you empty the Trash.", &[&what, &fmt::bytes(size)]),
                            Level::Ok,
                        );
                    }
                    Err(e) => self.toast(trf("Some items could not be moved to the Trash: {0}", &[&e]), Level::Danger),
                }
                self.after_trash(&paths);
            }
        }
    }

    /// Update every list after items went to the Trash.
    fn after_trash(&mut self, paths: &[PathBuf]) {
        let gone: Vec<PathBuf> = paths.iter().filter(|p| std::fs::symlink_metadata(p).is_err()).cloned().collect();
        for scan in self.scan.iter().chain(self.rescan.iter()) {
            for p in &gone {
                let st = scan.size_of(p).unwrap_or_default();
                scan.forget(p, st.size, st.files);
            }
        }
        let is_gone = |p: &PathBuf| gone.iter().any(|g| p.starts_with(g));
        self.stale.retain(|i| !is_gone(&i.path));
        self.junk.retain(|j| !is_gone(&j.path));
        if let Some(o) = &mut self.orphans {
            o.retain(|x| !is_gone(&x.path));
        }
        if let Some(a) = &mut self.apps {
            a.retain(|x| !is_gone(&x.path));
        }
        for l in self.leftovers.values_mut() {
            l.retain(|(p, _)| !is_gone(p));
        }
        if let Some(d) = &self.dupes {
            d.remove_paths(&gone);
        }
        if self.disk_sel.as_ref().is_some_and(is_gone) {
            self.disk_sel = None;
        }
        if self.app_sel.as_ref().is_some_and(is_gone) {
            self.app_sel = None;
        }
        for set in [&mut self.stale_checked, &mut self.junk_checked, &mut self.orphans_checked, &mut self.leftover_checked] {
            set.retain(|p| !is_gone(p));
        }
        let to_measure: Vec<usize> =
            self.targets.iter().enumerate().filter(|(_, t)| gone.iter().any(|g| g.starts_with(&t.path))).map(|(i, _)| i).collect();
        for i in to_measure {
            self.remeasure(i);
        }
        self.remeasure_by_id("trash");
        self.reload_dir();
    }

    pub fn remeasure(&mut self, i: usize) {
        let Some(t) = self.targets.get_mut(i) else { return };
        t.stat = None;
        let p = t.path.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Measured(i, disk::measure(&p)));
            ctx.request_repaint();
        });
    }

    pub fn remeasure_by_id(&mut self, id: &str) {
        if let Some(i) = self.targets.iter().position(|t| t.id == id) {
            self.remeasure(i);
        }
    }

    pub fn reload_startup(&mut self) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Startup(macpilot::startup::list()));
            ctx.request_repaint();
        });
    }

    pub fn load_leftovers(&mut self, app: &AppInfo) {
        if self.leftovers.contains_key(&app.path) {
            return;
        }
        self.leftovers.insert(app.path.clone(), Vec::new());
        let app = app.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let l = macpilot::apps::leftovers(&app);
            let _ = tx.send(Msg::Leftovers(app.path.clone(), l));
            ctx.request_repaint();
        });
    }

    fn check_pending(&mut self) {
        let mut still = Vec::new();
        let mut msg = None;
        for (pid, name, t) in std::mem::take(&mut self.pending) {
            if !self.snap.alive(pid) {
                msg = Some((trf("“{0}” (PID {1}) has quit.", &[&name, &pid]), Level::Ok));
            } else if t.elapsed() > Duration::from_secs(8) {
                msg = Some((
                    trf("“{0}” did not quit within 8 s — it may be waiting for an answer in a dialog. You can force quit it.", &[&name]),
                    Level::Warn,
                ));
            } else {
                still.push((pid, name, t));
            }
        }
        self.pending = still;
        if let Some((m, l)) = msg {
            self.toast(m, l);
        }
    }

    pub fn execute(&mut self, a: Action) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        match a {
            Action::Signal { pids, sig, admin } => {
                std::thread::spawn(move || {
                    let result = if admin {
                        procs::send_signal_admin(&pids, sig)
                    } else {
                        let errs: Vec<String> =
                            pids.iter().filter_map(|p| procs::send_signal(*p, sig).err().map(|e| format!("PID {p}: {e}"))).collect();
                        if errs.is_empty() { Ok(()) } else { Err(errs.join("; ")) }
                    };
                    let _ = tx.send(Msg::Signalled { result, pids, sig });
                    ctx.request_repaint();
                });
            }
            Action::QuitApp { app, main_pid } => {
                procs::quit_app(&app);
                let name = procs::bundle_name(&app);
                if let Some(p) = main_pid {
                    self.pending.push((p, name.clone(), Instant::now()));
                }
                self.toast(trf("Asked “{0}” to quit. If it asks about saving, answer in the app.", &[&name]), Level::Info);
            }
            Action::Trash { paths, size } => {
                self.toast(tr("Moving to the Trash…"), Level::Info);
                std::thread::spawn(move || {
                    let result = macpilot::trash::move_to_trash(&paths);
                    let _ = tx.send(Msg::Trashed { paths, size, result });
                    ctx.request_repaint();
                });
            }
            Action::EmptyTrash => {
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::TrashEmptied(macpilot::trash::empty_trash()));
                    ctx.request_repaint();
                });
            }
            Action::Startup { item, on } => {
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::StartupChanged(macpilot::startup::set_enabled(&item, on)));
                    ctx.request_repaint();
                });
            }
        }
    }

    // ------------------------------------------------------------------
    // Disk
    // ------------------------------------------------------------------

    /// Drop an app on the window to uninstall it (like AppCleaner), or a file or folder to find it on the disk.
    fn handle_drop(&mut self, ctx: &egui::Context) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering {
            let rect = ctx.content_rect();
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            p.rect_filled(rect, 0, Color32::from_black_alpha(150));
            let inner = rect.shrink(24.0);
            p.rect_stroke(inner, 16, egui::Stroke::new(3.0, widgets::C::ACCENT), egui::StrokeKind::Inside);
            p.text(inner.center(), egui::Align2::CENTER_CENTER, tr("Drop an app to uninstall it"), egui::FontId::proportional(26.0), Color32::WHITE);
            p.text(
                inner.center() + egui::vec2(0.0, 36.0),
                egui::Align2::CENTER_CENTER,
                tr("or a file or folder to see it on the disk"),
                egui::FontId::proportional(15.0),
                Color32::from_gray(220),
            );
        }
        let Some(path) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty())) else {
            return;
        };
        let is_app = path.extension().is_some_and(|x| x.eq_ignore_ascii_case("app")) && path.is_dir();
        if is_app {
            self.go_page(Page::Apps);
            self.apps_mode = AppsMode::Installed;
            // An app outside the Applications folders (Downloads, a disk image…) is added to the list.
            if let Some(apps) = &mut self.apps {
                if !apps.iter().any(|a| a.path == path) {
                    apps.insert(0, macpilot::apps::info(&path, None));
                }
                if let Some(a) = apps.iter().find(|a| a.path == path).cloned() {
                    self.load_leftovers(&a);
                }
            }
            self.app_sel = Some(path);
        } else if let Some(parent) = path.parent() {
            self.go_page(Page::Disk);
            self.disk_mode = DiskMode::List;
            self.go(parent.to_path_buf());
            self.disk_sel = Some(path);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    pub fn check_updates(&mut self) {
        if self.update_checking {
            return;
        }
        self.update_checking = true;
        self.update_checked = Some(Instant::now());
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Update(macpilot::update::check()));
            ctx.request_repaint();
        });
    }

    /// CPU and memory in the menu bar, refreshed every two seconds.
    fn update_menu_bar(&mut self) {
        if !self.settings.menu_bar {
            if self.last_status.take().is_some() {
                mac::set_status(None);
            }
            return;
        }
        if self.last_status.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
            return;
        }
        self.last_status = Some(Instant::now());
        let s = &self.snap;
        let mem_r = s.mem_used as f32 / s.mem_total.max(1) as f32;
        let mut lines =
            vec![trf("CPU: {0}", &[&fmt::pct(s.cpu_total)]), trf("Memory: {0} of {1}", &[&fmt::bytes(s.mem_used), &fmt::bytes(s.mem_total)])];
        if let Some((avail, total)) = widgets::data_volume(&self.disks) {
            lines.push(trf("Disk: {0} free of {1}", &[&fmt::bytes(avail), &fmt::bytes(total)]));
        }
        if let Some(p) = s.procs.iter().filter(|p| p.cpu >= 20.0).max_by(|a, b| a.cpu.total_cmp(&b.cpu)) {
            lines.push(trf("Busiest: {0} — {1} CPU", &[&p.app_name().unwrap_or_else(|| p.name.clone()), &fmt::pct(p.cpu)]));
        } else {
            lines.push(tr("Nothing is using much CPU").to_string());
        }
        let title = format!("{:.0}% · {:.0}%", s.cpu_total, mem_r * 100.0);
        mac::set_status(Some(mac::StatusInfo { title: &title, lines: &lines, open_label: tr("Open MacPilot"), quit_label: tr("Quit MacPilot") }));
    }

    /// Home folder: show the last saved results at once and refresh them in the background.
    pub fn start_home_scan(&mut self) {
        let home = macpilot::home();
        match Scan::load_cache(&home) {
            Some(cached) => {
                self.scan = Some(cached);
                self.rescan = Some(Scan::start(home.clone()));
                self.stale_dirty = true;
                self.junk_dirty = true;
                self.go(home);
            }
            None => self.start_scan(home),
        }
    }

    pub fn start_scan(&mut self, root: PathBuf) {
        self.rescan = None;
        self.scan = Some(Scan::start(root.clone()));
        self.big.clear();
        self.stale.clear();
        self.junk.clear();
        self.stale_dirty = true;
        self.junk_dirty = true;
        self.go(root);
    }

    pub fn start_dupes(&mut self) {
        let root = self.scan.as_ref().map(|s| s.root.clone()).unwrap_or_else(macpilot::home);
        self.dupes_keep.clear();
        self.dupes_selected.clear();
        self.dupes = Some(DupScan::start(root, self.settings.dupes_min_mb * 1_000_000));
    }

    pub fn go(&mut self, p: PathBuf) {
        let from = self.cwd.clone();
        self.cwd = p;
        self.path_input = fmt::path(&self.cwd);
        self.reload_dir();
        self.disk_sel = if self.entries.iter().any(|e| e.path == from) { Some(from) } else { None };
    }

    pub fn reload_dir(&mut self) {
        self.last_list = Instant::now();
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
        if let Some(s) = &self.scan {
            self.big = s.big_files();
        }
    }

    /// Results can be built once the scan is done — or from the finished part while it waits for a permission dialog.
    pub fn scan_ready(&self) -> bool {
        self.scan.as_ref().is_some_and(|s| s.done()) || self.scan_stalled
    }

    pub fn refresh_stale(&mut self) {
        if !self.scan_ready() {
            return;
        }
        let Some(scan) = &self.scan else { return };
        if !self.stale_dirty {
            return;
        }
        self.stale = disk::stale_items(scan, self.stale_days * 86_400);
        if scan.cached_at.is_some() {
            // Cached results may name things removed since.
            self.stale.retain(|i| std::fs::symlink_metadata(&i.path).is_ok());
        }
        let alive: HashSet<PathBuf> = self.stale.iter().map(|i| i.path.clone()).collect();
        self.stale_checked.retain(|p| alive.contains(p));
        self.stale_dirty = false;
    }

    pub fn refresh_junk(&mut self) {
        if !self.scan_ready() {
            return;
        }
        let Some(scan) = &self.scan else { return };
        if !self.junk_dirty {
            return;
        }
        self.junk = macpilot::devjunk::find(scan);
        if scan.cached_at.is_some() {
            self.junk.retain(|j| std::fs::symlink_metadata(&j.path).is_ok());
        }
        let cutoff = disk::now_unix() - self.settings.junk_days * 86_400;
        self.junk_checked = self.junk.iter().filter(|j| j.project_modified < cutoff).map(|j| j.path.clone()).collect();
        self.junk_dirty = false;
    }

    pub fn dir_total(&self) -> u64 {
        self.scan.as_ref().and_then(|s| s.dir(&self.cwd)).map(|d| d.size).unwrap_or_else(|| self.entries.iter().filter_map(|e| e.size).sum())
    }

    #[cfg(feature = "dev-tools")]
    fn handle_shot(&mut self, ctx: &egui::Context) {
        let Some((started, requested, path)) = self.shot.as_ref().map(|s| (s.started, s.requested, s.path.clone())) else { return };
        let busy = self.scan.as_ref().is_some_and(|s| !s.done())
            || self.rescan.is_some()
            || self.dupes.as_ref().is_some_and(|d| !d.done())
            || self.apps.is_none()
            || self.orphans.is_none()
            || self.startup.is_none()
            || self.targets.iter().any(|t| t.stat.is_none() && t.path.exists());
        if std::env::var("MACPILOT_DEBUG").is_ok() && busy {
            let waiting: Vec<&str> = self.targets.iter().filter(|t| t.stat.is_none() && t.path.exists()).map(|t| t.id).collect();
            eprintln!(
                "busy: scan={} files={} dupes={} apps={} orphans={} startup={} targets={waiting:?}",
                self.scan.as_ref().is_some_and(|s| !s.done()),
                self.scan.as_ref().map(|s| s.shared.files.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(0),
                self.dupes.as_ref().is_some_and(|d| !d.done()),
                self.apps.is_none(),
                self.orphans.is_none(),
                self.startup.is_none()
            );
        }
        // Permission dialogs can stall the scan; do not wait forever.
        // MACPILOT_SHOT_EARLY=1 captures without waiting (to see loading states).
        let busy = busy && started.elapsed() < Duration::from_secs(30) && std::env::var("MACPILOT_SHOT_EARLY").is_err();
        if started.elapsed() > Duration::from_secs(4) && !busy && !requested {
            if std::env::var("MACPILOT_SELECT").is_ok() {
                overview::dev_select(self);
            }
            if let Some(s) = &mut self.shot {
                s.requested = true;
                s.started = Instant::now();
            }
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        // Give the selection a moment to render, then capture once.
        let sent = self.shot.as_ref().is_some_and(|s| s.sent);
        if requested && !sent && started.elapsed() > Duration::from_millis(800) {
            if let Some(s) = &mut self.shot {
                s.sent = true;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let img =
            ctx.input(|i| i.raw.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if let Some(img) = img {
            let [w, h] = img.size;
            let mut buf = image::RgbaImage::new(w as u32, h as u32);
            for (i, px) in img.pixels.iter().enumerate() {
                buf.put_pixel((i % w) as u32, (i / w) as u32, image::Rgba(px.to_array()));
            }
            let _ = buf.save(&path);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    #[cfg(not(feature = "dev-tools"))]
    fn handle_shot(&mut self, _ctx: &egui::Context) {}
}

pub fn apply_theme(ctx: &egui::Context, t: Theme) {
    ctx.set_theme(match t {
        Theme::System => egui::ThemePreference::System,
        Theme::Light => egui::ThemePreference::Light,
        Theme::Dark => egui::ThemePreference::Dark,
    });
}

fn push_hist(h: &mut VecDeque<f32>, v: f32) {
    h.push_back(v);
    while h.len() > 60 {
        h.pop_front();
    }
}

impl eframe::App for Gui {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        Gui::logic(self, ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_shot(&ctx);

        self.handle_drop(&ctx);

        // With the menu bar item on, closing the window keeps MacPilot running there (⌘Q quits).
        if self.settings.menu_bar && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            mac::hide_window();
        }

        // ⌘1…⌘7 switch pages, ⌘F searches processes, ⌘, opens settings.
        let pages = [Page::Overview, Page::Procs, Page::Disk, Page::Clean, Page::Apps, Page::Startup, Page::Settings];
        let keys = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7];
        let mut go = None;
        let mut search = false;
        ctx.input(|i| {
            if i.modifiers.command {
                for (k, p) in keys.iter().zip(pages) {
                    if i.key_pressed(*k) {
                        go = Some(p);
                    }
                }
                if i.key_pressed(egui::Key::Comma) {
                    go = Some(Page::Settings);
                }
                if i.key_pressed(egui::Key::F) {
                    go = Some(Page::Procs);
                    search = true;
                }
            }
        });
        if let Some(p) = go {
            self.go_page(p);
            self.focus_search |= search;
        }

        egui::Panel::left("nav")
            .exact_size(214.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(C::bg_bar(ui)).inner_margin(egui::Margin::symmetric(12, 14)))
            .show(ui, |ui| self.sidebar(ui));

        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(C::bg_bar(ui)).inner_margin(egui::Margin::symmetric(14, 6)))
            .show(ui, |ui| self.status_bar(ui));

        match self.page {
            Page::Overview => overview::show(self, ui),
            Page::Procs => procs_view::show(self, ui),
            Page::Disk => disk_view::show(self, ui),
            Page::Clean => clean_view::show(self, ui),
            Page::Apps => apps_view::show(self, ui),
            Page::Startup => startup_view::show(self, ui),
            Page::Settings => settings_view::show(self, ui),
        }

        self.confirm_modal(&ctx);
    }
}

impl Gui {
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            widgets::app_logo(ui, 26.0);
            ui.label(RichText::new("MacPilot").size(18.0).strong());
        });
        ui.add_space(16.0);
        let items = [
            (Page::Overview, widgets::Icon::Overview, tr("Overview")),
            (Page::Procs, widgets::Icon::Procs, tr("Processes")),
            (Page::Disk, widgets::Icon::Disk, tr("Disk")),
            (Page::Clean, widgets::Icon::Clean, tr("Cleanup")),
            (Page::Apps, widgets::Icon::Apps, tr("Apps")),
            (Page::Startup, widgets::Icon::Startup, tr("Startup")),
            (Page::Settings, widgets::Icon::Settings, tr("Settings")),
        ];
        for (p, icon, label) in items {
            let badge = match p {
                Page::Startup => self.startup.as_ref().map(|s| s.iter().filter(|i| i.unwanted && !i.disabled).count()).filter(|n| *n > 0),
                Page::Settings => self.update.as_ref().map(|_| 1),
                _ => None,
            };
            if widgets::nav_item(ui, self.page == p, icon, label, badge).clicked() {
                self.go_page(p);
            }
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            let s = self.snap.clone();
            if let Some((avail, total)) = widgets::data_volume(&self.disks) {
                let r = total.saturating_sub(avail) as f32 / total.max(1) as f32;
                widgets::side_meter(ui, tr("Disk"), &trf("{0} free", &[&fmt::bytes(avail)]), r, None);
            }
            let mem_r = s.mem_used as f32 / s.mem_total.max(1) as f32;
            widgets::side_meter(ui, tr("Memory"), &format!("{} / {}", fmt::bytes(s.mem_used), fmt::bytes(s.mem_total)), mem_r, Some(&self.mem_hist));
            widgets::side_meter(ui, "CPU", &format!("{:.0}%", s.cpu_total), s.cpu_total / 100.0, Some(&self.cpu_hist));
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.scan_stalled && self.toast.is_none() {
                ui.label(
                    RichText::new(tr("The scan is waiting for macOS: answer the permission dialog, or give MacPilot Full Disk Access in Settings."))
                        .color(C::YELLOW),
                );
            } else {
                match &self.toast {
                    Some((s, l, _)) => {
                        ui.label(RichText::new(s).color(l.color(ui)));
                    }
                    None => {
                        let hint = match self.page {
                            Page::Overview => tr("MacPilot checks your Mac in the background. Nothing is removed without your confirmation."),
                            Page::Procs => tr("Select a process to see what it is. Right-click for actions. ⌘F to search."),
                            Page::Disk => tr("Double-click to open a folder. Removal always goes to the Trash."),
                            Page::Clean => tr("Cleanup moves items to the Trash — everything can be restored."),
                            Page::Apps => tr("Uninstall removes the app and the files it left in your Library."),
                            Page::Startup => tr("Disabling is reversible: the item is kept and can be turned back on."),
                            Page::Settings => tr("Settings are saved automatically."),
                        };
                        ui.label(RichText::new(hint).color(C::dim(ui)));
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let s = &self.snap;
                if s.is_root() {
                    ui.label(RichText::new("root").color(C::RED));
                }
                ui.label(
                    RichText::new(trf(
                        "{0} · on for {1}",
                        &[&fmt::n(s.procs.len() as u64, fmt::Noun::Process), &fmt::duration(sysinfo::System::uptime())],
                    ))
                    .color(C::dim(ui)),
                );
            });
        });
    }

    fn confirm_modal(&mut self, ctx: &egui::Context) {
        let Some(c) = &mut self.confirm else { return };
        let mut decision: Option<bool> = None;
        let resp = egui::Modal::new(egui::Id::new("confirm")).show(ctx, |ui| {
            ui.set_width(540.0);
            ui.add_space(4.0);
            let title_color = if c.danger { C::RED } else { C::ACCENT };
            ui.label(RichText::new(&c.title).size(18.0).strong().color(title_color));
            ui.add_space(8.0);
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for (s, l) in &c.lines {
                    ui.label(RichText::new(s).color(l.color(ui)));
                }
            });
            ui.add_space(10.0);
            let ok_enabled = match c.require {
                Some(word) => {
                    ui.horizontal(|ui| {
                        ui.label(tr("To confirm, type"));
                        ui.label(RichText::new(word).strong().color(C::RED));
                    });
                    let r = ui.add(egui::TextEdit::singleline(&mut c.input).hint_text(word).desired_width(200.0));
                    r.request_focus();
                    c.input.trim().eq_ignore_ascii_case(word)
                }
                None => true,
            };
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let fill = if c.danger { C::RED } else { C::ACCENT };
                    let btn = egui::Button::new(RichText::new(&c.button).strong().color(Color32::WHITE)).fill(fill).min_size(egui::vec2(120.0, 30.0));
                    if ui.add_enabled(ok_enabled, btn).clicked() {
                        decision = Some(true);
                    }
                    if ui.add(egui::Button::new(tr("Cancel")).min_size(egui::vec2(90.0, 30.0))).clicked() {
                        decision = Some(false);
                    }
                });
            });
            if ok_enabled && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                decision = Some(true);
            }
        });
        if resp.should_close() && decision.is_none() {
            decision = Some(false);
        }
        match decision {
            Some(true) => {
                let c = self.confirm.take().unwrap();
                self.execute(c.action);
            }
            Some(false) => self.confirm = None,
            None => {}
        }
    }
}

fn main() -> eframe::Result {
    // `MacPilot --autostart on|off|status` toggles or shows launch at login without opening a window.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--autostart") {
        if args.get(i + 1).map(String::as_str) == Some("status") {
            let how = if mac::login_item_supported() { "login item" } else { "LaunchAgent" };
            let state = if autostart::needs_approval() {
                "needs approval in System Settings"
            } else if autostart::enabled() {
                "on"
            } else {
                "off"
            };
            println!("Launch at login ({how}): {state}");
            return Ok(());
        }
        let on = args.get(i + 1).map(String::as_str) != Some("off");
        match autostart::set(on) {
            Ok(()) => println!("Launch at login {}", if on { "enabled" } else { "disabled" }),
            Err(e) => eprintln!("Error: {e}"),
        }
        return Ok(());
    }
    let vp = egui::ViewportBuilder::default().with_title("MacPilot").with_inner_size([1320.0, 840.0]).with_min_inner_size([1060.0, 640.0]);
    let opts = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native("MacPilot", opts, Box::new(|cc| Ok(Box::new(Gui::new(cc)))))
}
