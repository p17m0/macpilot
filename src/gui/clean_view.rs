//! Cleanup page: system junk and developer junk.

use std::path::PathBuf;

use eframe::egui::{self, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use macpilot::clean::{self, Kind};
use macpilot::disk::{self, DelSafety};
use macpilot::{fmt, tr, trf};

use crate::widgets::{self as w, C, Level};
use crate::{Action, CleanMode, Confirm, DiskMode, Gui, Page};

pub fn show(g: &mut Gui, ui: &mut Ui) {
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Cleanup")).size(24.0).strong());
            ui.add_space(12.0);
            w::segmented(ui, &mut g.clean_mode, &[(CleanMode::System, tr("System junk")), (CleanMode::Dev, tr("Developer junk"))]);
        });
        ui.add_space(8.0);
        match g.clean_mode {
            CleanMode::System => system(g, ui),
            CleanMode::Dev => dev(g, ui),
        }
    });
}

fn system(g: &mut Gui, ui: &mut Ui) {
    let safe_total: u64 = g.targets.iter().filter(|t| t.cleanable()).filter_map(|t| t.stat.map(|s| s.size)).sum();
    let measuring = g.targets.iter().any(|t| t.stat.is_none() && t.path.exists());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(tr("Can be freed safely")).color(C::dim(ui)));
            ui.horizontal(|ui| {
                ui.label(RichText::new(fmt::bytes(safe_total)).size(30.0).strong().color(C::GREEN));
                if measuring {
                    ui.spinner();
                }
            });
            ui.label(RichText::new(tr("Everything goes to the Trash. The space is freed when you empty it.")).color(C::dim(ui)));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr("⟳ Measure again")).clicked() {
                for i in 0..g.targets.len() {
                    g.remeasure(i);
                }
            }
        });
    });
    ui.add_space(10.0);
    stale_banner(g, ui);
    ui.add_space(8.0);
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let cols = ((ui.available_width() / 360.0).floor() as usize).clamp(1, 4);
        // Places that do not exist on this Mac are not shown.
        let shown: Vec<usize> = (0..g.targets.len())
            .filter(|i| {
                let t = &g.targets[*i];
                // Empty places are hidden too; the Trash card always stays.
                t.kind == Kind::Trash || (t.path.exists() && t.stat.is_none_or(|s| s.size > 0))
            })
            .collect();
        let n = shown.len();
        let mut action: Option<(usize, CardAction)> = None;
        for start in (0..n).step_by(cols) {
            ui.columns(cols, |columns| {
                for (ci, col) in columns.iter_mut().enumerate() {
                    let k = start + ci;
                    if k < n {
                        let i = shown[k];
                        if let Some(a) = target_card(g, col, i) {
                            action = Some((i, a));
                        }
                    }
                }
            });
            ui.add_space(8.0);
        }
        if let Some((i, a)) = action {
            match a {
                CardAction::Clean => ask_clean(g, i),
                CardAction::Open => {
                    let p = g.targets[i].path.clone();
                    g.go_page(Page::Disk);
                    g.disk_mode = DiskMode::List;
                    g.go(p);
                }
                CardAction::EmptyTrash => ask_empty_trash(g, i),
            }
        }
    });
}

enum CardAction {
    Clean,
    Open,
    EmptyTrash,
}

fn target_card(g: &Gui, ui: &mut Ui, i: usize) -> Option<CardAction> {
    let t = &g.targets[i];
    let exists = t.path.exists();
    let mut out = None;
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.set_min_height(150.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(t.label).size(15.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match t.kind {
                Kind::Clean => {
                    w::del_badge(ui, clean::target_safety(t));
                }
                Kind::Manual => {
                    w::badge(ui, tr("by hand"), C::YELLOW);
                }
                Kind::Trash => {}
            });
        });
        let (txt, color) = match (exists, t.stat) {
            (false, _) => (tr("none").to_string(), C::dim(ui)),
            (true, Some(s)) => (fmt::bytes(s.size), w::size_color(ui, s.size)),
            (true, None) => (tr("measuring…").to_string(), C::dim(ui)),
        };
        ui.label(RichText::new(txt).size(22.0).strong().color(color));
        ui.label(RichText::new(fmt::path(&t.path)).size(11.5).color(C::dim(ui)));
        ui.add_space(2.0);
        ui.label(RichText::new(t.hint).color(C::dim(ui)));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let has = exists && t.stat.is_some_and(|s| s.size > 0);
            match t.kind {
                Kind::Clean => {
                    if w::big_button(ui, tr("Clean"), C::GREEN, has).clicked() {
                        out = Some(CardAction::Clean);
                    }
                }
                Kind::Trash => {
                    if w::big_button(ui, tr("Empty Trash…"), C::RED, has).clicked() {
                        out = Some(CardAction::EmptyTrash);
                    }
                }
                Kind::Manual => {}
            }
            if t.kind != Kind::Trash
                && ui.add_enabled(exists, egui::Button::new(tr("Open")).corner_radius(8).min_size(egui::vec2(0.0, 32.0))).clicked()
            {
                out = Some(CardAction::Open);
            }
        });
    });
    out
}

fn ask_clean(g: &mut Gui, i: usize) {
    let t = g.targets[i].clone();
    let children: Vec<PathBuf> = match std::fs::read_dir(&t.path) {
        Ok(rd) => rd.flatten().map(|e| e.path()).filter(|c| disk::deletion_safety(c).0 != DelSafety::Blocked).collect(),
        Err(e) => {
            g.toast(format!("{}: {}", fmt::path(&t.path), disk::perm_hint(&e)), Level::Danger);
            return;
        }
    };
    if children.is_empty() {
        g.toast(trf("“{0}” is already empty.", &[&t.label]), Level::Info);
        return;
    }
    let st = t.stat.unwrap_or_default();
    let mut lines = vec![
        (format!("{} — {}, {}", fmt::path(&t.path), fmt::n(children.len() as u64, fmt::Noun::Item), fmt::bytes(st.size)), Level::Info),
        (t.hint.to_string(), Level::Info),
        (tr("The contents go to the Trash (the folder itself stays).").into(), Level::Ok),
    ];
    if t.id == "caches" {
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
    g.confirm = Some(Confirm::new(trf("Clean “{0}”?", &[&t.label]), lines, Action::Trash { paths: children, size: st.size }, tr("Clean")));
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
            C::YELLOW,
            tr("Partial results"),
            tr("The scan is waiting for a macOS permission dialog. Answer it, or give MacPilot Full Disk Access — the list will be completed."),
        );
        ui.add_space(6.0);
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
                ui.label(RichText::new(tr("Not used for a long time")).size(15.0).strong());
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
                if w::big_button(ui, tr("Review"), C::ACCENT, true).clicked() {
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
    ui.add_space(4.0);
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(trf("{1}: {0}, {2} in total", &[&fmt::n(g.junk.len() as u64, fmt::Noun::BuildFolder), &root, &fmt::bytes(total)]))
                        .color(C::dim(ui)),
                );
                ui.label(RichText::new(trf("{0} in inactive projects", &[&fmt::bytes(old)])).size(24.0).strong().color(C::GREEN));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = trf("Move {0} to Trash · {1}", &[&checked.len(), &fmt::bytes(checked_size)]);
                if w::big_button(ui, &label, C::RED, !checked.is_empty()).clicked() {
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
                .size(11.5)
                .color(C::dim(ui)),
        );
    });
    ui.add_space(6.0);
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
                    ui.label(RichText::new(t).strong().color(C::dim(ui)));
                });
            }
        })
        .body(|body| {
            body.rows(34.0, g.junk.len(), |mut row| {
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
                        ui.label(RichText::new(name).strong());
                        ui.label(RichText::new(fmt::path(&j.path)).size(11.0).color(C::dim(ui)));
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
                    ui.label(RichText::new(fmt::ago(j.project_modified)).color(if inactive { C::GREEN } else { C::dim(ui) })).on_hover_text(
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
