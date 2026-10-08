//! Apps page: uninstall apps with their leftovers, and clean leftovers of removed apps.

use std::path::PathBuf;

use eframe::egui::{self, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use macpilot::apps::AppInfo;
use macpilot::{disk, fmt, tr, trf};

use crate::icons;
use crate::widgets::{self as w, C, Level, Txt};
use crate::{Action, AppsMode, Confirm, Gui};

pub fn show(g: &mut Gui, ui: &mut Ui) {
    if g.apps_mode == AppsMode::Installed {
        egui::Panel::right("app_detail").resizable(true).default_size(380.0).min_size(320.0).frame(w::side_frame(ui)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| detail(g, ui));
        });
    }
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        w::centered(ui, |ui| {
            w::title_bar(
                ui,
                tr("Apps"),
                "",
                |ui| {
                    w::segmented(
                        ui,
                        &mut g.apps_mode,
                        &[
                            (AppsMode::Installed, tr("Installed")),
                            (AppsMode::Updates, tr("Updates")),
                            (AppsMode::Leftovers, tr("Leftovers of removed apps")),
                        ],
                    );
                },
                |_| {},
            );
            match g.apps_mode {
                AppsMode::Installed => installed(g, ui),
                AppsMode::Updates => updates(g, ui),
                AppsMode::Leftovers => leftovers(g, ui),
            }
        });
    });
}

/// Newer versions of installed apps. Nothing is asked over the network until the button is pressed.
fn updates(g: &mut Gui, ui: &mut Ui) {
    use macpilot::appupdates::Source;
    if g.apps.is_none() {
        w::waiting(ui, tr("Measuring apps…"));
        return;
    }
    // For automated UI checks: MACPILOT_PAGE=apps:updates:check
    #[cfg(feature = "dev-tools")]
    if g.app_updates.is_none() && std::env::var("MACPILOT_PAGE").is_ok_and(|p| p.ends_with(":check")) {
        g.check_app_updates();
    }
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width() - 220.0);
            let status = match (&g.app_updates, g.app_updates_checking) {
                (_, true) => tr("Checking…").to_string(),
                (Some(r), _) if r.updates.is_empty() => trf("No updates found. Checked {0} of {1} apps.", &[&r.checked, &r.total]),
                (Some(r), _) => trf("Updates: {0}. Checked {1} of {2} apps.", &[&r.updates.len(), &r.checked, &r.total]),
                (None, _) => tr("Not checked yet.").to_string(),
            };
            ui.label(RichText::new(status).semibold());
            ui.label(
                RichText::new(tr(
                    "MacPilot asks the App Store about apps installed from it, Homebrew about its casks, and the update feeds of apps that have one. Only app identifiers are sent. Other apps cannot be checked — they update themselves.",
                ))
                .callout()
                .color(C::dim(ui)),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if g.app_updates_checking {
                ui.spinner();
            } else if w::big_button(ui, tr("Check now"), C::accent(), true).clicked() {
                g.check_app_updates();
            }
        });
    });
    ui.add_space(w::sp::M);
    let Some(report) = g.app_updates.clone() else { return };
    if report.updates.is_empty() {
        if !g.app_updates_checking {
            w::empty(ui, tr("Everything that could be checked is up to date."));
        }
        return;
    }
    let mut go = None;
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(220.0).clip(true))
        .column(Column::exact(110.0).clip(true))
        .column(Column::exact(110.0).clip(true))
        .column(Column::exact(110.0))
        .column(Column::exact(170.0))
        .header(24.0, |mut h| {
            for t in [tr("App"), tr("You have"), tr("Available"), tr("Source"), ""] {
                h.col(|ui| {
                    ui.label(RichText::new(t).semibold().color(C::dim(ui)));
                });
            }
        })
        .body(|body| {
            body.rows(40.0, report.updates.len(), |mut row| {
                let u = &report.updates[row.index()];
                row.col(|ui| {
                    icons::app(ui, &u.app, 24.0);
                    ui.label(RichText::new(&u.name).semibold());
                });
                row.col(|ui| {
                    ui.label(RichText::new(&u.installed).color(C::dim(ui)));
                });
                row.col(|ui| {
                    ui.label(RichText::new(&u.latest).color(C::green()));
                });
                row.col(|ui| {
                    let source = match u.source {
                        Source::AppStore { .. } => "App Store",
                        Source::Homebrew { .. } => "Homebrew",
                        Source::Sparkle => tr("the app itself"),
                    };
                    ui.label(RichText::new(source).color(C::dim(ui)));
                });
                row.col(|ui| {
                    if g.app_updating.contains(&u.app) {
                        ui.spinner();
                        return;
                    }
                    let (label, hover) = match u.source {
                        Source::AppStore { .. } => (tr("Open in App Store"), tr("The App Store installs the update")),
                        Source::Homebrew { .. } => (tr("Update"), tr("Runs `brew upgrade --cask` in the background")),
                        Source::Sparkle => (tr("Open the app"), tr("The app offers the update when it starts")),
                    };
                    if ui.button(label).on_hover_text(hover).clicked() {
                        go = Some(u.clone());
                    }
                });
            });
        });
    if let Some(u) = go {
        g.update_app(u);
    }
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
    ui.add_space(w::sp::S);
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
                    ui.label(RichText::new(t).semibold().color(C::dim(ui)));
                });
            }
        })
        .body(|body| {
            body.rows(44.0, list.len(), |mut row| {
                let a = list[row.index()];
                row.set_selected(g.app_sel.as_ref() == Some(&a.path));
                row.col(|ui| {
                    icons::app(ui, &a.path, 24.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(RichText::new(&a.name).semibold());
                        ui.label(RichText::new(&a.bundle_id).caption().color(C::dim(ui)));
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
                        w::badge(ui, tr("running"), C::green());
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
        ui.add_space(w::sp::XXL);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(tr("Select an app")).size(w::ty::HEADLINE).color(C::dim(ui)));
            ui.add_space(w::sp::S);
            ui.label(RichText::new(tr("You will see how much space it takes with its data, and can uninstall it completely.")).color(C::dim(ui)));
        });
        return;
    };
    ui.horizontal(|ui| {
        icons::app(ui, &app.path, 40.0);
        ui.label(RichText::new(&app.name).title());
    });
    ui.label(RichText::new(format!("{} · {}", app.bundle_id, app.version)).color(C::dim(ui)));
    ui.add_space(w::sp::S);
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
        ui.add_space(w::sp::S);
        w::note(ui, C::yellow(), &trf("Not opened for {0}", &[&fmt::age_in(t)]), tr("If you no longer use it, uninstall it together with its data."));
    }
    ui.add_space(w::sp::M);
    if app.protected {
        w::note(ui, C::red(), tr("Built into macOS"), tr("This app is part of the system and cannot be removed."));
        return;
    }
    let running = macpilot::apps::is_running(&app.path, &g.snap);
    let mut chosen: Vec<(PathBuf, u64)> = vec![(app.path.clone(), app.size)];
    if let Some(l) = &left {
        chosen.extend(l.iter().filter(|(p, _)| g.leftover_checked.contains(p)).cloned());
    }
    let size: u64 = chosen.iter().map(|c| c.1).sum();
    ui.horizontal_wrapped(|ui| {
        let r = w::big_button(ui, &trf("Uninstall · {0}", &[&fmt::bytes(size)]), C::red(), !running)
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
        ui.label(RichText::new(tr("The app is running — quit it before uninstalling.")).color(C::yellow()));
    }
    ui.add_space(w::sp::M);
    ui.label(RichText::new(tr("Files it keeps in your Library")).semibold());
    ui.add_space(w::sp::XS);
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

/// Which places of ~/Library a group of leftovers is in: "Application Support, Caches, Preferences".
fn places(o: &macpilot::apps::Orphan) -> String {
    let lib = macpilot::home().join("Library");
    let mut v: Vec<String> = Vec::new();
    for (p, _) in &o.items {
        let place =
            p.strip_prefix(&lib).ok().and_then(|r| r.components().next()).map(|c| c.as_os_str().to_string_lossy().to_string()).unwrap_or_default();
        if !place.is_empty() && !v.contains(&place) {
            v.push(place);
        }
    }
    v.join(", ")
}

fn leftovers(g: &mut Gui, ui: &mut Ui) {
    let Some(orphans) = g.orphans.clone() else {
        w::waiting(ui, tr("Looking for leftovers…"));
        return;
    };
    let total: u64 = orphans.iter().map(|o| o.size).sum();
    let items: usize = orphans.iter().map(|o| o.items.len()).sum();
    let chosen: Vec<&macpilot::apps::Orphan> = orphans.iter().filter(|o| g.orphans_checked.contains(o.key())).collect();
    let chosen_size: u64 = chosen.iter().map(|o| o.size).sum();
    let all_chosen = !orphans.is_empty() && chosen.len() == orphans.len();
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(trf(
                        "{0} of {1} that are no longer installed",
                        &[&fmt::n(items as u64, fmt::Noun::Item), &fmt::n(orphans.len() as u64, fmt::Noun::App)],
                    ))
                    .color(C::dim(ui)),
                );
                ui.label(RichText::new(fmt::bytes(total)).metric().color(C::yellow()));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if w::big_button(ui, &trf("Move {0} to Trash · {1}", &[&chosen.len(), &fmt::bytes(chosen_size)]), C::red(), !chosen.is_empty())
                    .clicked()
                {
                    let note = tr("may contain your documents");
                    let list = chosen
                        .iter()
                        .flat_map(|o| {
                            let docs = o.may_hold_documents();
                            o.items.iter().map(move |(p, s)| (p.clone(), s.unwrap_or(0), if docs { note.to_string() } else { String::new() }))
                        })
                        .collect();
                    crate::disk_view::ask_trash_many(g, list, "{0}");
                }
                if all_chosen {
                    if w::plain_button(ui, tr("Deselect all")).clicked() {
                        g.orphans_checked.clear();
                    }
                } else if w::plain_button(ui, tr("Select all")).clicked() {
                    g.orphans_checked = orphans.iter().map(|o| o.key().to_path_buf()).collect();
                }
            });
        });
        ui.label(
            RichText::new(tr("Found by bundle id in every place apps keep data in your Library (Containers, Application Support, Caches, Logs, Preferences, WebKit, cookies…) and by the app's name. Apple's data, installed apps and their helpers are skipped."))
                .caption()
                .color(C::dim(ui)),
        );
    });
    ui.add_space(w::sp::S);
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
        .column(Column::exact(96.0))
        .column(Column::exact(140.0))
        .header(24.0, |mut h| {
            for t in ["", tr("Belonged to"), tr("Size"), tr("Changed")] {
                h.col(|ui| {
                    ui.label(RichText::new(t).semibold().color(C::dim(ui)));
                });
            }
        })
        .body(|body| {
            body.rows(44.0, orphans.len(), |mut row| {
                let o = &orphans[row.index()];
                row.col(|ui| {
                    let mut on = g.orphans_checked.contains(o.key());
                    if ui.checkbox(&mut on, "").changed() {
                        toggle = Some(o.key().to_path_buf());
                    }
                });
                row.col(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&o.name).semibold());
                            ui.label(RichText::new(&o.id).caption().color(C::dim(ui)));
                            if o.may_hold_documents() {
                                w::badge(ui, tr("may contain your documents"), C::yellow())
                                    .on_hover_text(tr("Sandboxed apps keep files you created (images, projects, notes) inside their container. Look inside before removing."));
                            }
                        });
                        let all: Vec<String> = o.items.iter().map(|(p, _)| fmt::path(p)).collect();
                        ui.label(RichText::new(format!("{} · {}", fmt::n(o.items.len() as u64, fmt::Noun::Item), places(o))).caption().color(C::dim(ui)))
                            .on_hover_text(all.join("\n"));
                    });
                });
                row.col(|ui| {
                    if o.size_unknown() && o.size == 0 {
                        ui.label(RichText::new(tr("unknown")).color(C::dim(ui))).on_hover_text(tr("Inside another app's container: macOS shows its size only with Full Disk Access."));
                    } else {
                        ui.label(RichText::new(fmt::bytes(o.size)).color(w::size_color(ui, o.size)));
                    }
                });
                row.col(|ui| {
                    let m = o.items.iter().filter_map(|(p, _)| std::fs::symlink_metadata(p).ok()).map(|m| std::os::unix::fs::MetadataExt::mtime(&m)).max();
                    w::time_cell(ui, m);
                });
                row.response().context_menu(|ui| {
                    for (p, _) in &o.items {
                        if ui.button(trf("Show {0} in Finder", &[&fmt::path(p)])).clicked() {
                            macpilot::trash::reveal_in_finder(p);
                            ui.close();
                        }
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
