//! Apps page: uninstall apps with their leftovers, and clean leftovers of removed apps.

use std::path::PathBuf;

use eframe::egui::{self, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use macpilot::apps::AppInfo;
use macpilot::{disk, fmt, tr, trf};

use crate::widgets::{self as w, C, Level};
use crate::{Action, AppsMode, Confirm, Gui};

pub fn show(g: &mut Gui, ui: &mut Ui) {
    if g.apps_mode == AppsMode::Installed {
        egui::Panel::right("app_detail").resizable(true).default_size(380.0).min_size(320.0).frame(w::side_frame(ui)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| detail(g, ui));
        });
    }
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Apps")).size(24.0).strong());
            ui.add_space(12.0);
            w::segmented(ui, &mut g.apps_mode, &[(AppsMode::Installed, tr("Installed")), (AppsMode::Leftovers, tr("Leftovers of removed apps"))]);
        });
        ui.add_space(8.0);
        match g.apps_mode {
            AppsMode::Installed => installed(g, ui),
            AppsMode::Leftovers => leftovers(g, ui),
        }
    });
}

fn installed(g: &mut Gui, ui: &mut Ui) {
    let Some(apps) = g.apps.clone() else {
        w::waiting(ui, tr("Measuring apps…"));
        return;
    };
    let total: u64 = apps.iter().map(|a| a.size).sum();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut g.apps_filter).hint_text(tr("Search apps…")).desired_width(260.0).margin(egui::Margin::symmetric(8, 6)),
        );
        ui.label(RichText::new(trf("{0} apps, {1}", &[&apps.len(), &fmt::bytes(total)])).color(C::dim(ui)));
    });
    ui.add_space(6.0);
    let f = g.apps_filter.to_lowercase();
    let list: Vec<&AppInfo> =
        apps.iter().filter(|a| f.is_empty() || a.name.to_lowercase().contains(&f) || a.bundle_id.to_lowercase().contains(&f)).collect();
    let snap = g.snap.clone();
    let mut select = None;
    TableBuilder::new(ui)
        .striped(true)
        .sense(Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(240.0).clip(true))
        .column(Column::exact(90.0))
        .column(Column::exact(86.0))
        .column(Column::exact(150.0))
        .column(Column::exact(90.0))
        .header(24.0, |mut h| {
            for t in [tr("App"), tr("Version"), tr("Size"), tr("Last opened"), ""] {
                h.col(|ui| {
                    ui.label(RichText::new(t).strong().color(C::dim(ui)));
                });
            }
        })
        .body(|body| {
            body.rows(30.0, list.len(), |mut row| {
                let a = list[row.index()];
                row.set_selected(g.app_sel.as_ref() == Some(&a.path));
                row.col(|ui| {
                    w::file_icon(ui, true, true, false);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(RichText::new(&a.name).strong());
                        ui.label(RichText::new(&a.bundle_id).size(11.0).color(C::dim(ui)));
                    });
                });
                row.col(|ui| {
                    ui.label(RichText::new(&a.version).color(C::dim(ui)));
                });
                row.col(|ui| {
                    ui.label(RichText::new(fmt::bytes(a.size)).color(w::size_color(ui, a.size)));
                });
                row.col(|ui| w::time_cell(ui, a.last_used));
                row.col(|ui| {
                    if macpilot::apps::is_running(&a.path, &snap) {
                        w::badge(ui, tr("running"), C::GREEN);
                    } else if a.protected {
                        w::badge(ui, tr("built-in"), C::dim(ui));
                    }
                });
                if row.response().clicked() {
                    select = Some(a.clone());
                }
            });
        });
    if let Some(a) = select {
        g.app_sel = Some(a.path.clone());
        g.load_leftovers(&a);
    }
}

fn detail(g: &mut Gui, ui: &mut Ui) {
    let Some(app) = g.app_sel.as_ref().and_then(|p| g.apps.as_ref()?.iter().find(|a| &a.path == p)).cloned() else {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(tr("Select an app")).size(16.0).color(C::dim(ui)));
            ui.add_space(6.0);
            ui.label(RichText::new(tr("You will see how much space it takes with its data, and can uninstall it completely.")).color(C::dim(ui)));
        });
        return;
    };
    ui.horizontal(|ui| {
        w::file_icon(ui, true, true, false);
        ui.label(RichText::new(&app.name).size(22.0).strong());
    });
    ui.label(RichText::new(format!("{} · {}", app.bundle_id, app.version)).color(C::dim(ui)));
    ui.add_space(8.0);
    let left = g.leftovers.get(&app.path).cloned();
    let left_size: u64 = left.iter().flatten().map(|(_, s)| s).sum();
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        w::kv(ui, tr("App"), fmt::bytes(app.size));
        w::kv(ui, tr("Its data"), if left.is_some() { fmt::bytes(left_size) } else { tr("measuring…").into() });
        w::kv(ui, tr("Last opened"), app.last_used.map(fmt::ago).unwrap_or(tr("unknown").into()));
        w::kv(ui, tr("Location"), fmt::path(&app.path));
    });
    if let Some(t) = app.last_used.filter(|t| disk::now_unix() - t > 180 * 86_400) {
        ui.add_space(8.0);
        w::note(ui, C::YELLOW, &trf("Not opened for {0}", &[&fmt::age_in(t)]), tr("If you no longer use it, uninstall it together with its data."));
    }
    ui.add_space(10.0);
    if app.protected {
        w::note(ui, C::RED, tr("Built into macOS"), tr("This app is part of the system and cannot be removed."));
        return;
    }
    let running = macpilot::apps::is_running(&app.path, &g.snap);
    let mut chosen: Vec<(PathBuf, u64)> = vec![(app.path.clone(), app.size)];
    if let Some(l) = &left {
        chosen.extend(l.iter().filter(|(p, _)| g.leftover_checked.contains(p)).cloned());
    }
    let size: u64 = chosen.iter().map(|c| c.1).sum();
    ui.horizontal_wrapped(|ui| {
        let r = w::big_button(ui, &trf("Uninstall · {0}", &[&fmt::bytes(size)]), C::RED, !running)
            .on_disabled_hover_text(tr("Quit the app first (Processes page)."));
        if r.clicked() {
            let mut lines =
                vec![(trf("“{0}” and {1} of its files and folders — {2}.", &[&app.name, &(chosen.len() - 1), &fmt::bytes(size)]), Level::Info)];
            for (p, s) in chosen.iter().skip(1).take(10) {
                lines.push((format!("• {} — {}", fmt::path(p), fmt::bytes(*s)), Level::Info));
            }
            lines.push((tr("Settings and data of the app are removed too. Everything goes to the Trash and can be restored.").into(), Level::Ok));
            if app.path.starts_with("/Applications") {
                lines.push((tr("Finder may ask for your password if the app was installed for all users.").into(), Level::Info));
            }
            let paths = chosen.iter().map(|c| c.0.clone()).collect();
            g.confirm = Some(Confirm::new(trf("Uninstall “{0}”?", &[&app.name]), lines, Action::Trash { paths, size }, tr("Uninstall")));
        }
        if w::plain_button(ui, tr("Show in Finder")).clicked() {
            macpilot::trash::reveal_in_finder(&app.path);
        }
    });
    if running {
        ui.label(RichText::new(tr("The app is running — quit it before uninstalling.")).color(C::YELLOW));
    }
    ui.add_space(12.0);
    ui.label(RichText::new(tr("Files it keeps in your Library")).strong());
    ui.add_space(4.0);
    match left {
        None => {
            ui.spinner();
        }
        Some(l) if l.is_empty() => {
            ui.label(RichText::new(tr("Nothing found.")).color(C::dim(ui)));
        }
        Some(l) => {
            let mut toggle = None;
            for (p, s) in &l {
                ui.horizontal(|ui| {
                    let mut on = g.leftover_checked.contains(p);
                    if ui.checkbox(&mut on, "").changed() {
                        toggle = Some(p.clone());
                    }
                    ui.add_sized([70.0, 18.0], egui::Label::new(RichText::new(fmt::bytes(*s)).color(w::size_color(ui, *s))));
                    ui.add(egui::Label::new(fmt::path(p)).truncate()).on_hover_text(fmt::path(p));
                });
            }
            if let Some(p) = toggle {
                if !g.leftover_checked.remove(&p) {
                    g.leftover_checked.insert(p);
                }
            }
        }
    }
}

fn leftovers(g: &mut Gui, ui: &mut Ui) {
    let Some(orphans) = g.orphans.clone() else {
        w::waiting(ui, tr("Looking for leftovers…"));
        return;
    };
    let total: u64 = orphans.iter().map(|o| o.size).sum();
    let checked: Vec<(PathBuf, u64)> = orphans.iter().filter(|o| g.orphans_checked.contains(&o.path)).map(|o| (o.path.clone(), o.size)).collect();
    let checked_size: u64 = checked.iter().map(|c| c.1).sum();
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(trf("{0} of apps that are no longer installed", &[&fmt::n(orphans.len() as u64, fmt::Noun::Folder)]))
                        .color(C::dim(ui)),
                );
                ui.label(RichText::new(fmt::bytes(total)).size(24.0).strong().color(C::YELLOW));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w::big_button(ui, &trf("Move {0} to Trash · {1}", &[&checked.len(), &fmt::bytes(checked_size)]), C::RED, !checked.is_empty())
                    .clicked()
                {
                    let items = checked.iter().map(|(p, s)| (p.clone(), *s, String::new())).collect();
                    crate::disk_view::ask_trash_many(g, items, "");
                }
                if w::plain_button(ui, tr("Select all")).on_hover_text(tr("Folders that may contain your documents are not selected")).clicked() {
                    g.orphans_checked = orphans.iter().filter(|o| !o.may_hold_documents()).map(|o| o.path.clone()).collect();
                }
            });
        });
        ui.label(
            RichText::new(tr("Found by bundle id in Containers, Application Support, Caches and Saved Application State. Apple's own data and data of installed apps are skipped. Check the list: a few helpers live outside the Applications folder."))
                .size(11.5)
                .color(C::dim(ui)),
        );
    });
    ui.add_space(6.0);
    if orphans.is_empty() {
        w::empty(ui, tr("No leftovers found 👍"));
        return;
    }
    let mut toggle = None;
    TableBuilder::new(ui)
        .striped(true)
        .sense(Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(28.0))
        .column(Column::remainder().at_least(260.0).clip(true))
        .column(Column::exact(86.0))
        .column(Column::exact(140.0))
        .header(24.0, |mut h| {
            for t in ["", tr("Belonged to"), tr("Size"), tr("Changed")] {
                h.col(|ui| {
                    ui.label(RichText::new(t).strong().color(C::dim(ui)));
                });
            }
        })
        .body(|body| {
            body.rows(34.0, orphans.len(), |mut row| {
                let o = &orphans[row.index()];
                row.col(|ui| {
                    let mut on = g.orphans_checked.contains(&o.path);
                    if ui.checkbox(&mut on, "").changed() {
                        toggle = Some(o.path.clone());
                    }
                });
                row.col(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&o.id).strong());
                            if o.may_hold_documents() {
                                w::badge(ui, tr("may contain your documents"), C::YELLOW)
                                    .on_hover_text(tr("Sandboxed apps keep files you created (images, projects, notes) inside their container. Look inside before removing."));
                            }
                        });
                        ui.label(RichText::new(fmt::path(&o.path)).size(11.0).color(C::dim(ui)));
                    });
                });
                row.col(|ui| {
                    ui.label(RichText::new(fmt::bytes(o.size)).color(w::size_color(ui, o.size)));
                });
                row.col(|ui| {
                    let m = std::fs::symlink_metadata(&o.path).ok().map(|m| std::os::unix::fs::MetadataExt::mtime(&m));
                    w::time_cell(ui, m);
                });
                row.response().context_menu(|ui| {
                    if ui.button(tr("Show in Finder")).clicked() {
                        macpilot::trash::reveal_in_finder(&o.path);
                        ui.close();
                    }
                });
            });
        });
    if let Some(p) = toggle {
        if !g.orphans_checked.remove(&p) {
            g.orphans_checked.insert(p);
        }
    }
}
