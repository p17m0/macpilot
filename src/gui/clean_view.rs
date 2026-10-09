//! Cleanup page: system junk and developer junk.

use std::path::PathBuf;

use eframe::egui::{self, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use macpilot::clean::{self, Kind};
use macpilot::disk::{self, DelSafety};
use macpilot::{fmt, tr, trf};

use crate::icons;
use crate::widgets::{self as w, C, Level, Txt};
use crate::{Action, CleanMode, Confirm, DiskMode, Gui, Page};

pub fn show(g: &mut Gui, ui: &mut Ui) {
    if !g.settings.file_access {
        crate::access_view::show(g, ui);
        return;
    }
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        w::centered(ui, |ui| {
            w::title_bar(
                ui,
                tr("Cleanup"),
                "",
                |ui| {
                    w::segmented(
                        ui,
                        &mut g.clean_mode,
                        &[(CleanMode::System, tr("System junk")), (CleanMode::Dev, tr("Developer junk")), (CleanMode::History, tr("History"))],
                    );
                },
                |_| {},
            );
            match g.clean_mode {
                CleanMode::System => system(g, ui),
                CleanMode::Dev => dev(g, ui),
                CleanMode::History => history(g, ui),
            }
        });
    });
}

/// What MacPilot moved to the Trash, newest first, with "Put Back" while it is still there.
fn history(g: &mut Gui, ui: &mut Ui) {
    use macpilot::trashlog::State;
    let in_trash: u64 = g.trash_log.iter().filter(|e| e.state == State::InTrash).map(|e| e.size).sum();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(tr("Moved to the Trash by MacPilot and still there")).color(C::dim(ui)));
            ui.label(RichText::new(fmt::bytes(in_trash)).metric().color(C::text(ui)));
            ui.label(RichText::new(tr("Kept for 90 days. Anything still in the Trash can be put back where it was.")).color(C::dim(ui)));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr("Open the Trash")).clicked() {
                macpilot::trash::open_trash();
            }
        });
    });
    ui.add_space(w::sp::M);
    if g.trash_log.is_empty() {
        w::empty(ui, tr("Nothing yet. Everything MacPilot moves to the Trash will be listed here."));
        return;
    }
    let mut back: Option<(PathBuf, i64)> = None;
    let log = &g.trash_log;
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(110.0))
        .column(Column::remainder().at_least(200.0).clip(true))
        .column(Column::exact(90.0))
        .column(Column::exact(150.0))
        .header(24.0, |mut h| {
            for t in [tr("When"), tr("What"), tr("Size"), ""] {
                h.col(|ui| {
                    ui.label(RichText::new(t).semibold().color(C::dim(ui)));
                });
            }
        })
        .body(|body| {
            body.rows(30.0, log.len(), |mut row| {
                // Newest first.
                let e = &log[log.len() - 1 - row.index()];
                let dim = e.state != State::InTrash;
                row.col(|ui| {
                    ui.label(RichText::new(fmt::date(e.at)).color(C::dim(ui))).on_hover_text(fmt::ago(e.at));
                });
                row.col(|ui| {
                    let color = if dim { C::dim(ui) } else { C::text(ui) };
                    ui.label(RichText::new(fmt::path(&e.path)).color(color)).on_hover_text(e.path.to_string_lossy());
                });
                row.col(|ui| {
                    ui.label(RichText::new(if e.size > 0 { fmt::bytes(e.size) } else { "—".into() }).color(C::dim(ui)));
                });
                row.col(|ui| match e.state {
                    State::InTrash => {
                        if ui.button(tr("Put Back")).on_hover_text(tr("Move it from the Trash to where it was")).clicked() {
                            back = Some((e.path.clone(), e.at));
                        }
                    }
                    State::Restored => {
                        ui.label(RichText::new(tr("put back")).color(C::green()));
                    }
                    State::Emptied => {
                        ui.label(RichText::new(tr("Trash emptied")).color(C::dim(ui)));
                    }
                });
            });
        });
    if let Some((path, at)) = back {
        g.put_back(path, at);
    }
}

/// Everything in one list: the safe places are ticked from the start, one button cleans them all.
fn system(g: &mut Gui, ui: &mut Ui) {
    let measuring = g.targets.iter().any(|t| t.stat.is_none() && disk::present(&t.path));
    let size_of = |t: &clean::Target| t.stat.map(|s| s.size).unwrap_or(0);
    // Places that do not exist on this Mac, and empty ones, are not shown. The Trash has a line of its own.
    let mut rows: Vec<usize> = (0..g.targets.len())
        .filter(|i| {
            let t = &g.targets[*i];
            t.kind != Kind::Trash && disk::present(&t.path) && t.stat.is_none_or(|s| s.size > 0)
        })
        .collect();
    // What can be cleaned comes first; biggest first once everything is measured (rows do not
    // jump around while sizes come in).
    rows.sort_by_key(|i| {
        let t = &g.targets[*i];
        (!t.cleanable(), std::cmp::Reverse(if measuring { 0 } else { size_of(t) }))
    });
    let cleanable: Vec<usize> = rows.iter().copied().filter(|i| g.targets[*i].cleanable()).collect();
    let selected: Vec<usize> =
        cleanable.iter().copied().filter(|i| g.clean_checked.contains(g.targets[*i].id) && size_of(&g.targets[*i]) > 0).collect();
    let selected_size: u64 = selected.iter().map(|i| size_of(&g.targets[*i])).sum();
    let all_on = !cleanable.is_empty() && cleanable.iter().all(|i| g.clean_checked.contains(g.targets[*i].id));

    let mut clean_now: Option<Vec<usize>> = None;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(tr("Selected to clean")).color(C::dim(ui)));
            ui.horizontal(|ui| {
                ui.label(RichText::new(fmt::bytes(selected_size)).metric().color(C::text(ui)));
                if measuring {
                    ui.spinner();
                }
            });
            ui.label(RichText::new(tr("The safe ones are already ticked. Everything goes to the Trash and can be put back.")).color(C::dim(ui)));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = trf("Clean selected · {0}", &[&fmt::bytes(selected_size)]);
            if w::big_button(ui, &label, C::accent(), !selected.is_empty()).clicked() {
                clean_now = Some(selected.clone());
            }
            if w::plain_button(ui, tr("⟳ Measure again")).clicked() {
                g.remeasure_targets();
            }
        });
    });
    ui.add_space(w::sp::M);
    trash_line(g, ui);
    ui.add_space(w::sp::S);

    let mut toggle: Option<&'static str> = None;
    let mut toggle_all = false;
    let mut open: Option<usize> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if rows.is_empty() {
            w::empty(ui, tr("Nothing to clean — all tidy."));
        } else {
            TableBuilder::new(ui)
                .striped(true)
                .vscroll(false)
                .sense(Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(28.0))
                .column(Column::remainder().at_least(240.0).clip(true))
                .column(Column::exact(110.0))
                .column(Column::exact(90.0))
                .column(Column::exact(96.0))
                .header(24.0, |mut h| {
                    h.col(|ui| {
                        let mut on = all_on;
                        let tip = if all_on { tr("Deselect all") } else { tr("Select all") };
                        if ui.add_enabled(!cleanable.is_empty(), egui::Checkbox::new(&mut on, "")).on_hover_text(tip).changed() {
                            toggle_all = true;
                        }
                    });
                    for t in [tr("What"), "", tr("Size"), ""] {
                        h.col(|ui| {
                            ui.label(RichText::new(t).semibold().color(C::dim(ui)));
                        });
                    }
                })
                .body(|body| {
                    body.rows(44.0, rows.len(), |mut row| {
                        let i = rows[row.index()];
                        let t = &g.targets[i];
                        row.col(|ui| {
                            if t.cleanable() {
                                let mut on = g.clean_checked.contains(t.id);
                                if ui.checkbox(&mut on, "").changed() {
                                    toggle = Some(t.id);
                                }
                            }
                        });
                        row.col(|ui| {
                            icons::app(ui, &target_icon(t), 22.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.add(egui::Label::new(RichText::new(t.label).semibold()).selectable(false));
                                // One line here; the whole hint and the path on hover.
                                let hint = RichText::new(t.hint.replace('`', "")).caption().color(C::dim(ui));
                                ui.add(egui::Label::new(hint).selectable(false).truncate()).on_hover_ui(|ui| {
                                    ui.set_max_width(420.0);
                                    w::text_with_code(ui, t.hint, C::text(ui));
                                    ui.label(RichText::new(fmt::path(&t.path)).caption().color(C::dim(ui)));
                                });
                            });
                        });
                        row.col(|ui| match t.kind {
                            Kind::Manual => {
                                w::badge(ui, tr("by hand"), C::yellow());
                            }
                            _ => {
                                w::del_badge(ui, clean::target_safety(t));
                            }
                        });
                        row.col(|ui| {
                            let (txt, color) = match t.stat {
                                Some(s) => (fmt::bytes(s.size), w::size_color(ui, s.size)),
                                None => (tr("measuring…").to_string(), C::dim(ui)),
                            };
                            ui.add(egui::Label::new(RichText::new(txt).color(color)).selectable(false));
                        });
                        row.col(|ui| {
                            if ui.button(tr("Open")).clicked() {
                                open = Some(i);
                            }
                        });
                        let resp = row.response();
                        if resp.clicked() && t.cleanable() {
                            toggle = Some(t.id);
                        }
                        resp.context_menu(|ui| {
                            if t.cleanable() && ui.add_enabled(size_of(t) > 0, egui::Button::new(tr("Clean only this"))).clicked() {
                                clean_now = Some(vec![i]);
                                ui.close();
                            }
                            if ui.button(tr("Open")).clicked() {
                                open = Some(i);
                                ui.close();
                            }
                            if ui.button(tr("Show in Finder")).clicked() {
                                macpilot::trash::reveal_in_finder(&t.path);
                                ui.close();
                            }
                        });
                    });
                });
        }
        ui.add_space(w::sp::M);
        stale_banner(g, ui);
    });

    if toggle_all {
        for i in &cleanable {
            let id = g.targets[*i].id;
            if all_on {
                g.clean_checked.remove(id);
            } else {
                g.clean_checked.insert(id);
            }
        }
    }
    if let Some(id) = toggle {
        if !g.clean_checked.remove(id) {
            g.clean_checked.insert(id);
        }
    }
    if let Some(i) = open {
        let p = g.targets[i].path.clone();
        g.go_page(Page::Disk);
        g.disk_mode = DiskMode::List;
        g.go(p);
    }
    if let Some(which) = clean_now {
        ask_clean(g, &which);
    }
}

/// The Trash, always in sight: cleaning only moves things there, emptying it frees the space.
fn trash_line(g: &mut Gui, ui: &mut Ui) {
    let Some(i) = g.targets.iter().position(|t| t.kind == Kind::Trash) else { return };
    let t = &g.targets[i];
    let size = t.stat.map(|s| s.size);
    let mut empty = false;
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            icons::app(ui, &target_icon(t), 22.0);
            ui.label(RichText::new(t.label).headline());
            ui.label(RichText::new(size.map_or(tr("measuring…").to_string(), fmt::bytes)).headline().color(C::dim(ui)));
            ui.label(RichText::new(tr("The space is freed only when the Trash is emptied.")).color(C::dim(ui)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w::tinted_button(ui, tr("Empty Trash…"), C::red(), size.is_some_and(|s| s > 0)).clicked() {
                    empty = true;
                }
            });
        });
    });
    if empty {
        ask_empty_trash(g, i);
    }
}

/// What a cleanup place looks like: the app it belongs to when there is one (Xcode, Mail, Docker…),
/// otherwise the place's own Finder icon.
fn target_icon(t: &clean::Target) -> PathBuf {
    let xcode = "/Applications/Xcode.app";
    let app = match t.id {
        "derived" | "devsupport" | "previews" | "archives" | "simcache" => Some(xcode.to_string()),
        "simulators" => Some(format!("{xcode}/Contents/Developer/Applications/Simulator.app")),
        "logs" => Some("/System/Applications/Utilities/Console.app".into()),
        "maildl" => Some("/System/Applications/Mail.app".into()),
        "mobilebackup" | "ipsw" => Some("/System/Library/CoreServices/Finder.app".into()),
        "docker" => Some("/Applications/Docker.app".into()),
        _ => None,
    };
    app.map(PathBuf::from).filter(|p| p.exists()).unwrap_or_else(|| t.path.clone())
}

/// One confirmation for all the given places.
fn ask_clean(g: &mut Gui, which: &[usize]) {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut lines: Vec<(String, Level)> = Vec::new();
    let mut installers: Vec<PathBuf> = Vec::new();
    let mut total = 0;
    let mut failed = None;
    for &i in which {
        let t = &g.targets[i];
        let children: Vec<PathBuf> = match std::fs::read_dir(&t.path) {
            // Not the whole Downloads folder: only the installers that have been lying there for a month.
            Ok(_) if t.id == "installers" => clean::old_installers(&t.path, disk::now_unix()).into_iter().map(|f| f.0).collect(),
            Ok(rd) => rd.flatten().map(|e| e.path()).filter(|c| disk::deletion_safety(c).0 != DelSafety::Blocked).collect(),
            Err(e) => {
                failed = Some(format!("{}: {}", fmt::path(&t.path), disk::perm_hint(&e)));
                continue;
            }
        };
        if children.is_empty() {
            continue;
        }
        let size = t.stat.map(|s| s.size).unwrap_or(0);
        total += size;
        lines.push((format!("{} — {}, {}", t.label, fmt::n(children.len() as u64, fmt::Noun::Item), fmt::bytes(size)), Level::Info));
        if t.id == "installers" {
            installers = children.clone();
        }
        paths.extend(children);
    }
    if paths.is_empty() {
        match (failed, which) {
            (Some(e), _) => g.toast(e, Level::Danger),
            (None, [i]) => g.toast(trf("“{0}” is already empty.", &[&g.targets[*i].label]), Level::Info),
            (None, _) => g.toast(tr("Nothing to clean up right now."), Level::Info),
        }
        return;
    }
    let title = match which {
        [i] => {
            let t = &g.targets[*i];
            lines[0].0 = format!("{} — {}", fmt::path(&t.path), lines[0].0.rsplit(" — ").next().unwrap_or_default());
            lines.push((t.hint.replace('`', ""), Level::Info));
            trf("Clean “{0}”?", &[&t.label])
        }
        _ => tr("Clean the selected places?").to_string(),
    };
    lines.push((tr("The contents go to the Trash (the folder itself stays).").into(), Level::Ok));
    if !installers.is_empty() {
        lines.push((tr("Only these installers go to the Trash; everything else in Downloads stays.").into(), Level::Ok));
        lines.extend(installers.iter().take(8).map(|c| (c.file_name().unwrap_or_default().to_string_lossy().to_string(), Level::Info)));
        if installers.len() > 8 {
            lines.push((trf("…and {0} more", &[&(installers.len() - 8)]), Level::Info));
        }
    }
    if which.iter().any(|i| g.targets[*i].id == "caches") {
        let running: Vec<String> = g
            .snap
            .procs
            .iter()
            .filter(|p| p.is_main_app && p.uid == Some(g.snap.my_uid) && !p.exe.as_deref().is_some_and(|e| e.starts_with("/System")))
            .map(|p| p.name.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .take(6)
            .collect();
        if !running.is_empty() {
            lines.push((trf("Tip: quit apps first: {0}", &[&running.join(", ")]), Level::Warn));
        }
    }
    g.confirm = Some(Confirm::new(title, lines, Action::Trash { paths, size: total }, tr("Clean")));
}

fn ask_empty_trash(g: &mut Gui, i: usize) {
    let size = g.targets[i].stat.map(|s| s.size).unwrap_or(0);
    let lines = vec![
        (trf("{0} will be deleted permanently.", &[&fmt::bytes(size)]), Level::Warn),
        (tr("This cannot be undone: files in the Trash can no longer be restored.").into(), Level::Danger),
    ];
    let mut c = Confirm::new(tr("Empty the Trash?"), lines, Action::EmptyTrash, tr("Empty Trash"));
    c.danger = true;
    c.require = Some(w::yes_word());
    g.confirm = Some(c);
}

/// Shown while results come from an unfinished scan.
pub fn partial_note(g: &Gui, ui: &mut Ui) {
    if g.scan.as_ref().is_some_and(|s| !s.done()) {
        w::note(
            ui,
            C::yellow(),
            tr("Partial results"),
            tr("The scan is waiting for a macOS permission dialog. Answer it, or give MacPilot Full Disk Access — the list will be completed."),
        );
        ui.add_space(w::sp::S);
    }
}

/// Pointer to the "Not used" analysis on the Disk page.
fn stale_banner(g: &mut Gui, ui: &mut Ui) {
    let ready = g.scan_ready();
    let mut open = false;
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(tr("Not used for a long time")).headline());
                if ready {
                    let total: u64 = g.stale.iter().map(|i| i.size).sum();
                    ui.label(trf(
                        "{0} · {1} — not opened for more than {2}. Photos, video and music are not included.",
                        &[&fmt::n(g.stale.len() as u64, fmt::Noun::Item), &fmt::bytes(total), &fmt::n_in(g.stale_days as u64, fmt::Noun::Day)],
                    ));
                } else {
                    ui.label(RichText::new(tr("Needs a disk scan — it is running in the background.")).color(C::dim(ui)));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w::plain_button(ui, tr("Review")).clicked() {
                    open = true;
                }
            });
        });
    });
    if open {
        g.go_page(Page::Disk);
        g.disk_mode = DiskMode::Stale;
    }
}

// ---------------------------------------------------------------------------
// Developer junk
// ---------------------------------------------------------------------------

fn dev(g: &mut Gui, ui: &mut Ui) {
    if !g.scan_ready() {
        w::waiting(ui, tr("Looking for build folders…"));
        return;
    }
    partial_note(g, ui);
    let Some(scan) = &g.scan else { return };
    let root = fmt::place(&scan.root);
    let cutoff = disk::now_unix() - g.settings.junk_days * 86_400;
    let total: u64 = g.junk.iter().map(|j| j.size).sum();
    let old: u64 = g.junk.iter().filter(|j| j.project_modified < cutoff).map(|j| j.size).sum();
    let checked: Vec<(PathBuf, u64)> = g.junk.iter().filter(|j| g.junk_checked.contains(&j.path)).map(|j| (j.path.clone(), j.size)).collect();
    let checked_size: u64 = checked.iter().map(|c| c.1).sum();

    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Recommend removing builds of projects not changed for")).color(C::dim(ui)));
        let before = g.settings.junk_days;
        w::segmented(ui, &mut g.settings.junk_days, &[(7, tr("1 week")), (30, tr("1 month")), (90, tr("3 months")), (365, tr("1 year"))]);
        if before != g.settings.junk_days {
            g.save_settings();
            g.junk_dirty = true;
            g.refresh_junk();
        }
    });
    ui.add_space(w::sp::XS);
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(trf("{1}: {0}, {2} in total", &[&fmt::n(g.junk.len() as u64, fmt::Noun::BuildFolder), &root, &fmt::bytes(total)]))
                        .color(C::dim(ui)),
                );
                ui.label(RichText::new(trf("{0} in inactive projects", &[&fmt::bytes(old)])).metric().color(C::text(ui)));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = trf("Move {0} to Trash · {1}", &[&checked.len(), &fmt::bytes(checked_size)]);
                if w::big_button(ui, &label, C::red(), !checked.is_empty()).clicked() {
                    let size = checked_size;
                    let paths: Vec<PathBuf> = checked.iter().map(|c| c.0.clone()).collect();
                    let lines = vec![
                        (format!("{}, {}", fmt::n(paths.len() as u64, fmt::Noun::Folder), fmt::bytes(size)), Level::Info),
                        (tr("They are recreated by the next build or install (npm install, cargo build, gradle…).").into(), Level::Ok),
                        (tr("Everything goes to the Trash and can be restored until the Trash is emptied.").into(), Level::Ok),
                    ];
                    g.confirm = Some(Confirm::new(tr("Remove build folders?"), lines, Action::Trash { paths, size }, tr("Move to Trash")));
                }
                if w::plain_button(ui, tr("Select all")).clicked() {
                    g.junk_checked = g.junk.iter().map(|j| j.path.clone()).collect();
                }
                if w::plain_button(ui, tr("Select inactive")).clicked() {
                    g.junk_checked = g.junk.iter().filter(|j| j.project_modified < cutoff).map(|j| j.path.clone()).collect();
                }
            });
        });
        ui.label(
            RichText::new(tr("Build output and dependencies can always be recreated. What matters is when the project itself was last changed — builds of old projects are pure junk."))
                .caption()
                .color(C::dim(ui)),
        );
    });
    ui.add_space(w::sp::S);
    if g.junk.is_empty() {
        w::empty(ui, tr("No build folders found."));
        return;
    }
    let mut toggle = None;
    TableBuilder::new(ui)
        .striped(true)
        .sense(Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(28.0))
        .column(Column::remainder().at_least(240.0).clip(true))
        .column(Column::exact(170.0))
        .column(Column::exact(86.0))
        .column(Column::exact(150.0))
        .header(24.0, |mut h| {
            for t in ["", tr("Project"), tr("Kind"), tr("Size"), tr("Project changed")] {
                h.col(|ui| {
                    ui.label(RichText::new(t).semibold().color(C::dim(ui)));
                });
            }
        })
        .body(|body| {
            body.rows(44.0, g.junk.len(), |mut row| {
                let j = &g.junk[row.index()];
                row.col(|ui| {
                    let mut on = g.junk_checked.contains(&j.path);
                    if ui.checkbox(&mut on, "").changed() {
                        toggle = Some(j.path.clone());
                    }
                });
                row.col(|ui| {
                    w::file_icon(ui, true, false, false);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let name = j.project.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        ui.label(RichText::new(name).semibold());
                        ui.label(RichText::new(fmt::path(&j.path)).caption().color(C::dim(ui)));
                    });
                });
                row.col(|ui| {
                    ui.label(RichText::new(j.kind).color(C::dim(ui)));
                });
                row.col(|ui| {
                    ui.label(RichText::new(fmt::bytes(j.size)).color(w::size_color(ui, j.size)));
                });
                row.col(|ui| {
                    let inactive = j.project_modified < cutoff;
                    ui.label(RichText::new(fmt::ago(j.project_modified)).color(if inactive { C::green() } else { C::dim(ui) })).on_hover_text(
                        if inactive {
                            tr("Inactive project — its build is safe to remove")
                        } else {
                            tr("Active project — it will be rebuilt on the next build")
                        },
                    );
                });
                row.response().context_menu(|ui| {
                    if ui.button(tr("Show in Finder")).clicked() {
                        macpilot::trash::reveal_in_finder(&j.path);
                        ui.close();
                    }
                });
            });
        });
    if let Some(p) = toggle {
        if !g.junk_checked.remove(&p) {
            g.junk_checked.insert(p);
        }
    }
}
