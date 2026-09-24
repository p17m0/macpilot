//! Terminal UI rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Clear, Gauge, Paragraph, Row, Table, TableState, Tabs, Wrap};

use crate::app::{App, DiskView, Level, Mode, ProcRow, ProcView, SortKey, Tab};
use crate::disk::{self, DelSafety};
use crate::procs::{self, Safety};
use crate::{clean, fmt, tr, trf};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const SELECTED: Color = Color::Rgb(40, 60, 80);

fn lvl_color(l: Level) -> Color {
    match l {
        Level::Info => Color::Reset,
        Level::Ok => Color::Green,
        Level::Warn => Color::Yellow,
        Level::Danger => Color::Red,
    }
}

fn safety_color(s: Safety) -> Color {
    match s {
        Safety::User => Color::Green,
        Safety::System => Color::Yellow,
        Safety::Critical => Color::Red,
    }
}

fn del_color(s: DelSafety) -> Color {
    match s {
        DelSafety::Safe => Color::Green,
        DelSafety::Careful => Color::Yellow,
        DelSafety::Blocked => Color::Red,
    }
}

fn size_color(s: u64) -> Color {
    if s >= 10_000_000_000 {
        Color::Red
    } else if s >= 1_000_000_000 {
        Color::Yellow
    } else if s >= 100_000_000 {
        Color::Reset
    } else {
        DIM
    }
}

fn block(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::new().fg(DIM)).title(title)
}

fn kv(k: &str, v: impl Into<String>) -> Line<'static> {
    Line::from(vec![Span::styled(format!("{k:<14}"), Style::new().fg(DIM)), Span::raw(v.into())])
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let [top, sys, main, keys, status] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(5), Constraint::Length(1), Constraint::Length(1)])
            .areas(f.area());
    draw_tabs(f, app, top);
    draw_sysline(f, app, sys);
    match app.tab {
        Tab::Procs => draw_procs(f, app, main),
        Tab::Disk => draw_disk(f, app, main),
        Tab::Clean => draw_clean(f, app, main),
        Tab::Help => draw_help(f, main),
    }
    draw_keys(f, app, keys);
    draw_status(f, app, status);
    if let Mode::Confirm(c) = &app.mode {
        draw_confirm(f, c);
    }
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let idx = match app.tab {
        Tab::Procs => 0,
        Tab::Disk => 1,
        Tab::Clean => 2,
        Tab::Help => 3,
    };
    let [l, r] = Layout::horizontal([Constraint::Min(10), Constraint::Length(30)]).areas(area);
    let titles =
        vec![format!(" 1 {} ", tr("Processes")), format!(" 2 {} ", tr("Disk")), format!(" 3 {} ", tr("Cleanup")), format!(" ? {} ", tr("Help"))];
    let tabs = Tabs::new(titles)
        .select(idx)
        .style(Style::new().fg(DIM))
        .highlight_style(Style::new().fg(Color::Black).bg(ACCENT).bold())
        .divider("")
        .padding("", " ");
    f.render_widget(Line::from(Span::styled(" macpilot ", Style::new().bold().fg(ACCENT))), l);
    let [_, tabs_area] = Layout::horizontal([Constraint::Length(10), Constraint::Min(10)]).areas(l);
    f.render_widget(tabs, tabs_area);
    let root = if app.mon.is_root() { Span::styled(" root ", Style::new().fg(Color::Black).bg(Color::Red)) } else { Span::raw("") };
    let host = sysinfo::System::host_name().unwrap_or_default();
    f.render_widget(Line::from(vec![root, Span::styled(format!(" {host} "), Style::new().fg(DIM))]).right_aligned(), r);
}

fn data_volume(app: &App) -> Option<(u64, u64)> {
    let l = app.disks.list();
    l.iter()
        .find(|d| d.mount_point() == std::path::Path::new("/System/Volumes/Data"))
        .or_else(|| l.iter().find(|d| d.mount_point() == std::path::Path::new("/")))
        .map(|d| (d.available_space(), d.total_space()))
}

fn draw_sysline(f: &mut Frame, app: &App, area: Rect) {
    let m = &app.mon;
    let mem_pct = m.mem_used() as f64 / m.mem_total().max(1) as f64 * 100.0;
    let la = sysinfo::System::load_average();
    let cpu = m.cpu_total();
    let col = |v: f64, warn: f64, bad: f64| {
        if v >= bad {
            Color::Red
        } else if v >= warn {
            Color::Yellow
        } else {
            Color::Green
        }
    };
    let mut spans = vec![
        Span::styled(" CPU ", Style::new().fg(DIM)),
        Span::styled(format!("{cpu:5.1}%"), Style::new().fg(col(cpu as f64, 60.0, 85.0)).bold()),
        Span::styled(format!("  {} ", tr("Memory")), Style::new().fg(DIM)),
        Span::styled(format!("{} / {}", fmt::bytes(m.mem_used()), fmt::bytes(m.mem_total())), Style::new().fg(col(mem_pct, 75.0, 90.0)).bold()),
        Span::styled("  Swap ", Style::new().fg(DIM)),
        Span::styled(fmt::bytes(m.swap_used()), Style::new().fg(if m.swap_used() > 4_000_000_000 { Color::Yellow } else { Color::Reset })),
        Span::styled(format!("  {} {:.1} {:.1} {:.1}", tr("Load"), la.one, la.five, la.fifteen), Style::new().fg(DIM)),
        Span::styled(format!("  {}", trf("up {0}", &[&fmt::duration(sysinfo::System::uptime())])), Style::new().fg(DIM)),
    ];
    if let Some((avail, total)) = data_volume(app) {
        let pct = avail as f64 / total.max(1) as f64 * 100.0;
        spans.push(Span::styled(format!("  {} ", tr("Disk")), Style::new().fg(DIM)));
        spans.push(Span::styled(
            trf("{0} free", &[&fmt::bytes(avail)]),
            Style::new()
                .fg(if pct < 5.0 {
                    Color::Red
                } else if pct < 15.0 {
                    Color::Yellow
                } else {
                    Color::Green
                })
                .bold(),
        ));
    }
    f.render_widget(Line::from(spans), area);
}

fn draw_keys(f: &mut Frame, app: &App, area: Rect) {
    let keys: Vec<(&str, &str)> = match (&app.mode, app.tab) {
        (Mode::Filter, _) => vec![("type", tr("filter")), ("Enter", tr("done")), ("Esc", tr("clear"))],
        (Mode::GotoPath(_), _) => vec![("type", tr("path")), ("Enter", tr("go")), ("Esc", tr("cancel"))],
        (Mode::Confirm(c), _) if c.require.is_some() => vec![("Enter", tr("confirm")), ("Esc", tr("cancel"))],
        (Mode::Confirm(_), _) => vec![("y/Enter", tr("confirm")), ("n/Esc", tr("cancel"))],
        (_, Tab::Procs) => vec![
            ("↑↓", tr("select")),
            ("/", tr("search")),
            ("v", tr("view")),
            ("c m p n", tr("sort")),
            ("u", tr("only mine")),
            ("x", tr("stop")),
            ("K", tr("force quit")),
            ("s", tr("pause")),
            ("o", "Finder"),
            ("q", tr("quit")),
        ],
        (_, Tab::Disk) => vec![
            ("↑↓", tr("select")),
            ("Enter", tr("open")),
            ("←", tr("back")),
            ("d", tr("to Trash")),
            ("i", tr("safe to remove?")),
            ("b", tr("large files")),
            ("a", tr("not used")),
            ("r", tr("rescan")),
            ("/", tr("path")),
        ],
        (_, Tab::Clean) => {
            vec![("↑↓", tr("select")), ("d", tr("clean")), ("Enter", tr("open on disk")), ("r", tr("measure again")), ("q", tr("quit"))]
        }
        (_, Tab::Help) => vec![("1 2 3", tr("pages")), ("Esc", tr("back")), ("q", tr("quit"))],
    };
    let mut spans = vec![Span::raw(" ")];
    for (k, d) in keys {
        spans.push(Span::styled(k, Style::new().fg(Color::Black).bg(DIM)));
        spans.push(Span::styled(format!(" {d}  "), Style::new().fg(DIM)));
    }
    f.render_widget(Line::from(spans), area);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let line = match (&app.mode, &app.status) {
        (Mode::Filter, _) => Line::from(vec![
            Span::styled(format!(" {}: ", tr("Search")), Style::new().fg(ACCENT).bold()),
            Span::raw(app.filter.clone()),
            Span::raw("▏"),
        ]),
        (Mode::GotoPath(s), _) => {
            Line::from(vec![Span::styled(format!(" {}: ", tr("Go to")), Style::new().fg(ACCENT).bold()), Span::raw(s.clone()), Span::raw("▏")])
        }
        (_, Some((s, l, _))) => Line::from(Span::styled(format!(" {s}"), Style::new().fg(lvl_color(*l)))),
        _ => Line::from(Span::styled(format!(" {}", tr("Tab — next page, ? — help")), Style::new().fg(DIM))),
    };
    f.render_widget(line, area);
}

// ---------------------------------------------------------------------------
// Processes
// ---------------------------------------------------------------------------

fn cpu_color(c: f32) -> Color {
    if c >= 80.0 {
        Color::Red
    } else if c >= 25.0 {
        Color::Yellow
    } else if c >= 1.0 {
        Color::Reset
    } else {
        DIM
    }
}

fn mem_color(m: u64) -> Color {
    if m >= 2_000_000_000 {
        Color::Red
    } else if m >= 500_000_000 {
        Color::Yellow
    } else if m >= 50_000_000 {
        Color::Reset
    } else {
        DIM
    }
}

fn draw_procs(f: &mut Frame, app: &mut App, area: Rect) {
    let (table_area, detail_area) = if area.width >= 130 {
        let [a, b] = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).areas(area);
        (a, b)
    } else {
        let [a, b] = Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);
        (a, b)
    };
    let arrow = |k: SortKey| if app.sort == k { if app.sort_desc { "▼" } else { "▲" } } else { "" };
    let view_name = match app.view {
        ProcView::Flat => tr("list"),
        ProcView::Tree => tr("tree"),
        ProcView::Apps => tr("by app"),
    };
    let mut title = vec![
        Span::styled(format!(" {} ", tr("Processes")), Style::new().bold()),
        Span::styled(format!("[{view_name}] "), Style::new().fg(ACCENT)),
        Span::styled(format!("{} ", app.rows.len()), Style::new().fg(DIM)),
    ];
    if app.only_mine {
        title.push(Span::styled(format!("[{}] ", tr("only mine")), Style::new().fg(Color::Magenta)));
    }
    if !app.filter.is_empty() {
        title.push(Span::styled(format!("[{}: {}] ", tr("search"), app.filter), Style::new().fg(Color::Magenta)));
    }
    let header_style = Style::new().fg(ACCENT).bold();
    let (header, widths, rows): (Row, Vec<Constraint>, Vec<Row>) = if app.view == ProcView::Apps {
        let header = Row::new(vec![
            format!("{}{}", tr("App"), arrow(SortKey::Name)),
            format!("{}{}", tr("Procs"), arrow(SortKey::Pid)),
            format!("CPU%{}", arrow(SortKey::Cpu)),
            format!("{}{}", tr("Memory"), arrow(SortKey::Mem)),
            tr("Safety").to_string(),
        ])
        .style(header_style);
        let rows = app
            .rows
            .iter()
            .filter_map(|r| match r {
                ProcRow::Group(g) => Some(Row::new(vec![
                    Cell::from(if g.app.is_some() { format!("◆ {}", g.label) } else { g.label.clone() }),
                    Cell::from(g.pids.len().to_string()).style(Style::new().fg(if g.pids.len() >= 200 { Color::Red } else { DIM })),
                    Cell::from(format!("{:6.1}", g.cpu)).style(Style::new().fg(cpu_color(g.cpu))),
                    Cell::from(fmt::bytes(g.mem)).style(Style::new().fg(mem_color(g.mem))),
                    Cell::from(g.safety.label()).style(Style::new().fg(safety_color(g.safety))),
                ])),
                _ => None,
            })
            .collect();
        (header, vec![Constraint::Min(20), Constraint::Length(7), Constraint::Length(8), Constraint::Length(10), Constraint::Length(10)], rows)
    } else {
        let header = Row::new(vec![
            format!("PID{}", arrow(SortKey::Pid)),
            format!("{}{}", tr("Name"), arrow(SortKey::Name)),
            tr("User").to_string(),
            format!("CPU%{}", arrow(SortKey::Cpu)),
            format!("{}{}", tr("Memory"), arrow(SortKey::Mem)),
            tr("Status").to_string(),
            tr("Safety").to_string(),
        ])
        .style(header_style);
        let rows = app
            .rows
            .iter()
            .filter_map(|r| match r {
                ProcRow::Proc { pid, depth } => app.mon.get(*pid).map(|p| (p, *depth)),
                _ => None,
            })
            .map(|(p, depth)| {
                let indent = if depth > 0 { format!("{}└ ", "  ".repeat(depth.saturating_sub(1).min(12))) } else { String::new() };
                let status_style = if p.stopped { Style::new().fg(Color::Magenta).bold() } else { Style::new().fg(DIM) };
                Row::new(vec![
                    Cell::from(p.pid.to_string()).style(Style::new().fg(DIM)),
                    Cell::from(format!("{indent}{}", p.name)),
                    Cell::from(p.user.clone()).style(Style::new().fg(DIM)),
                    Cell::from(format!("{:6.1}", p.cpu)).style(Style::new().fg(cpu_color(p.cpu))),
                    Cell::from(fmt::bytes(p.mem)).style(Style::new().fg(mem_color(p.mem))),
                    Cell::from(p.status).style(status_style),
                    Cell::from(p.safety.label()).style(Style::new().fg(safety_color(p.safety))),
                ])
            })
            .collect();
        (
            header,
            vec![
                Constraint::Length(7),
                Constraint::Min(18),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
            rows,
        )
    };
    let table = Table::new(rows, widths)
        .header(header)
        .block(block(Line::from(title)))
        .row_highlight_style(Style::new().bg(SELECTED).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    let mut state = TableState::default().with_selected(if app.rows.is_empty() { None } else { Some(app.sel) });
    f.render_stateful_widget(table, table_area, &mut state);
    draw_proc_detail(f, app, detail_area);
}

fn draw_proc_detail(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    if let Some(g) = app.selected_group() {
        lines.push(Line::from(Span::styled(g.label.clone(), Style::new().bold().fg(ACCENT))));
        if let Some(a) = &g.app {
            lines.push(kv(tr("Location"), fmt::path(a)));
        }
        lines.push(kv(tr("Processes"), g.pids.len().to_string()));
        lines.push(kv("CPU", format!("{:.1}%", g.cpu)));
        lines.push(kv(tr("Memory"), fmt::bytes(g.mem)));
        lines.push(Line::from(Span::styled(g.safety.explain(), Style::new().fg(safety_color(g.safety)))));
        lines.push(Line::raw(""));
        let mut members: Vec<_> = g.pids.iter().filter_map(|p| app.mon.get(*p)).collect();
        members.sort_by_key(|a| std::cmp::Reverse(a.mem));
        for p in members.iter().take(30) {
            lines.push(Line::from(vec![
                Span::styled(format!("{:>6} ", p.pid), Style::new().fg(DIM)),
                Span::styled(format!("{:>9} ", fmt::bytes(p.mem)), Style::new().fg(mem_color(p.mem))),
                Span::styled(format!("{:5.1}% ", p.cpu), Style::new().fg(cpu_color(p.cpu))),
                Span::raw(p.name.clone()),
            ]));
        }
    } else if let Some(p) = app.selected_proc() {
        lines.push(Line::from(vec![
            Span::styled(p.name.clone(), Style::new().bold().fg(ACCENT)),
            Span::styled(format!("  PID {}", p.pid), Style::new().fg(DIM)),
        ]));
        lines.push(Line::from(Span::styled(procs::describe(p), Style::new().fg(Color::White))));
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(format!("{:<14}", tr("Safety")), Style::new().fg(DIM)),
            Span::styled(p.safety.label(), Style::new().fg(safety_color(p.safety)).bold()),
        ]));
        lines.push(Line::from(Span::styled(p.safety.explain(), Style::new().fg(safety_color(p.safety)))));
        lines.push(Line::raw(""));
        lines.push(kv(tr("User"), p.user.clone()));
        let parent = p.ppid.map(|pp| format!("{pp} ({})", app.mon.get(pp).map(|x| x.name.as_str()).unwrap_or("?"))).unwrap_or("—".into());
        lines.push(kv(tr("Parent"), parent));
        lines.push(kv(tr("Status"), p.status));
        lines.push(kv(tr("Running for"), fmt::duration(p.run_time)));
        lines.push(kv("CPU", format!("{:.1}%", p.cpu)));
        lines.push(kv(tr("Memory"), fmt::bytes(p.mem)));
        lines.push(kv(tr("Program"), p.exe.as_ref().map(|e| fmt::path(e)).unwrap_or(tr("no access").into())));
        if !p.cmd.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(tr("Command line"), Style::new().fg(DIM))));
            lines.push(Line::raw(p.cmd.clone()));
        }
    }
    f.render_widget(Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).block(block(format!(" {} ", tr("Details")))), area);
}

// ---------------------------------------------------------------------------
// Disk
// ---------------------------------------------------------------------------

fn draw_disk(f: &mut Frame, app: &mut App, area: Rect) {
    let [head, body] = Layout::vertical([Constraint::Length(4), Constraint::Min(3)]).areas(area);
    let [g, s] = Layout::vertical([Constraint::Length(3), Constraint::Length(1)]).areas(head);
    if let Some((avail, total)) = data_volume(app) {
        let used = total.saturating_sub(avail);
        let ratio = used as f64 / total.max(1) as f64;
        let color = if ratio > 0.95 {
            Color::Red
        } else if ratio > 0.85 {
            Color::Yellow
        } else {
            Color::Green
        };
        let gauge = Gauge::default()
            .block(block(format!(" {} ", tr("Startup disk"))))
            .gauge_style(Style::new().fg(color).bg(Color::Rgb(30, 30, 30)))
            .ratio(ratio.clamp(0.0, 1.0))
            .label(format!("{} · {}", trf("{0} of {1} used", &[&fmt::bytes(used), &fmt::bytes(total)]), trf("{0} free", &[&fmt::bytes(avail)])));
        f.render_widget(gauge, g);
    }
    let line = match &app.scan {
        Some(sc) => {
            use std::sync::atomic::Ordering::Relaxed;
            let (files, bytes, errs) = (sc.shared.files.load(Relaxed), sc.shared.bytes.load(Relaxed), sc.shared.errors.load(Relaxed));
            let mut v = if let Some(t) = sc.finished_in {
                vec![Span::styled(
                    format!(
                        " ✔ {}",
                        trf(
                            "Scanned {0}: {1} in {2} files in {3} s",
                            &[&fmt::path(&sc.root), &fmt::bytes(bytes), &fmt::count(files), &format!("{:.1}", t.as_secs_f32())]
                        )
                    ),
                    Style::new().fg(Color::Green),
                )]
            } else {
                vec![Span::styled(
                    format!(" ⠿ {}", trf("Scanning {0}… {1} files, {2}", &[&fmt::path(&sc.root), &fmt::count(files), &fmt::bytes(bytes)])),
                    Style::new().fg(Color::Yellow),
                )]
            };
            if errs > 0 {
                v.push(Span::styled(format!("  {}", trf("· no access to {0} folders", &[&fmt::count(errs)])), Style::new().fg(DIM)));
            }
            Line::from(v)
        }
        None => Line::raw(format!(" {}", tr("Not scanned yet"))),
    };
    f.render_widget(line, s);

    let (list, info) = if body.width >= 120 {
        let [a, b] = Layout::horizontal([Constraint::Percentage(68), Constraint::Percentage(32)]).areas(body);
        (a, Some(b))
    } else {
        (body, None)
    };
    match app.disk_view {
        DiskView::Browse => draw_browse(f, app, list),
        DiskView::BigFiles => {
            draw_paths(f, list, &format!(" {} ", tr("Large files")), app.big.iter().map(|(s, p)| (*s, p.clone(), String::new())).collect(), app.bsel)
        }
        DiskView::Stale => {
            let total: u64 = app.stale.iter().map(|i| i.size).sum();
            let title = format!(" {} · {} ", trf("Not used for {0}+ days", &[&app.stale_days]), fmt::bytes(total));
            let rows = app.stale.iter().map(|i| (i.size, i.path.clone(), fmt::age(i.used))).collect();
            draw_paths(f, list, &title, rows, app.ssel)
        }
    }
    if let Some(info) = info {
        draw_disk_info(f, app, info);
    }
}

fn draw_browse(f: &mut Frame, app: &mut App, area: Rect) {
    let total = app.dir_total().map(|d| d.size).unwrap_or_else(|| app.entries.iter().filter_map(|e| e.size).sum());
    let title = Line::from(vec![
        Span::styled(format!(" {} ", fmt::path(&app.cwd)), Style::new().bold()),
        Span::styled(format!("{} · {} ", fmt::bytes(total), trf("{0} items", &[&app.entries.len()])), Style::new().fg(DIM)),
    ]);
    if let Some(e) = &app.entries_err {
        f.render_widget(Paragraph::new(e.clone()).wrap(Wrap { trim: true }).fg(Color::Red).block(block(title)), area);
        return;
    }
    let bar_w = 16usize;
    let rows: Vec<Row> = app
        .entries
        .iter()
        .map(|e| {
            let (sz, frac) = match e.size {
                Some(s) => (fmt::bytes(s), if total > 0 { s as f64 / total as f64 } else { 0.0 }),
                None => ("…".into(), 0.0),
            };
            let filled = ((frac * bar_w as f64).round() as usize).min(bar_w);
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(bar_w - filled));
            let (safety, _) = disk::deletion_safety(&e.path);
            let icon = if e.is_link {
                "↪ "
            } else if e.is_dir {
                "▸ "
            } else {
                "  "
            };
            let changed = e.mtime.filter(|t| *t > 0).map(fmt::ago).unwrap_or_default();
            Row::new(vec![
                Cell::from(format!("{sz:>9}")).style(Style::new().fg(size_color(e.size.unwrap_or(0)))),
                Cell::from(bar).style(Style::new().fg(size_color(e.size.unwrap_or(0)))),
                Cell::from(format!("{icon}{}", e.name)).style(if e.is_dir { Style::new().fg(Color::Blue).bold() } else { Style::new() }),
                Cell::from(changed).style(Style::new().fg(DIM)),
                Cell::from(safety.label()).style(Style::new().fg(del_color(safety))),
            ])
        })
        .collect();
    let header = Row::new(vec![tr("Size"), "", tr("Name"), tr("Modified"), tr("Removal")]).style(Style::new().fg(ACCENT).bold());
    let table = Table::new(
        rows,
        [Constraint::Length(9), Constraint::Length(bar_w as u16), Constraint::Min(20), Constraint::Length(18), Constraint::Length(11)],
    )
    .header(header)
    .block(block(title))
    .row_highlight_style(Style::new().bg(SELECTED).add_modifier(Modifier::BOLD))
    .highlight_symbol("▶ ");
    let mut st = TableState::default().with_selected(if app.entries.is_empty() { None } else { Some(app.dsel) });
    f.render_stateful_widget(table, area, &mut st);
}

fn draw_paths(f: &mut Frame, area: Rect, title: &str, items: Vec<(u64, std::path::PathBuf, String)>, sel: usize) {
    let rows: Vec<Row> = items
        .iter()
        .map(|(s, p, note)| {
            let (safety, _) = disk::deletion_safety(p);
            Row::new(vec![
                Cell::from(format!("{:>9}", fmt::bytes(*s))).style(Style::new().fg(size_color(*s))),
                Cell::from(fmt::path(p)),
                Cell::from(note.clone()).style(Style::new().fg(Color::Yellow)),
                Cell::from(safety.label()).style(Style::new().fg(del_color(safety))),
            ])
        })
        .collect();
    let table = Table::new(rows, [Constraint::Length(9), Constraint::Min(30), Constraint::Length(18), Constraint::Length(11)])
        .header(Row::new(vec![tr("Size"), tr("Path"), "", tr("Removal")]).style(Style::new().fg(ACCENT).bold()))
        .block(block(title.to_string()))
        .row_highlight_style(Style::new().bg(SELECTED).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    let mut st = TableState::default().with_selected(if items.is_empty() { None } else { Some(sel) });
    f.render_stateful_widget(table, area, &mut st);
}

fn draw_disk_info(f: &mut Frame, app: &App, area: Rect) {
    let sel = match app.disk_view {
        DiskView::Browse => app.entries.get(app.dsel).map(|e| (e.path.clone(), e.size, e.is_dir)),
        DiskView::BigFiles => app.big.get(app.bsel).map(|(s, p)| (p.clone(), Some(*s), false)),
        DiskView::Stale => app.stale.get(app.ssel).map(|i| (i.path.clone(), Some(i.size), i.is_dir)),
    };
    let mut lines = Vec::new();
    if let Some((path, size, is_dir)) = sel {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        lines.push(Line::from(Span::styled(name, Style::new().bold().fg(ACCENT))));
        lines.push(Line::from(Span::styled(fmt::path(&path), Style::new().fg(DIM))));
        lines.push(Line::raw(""));
        lines.push(kv(tr("Size"), size.map(fmt::bytes).unwrap_or(tr("measuring…").into())));
        let st = if is_dir { app.scan.as_ref().and_then(|s| s.dir(&path)) } else { None };
        let md = std::fs::symlink_metadata(&path).ok();
        let (mt, used) = match (st, &md) {
            (Some(st), _) => (Some(st.modified), Some(st.used)),
            (None, Some(m)) => (Some(std::os::unix::fs::MetadataExt::mtime(m)), Some(disk::used_time(m))),
            _ => (None, None),
        };
        if let Some(t) = mt.filter(|t| *t > 0) {
            lines.push(kv(tr("Modified"), format!("{} ({})", fmt::date(t), fmt::ago(t))));
        }
        if let Some(t) = used.filter(|t| *t > 0) {
            lines.push(kv(tr("Opened"), format!("{} ({})", fmt::date(t), fmt::ago(t))));
            if disk::now_unix() - t >= 180 * 86_400 && !disk::is_media(&path) {
                lines.push(Line::from(Span::styled(trf("Not used for {0}", &[&fmt::age_in(t)]), Style::new().fg(Color::Yellow))));
            }
        }
        let (s, why) = disk::deletion_safety(&path);
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(format!("{:<14}", tr("Removal")), Style::new().fg(DIM)),
            Span::styled(s.label(), Style::new().fg(del_color(s)).bold()),
        ]));
        lines.push(Line::from(Span::styled(why, Style::new().fg(del_color(s)))));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(tr("Removal always goes to the Trash."), Style::new().fg(DIM))));
    }
    f.render_widget(Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).block(block(format!(" {} ", tr("Selected")))), area);
}

// ---------------------------------------------------------------------------
// Cleanup and help
// ---------------------------------------------------------------------------

fn draw_clean(f: &mut Frame, app: &mut App, area: Rect) {
    let [list, info] = Layout::vertical([Constraint::Min(5), Constraint::Length(5)]).areas(area);
    let total: u64 = app.targets.iter().filter(|t| t.cleanable()).filter_map(|t| t.stat.map(|s| s.size)).sum();
    let rows: Vec<Row> = app
        .targets
        .iter()
        .map(|t| {
            let exists = t.path.exists();
            let size = match (exists, t.stat) {
                (false, _) => tr("none").to_string(),
                (true, Some(s)) => fmt::bytes(s.size),
                (true, None) => "…".to_string(),
            };
            let s = clean::target_safety(t);
            let action = if t.cleanable() {
                Span::styled(format!("d — {}", tr("clean")), Style::new().fg(del_color(s)))
            } else {
                Span::styled(tr("by hand"), Style::new().fg(Color::Yellow))
            };
            Row::new(vec![
                Cell::from(format!("{size:>9}")).style(Style::new().fg(if exists { size_color(t.stat.map(|s| s.size).unwrap_or(0)) } else { DIM })),
                Cell::from(t.label),
                Cell::from(fmt::path(&t.path)).style(Style::new().fg(DIM)),
                Cell::from(action),
            ])
        })
        .collect();
    let title = Line::from(vec![
        Span::styled(format!(" {} ", tr("Cleanup")), Style::new().bold()),
        Span::styled(format!("· {} ", trf("{0} can be freed", &[&fmt::bytes(total)])), Style::new().fg(Color::Green)),
    ]);
    let table = Table::new(rows, [Constraint::Length(9), Constraint::Length(30), Constraint::Min(20), Constraint::Length(16)])
        .block(block(title))
        .row_highlight_style(Style::new().bg(SELECTED).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    let mut st = TableState::default().with_selected(Some(app.csel));
    f.render_stateful_widget(table, list, &mut st);
    if let Some(t) = app.targets.get(app.csel) {
        let p = Paragraph::new(vec![Line::from(Span::styled(t.label, Style::new().bold().fg(ACCENT))), Line::raw(t.hint)])
            .wrap(Wrap { trim: true })
            .block(block(""));
        f.render_widget(p, info);
    }
}

fn draw_help(f: &mut Frame, area: Rect) {
    let h = |s: &'static str| Line::from(Span::styled(s, Style::new().bold().fg(ACCENT)));
    let t = |s: &'static str| Line::raw(format!("  {s}"));
    let lines = vec![
        h(tr("Processes (1)")),
        t(tr("v — view: list → by app → tree.  / — search.  c/m/p/n — sort by CPU/memory/PID/name.  u — only mine")),
        t(tr("x — stop gently (apps quit like ⌘Q).  K — force quit.  s — pause/resume.  o — show in Finder")),
        t(tr("Critical macOS processes cannot be stopped; system ones need you to type “yes”.")),
        Line::raw(""),
        h(tr("Disk (2)")),
        t(tr("Enter/→ — open, ←/⌫ — back, / — go to path, r — scan this folder, b — large files, a — not used lately")),
        t(tr("d — move to the Trash (never deleted permanently).  i — explain whether it is safe to remove")),
        Line::raw(""),
        h(tr("Cleanup (3)")),
        t(tr("Well-known junk places with sizes. d — move the contents to the Trash.")),
        Line::raw(""),
        h(tr("Tips")),
        t(tr("The window app (MacPilot.app) also has an overview, an uninstaller, duplicates, developer junk and startup items.")),
        t(tr("Give your terminal Full Disk Access to see every folder. Keys also work with the Russian layout.")),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block(format!(" {} ", tr("Help")))), area);
}

fn draw_confirm(f: &mut Frame, c: &crate::app::Confirm) {
    let area = f.area();
    let w = area.width.saturating_sub(4).min(90);
    let inner_w = w.saturating_sub(4).max(10) as usize;
    let est: usize = c.lines.iter().map(|(s, _)| s.chars().count() / inner_w + 1).sum::<usize>() + 2;
    let h = (est as u16 + 2).min(area.height.saturating_sub(2));
    let rect = Rect { x: area.x + (area.width - w) / 2, y: area.y + area.height.saturating_sub(h) / 2, width: w, height: h };
    f.render_widget(Clear, rect);
    let danger = c.require.is_some() || c.lines.iter().any(|(_, l)| *l == Level::Danger);
    let border = if danger { Color::Red } else { ACCENT };
    let mut lines: Vec<Line> = c.lines.iter().map(|(s, l)| Line::from(Span::styled(s.clone(), Style::new().fg(lvl_color(*l))))).collect();
    lines.push(Line::raw(""));
    match c.require {
        Some(word) => lines.push(Line::from(vec![
            Span::raw(format!("{} ", tr("To confirm, type"))),
            Span::styled(word, Style::new().bold().fg(Color::Red)),
            Span::raw(": "),
            Span::styled(c.input.clone(), Style::new().bold()),
            Span::raw("▏"),
        ])),
        None => lines.push(Line::from(vec![
            Span::styled(" y ", Style::new().fg(Color::Black).bg(Color::Green)),
            Span::raw(format!(" {}   ", tr("yes"))),
            Span::styled(" n ", Style::new().fg(Color::Black).bg(DIM)),
            Span::raw(format!(" {}", tr("no"))),
        ])),
    }
    let p = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::new().fg(border))
            .title(Span::styled(format!(" {} ", c.title), Style::new().bold().fg(border))),
    );
    f.render_widget(p, rect);
}
