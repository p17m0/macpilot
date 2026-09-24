//! Processes page.

use std::collections::HashSet;
use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use macpilot::procs::{self, AppGroup, ProcInfo, Safety, Snapshot};
use macpilot::{fmt, tr, trf};

use crate::widgets::{self as w, C, Level};
use crate::{Action, Confirm, DiskMode, Gui, Page, ProcView, Sel, SortKey};

enum Row {
    Proc { pid: u32, depth: usize },
    Group(AppGroup),
}

fn matches(g: &Gui, p: &ProcInfo) -> bool {
    if g.only_mine && p.uid != Some(g.snap.my_uid) {
        return false;
    }
    if g.filter.is_empty() {
        return true;
    }
    let f = g.filter.to_lowercase();
    p.name.to_lowercase().contains(&f)
        || p.pid.to_string() == f
        || p.app_name().is_some_and(|a| a.to_lowercase().contains(&f))
        || p.cmd.to_lowercase().contains(&f)
}

fn cmp_procs(key: SortKey, desc: bool) -> impl Fn(&ProcInfo, &ProcInfo) -> std::cmp::Ordering {
    move |a, b| {
        let o = match key {
            SortKey::Cpu => a.cpu.total_cmp(&b.cpu).then(a.mem.cmp(&b.mem)),
            SortKey::Mem | SortKey::Count => a.mem.cmp(&b.mem),
            SortKey::Pid => a.pid.cmp(&b.pid),
            SortKey::Name => b.name.to_lowercase().cmp(&a.name.to_lowercase()),
        };
        if desc { o.reverse() } else { o }
    }
}

fn build_rows(g: &Gui) -> Vec<Row> {
    let s = &g.snap;
    let cmp = cmp_procs(g.sort, g.sort_desc);
    match g.view {
        ProcView::Flat => {
            let mut v: Vec<&ProcInfo> = s.procs.iter().filter(|p| matches(g, p)).collect();
            v.sort_by(|a, b| cmp(a, b));
            v.into_iter().map(|p| Row::Proc { pid: p.pid, depth: 0 }).collect()
        }
        ProcView::Tree => {
            let mut visible: HashSet<u32> = HashSet::new();
            for p in s.procs.iter().filter(|p| matches(g, p)) {
                let mut cur = Some(p.pid);
                while let Some(pid) = cur {
                    if !visible.insert(pid) {
                        break;
                    }
                    cur = s.get(pid).and_then(|x| x.ppid).filter(|pp| *pp != pid);
                }
            }
            s.tree_order(&visible, &cmp).into_iter().map(|(pid, depth)| Row::Proc { pid, depth }).collect()
        }
        ProcView::Apps => {
            let mut v: Vec<AppGroup> = s.groups().into_iter().filter(|gr| gr.pids.iter().any(|p| s.get(*p).is_some_and(|x| matches(g, x)))).collect();
            let (key, desc) = (g.sort, g.sort_desc);
            v.sort_by(|a, b| {
                let o = match key {
                    SortKey::Cpu => a.cpu.total_cmp(&b.cpu),
                    SortKey::Mem => a.mem.cmp(&b.mem),
                    SortKey::Count | SortKey::Pid => a.pids.len().cmp(&b.pids.len()),
                    SortKey::Name => b.label.to_lowercase().cmp(&a.label.to_lowercase()),
                };
                if desc { o.reverse() } else { o }
            });
            v.into_iter().map(Row::Group).collect()
        }
    }
}

pub fn show(g: &mut Gui, ui: &mut Ui) {
    egui::Panel::right("proc_detail").resizable(true).default_size(400.0).min_size(320.0).frame(w::side_frame(ui)).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| detail(g, ui));
    });
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        toolbar(g, ui);
        ui.add_space(8.0);
        let rows = build_rows(g);
        table(g, ui, &rows);
    });
}

fn toolbar(g: &mut Gui, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let search = ui.add(
            egui::TextEdit::singleline(&mut g.filter)
                .hint_text(tr("Search: name, PID, app, command…"))
                .desired_width(300.0)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        if g.focus_search {
            search.request_focus();
            g.focus_search = false;
        }
        if !g.filter.is_empty() && ui.small_button("✕").on_hover_text(tr("Clear search")).clicked() {
            g.filter.clear();
        }
        ui.add_space(8.0);
        let before = g.view;
        w::segmented(ui, &mut g.view, &[(ProcView::Apps, tr("By app")), (ProcView::Flat, tr("All processes")), (ProcView::Tree, tr("Tree"))]);
        if before != g.view {
            if g.view == ProcView::Apps && g.sort == SortKey::Pid {
                g.sort = SortKey::Mem;
            }
            if g.view != ProcView::Apps && g.sort == SortKey::Count {
                g.sort = SortKey::Mem;
            }
        }
        ui.add_space(8.0);
        ui.checkbox(&mut g.only_mine, tr("Only mine"));
    });
}

fn sort_header(g: &mut Gui, ui: &mut Ui, label: &str, key: SortKey) {
    let arrow = if g.sort == key { if g.sort_desc { " ▼" } else { " ▲" } } else { "" };
    let color = if g.sort == key { C::ACCENT } else { C::dim(ui) };
    let r = ui.add(egui::Label::new(RichText::new(format!("{label}{arrow}")).strong().color(color)).sense(Sense::click()));
    if r.clicked() {
        if g.sort == key {
            g.sort_desc = !g.sort_desc;
        } else {
            g.sort = key;
            g.sort_desc = matches!(key, SortKey::Cpu | SortKey::Mem | SortKey::Count);
        }
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand);
}

fn plain_header(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).strong().color(C::dim(ui)));
}

enum CtxAction {
    Stop,
    Kill,
    Finder,
    Disk,
}

fn table(g: &mut Gui, ui: &mut Ui, rows: &[Row]) {
    let snap = g.snap.clone();
    let ports = g.ports.lock().unwrap().clone();
    let max_mem = rows
        .iter()
        .map(|r| match r {
            Row::Proc { pid, .. } => snap.get(*pid).map(|p| p.mem).unwrap_or(0),
            Row::Group(gr) => gr.mem,
        })
        .max()
        .unwrap_or(1)
        .max(1);
    let mut clicked: Option<Sel> = None;
    let mut ctx_action: Option<(Sel, CtxAction)> = None;
    let apps = g.view == ProcView::Apps;
    let mut tb = TableBuilder::new(ui).striped(true).sense(Sense::click()).cell_layout(egui::Layout::left_to_right(egui::Align::Center));
    tb = if apps {
        tb.column(Column::remainder().at_least(220.0).clip(true))
            .column(Column::exact(90.0))
            .column(Column::exact(70.0))
            .column(Column::exact(170.0))
            .column(Column::exact(96.0))
    } else {
        tb.column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::exact(64.0))
            .column(Column::exact(90.0).clip(true))
            .column(Column::exact(64.0))
            .column(Column::exact(150.0))
            .column(Column::exact(82.0))
            .column(Column::exact(96.0))
    };
    tb.header(24.0, |mut h| {
        if apps {
            h.col(|ui| sort_header(g, ui, tr("App"), SortKey::Name));
            h.col(|ui| sort_header(g, ui, tr("Processes"), SortKey::Count));
            h.col(|ui| sort_header(g, ui, "CPU", SortKey::Cpu));
            h.col(|ui| sort_header(g, ui, tr("Memory"), SortKey::Mem));
            h.col(|ui| plain_header(ui, tr("Safety")));
        } else {
            h.col(|ui| sort_header(g, ui, tr("Name"), SortKey::Name));
            h.col(|ui| sort_header(g, ui, "PID", SortKey::Pid));
            h.col(|ui| plain_header(ui, tr("User")));
            h.col(|ui| sort_header(g, ui, "CPU", SortKey::Cpu));
            h.col(|ui| sort_header(g, ui, tr("Memory"), SortKey::Mem));
            h.col(|ui| plain_header(ui, tr("Status")));
            h.col(|ui| plain_header(ui, tr("Safety")));
        }
    })
    .body(|body| {
        body.rows(26.0, rows.len(), |mut row| match &rows[row.index()] {
            Row::Group(gr) => {
                let sel = Sel::Group(gr.key.clone());
                row.set_selected(g.sel.as_ref() == Some(&sel));
                row.col(|ui| {
                    w::dot(ui, if gr.app.is_some() { C::ACCENT } else { C::track(ui) });
                    ui.label(RichText::new(&gr.label).color(C::text(ui)));
                    if gr.pids.iter().any(|p| ports.contains_key(p)) {
                        w::badge(ui, tr("network"), C::PURPLE);
                    }
                });
                row.col(|ui| {
                    ui.label(RichText::new(gr.pids.len().to_string()).color(if gr.pids.len() >= 200 { C::RED } else { C::dim(ui) }));
                });
                row.col(|ui| {
                    ui.label(RichText::new(format!("{:.1}%", gr.cpu)).color(w::cpu_color(ui, gr.cpu)));
                });
                row.col(|ui| mem_cell(ui, gr.mem, max_mem));
                row.col(|ui| {
                    w::safety_badge(ui, gr.safety);
                });
                let resp = row.response();
                if resp.clicked() {
                    clicked = Some(sel.clone());
                }
                resp.context_menu(|ui| ctx_menu(ui, &sel, gr.app.is_some(), &mut ctx_action));
            }
            Row::Proc { pid, depth } => {
                let Some(p) = snap.get(*pid) else { return };
                let sel = Sel::Pid(*pid);
                row.set_selected(g.sel.as_ref() == Some(&sel));
                row.col(|ui| {
                    if *depth > 0 {
                        ui.add_space((*depth as f32 * 14.0).min(200.0));
                        ui.label(RichText::new("└").color(C::dim(ui)));
                    }
                    ui.label(RichText::new(&p.name).color(C::text(ui)));
                    if ports.contains_key(pid) {
                        w::badge(ui, tr("network"), C::PURPLE);
                    }
                });
                row.col(|ui| {
                    ui.label(RichText::new(p.pid.to_string()).color(C::dim(ui)));
                });
                row.col(|ui| {
                    ui.label(RichText::new(&p.user).color(C::dim(ui)));
                });
                row.col(|ui| {
                    ui.label(RichText::new(format!("{:.1}%", p.cpu)).color(w::cpu_color(ui, p.cpu)));
                });
                row.col(|ui| mem_cell(ui, p.mem, max_mem));
                row.col(|ui| {
                    if p.stopped {
                        w::badge(ui, tr("paused"), C::PURPLE);
                    } else {
                        ui.label(RichText::new(p.status).color(C::dim(ui)));
                    }
                });
                row.col(|ui| {
                    w::safety_badge(ui, p.safety);
                });
                let resp = row.response();
                if resp.clicked() {
                    clicked = Some(sel.clone());
                }
                resp.context_menu(|ui| ctx_menu(ui, &sel, p.is_main_app, &mut ctx_action));
            }
        });
    });
    if let Some(s) = clicked {
        g.sel = Some(s);
    }
    if let Some((s, a)) = ctx_action {
        g.sel = Some(s);
        match a {
            CtxAction::Stop => ask_stop(g, false),
            CtxAction::Kill => ask_stop(g, true),
            CtxAction::Finder => reveal(g),
            CtxAction::Disk => show_on_disk(g),
        }
    }
}

fn ctx_menu(ui: &mut Ui, sel: &Sel, is_app: bool, out: &mut Option<(Sel, CtxAction)>) {
    let stop = if is_app { tr("Quit app (like ⌘Q)") } else { tr("Stop (gently)") };
    let items = [
        (stop, CtxAction::Stop, false),
        (tr("Force quit"), CtxAction::Kill, true),
        (tr("Show in Finder"), CtxAction::Finder, false),
        (tr("Show on the Disk page"), CtxAction::Disk, false),
    ];
    for (label, a, red) in items {
        let text = if red { RichText::new(label).color(C::RED) } else { RichText::new(label) };
        if ui.button(text).clicked() {
            *out = Some((sel.clone(), a));
            ui.close();
        }
    }
}

fn mem_cell(ui: &mut Ui, mem: u64, max: u64) {
    ui.add_sized([70.0, 18.0], egui::Label::new(RichText::new(fmt::bytes(mem)).color(w::mem_color(ui, mem))));
    let color = if mem >= 2_000_000_000 {
        C::RED
    } else if mem >= 500_000_000 {
        C::YELLOW
    } else {
        C::ACCENT
    };
    w::bar(ui, (mem as f64 / max as f64) as f32, egui::vec2(64.0, 5.0), color.gamma_multiply(0.85));
}

// ---------------------------------------------------------------------------
// Details
// ---------------------------------------------------------------------------

fn selected_group(g: &Gui) -> Option<AppGroup> {
    let Some(Sel::Group(k)) = &g.sel else { return None };
    g.snap.groups().into_iter().find(|gr| &gr.key == k)
}

fn selected_proc(g: &Gui) -> Option<ProcInfo> {
    match &g.sel {
        Some(Sel::Pid(p)) => g.snap.get(*p).cloned(),
        Some(Sel::Group(_)) => {
            let gr = selected_group(g)?;
            gr.pids.iter().filter_map(|p| g.snap.get(*p)).max_by_key(|p| (p.is_main_app, p.mem)).cloned()
        }
        None => None,
    }
}

fn detail(g: &mut Gui, ui: &mut Ui) {
    let snap = g.snap.clone();
    if let Some(gr) = selected_group(g) {
        group_detail(g, ui, &snap, &gr);
    } else if let Some(p) = selected_proc(g) {
        proc_detail(g, ui, &snap, &p);
    } else {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(tr("Select a process or an app")).size(16.0).color(C::dim(ui)));
            ui.add_space(6.0);
            ui.label(RichText::new(tr("You will see what it is and whether it is safe to stop.")).color(C::dim(ui)));
        });
        ui.add_space(24.0);
        summary(ui, &snap);
    }
}

fn summary(ui: &mut Ui, s: &Snapshot) {
    let mut groups = s.groups();
    for (title, by_cpu) in [(tr("Most memory"), false), (tr("Most CPU"), true)] {
        w::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(title).strong());
            ui.add_space(4.0);
            if by_cpu {
                groups.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
            } else {
                groups.sort_by_key(|a| std::cmp::Reverse(a.mem));
            }
            for gr in groups.iter().take(6) {
                ui.horizontal(|ui| {
                    ui.label(&gr.label);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if by_cpu {
                            ui.label(RichText::new(format!("{:.1}%", gr.cpu)).color(w::cpu_color(ui, gr.cpu)));
                        } else {
                            ui.label(RichText::new(fmt::bytes(gr.mem)).color(w::mem_color(ui, gr.mem)));
                        }
                    });
                });
            }
        });
        ui.add_space(8.0);
    }
}

fn stats_row(ui: &mut Ui, items: &[(&str, String, Color32)]) {
    ui.columns(items.len(), |cols| {
        for (i, (k, v, c)) in items.iter().enumerate() {
            w::card(&mut cols[i], |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new(*k).size(11.0).color(C::dim(ui)));
                ui.label(RichText::new(v).size(17.0).strong().color(*c));
            });
        }
    });
}

fn safety_block(ui: &mut Ui, s: Safety) {
    let icon = match s {
        Safety::User => "✔",
        Safety::System => "⚠",
        Safety::Critical => "⛔",
    };
    w::note(ui, w::safety_color(s), &format!("{icon} {}: {}", tr("Safety"), s.label()), s.explain());
}

fn ports_block(g: &Gui, ui: &mut Ui, pids: &[u32]) {
    let ports = g.ports.lock().unwrap();
    let mut all: Vec<String> = pids.iter().filter_map(|p| ports.get(p)).flatten().cloned().collect();
    all.sort();
    all.dedup();
    if all.is_empty() {
        return;
    }
    ui.add_space(8.0);
    let local = all.iter().all(|p| p.ends_with("(local)"));
    let text = if local { tr("Listens on this Mac only.") } else { tr("Accepts connections from the network.") };
    w::note(ui, C::PURPLE, &trf("Open ports: {0}", &[&all.join(", ")]), text);
}

fn group_detail(g: &mut Gui, ui: &mut Ui, s: &Snapshot, gr: &AppGroup) {
    ui.label(RichText::new(&gr.label).size(22.0).strong());
    let kind = if gr.app.is_some() { tr("App") } else { tr("Program") };
    ui.label(RichText::new(format!("{kind} · {}", trf("{0} process(es)", &[&gr.pids.len()]))).color(C::dim(ui)));
    ui.add_space(6.0);
    if let Some(m) = gr.pids.iter().filter_map(|p| s.get(*p)).max_by_key(|p| (p.is_main_app, p.mem)) {
        ui.label(procs::describe(m));
    }
    ui.add_space(8.0);
    stats_row(
        ui,
        &[
            ("CPU", format!("{:.1}%", gr.cpu), w::cpu_color(ui, gr.cpu)),
            (tr("Memory"), fmt::bytes(gr.mem), w::mem_color(ui, gr.mem)),
            (tr("Processes"), gr.pids.len().to_string(), if gr.pids.len() >= 200 { C::RED } else { C::text(ui) }),
        ],
    );
    if gr.pids.len() >= 200 {
        ui.add_space(8.0);
        w::note(
            ui,
            C::RED,
            tr("Looks like a malfunction"),
            tr("That many copies of one app is abnormal and usually means it is stuck in a loop. Quit and reopen it."),
        );
    }
    ui.add_space(8.0);
    safety_block(ui, gr.safety);
    ports_block(g, ui, &gr.pids);
    ui.add_space(10.0);
    action_buttons(g, ui, gr.safety, gr.app.is_some(), None);
    if let Some(a) = &gr.app {
        ui.add_space(6.0);
        w::kv(ui, tr("Location"), fmt::path(a));
    }
    ui.add_space(12.0);
    ui.label(RichText::new(tr("Processes (by memory)")).strong());
    ui.add_space(4.0);
    let mut members: Vec<&ProcInfo> = gr.pids.iter().filter_map(|p| s.get(*p)).collect();
    members.sort_by_key(|a| std::cmp::Reverse(a.mem));
    let mut jump = None;
    for p in members.iter().take(40) {
        let r = ui
            .horizontal(|ui| {
                ui.add_sized([56.0, 18.0], egui::Label::new(RichText::new(p.pid.to_string()).color(C::dim(ui))));
                ui.add_sized([72.0, 18.0], egui::Label::new(RichText::new(fmt::bytes(p.mem)).color(w::mem_color(ui, p.mem))));
                ui.add_sized([52.0, 18.0], egui::Label::new(RichText::new(format!("{:.1}%", p.cpu)).color(w::cpu_color(ui, p.cpu))));
                ui.add(egui::Label::new(&p.name).truncate());
            })
            .response
            .interact(Sense::click())
            .on_hover_text(tr("Click to open this process"));
        if r.clicked() {
            jump = Some(p.pid);
        }
    }
    if members.len() > 40 {
        ui.label(RichText::new(trf("…and {0} more", &[&(members.len() - 40)])).color(C::dim(ui)));
    }
    if let Some(pid) = jump {
        g.view = ProcView::Flat;
        g.sel = Some(Sel::Pid(pid));
    }
}

fn proc_detail(g: &mut Gui, ui: &mut Ui, s: &Snapshot, p: &ProcInfo) {
    ui.label(RichText::new(&p.name).size(22.0).strong());
    ui.label(RichText::new(format!("PID {} · {} · {}", p.pid, p.user, trf("running for {0}", &[&fmt::duration(p.run_time)]))).color(C::dim(ui)));
    ui.add_space(6.0);
    ui.label(procs::describe(p));
    ui.add_space(8.0);
    stats_row(
        ui,
        &[
            ("CPU", format!("{:.1}%", p.cpu), w::cpu_color(ui, p.cpu)),
            (tr("Memory"), fmt::bytes(p.mem), w::mem_color(ui, p.mem)),
            (tr("Disk I/O"), fmt::bytes(p.disk_read + p.disk_write), C::text(ui)),
        ],
    );
    ui.add_space(8.0);
    safety_block(ui, p.safety);
    ports_block(g, ui, &[p.pid]);
    ui.add_space(10.0);
    action_buttons(g, ui, p.safety, p.is_main_app, Some(p));
    ui.add_space(12.0);
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        w::kv(ui, tr("Status"), if p.stopped { tr("paused").to_string() } else { p.status.to_string() });
        if let Some(pp) = p.ppid {
            let pname = s.get(pp).map(|x| x.name.clone()).unwrap_or("?".into());
            ui.horizontal(|ui| {
                ui.add_sized([120.0, 18.0], egui::Label::new(RichText::new(tr("Parent")).color(C::dim(ui))));
                if ui.link(format!("{pname} ({pp})")).clicked() {
                    g.view = ProcView::Flat;
                    g.sel = Some(Sel::Pid(pp));
                }
            });
        }
        let children = s.procs.iter().filter(|c| c.ppid == Some(p.pid)).count();
        if children > 0 {
            w::kv(ui, tr("Children"), children.to_string());
        }
        if let Some(a) = p.app_name() {
            w::kv(ui, tr("App"), a);
        }
        w::kv(ui, tr("Virtual memory"), fmt::bytes(p.vmem));
        w::kv(ui, tr("Program"), p.exe.as_ref().map(|e| fmt::path(e)).unwrap_or(tr("no access").into()));
        if let Some(c) = &p.cwd {
            w::kv(ui, tr("Folder"), fmt::path(c));
        }
    });
    if !p.cmd.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(tr("Command line")).color(C::dim(ui)));
        let mut cmd = p.cmd.clone();
        ui.add(egui::TextEdit::multiline(&mut cmd).font(egui::TextStyle::Monospace).desired_rows(2).desired_width(f32::INFINITY));
    }
    if p.exe.is_none() && !s.is_root() {
        ui.add_space(8.0);
        ui.label(
            RichText::new(tr("This process belongs to another user (usually root): macOS hides its details from regular apps.")).color(C::dim(ui)),
        );
    }
}

fn action_buttons(g: &mut Gui, ui: &mut Ui, safety: Safety, is_app: bool, single: Option<&ProcInfo>) {
    let blocked = safety == Safety::Critical;
    ui.horizontal_wrapped(|ui| {
        let (label, hint) = if is_app {
            (tr("Quit app"), tr("Quits like ⌘Q — the app saves its data and asks about unsaved documents."))
        } else {
            (tr("Stop"), tr("Asks the process to quit (SIGTERM) so it can close everything properly."))
        };
        if w::big_button(ui, label, C::ACCENT, !blocked).on_hover_text(hint).on_disabled_hover_text(safety.explain()).clicked() {
            ask_stop(g, false);
        }
        if w::big_button(ui, tr("Force quit"), C::RED, !blocked)
            .on_hover_text(tr("Kills the process immediately (SIGKILL). Unsaved work is lost. Use only if a normal stop did not help."))
            .on_disabled_hover_text(safety.explain())
            .clicked()
        {
            ask_stop(g, true);
        }
        if let Some(p) = single {
            let label = if p.stopped { tr("Resume") } else { tr("Pause") };
            let r = ui
                .add_enabled(safety == Safety::User, egui::Button::new(label).corner_radius(8).min_size(egui::vec2(0.0, 32.0)))
                .on_hover_text(tr("Freeze the process for a while (it stops using CPU), then resume it."))
                .on_disabled_hover_text(tr("Only your own processes can be paused: freezing a system one can hang the Mac."));
            if r.clicked() {
                let sig = if p.stopped { libc::SIGCONT } else { libc::SIGSTOP };
                match procs::send_signal(p.pid, sig) {
                    Ok(()) => g.toast(if p.stopped { trf("“{0}” resumed", &[&p.name]) } else { trf("“{0}” paused", &[&p.name]) }, Level::Ok),
                    Err(e) => g.toast(trf("Failed: {0}", &[&e]), Level::Danger),
                }
            }
        }
        if w::plain_button(ui, tr("Show in Finder")).clicked() {
            reveal(g);
        }
        if w::plain_button(ui, tr("On disk")).on_hover_text(tr("Open the program's folder on the Disk page")).clicked() {
            show_on_disk(g);
        }
    });
}

fn target_path(g: &Gui) -> Option<PathBuf> {
    selected_proc(g).and_then(|p| p.app.clone().or(p.exe.clone()))
}

fn reveal(g: &mut Gui) {
    match target_path(g) {
        Some(p) => macpilot::trash::reveal_in_finder(&p),
        None => g.toast(tr("The program's path is not available."), Level::Warn),
    }
}

fn show_on_disk(g: &mut Gui) {
    let Some(p) = target_path(g) else {
        g.toast(tr("The program's path is not available."), Level::Warn);
        return;
    };
    let Some(parent) = p.parent().map(|x| x.to_path_buf()) else { return };
    g.go_page(Page::Disk);
    g.disk_mode = DiskMode::List;
    g.go(parent);
    g.disk_sel = Some(p);
}

/// Prepare the stop dialog.
pub fn ask_stop(g: &mut Gui, force: bool) {
    let s = g.snap.clone();
    let (pids, name, safety, app, is_app_quit) = if let Some(gr) = selected_group(g) {
        let main = gr.app.is_some() && gr.pids.iter().any(|p| s.get(*p).is_some_and(|x| x.is_main_app));
        (gr.pids.clone(), gr.label.clone(), gr.safety, gr.app.clone(), main)
    } else if let Some(p) = selected_proc(g) {
        (vec![p.pid], p.name.clone(), p.safety, p.app.clone(), p.is_main_app)
    } else {
        return;
    };
    if safety == Safety::Critical {
        let who = pids.iter().filter_map(|p| s.get(*p)).find(|p| p.safety == Safety::Critical);
        let why = match who {
            Some(p) if p.pid == s.my_pid => tr("this is MacPilot itself — just close the window").to_string(),
            Some(p) => trf("“{0}” (PID {1}) is vital to macOS", &[&p.name, &p.pid]),
            None => String::new(),
        };
        g.toast(trf("Blocked: {0}.", &[&why]), Level::Danger);
        return;
    }
    let foreign = !s.is_root() && pids.iter().filter_map(|p| s.get(*p)).any(|p| p.uid != Some(s.my_uid));
    let what = if pids.len() > 1 { trf("“{0}” — {1} process(es)", &[&name, &pids.len()]) } else { format!("“{name}” (PID {})", pids[0]) };
    let mut lines = vec![(what, Level::Info)];
    let (title, button, action, danger) = if force {
        lines.push((tr("The process will be killed immediately (SIGKILL).").into(), Level::Warn));
        lines.push((tr("Unsaved work will be lost. Use only if a normal stop did not help.").into(), Level::Danger));
        (tr("Force quit?"), tr("Force quit"), Action::Signal { pids: pids.clone(), sig: libc::SIGKILL, admin: foreign }, true)
    } else if let (true, Some(app)) = (is_app_quit, app) {
        lines.push((tr("The app quits as if you pressed ⌘Q: it saves its state and asks about unsaved documents.").into(), Level::Ok));
        let main_pid = pids.iter().copied().find(|p| s.get(*p).is_some_and(|x| x.is_main_app));
        (tr("Quit the app?"), tr("Quit"), Action::QuitApp { app, main_pid }, false)
    } else {
        lines.push((tr("The process is asked to quit (SIGTERM) and can close its files and connections properly.").into(), Level::Ok));
        (tr("Stop the process?"), tr("Stop"), Action::Signal { pids: pids.clone(), sig: libc::SIGTERM, admin: foreign }, false)
    };
    if foreign {
        lines.push((tr("It belongs to another user — macOS will ask for an administrator password.").into(), Level::Warn));
    }
    let mut c = Confirm::new(title, lines, action, button);
    if safety == Safety::System {
        c.lines.push((Safety::System.explain().into(), Level::Danger));
        c.require = Some(w::yes_word());
    }
    c.danger = danger || c.require.is_some();
    g.confirm = Some(c);
}
