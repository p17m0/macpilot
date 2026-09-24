//! Overview: the state of the Mac and recommendations with one-click actions.

use eframe::egui::{self, Color32, RichText, Ui};
use macpilot::{disk, fmt, tr, trf};

use crate::widgets::{self as w, C};
use crate::{AppsMode, CleanMode, DiskMode, Gui, Page, ProcView, Sel};

struct Rec {
    color: Color32,
    title: String,
    text: String,
    button: &'static str,
    go: Box<dyn FnOnce(&mut Gui)>,
}

fn recommendations(g: &Gui) -> Vec<Rec> {
    let mut out = Vec::new();
    let s = &g.snap;

    if !g.full_disk_access {
        out.push(Rec {
            color: C::ACCENT,
            title: tr("Give MacPilot Full Disk Access").into(),
            text: tr("Without it macOS hides some folders and may pause the scan with permission dialogs. MacPilot only reads — nothing is removed without you.").into(),
            button: tr("Open settings"),
            go: Box::new(|_| macpilot::open_full_disk_access_settings()),
        });
    }

    if let Some((avail, total)) = w::data_volume(&g.disks) {
        let free = avail as f64 / total.max(1) as f64;
        if free < 0.10 {
            out.push(Rec {
                color: C::RED,
                title: trf("Only {0} free on the disk", &[&fmt::bytes(avail)]),
                text: tr("macOS needs free space for updates, swap and snapshots. Below 10% the Mac slows down.").into(),
                button: tr("Free up space"),
                go: Box::new(|g| g.go_page(Page::Clean)),
            });
        }
    }

    // A crashed app that keeps spawning copies of itself.
    let groups = s.groups();
    if let Some(gr) = groups.iter().filter(|gr| gr.pids.len() >= 200).max_by_key(|gr| gr.pids.len()) {
        let key = gr.key.clone();
        out.push(Rec {
            color: C::RED,
            title: trf("“{0}” is running {1} processes", &[&gr.label, &gr.pids.len()]),
            text: tr("That is abnormal and usually means the app is stuck in a loop. Quit and reopen it.").into(),
            button: tr("Show"),
            go: Box::new(move |g| {
                g.view = ProcView::Apps;
                g.sel = Some(Sel::Group(key));
                g.go_page(Page::Procs);
            }),
        });
    }

    let mem_r = s.mem_used as f64 / s.mem_total.max(1) as f64;
    if s.pressure >= 2 || s.swap_used > 4_000_000_000 || mem_r > 0.92 {
        let mut top = groups.clone();
        top.sort_by_key(|a| std::cmp::Reverse(a.mem));
        let names: Vec<String> = top.iter().take(3).map(|g| format!("{} ({})", g.label, fmt::bytes(g.mem))).collect();
        out.push(Rec {
            color: if s.pressure >= 4 { C::RED } else { C::YELLOW },
            title: tr("Memory is under pressure").into(),
            text: trf("Swap in use: {0}. Biggest users: {1}.", &[&fmt::bytes(s.swap_used), &names.join(", ")]),
            button: tr("See processes"),
            go: Box::new(|g| {
                g.view = ProcView::Apps;
                g.go_page(Page::Procs);
            }),
        });
    }

    if let Some(hot) =
        s.procs.iter().filter(|p| p.cpu >= 90.0 && p.safety != macpilot::procs::Safety::Critical).max_by(|a, b| a.cpu.total_cmp(&b.cpu))
    {
        let pid = hot.pid;
        out.push(Rec {
            color: C::YELLOW,
            title: trf("“{0}” is using {1}% CPU", &[&hot.name, &format!("{:.0}", hot.cpu)]),
            text: tr("If it is not doing something you asked for, it may be stuck.").into(),
            button: tr("Show"),
            go: Box::new(move |g| {
                g.view = ProcView::Flat;
                g.sel = Some(Sel::Pid(pid));
                g.go_page(Page::Procs);
            }),
        });
    }

    if let Some(items) = &g.startup {
        let bad: Vec<&str> = items.iter().filter(|i| i.unwanted && !i.disabled).map(|i| i.label.as_str()).collect();
        if !bad.is_empty() {
            out.push(Rec {
                color: C::RED,
                title: trf("{0} unwanted startup item(s)", &[&bad.len()]),
                text: trf("Known nagware/adware starts with your Mac: {0}.", &[&bad.join(", ")]),
                button: tr("Review"),
                go: Box::new(|g| g.go_page(Page::Startup)),
            });
        }
        let broken = items.iter().filter(|i| i.broken && !i.disabled).count();
        if broken > 0 {
            out.push(Rec {
                color: C::YELLOW,
                title: trf("{0} broken startup item(s)", &[&broken]),
                text: tr("They point to programs that no longer exist — leftovers of removed apps.").into(),
                button: tr("Review"),
                go: Box::new(|g| g.go_page(Page::Startup)),
            });
        }
    }

    let caches: u64 = g.targets.iter().filter(|t| t.cleanable()).filter_map(|t| t.stat.map(|s| s.size)).sum();
    if caches > 500_000_000 {
        out.push(Rec {
            color: C::GREEN,
            title: trf("{0} of caches and logs", &[&fmt::bytes(caches)]),
            text: tr("Apps recreate them when needed. Safe to clean.").into(),
            button: tr("Clean up"),
            go: Box::new(|g| {
                g.clean_mode = CleanMode::System;
                g.go_page(Page::Clean);
            }),
        });
    }

    let cutoff = disk::now_unix() - g.settings.junk_days * 86_400;
    let old_junk: Vec<_> = g.junk.iter().filter(|j| j.project_modified < cutoff).collect();
    let junk_size: u64 = old_junk.iter().map(|j| j.size).sum();
    if junk_size > 200_000_000 {
        out.push(Rec {
            color: C::GREEN,
            title: trf("{0} of old build folders", &[&fmt::bytes(junk_size)]),
            text: trf(
                "{0} projects not changed for {1}+ days still keep node_modules, target, build… They are rebuilt on demand.",
                &[&old_junk.len(), &g.settings.junk_days],
            ),
            button: tr("Review"),
            go: Box::new(|g| {
                g.clean_mode = CleanMode::Dev;
                g.go_page(Page::Clean);
            }),
        });
    }

    let stale: u64 = g.stale.iter().map(|i| i.size).sum();
    if stale > 500_000_000 {
        out.push(Rec {
            color: C::YELLOW,
            title: trf("{0} not used for a long time", &[&fmt::bytes(stale)]),
            text: trf(
                "{0} items were not opened or changed for {1}+ days (photos, video and music are not included).",
                &[&g.stale.len(), &g.stale_days],
            ),
            button: tr("Review"),
            go: Box::new(|g| {
                g.disk_mode = DiskMode::Stale;
                g.go_page(Page::Disk);
            }),
        });
    }

    if let Some(apps) = &g.apps {
        let old = disk::now_unix() - 180 * 86_400;
        let unused: Vec<_> = apps.iter().filter(|a| !a.protected && a.last_used.is_some_and(|t| t < old)).collect();
        let size: u64 = unused.iter().map(|a| a.size).sum();
        if size > 500_000_000 {
            out.push(Rec {
                color: C::YELLOW,
                title: trf("{0} apps not opened for 6+ months", &[&unused.len()]),
                text: trf("Together they take {0}. Uninstall the ones you do not need, with their leftovers.", &[&fmt::bytes(size)]),
                button: tr("Review"),
                go: Box::new(|g| {
                    g.apps_mode = AppsMode::Installed;
                    g.go_page(Page::Apps);
                }),
            });
        }
    }
    if let Some(o) = &g.orphans {
        let size: u64 = o.iter().map(|x| x.size).sum();
        if size > 100_000_000 {
            out.push(Rec {
                color: C::YELLOW,
                title: trf("{0} left by removed apps", &[&fmt::bytes(size)]),
                text: trf("{0} folders in your Library belong to apps that are no longer installed.", &[&o.len()]),
                button: tr("Review"),
                go: Box::new(|g| {
                    g.apps_mode = AppsMode::Leftovers;
                    g.go_page(Page::Apps);
                }),
            });
        }
    }

    if let Some(t) = g.targets.iter().find(|t| t.id == "trash") {
        if let Some(st) = t.stat.filter(|s| s.size > 1_000_000_000) {
            out.push(Rec {
                color: C::GREEN,
                title: trf("{0} in the Trash", &[&fmt::bytes(st.size)]),
                text: tr("Deleted files still take space until the Trash is emptied.").into(),
                button: tr("Open Cleanup"),
                go: Box::new(|g| {
                    g.clean_mode = CleanMode::System;
                    g.go_page(Page::Clean);
                }),
            });
        }
    }
    out
}

fn stat_card(ui: &mut Ui, title: &str, value: String, sub: String, ratio: f32, hist: Option<&std::collections::VecDeque<f32>>) {
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(title).color(C::dim(ui)));
        ui.label(RichText::new(value).size(22.0).strong().color(w::ratio_color(ratio)));
        ui.label(RichText::new(sub).size(11.5).color(C::dim(ui)));
        ui.add_space(4.0);
        let width = ui.available_width();
        match hist {
            Some(h) => {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::hover());
                w::paint_sparkline(ui, h, rect, w::ratio_color(ratio));
            }
            None if ratio > 0.0 => {
                w::bar(ui, ratio, egui::vec2(width, 8.0), w::ratio_color(ratio));
                ui.add_space(26.0);
            }
            None => ui.add_space(34.0),
        }
    });
}

pub fn show(g: &mut Gui, ui: &mut Ui) {
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let host = sysinfo::System::host_name().unwrap_or_default();
            let os = sysinfo::System::long_os_version().unwrap_or_default();
            w::header(ui, tr("Overview"), &format!("{host} · {os} · {}", trf("up {0}", &[&fmt::duration(sysinfo::System::uptime())])));

            let s = g.snap.clone();
            ui.columns(4, |cols| {
                stat_card(
                    &mut cols[0],
                    "CPU",
                    format!("{:.0}%", s.cpu_total),
                    trf("{0} cores", &[&s.cpu_count]),
                    s.cpu_total / 100.0,
                    Some(&g.cpu_hist),
                );
                let mem_r = s.mem_used as f32 / s.mem_total.max(1) as f32;
                let pressure = match s.pressure {
                    4 => tr("pressure: critical"),
                    2 => tr("pressure: high"),
                    _ => tr("pressure: normal"),
                };
                stat_card(
                    &mut cols[1],
                    tr("Memory"),
                    fmt::bytes(s.mem_used),
                    format!("{} {} · {} · swap {}", tr("of"), fmt::bytes(s.mem_total), pressure, fmt::bytes(s.swap_used)),
                    if s.pressure >= 4 {
                        1.0
                    } else if s.pressure >= 2 {
                        0.8
                    } else {
                        mem_r.min(0.7)
                    },
                    Some(&g.mem_hist),
                );
                if let Some((avail, total)) = w::data_volume(&g.disks) {
                    let used = total.saturating_sub(avail);
                    stat_card(
                        &mut cols[2],
                        tr("Disk"),
                        trf("{0} free", &[&fmt::bytes(avail)]),
                        trf("{0} of {1} used", &[&fmt::bytes(used), &fmt::bytes(total)]),
                        used as f32 / total.max(1) as f32,
                        None,
                    );
                }
                let groups = s.groups();
                let apps = groups.iter().filter(|g| g.app.is_some()).count();
                stat_card(&mut cols[3], tr("Processes"), s.procs.len().to_string(), trf("{0} apps running", &[&apps]), 0.0, None);
            });

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Recommendations")).size(17.0).strong());
                let scanning = g.scan.as_ref().is_some_and(|s| !s.done());
                if scanning || g.apps.is_none() || g.orphans.is_none() {
                    ui.spinner();
                    ui.label(RichText::new(tr("checking your Mac…")).color(C::dim(ui)));
                }
            });
            ui.add_space(6.0);
            let recs = recommendations(g);
            if recs.is_empty() {
                w::card(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new(tr("✔ All good")).size(16.0).strong().color(C::GREEN));
                    ui.label(tr("No problems found. MacPilot keeps checking while it is open."));
                });
            }
            let mut action = None;
            for (i, r) in recs.iter().enumerate() {
                egui::Frame::new()
                    .fill(C::card(ui))
                    .corner_radius(10)
                    .inner_margin(egui::Margin::same(12))
                    .stroke(egui::Stroke::new(1.0, C::track(ui)))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(6.0, 40.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 3, r.color);
                            ui.vertical(|ui| {
                                ui.set_max_width(ui.available_width() - 150.0);
                                ui.label(RichText::new(&r.title).size(15.0).strong());
                                ui.label(RichText::new(&r.text).color(C::dim(ui)));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if w::plain_button(ui, r.button).clicked() {
                                    action = Some(i);
                                }
                            });
                        });
                    });
                ui.add_space(6.0);
            }
            if let Some(i) = action {
                if let Some(r) = recs.into_iter().nth(i) {
                    (r.go)(g);
                }
            }
        });
    });
}

/// Developer screenshots: select something meaningful on the current page.
#[cfg(feature = "dev-tools")]
pub fn dev_select(g: &mut Gui) {
    match g.page {
        Page::Procs => {
            let mut gr = g.snap.groups();
            gr.sort_by_key(|a| std::cmp::Reverse(a.mem));
            g.sel = gr.first().map(|x| Sel::Group(x.key.clone()));
            if std::env::var("MACPILOT_CONFIRM").is_ok() {
                crate::procs_view::ask_stop(g, false);
            }
        }
        Page::Disk => {
            g.disk_sel = match g.disk_mode {
                DiskMode::Stale => g.stale.first().map(|i| i.path.clone()),
                _ => g.entries.get(1).map(|e| e.path.clone()),
            };
        }
        Page::Apps => {
            if let Some(a) = g.apps.as_ref().and_then(|a| a.iter().find(|a| !a.protected)).cloned() {
                g.app_sel = Some(a.path.clone());
                g.load_leftovers(&a);
            }
        }
        _ => {}
    }
}
