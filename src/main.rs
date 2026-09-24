//! `macpilot` — the terminal app and command-line reports.

mod app;
mod ui;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

use macpilot::i18n::{self, Lang};
use macpilot::settings::Settings;
// `app` and `ui` refer to the core as `crate::…`.
use macpilot::{apps, clean, devjunk, disk, dupes, fmt, procs, startup, tr, trf};
use ratatui::crossterm::event::{self, Event, KeyEventKind};

fn help() -> String {
    format!(
        "MacPilot {}\n\n{}\n  macpilot                 {}\n  macpilot disk [PATH]     {}\n  macpilot clean           {}\n  macpilot stale [DAYS]    {}\n  macpilot junk [DAYS]     {}\n  macpilot apps            {}\n  macpilot leftovers       {}\n  macpilot startup         {}\n  macpilot dupes [PATH]    {}\n\n{}\n  --lang en|fr|es|de|ru    {}\n",
        env!("CARGO_PKG_VERSION"),
        tr("Usage:"),
        tr("interactive terminal UI"),
        tr("open the disk analyzer (home folder by default)"),
        tr("open Cleanup"),
        tr("list what has not been used for DAYS days (default 180)"),
        tr("list build folders of projects untouched for DAYS days (default 30)"),
        tr("list installed apps by size and last use"),
        tr("list leftovers of removed apps"),
        tr("list startup items"),
        tr("find duplicate files"),
        tr("Options:"),
        tr("interface language"),
    )
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let settings = Settings::load();
    if let Some(i) = args.iter().position(|a| a == "--lang") {
        let l = args.get(i + 1).and_then(|c| Lang::from_code(c)).unwrap_or(Lang::En);
        i18n::set_lang(l);
        args.drain(i..(i + 2).min(args.len()));
    } else {
        settings.apply_lang();
    }
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let num = |d: i64| args.get(1).and_then(|s| s.parse().ok()).unwrap_or(d);
    match cmd {
        "-h" | "--help" | "help" => print!("{}", help()),
        "-V" | "--version" => println!("macpilot {}", env!("CARGO_PKG_VERSION")),
        "stale" => report_stale(num(settings.stale_days)),
        "junk" => report_junk(num(settings.junk_days)),
        "apps" => report_apps(),
        "leftovers" => report_leftovers(),
        "startup" => report_startup(),
        "dupes" => report_dupes(args.get(1).map(PathBuf::from).unwrap_or_else(macpilot::home), settings.dupes_min_mb),
        _ => {
            if !std::io::stdout().is_terminal() {
                eprintln!("{}", tr("macpilot is interactive — run it in a terminal, or see `macpilot --help` for reports."));
                std::process::exit(1);
            }
            if let Err(e) = run_tui(&args, &settings) {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
}

fn run_tui(args: &[String], settings: &Settings) -> std::io::Result<()> {
    let mut app = app::App::new(settings.stale_days);
    match args.first().map(String::as_str) {
        Some("disk") => {
            let root = args.get(1).map(PathBuf::from).and_then(|p| p.canonicalize().ok()).unwrap_or_else(macpilot::home);
            app.tab = app::Tab::Disk;
            app.start_scan(root);
        }
        Some("clean") => app.tab = app::Tab::Clean,
        _ => {}
    }
    // ratatui::init installs a panic hook that restores the terminal.
    let mut terminal = ratatui::init();
    let res = (|| -> std::io::Result<()> {
        while !app.quit {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            if event::poll(Duration::from_millis(150))? {
                if let Event::Key(k) = event::read()? {
                    if k.kind == KeyEventKind::Press {
                        app.on_key(k);
                    }
                }
                // Handle queued keys at once so scrolling does not lag.
                while event::poll(Duration::ZERO)? {
                    if let Event::Key(k) = event::read()? {
                        if k.kind == KeyEventKind::Press {
                            app.on_key(k);
                        }
                    }
                }
            }
            app.tick();
        }
        Ok(())
    })();
    ratatui::restore();
    res
}

fn scan_home() -> disk::Scan {
    let root = macpilot::home();
    eprintln!("{}", trf("Scanning {0}…", &[&fmt::path(&root)]));
    let scan = disk::Scan::start(root);
    while !scan.done() {
        std::thread::sleep(Duration::from_millis(200));
    }
    scan
}

fn report_stale(days: i64) {
    let scan = scan_home();
    let items = disk::stale_items(&scan, days * 86_400);
    let total: u64 = items.iter().map(|i| i.size).sum();
    println!("{}\n", trf("Not used for {0}+ days (photos, video and music excluded): {1} items, {2}", &[&days, &items.len(), &fmt::bytes(total)]));
    for i in items.iter().take(80) {
        println!(
            "{:>10}  {:<10} {:<22} {}{}",
            fmt::bytes(i.size),
            i.safety.label(),
            fmt::ago(i.used),
            fmt::path(&i.path),
            if i.is_dir { "/" } else { "" }
        );
    }
}

fn report_junk(days: i64) {
    let scan = scan_home();
    let junk = devjunk::find(&scan);
    let cutoff = disk::now_unix() - days * 86_400;
    let old: u64 = junk.iter().filter(|j| j.project_modified < cutoff).map(|j| j.size).sum();
    let total: u64 = junk.iter().map(|j| j.size).sum();
    println!(
        "{}\n",
        trf("{0} build folders, {1}; in projects untouched for {2}+ days: {3}", &[&junk.len(), &fmt::bytes(total), &days, &fmt::bytes(old)])
    );
    for j in junk.iter().take(80) {
        let mark = if j.project_modified < cutoff { "●" } else { " " };
        println!("{mark} {:>10}  {:<22} {:<22} {}", fmt::bytes(j.size), j.kind, fmt::ago(j.project_modified), fmt::path(&j.path));
    }
}

fn report_apps() {
    let list = apps::list();
    let total: u64 = list.iter().map(|a| a.size).sum();
    println!("{}\n", trf("{0} apps, {1}", &[&list.len(), &fmt::bytes(total)]));
    for a in &list {
        let used = a.last_used.map(fmt::ago).unwrap_or_else(|| "—".into());
        println!("{:>10}  {:<24} {:<30} {}", fmt::bytes(a.size), used, a.name, a.bundle_id);
    }
}

fn report_leftovers() {
    let list = apps::list();
    let orphans = apps::orphans(&list);
    let total: u64 = orphans.iter().map(|o| o.size).sum();
    println!("{}\n", trf("{0} folders of apps that are no longer installed", &[&orphans.len()]) + &format!(", {}", fmt::bytes(total)));
    for o in &orphans {
        println!("{:>10}  {:<40} {}", fmt::bytes(o.size), o.id, fmt::path(&o.path));
    }
}

fn report_startup() {
    let mon = procs::Monitor::new();
    for it in startup::list() {
        let running = !it.program.is_empty() && mon.procs.iter().any(|p| p.exe.as_deref().is_some_and(|e| e.to_string_lossy() == it.program));
        let state = if it.disabled {
            tr("off")
        } else if running {
            tr("running")
        } else {
            tr("on")
        };
        let flags = [(it.unwanted, tr("unwanted")), (it.broken, tr("broken"))]
            .iter()
            .filter(|(b, _)| *b)
            .map(|(_, s)| format!("[{s}]"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("{:<8} {:<50} {:<18} {} {flags}", state, it.label, it.vendor, it.program);
    }
}

fn report_dupes(root: PathBuf, min_mb: u64) {
    eprintln!("{}", trf("Scanning {0}…", &[&fmt::path(&root)]));
    let s = dupes::DupScan::start(root, min_mb * 1_000_000);
    while !s.done() {
        std::thread::sleep(Duration::from_millis(200));
    }
    let groups = s.groups.lock().unwrap().clone();
    let wasted: u64 = groups.iter().map(|g| g.wasted()).sum();
    println!("{}\n", trf("{0} groups of identical files, {1} can be freed", &[&groups.len(), &fmt::bytes(wasted)]));
    for g in groups.iter().take(50) {
        println!("{} × {}", g.files.len(), fmt::bytes(g.size));
        for f in &g.files {
            println!("    {}", fmt::path(&f.path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn dump(t: &ratatui::Terminal<TestBackend>) -> String {
        let b = t.backend().buffer();
        let mut s = String::new();
        for y in 0..b.area.height {
            for x in 0..b.area.width {
                s.push_str(b[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    /// Renders every page with fake key presses (run with `--ignored --nocapture` to look at it).
    #[test]
    #[ignore]
    fn snapshot() {
        i18n::set_lang(std::env::var("MACPILOT_LANG").ok().and_then(|l| Lang::from_code(&l)).unwrap_or(Lang::En));
        let mut t = ratatui::Terminal::new(TestBackend::new(150, 40)).unwrap();
        let mut app = app::App::new(180);
        let key = |app: &mut app::App, c: KeyCode| app.on_key(KeyEvent::new(c, KeyModifiers::NONE));
        std::thread::sleep(Duration::from_millis(1600));
        app.tick();
        for (name, keys) in [
            ("procs", vec![]),
            ("apps view", vec![KeyCode::Char('v')]),
            ("disk", vec![KeyCode::Char('2')]),
            ("clean", vec![KeyCode::Char('3')]),
            ("help", vec![KeyCode::Char('?')]),
        ] {
            for k in keys {
                key(&mut app, k);
            }
            if name == "disk" {
                while !app.scan.as_ref().unwrap().done() {
                    std::thread::sleep(Duration::from_millis(300));
                }
                app.tick();
            }
            t.draw(|f| ui::draw(f, &mut app)).unwrap();
            println!("=== {name}\n{}", dump(&t));
        }
    }
}
