//! Overview: the state of the Mac and recommendations with one-click actions.

use eframe::egui::{self, Color32, RichText, Ui};
use macpilot::{disk, fmt, tr, trf};

use crate::widgets::{self as w, C, Txt};
use crate::{AppsMode, CleanMode, DiskMode, Gui, Page, ProcView, Sel};

struct Rec {
    color: Color32,
    title: String,
    text: String,
    button: &'static str,
    go: Box<dyn FnOnce(&mut Gui)>,
}

fn battery_recs(g: &Gui, groups: &[macpilot::procs::AppGroup], out: &mut Vec<Rec>) {
    let info = g.power.lock().unwrap().clone();
    let Some(b) = info.battery() else { return };
    let to_battery = || -> Box<dyn FnOnce(&mut Gui)> { Box::new(|g: &mut Gui| g.go_page(Page::Battery)) };
    if b.needs_service() {
        out.push(Rec {
            color: C::red(),
            title: tr("The battery needs service").into(),
            text: trf("Maximum capacity: {0} of new.", &[&b.health_pct().map(|h| format!("{h}%")).unwrap_or_default()]),
            button: tr("Battery"),
            go: to_battery(),
        });
    }
    if b.temperature >= 40.0 {
        out.push(Rec {
            color: C::yellow(),
            title: trf("The battery is hot: {0}", &[&fmt::celsius(b.temperature)]),
            text: tr("Heat wears batteries fastest. Heavy apps, charging on a soft surface or in the sun make it worse.").into(),
            button: tr("Battery"),
            go: to_battery(),
        });
    }
    if b.plugged {
        return;
    }
    // On battery: apps that drain it.
    if let Some(gr) = groups
        .iter()
        .filter(|gr| !gr.safety.blocked() && gr.power.is_some_and(|p| p >= 4.0))
        .max_by(|a, b| a.power.unwrap_or(0.0).total_cmp(&b.power.unwrap_or(0.0)))
    {
        let key = gr.key.clone();
        out.push(Rec {
            color: C::yellow(),
            title: trf("“{0}” uses a lot of energy", &[&gr.label]),
            text: trf("{0} right now. Quit it when you do not need it, and the charge lasts longer.", &[&fmt::watts(gr.power.unwrap_or(0.0))]),
            button: tr("Show"),
            go: Box::new(move |g| {
                g.view = ProcView::Apps;
                g.sel = Some(Sel::Group(key));
                g.go_page(Page::Procs);
            }),
        });
    }
    if let Some(bl) = info.blockers.iter().find(|bl| bl.seconds >= 30 * 60) {
        out.push(Rec {
            color: C::yellow(),
            title: trf("“{0}” keeps the Mac awake", &[&crate::battery_view::blocker_name(g, bl)]),
            text: trf("For {0} already. On battery this drains it even while you are away.", &[&fmt::duration(bl.seconds)]),
            button: tr("Battery"),
            go: to_battery(),
        });
    }
}

/// "CPU 52 °C · fans 1 400 rpm" for the menu bar menu.
pub fn sensors_line(s: &macpilot::sensors::Sensors) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(t) = s.cpu {
        parts.push(format!("CPU {}", fmt::temp(t)));
    }
    if let Some(t) = s.gpu {
        parts.push(format!("GPU {}", fmt::temp(t)));
    }
    if !s.fans.is_empty() {
        let fastest = s.fans.iter().map(|f| f.rpm).fold(0.0, f32::max);
        parts.push(if fastest < 1.0 { tr("fans are off").to_string() } else { trf("fans {0}", &[&fmt::rpm(fastest)]) });
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn recommendations(g: &Gui) -> Vec<Rec> {
    let mut out = Vec::new();
    let s = &g.snap;

    if let Some(u) = &g.update {
        let can_install = macpilot::update::replaceable_app().is_some();
        let text = if g.update_installing {
            tr("Downloading and checking the new version…").to_string()
        } else if can_install {
            trf("You have {0}. MacPilot downloads the new version, checks it, replaces itself and opens again.", &[&env!("CARGO_PKG_VERSION")])
        } else {
            trf("You have {0}. Download the new version and replace the app in Applications.", &[&env!("CARGO_PKG_VERSION")])
        };
        let url = u.url.clone();
        out.push(Rec {
            color: C::green(),
            title: trf("MacPilot {0} is available", &[&u.version]),
            text,
            button: if can_install { tr("Update and relaunch") } else { tr("Download") },
            go: Box::new(move |g| {
                if can_install {
                    g.install_update();
                } else {
                    let _ = std::process::Command::new("open").arg(&url).spawn();
                }
            }),
        });
    }

    if !g.settings.file_access {
        out.push(Rec {
            color: C::accent(),
            title: tr("Disk and Cleanup are waiting for your permission").into(),
            text: tr("MacPilot has not looked at any of your files yet. Allow it to see what takes space — you choose which folders.").into(),
            button: tr("Set up"),
            go: Box::new(|g| g.go_page(Page::Disk)),
        });
    } else if !g.full_disk_access {
        // How much of the disk is out of sight right now (data volume minus what the scan sees).
        let hidden = {
            let sp = g.space.lock().unwrap();
            let home = g
                .scan
                .as_ref()
                .filter(|s| s.root == macpilot::home() && (s.done() || s.cached_at.is_some()))
                .and_then(|s| s.dir(&s.root))
                .map(|d| d.size);
            let outside: u64 = sp.outside.iter().filter_map(|o| o.2).sum();
            sp.volumes.zip(home).map(|(v, h)| v.data.saturating_sub(h + outside)).filter(|h| *h > 10_000_000_000)
        };
        let mut text = tr("Without it macOS hides some folders and may pause the scan with permission dialogs. MacPilot only reads — nothing is removed without you.").to_string();
        if let Some(h) = hidden {
            text = trf(
                "About {0} of your disk is hidden from MacPilot now — usually data of other apps, like Docker or virtual machines.",
                &[&fmt::bytes(h)],
            ) + " "
                + &text;
        }
        out.push(Rec {
            color: C::accent(),
            title: tr("Give MacPilot Full Disk Access").into(),
            text,
            button: tr("Open settings"),
            go: Box::new(|_| macpilot::open_full_disk_access_settings()),
        });
    }

    let hot = g.sensors.lock().unwrap().as_ref().and_then(|s| s.cpu_max).filter(|t| *t >= 95.0);
    if let Some(t) = hot {
        out.push(Rec {
            color: C::red(),
            title: trf("The processor is very hot: {0}", &[&fmt::temp(t)]),
            text: tr("macOS slows the Mac down at this temperature. See which apps load the processor and quit the ones you do not need.").into(),
            button: tr("Show processes"),
            go: Box::new(|g| {
                g.sort = crate::SortKey::Cpu;
                g.sort_desc = true;
                g.go_page(Page::Procs);
            }),
        });
    }

    if let Some((avail, total)) = w::data_volume(&g.disks) {
        let free = avail as f64 / total.max(1) as f64;
        if free < 0.10 {
            out.push(Rec {
                color: C::red(),
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
            color: C::red(),
            title: trf("“{0}” is running {1}", &[&gr.label, &fmt::n(gr.pids.len() as u64, fmt::Noun::Process)]),
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
            color: if s.pressure >= 4 { C::red() } else { C::yellow() },
            title: tr("Memory is under pressure").into(),
            text: if s.swap_used > 0 {
                trf("Swap in use: {0}. Biggest users: {1}.", &[&fmt::bytes(s.swap_used), &names.join(", ")])
            } else {
                trf("Biggest users: {0}.", &[&names.join(", ")])
            },
            button: tr("See processes"),
            go: Box::new(|g| {
                g.view = ProcView::Apps;
                g.go_page(Page::Procs);
            }),
        });
    }

    battery_recs(g, &groups, &mut out);

    // The home folder grew a lot lately: say where.
    if let Some((Some(at), list)) = &g.changes {
        let grown: u64 = list.iter().filter(|c| c.delta() > 0).map(|c| c.delta() as u64).sum();
        if let Some(top) = list.iter().find(|c| c.delta() > 0).filter(|_| grown >= 5_000_000_000) {
            out.push(Rec {
                color: C::yellow(),
                title: trf("Your files grew by {0} since {1}", &[&fmt::bytes(grown), &fmt::date(*at)]),
                text: trf("Most of it: {0} (+{1}).", &[&fmt::path(&top.path), &fmt::bytes(top.delta() as u64)]),
                button: tr("Show"),
                go: Box::new(|g| {
                    g.disk_mode = DiskMode::Summary;
                    g.go_page(Page::Disk);
                }),
            });
        }
    }

    if let Some(hot) = s.procs.iter().filter(|p| p.cpu >= 90.0 && !p.safety.blocked()).max_by(|a, b| a.cpu.total_cmp(&b.cpu)) {
        let pid = hot.pid;
        out.push(Rec {
            color: C::yellow(),
            title: trf("“{0}” is using {1} CPU", &[&hot.name, &fmt::pct(hot.cpu)]),
            text: tr("100% means one fully busy core. If it is not doing something you asked for, it may be stuck.").into(),
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
                color: C::red(),
                title: trf("{0} unwanted startup item(s)", &[&bad.len()]),
                text: trf("Known nagware/adware starts with your Mac: {0}.", &[&bad.join(", ")]),
                button: tr("Review"),
                go: Box::new(|g| g.go_page(Page::Startup)),
            });
        }
        let broken = items.iter().filter(|i| i.broken && !i.disabled).count();
        if broken > 0 {
            out.push(Rec {
                color: C::yellow(),
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
            color: C::accent(),
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
            color: C::accent(),
            title: trf("{0} of old build folders", &[&fmt::bytes(junk_size)]),
            text: trf(
                "{0} not changed for {1} still keep node_modules, target, build… They are rebuilt on demand.",
                &[&fmt::n(old_junk.len() as u64, fmt::Noun::Project), &fmt::n_in(g.settings.junk_days as u64, fmt::Noun::Day)],
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
            color: C::yellow(),
            title: trf("{0} not used for a long time", &[&fmt::bytes(stale)]),
            text: trf(
                "{0} were not opened or changed for more than {1} (photos, video and music are not included).",
                &[&fmt::n(g.stale.len() as u64, fmt::Noun::Item), &fmt::n_in(g.stale_days as u64, fmt::Noun::Day)],
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
                color: C::yellow(),
                title: trf("{0} not opened for 6+ months", &[&fmt::n(unused.len() as u64, fmt::Noun::App)]),
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
                color: C::yellow(),
                title: trf("{0} left by removed apps", &[&fmt::bytes(size)]),
                text: trf(
                    "{0} in your Library belong to apps that are no longer installed.",
                    &[&fmt::n(o.iter().map(|x| x.items.len() as u64).sum::<u64>(), fmt::Noun::Item)],
                ),
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
                color: C::accent(),
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

pub fn stat_card(ui: &mut Ui, title: &str, value: String, sub: String, ratio: f32, hist: Option<&std::collections::VecDeque<f32>>) {
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(title).color(C::dim(ui)));
        ui.add(egui::Label::new(RichText::new(value).metric().color(w::value_color(ui, ratio))).truncate());
        // One line, so the cards of a row keep the same height; the whole text is in the tooltip.
        ui.add(egui::Label::new(RichText::new(sub).caption().color(C::dim(ui))).truncate());
        ui.add_space(w::sp::S);
        let width = ui.available_width();
        match hist {
            Some(h) => {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::hover());
                w::paint_sparkline(ui, h, rect, w::ratio_color(ratio));
            }
            None if ratio > 0.0 => {
                w::bar(ui, ratio, egui::vec2(width, 8.0), w::ratio_color(ratio));
                ui.add_space(w::sp::XL);
            }
            None => ui.add_space(w::sp::XXL),
        }
    });
}

pub fn show(g: &mut Gui, ui: &mut Ui) {
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        w::centered(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let host = sysinfo::System::host_name().unwrap_or_default();
                let os = sysinfo::System::long_os_version().unwrap_or_default();
                w::header(ui, tr("Overview"), &format!("{host} · {os} · {}", trf("on for {0}", &[&fmt::duration(sysinfo::System::uptime())])));

                let s = g.snap.clone();
                // "12 cores · 52 °C · fans 1 400 rpm"
                let mut cpu_sub = fmt::n(s.cpu_count as u64, fmt::Noun::Core);
                if let Some(sen) = g.sensors.lock().unwrap().as_ref() {
                    if let Some(t) = sen.cpu {
                        cpu_sub += &format!(" · {}", fmt::temp(t));
                    }
                    let fastest = sen.fans.iter().map(|f| f.rpm).fold(0.0, f32::max);
                    if fastest >= 1.0 {
                        cpu_sub += &format!(" · {}", trf("fans {0}", &[&fmt::rpm(fastest)]));
                    }
                }
                ui.columns(4, |cols| {
                    stat_card(&mut cols[0], "CPU", fmt::pct0(s.cpu_total), cpu_sub, s.cpu_total / 100.0, Some(&g.cpu_hist));
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
                        trf("of {0} · {1} · swap {2}", &[&fmt::bytes(s.mem_total), &pressure, &fmt::bytes(s.swap_used)]),
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
                    stat_card(
                        &mut cols[3],
                        tr("Processes"),
                        s.procs.len().to_string(),
                        trf("{0} running", &[&fmt::n(apps as u64, fmt::Noun::App)]),
                        0.0,
                        None,
                    );
                });

                ui.add_space(w::sp::L);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Recommendations")).section());
                    let scanning = g.scan.as_ref().is_some_and(|s| !s.done()) || g.rescan.is_some();
                    if scanning || g.apps.is_none() || g.orphans.is_none() {
                        ui.spinner();
                        ui.label(RichText::new(tr("checking your Mac…")).color(C::dim(ui)));
                    }
                });
                ui.add_space(w::sp::S);
                let recs = recommendations(g);
                if recs.is_empty() {
                    w::card(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            w::pixel_friend(ui, 48.0);
                            ui.add_space(w::sp::M);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(tr("✔ All good")).section());
                                ui.label(RichText::new(tr("No problems found. MacPilot keeps checking while it is open.")).color(C::dim(ui)));
                            });
                        });
                    });
                }
                let mut action = None;
                for (i, r) in recs.iter().enumerate() {
                    w::card_frame(ui).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(6.0, 40.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, if w::classic() { 0 } else { 3 }, r.color);
                            // The button takes its room first; the text wraps in what is left.
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if w::plain_button(ui, r.button).clicked() {
                                    action = Some(i);
                                }
                                ui.add_space(w::sp::M);
                                ui.vertical(|ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.label(RichText::new(&r.title).headline());
                                    ui.label(RichText::new(&r.text).color(C::dim(ui)));
                                });
                            });
                        });
                    });
                    ui.add_space(w::sp::S);
                }
                if let Some(i) = action {
                    if let Some(r) = recs.into_iter().nth(i) {
                        (r.go)(g);
                    }
                }
            });
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
