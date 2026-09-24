//! Startup page: launch agents and daemons.

use eframe::egui::{self, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use macpilot::startup::{Scope, StartupItem};
use macpilot::{fmt, tr, trf};

use crate::widgets::{self as w, C, Level};
use crate::{Action, Confirm, Gui};

fn scope_text(s: Scope) -> &'static str {
    match s {
        Scope::User => tr("you, at login"),
        Scope::AllUsers => tr("all users, at login"),
        Scope::System => tr("system, at boot"),
    }
}

pub fn show(g: &mut Gui, ui: &mut Ui) {
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Startup")).size(24.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w::plain_button(ui, tr("Login Items settings…")).on_hover_text(tr("Apps set to “Open at Login” are managed by macOS in System Settings.")).clicked() {
                    macpilot::startup::open_login_items_settings();
                }
                if w::plain_button(ui, tr("⟳ Refresh")).clicked() {
                    g.reload_startup();
                }
            });
        });
        ui.label(RichText::new(tr("Programs that start by themselves with your Mac. Turning one off is reversible — nothing is deleted.")).color(C::dim(ui)));
        ui.add_space(8.0);
        let Some(items) = g.startup.clone() else {
            w::waiting(ui, tr("Reading startup items…"));
            return;
        };
        let unwanted = items.iter().filter(|i| i.unwanted && !i.disabled).count();
        if unwanted > 0 {
            w::note(ui, C::RED, &trf("{0} unwanted item(s) found", &[&unwanted]), tr("MacKeeper and similar “cleaners” are known for nagging and fake alerts. Turn them off here and uninstall the app on the Apps page."));
            ui.add_space(6.0);
        }
        if items.is_empty() {
            w::empty(ui, tr("No startup items."));
            return;
        }
        let snap = g.snap.clone();
        let mut toggle: Option<(StartupItem, bool)> = None;
        let mut remove: Option<StartupItem> = None;
        TableBuilder::new(ui)
            .striped(true)
            .sense(Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(52.0))
            .column(Column::remainder().at_least(260.0).clip(true))
            .column(Column::exact(150.0))
            .column(Column::exact(150.0))
            .column(Column::exact(120.0))
            .header(24.0, |mut h| {
                for t in [tr("On"), tr("Item"), tr("Vendor"), tr("Starts for"), tr("Status")] {
                    h.col(|ui| {
                        ui.label(RichText::new(t).strong().color(C::dim(ui)));
                    });
                }
            })
            .body(|body| {
                body.rows(40.0, items.len(), |mut row| {
                    let it = &items[row.index()];
                    row.col(|ui| {
                        let mut on = !it.disabled;
                        if w::switch(ui, &mut on).on_hover_text(if it.disabled { tr("Turn on") } else { tr("Turn off") }).changed() {
                            toggle = Some((it.clone(), on));
                        }
                    });
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&it.label).strong());
                                if it.unwanted {
                                    w::badge(ui, tr("unwanted"), C::RED);
                                }
                                if it.broken {
                                    w::badge(ui, tr("broken"), C::YELLOW);
                                }
                            });
                            let prog = if it.program.is_empty() { fmt::path(&it.path) } else { it.program.clone() };
                            ui.label(RichText::new(prog).size(11.0).color(C::dim(ui)));
                        });
                    });
                    row.col(|ui| {
                        ui.label(&it.vendor);
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(scope_text(it.scope)).color(C::dim(ui)));
                    });
                    row.col(|ui| {
                        let running = !it.program.is_empty() && snap.procs.iter().any(|p| p.exe.as_deref().is_some_and(|e| e.to_string_lossy() == it.program));
                        if it.disabled {
                            w::badge(ui, tr("off"), C::dim(ui));
                        } else if running {
                            w::badge(ui, tr("running"), C::GREEN);
                        } else if it.broken {
                            w::badge(ui, tr("program missing"), C::YELLOW);
                        } else {
                            w::badge(ui, tr("not running"), C::dim(ui));
                        }
                    });
                    let p = it.path.clone();
                    let item = it.clone();
                    row.response().context_menu(|ui| {
                        if ui.button(tr("Show in Finder")).clicked() {
                            macpilot::trash::reveal_in_finder(&p);
                            ui.close();
                        }
                        if item.scope == Scope::User && ui.button(RichText::new(tr("Move to Trash…")).color(C::RED)).clicked() {
                            remove = Some(item.clone());
                            ui.close();
                        }
                    });
                });
            });
        if let Some((item, on)) = toggle {
            if on {
                g.execute(Action::Startup { item, on });
            } else {
                let mut lines = vec![
                    (trf("“{0}” will no longer start automatically.", &[&item.label]), Level::Info),
                    (trf("It runs: {0}", &[&item.program]), Level::Info),
                    (tr("You can turn it back on at any time.").into(), Level::Ok),
                ];
                if item.scope == Scope::System {
                    lines.push((tr("This is a system-wide item: macOS will ask for an administrator password. Features of its app (VPN, drivers, updates) may stop working.").into(), Level::Warn));
                }
                g.confirm = Some(Confirm::new(tr("Turn off this startup item?"), lines, Action::Startup { item, on: false }, tr("Turn off")));
            }
        }
        if let Some(item) = remove {
            let lines = vec![
                (fmt::path(&item.path), Level::Info),
                (tr("The item file goes to the Trash. Use this for leftovers of apps you already removed.").into(), Level::Ok),
            ];
            g.confirm = Some(Confirm::new(tr("Remove this startup item?"), lines, Action::Trash { paths: vec![item.path.clone()], size: 0 }, tr("Move to Trash")));
        }
    });
}
