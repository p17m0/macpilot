//! Settings page.

use eframe::egui::{self, RichText, Ui};
use macpilot::i18n::Lang;
use macpilot::settings::Theme;
use macpilot::{tr, trf};

use crate::widgets::{self as w, C, Level};
use crate::{Gui, autostart};

fn row(ui: &mut Ui, title: &str, hint: &str, add: impl FnOnce(&mut Ui)) {
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width() - 330.0);
                ui.label(RichText::new(title).strong());
                if !hint.is_empty() {
                    ui.label(RichText::new(hint).size(12.0).color(C::dim(ui)));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
        });
    });
    ui.add_space(6.0);
}

pub fn show(g: &mut Gui, ui: &mut Ui) {
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.set_max_width(820.0);
            w::header(ui, tr("Settings"), "");

            let mut lang_changed = false;
            row(ui, tr("Language"), tr("Language of the interface. “System” follows macOS."), |ui| {
                let current = g.settings.lang;
                let label = current.map(|l| l.native_name()).unwrap_or(tr("System"));
                egui::ComboBox::from_id_salt("lang").selected_text(label).width(160.0).show_ui(ui, |ui| {
                    if ui.selectable_label(current.is_none(), tr("System")).clicked() {
                        g.settings.lang = None;
                        lang_changed = true;
                    }
                    for l in Lang::ALL {
                        if ui.selectable_label(current == Some(l), l.native_name()).clicked() {
                            g.settings.lang = Some(l);
                            lang_changed = true;
                        }
                    }
                });
            });
            if lang_changed {
                g.language_changed();
            }

            let mut theme = g.settings.theme;
            row(ui, tr("Appearance"), "", |ui| {
                w::segmented(ui, &mut theme, &[(Theme::Dark, tr("Dark")), (Theme::Light, tr("Light")), (Theme::System, tr("System"))]);
            });
            if theme != g.settings.theme {
                g.settings.theme = theme;
                crate::apply_theme(ui.ctx(), theme);
                g.save_settings();
            }

            let mut auto = g.autostart;
            row(ui, tr("Open at login"), tr("Start MacPilot automatically when you log in."), |ui| {
                w::switch(ui, &mut auto, tr("Open at login"));
            });
            if auto != g.autostart {
                match autostart::set(auto) {
                    Ok(()) => g.autostart = auto,
                    Err(e) => g.toast(trf("Could not change launch at login: {0}", &[&e]), Level::Danger),
                }
            }

            if g.autostart && autostart::needs_approval() {
                w::note(ui, C::YELLOW, tr("Launch at login is switched off in System Settings"), tr("Turn MacPilot on in System Settings → General → Login Items."));
                if ui.button(tr("Login Items settings…")).clicked() {
                    crate::mac::open_login_items_settings();
                }
            }

            let mut bar = g.settings.menu_bar;
            row(
                ui,
                tr("Show in the menu bar"),
                tr("CPU and memory at a glance. Closing the window keeps MacPilot there; quit with ⌘Q or from its menu."),
                |ui| {
                    w::switch(ui, &mut bar, tr("Show in the menu bar"));
                },
            );
            if bar != g.settings.menu_bar {
                g.settings.menu_bar = bar;
                g.save_settings();
            }

            let mut scan = g.settings.scan_on_start;
            row(ui, tr("Scan the home folder at launch"), tr("Needed for the Overview, Cleanup and “Not used” lists. Runs at low priority."), |ui| {
                w::switch(ui, &mut scan, tr("Scan the home folder at launch"));
            });
            if scan != g.settings.scan_on_start {
                g.settings.scan_on_start = scan;
                g.save_settings();
            }

            let mut stale = g.settings.stale_days;
            row(ui, tr("“Not used” threshold"), tr("Items not opened or changed for longer than this are listed."), |ui| {
                w::segmented(ui, &mut stale, &[(730, tr("2 years")), (365, tr("1 year")), (180, tr("6 months")), (90, tr("3 months"))]);
            });
            if stale != g.settings.stale_days {
                g.settings.stale_days = stale;
                g.stale_days = stale;
                g.stale_dirty = true;
                g.save_settings();
            }

            let mut junk = g.settings.junk_days;
            row(ui, tr("Inactive project after"), tr("Build folders of projects not changed for this long are pre-selected for removal."), |ui| {
                w::segmented(ui, &mut junk, &[(365, tr("1 year")), (90, tr("3 months")), (30, tr("1 month")), (7, tr("1 week"))]);
            });
            if junk != g.settings.junk_days {
                g.settings.junk_days = junk;
                g.junk_dirty = true;
                g.save_settings();
            }

            let mut mb = g.settings.dupes_min_mb;
            row(ui, tr("Duplicates: ignore files smaller than"), "", |ui| {
                w::segmented(ui, &mut mb, &[(100, "100 MB"), (10, "10 MB"), (1, "1 MB")]);
            });
            if mb != g.settings.dupes_min_mb {
                g.settings.dupes_min_mb = mb;
                g.save_settings();
            }

            ui.add_space(10.0);
            w::card(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new(tr("Permissions")).strong());
                ui.label(RichText::new(tr("For a complete picture give MacPilot Full Disk Access. When you first remove something, allow MacPilot to control Finder — then “Put Back” works in the Trash.")).color(C::dim(ui)));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if w::plain_button(ui, tr("Open Full Disk Access settings…")).clicked() {
                        macpilot::open_full_disk_access_settings();
                    }
                    if g.full_disk_access {
                        w::badge(ui, tr("granted"), C::GREEN);
                    } else {
                        w::badge(ui, tr("not granted"), C::YELLOW);
                    }
                });
            });
            updates(g, ui);
            ui.add_space(10.0);
            ui.label(RichText::new(format!("MacPilot {} · MIT License", env!("CARGO_PKG_VERSION"))).color(C::dim(ui)));
        });
    });
}

fn updates(g: &mut Gui, ui: &mut Ui) {
    let Some(_) = macpilot::update::repo() else { return };
    ui.add_space(12.0);
    if let Some(u) = g.update.clone() {
        w::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(trf("MacPilot {0} is available", &[&u.version])).strong().color(C::GREEN));
                    ui.label(
                        RichText::new(trf(
                            "You have {0}. Download the new version and replace the app in Applications.",
                            &[&env!("CARGO_PKG_VERSION")],
                        ))
                        .size(12.0)
                        .color(C::dim(ui)),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new(tr("Download")).color(egui::Color32::WHITE)).fill(C::ACCENT)).clicked() {
                        let _ = std::process::Command::new("open").arg(&u.url).spawn();
                    }
                });
            });
        });
        ui.add_space(8.0);
    }
    let mut on = g.settings.check_updates;
    let status = if g.update_checking {
        tr("Checking…").to_string()
    } else if let Some(e) = &g.update_error {
        trf("Could not check: {0}", &[e])
    } else if g.update.is_none() && g.settings.check_updates {
        tr("You have the latest version.").to_string()
    } else {
        String::new()
    };
    let hint = format!("{} {}", tr("Once a day MacPilot asks GitHub for the latest release. Nothing else is sent."), status);
    row(ui, tr("Check for updates"), &hint, |ui| {
        w::switch(ui, &mut on, tr("Check for updates"));
        if ui.add_enabled(!g.update_checking, egui::Button::new(tr("Check now"))).clicked() {
            g.check_updates();
        }
    });
    if on != g.settings.check_updates {
        g.settings.check_updates = on;
        g.save_settings();
    }
}
