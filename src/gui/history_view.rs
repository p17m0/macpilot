//! History page: everything MacPilot did on this Mac, newest first.

use eframe::egui::{self, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use macpilot::actionlog::Kind;
use macpilot::{disk, fmt, tr};

use crate::widgets::{self as w, C, Txt};
use crate::{CleanMode, Gui, Page};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Cleanup,
    Processes,
    Other,
}

impl Filter {
    fn shows(self, k: Kind) -> bool {
        match self {
            Filter::All => true,
            Filter::Cleanup => k.is_cleanup(),
            Filter::Processes => k.is_process(),
            Filter::Other => !k.is_cleanup() && !k.is_process(),
        }
    }
}

/// "3 hours ago" for the last day, the date otherwise.
fn when(at: i64) -> String {
    if disk::now_unix() - at < 86_400 { fmt::ago(at) } else { fmt::date(at) }
}

pub fn show(g: &mut Gui, ui: &mut Ui) {
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        w::centered(ui, |ui| {
            let mut clear = false;
            let has_any = !g.actions.is_empty();
            w::title_bar(
                ui,
                tr("Activity"),
                "",
                |ui| {
                    w::segmented(
                        ui,
                        &mut g.history_filter,
                        &[
                            (Filter::All, tr("Everything")),
                            (Filter::Cleanup, tr("Cleanup")),
                            (Filter::Processes, tr("Processes")),
                            (Filter::Other, tr("Startup and apps")),
                        ],
                    );
                },
                |ui| {
                    if ui.add_enabled(has_any, egui::Button::new(tr("Clear history")).corner_radius(w::button_radius())).clicked() {
                        clear = true;
                    }
                },
            );
            if clear {
                macpilot::actionlog::clear(&mut g.actions);
            }
            let month = disk::now_unix() - 30 * 86_400;
            let trashed: u64 = g.actions.iter().filter(|e| e.at >= month && matches!(e.kind, Kind::Trash | Kind::AutoClean)).map(|e| e.size).sum();
            let mut to_trash_log = false;
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr("Moved to the Trash in the last 30 days")).color(C::dim(ui)));
                    ui.label(RichText::new(fmt::bytes(trashed)).metric().color(C::text(ui)));
                    ui.label(RichText::new(tr("What MacPilot did on this Mac, kept for 90 days. Nothing leaves the Mac.")).color(C::dim(ui)));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::plain_button(ui, tr("Put things back…"))
                        .on_hover_text(tr("The list of what is still in the Trash, with Put Back"))
                        .clicked()
                    {
                        to_trash_log = true;
                    }
                });
            });
            if to_trash_log {
                g.clean_mode = CleanMode::History;
                g.go_page(Page::Clean);
            }
            ui.add_space(w::sp::M);
            // Newest first.
            let rows: Vec<usize> = (0..g.actions.len()).rev().filter(|i| g.history_filter.shows(g.actions[*i].kind)).collect();
            if rows.is_empty() {
                w::empty(
                    ui,
                    if has_any {
                        tr("Nothing of this kind yet.")
                    } else {
                        tr("Nothing yet. Everything MacPilot does on this Mac will be listed here.")
                    },
                );
                return;
            }
            let log = &g.actions;
            TableBuilder::new(ui)
                .striped(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(130.0))
                .column(Column::exact(270.0).clip(true))
                .column(Column::remainder().at_least(200.0).clip(true))
                .column(Column::exact(90.0))
                .header(24.0, |mut h| {
                    for t in [tr("When"), tr("Action"), tr("What"), tr("Size")] {
                        h.col(|ui| {
                            ui.label(RichText::new(t).semibold().color(C::dim(ui)));
                        });
                    }
                })
                .body(|body| {
                    body.rows(30.0, rows.len(), |mut row| {
                        let e = &log[rows[row.index()]];
                        row.col(|ui| {
                            ui.label(RichText::new(when(e.at)).color(C::dim(ui))).on_hover_text(fmt::date(e.at));
                        });
                        row.col(|ui| {
                            let color = match e.kind {
                                Kind::EmptyTrash | Kind::ForceQuit | Kind::Snapshots => C::yellow(),
                                Kind::PutBack => C::green(),
                                _ => C::accent(),
                            };
                            w::dot(ui, color);
                            ui.label(e.kind.label());
                        });
                        row.col(|ui| {
                            // "12 items: a, b, c…" when it was about several things.
                            let text = match (e.count, e.what.is_empty()) {
                                (n, false) if n > 1 => format!("{}: {}", fmt::n(n, fmt::Noun::Item), e.what),
                                (n, true) if n > 0 => fmt::n(n, fmt::Noun::Item),
                                _ => e.what.clone(),
                            };
                            ui.label(RichText::new(&text).color(C::text(ui))).on_hover_text(text);
                        });
                        row.col(|ui| {
                            ui.label(RichText::new(if e.size > 0 { fmt::bytes(e.size) } else { "—".into() }).color(C::dim(ui)));
                        });
                    });
                });
        });
    });
}
