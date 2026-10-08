//! Settings page.

use eframe::egui::{self, RichText, Ui};
use macpilot::i18n::Lang;
use macpilot::settings::{Theme, UiStyle};
use macpilot::{tr, trf};

use crate::widgets::{self as w, C, Level, Txt};
use crate::{Gui, autostart};

thread_local! {
    /// The next row is the first of its group (no divider above it).
    static FIRST_ROW: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// Related settings in one rounded box with thin dividers, as in System Settings.
fn group(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    w::card_frame(ui).inner_margin(egui::Margin::symmetric(16, 4)).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        FIRST_ROW.set(true);
        add(ui);
    });
    ui.add_space(w::sp::L);
}

fn row(ui: &mut Ui, title: &str, hint: &str, add: impl FnOnce(&mut Ui)) {
    if !FIRST_ROW.replace(false) {
        let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter().hline(r.x_range(), r.center().y, egui::Stroke::new(1.0, C::track(ui)));
    }
    ui.add_space(w::sp::S);
    ui.horizontal(|ui| {
        ui.set_min_height(32.0);
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width() - 330.0);
            ui.label(RichText::new(title).semibold());
            if !hint.is_empty() {
                ui.label(RichText::new(hint).callout().color(C::dim(ui)));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
    });
    ui.add_space(w::sp::S);
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Privacy,
    Disk,
}

pub fn show(g: &mut Gui, ui: &mut Ui) {
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        w::centered(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.set_max_width(820.0);
                w::title_bar(
                    ui,
                    tr("Settings"),
                    "",
                    |ui| {
                        w::segmented(
                            ui,
                            &mut g.settings_tab,
                            &[
                                (SettingsTab::General, tr("General")),
                                (SettingsTab::Privacy, tr("Privacy")),
                                (SettingsTab::Disk, tr("Disk and Cleanup")),
                            ],
                        );
                    },
                    |_| {},
                );
                match g.settings_tab {
                    SettingsTab::General => general(g, ui),
                    SettingsTab::Privacy => privacy(g, ui),
                    SettingsTab::Disk => disk(g, ui),
                }
                ui.add_space(w::sp::M);
                ui.label(RichText::new(format!("MacPilot {} · MIT License", env!("CARGO_PKG_VERSION"))).color(C::dim(ui)));
            });
        });
    });
}

fn general(g: &mut Gui, ui: &mut Ui) {
    group(ui, |ui| look(g, ui));
    group(ui, |ui| behaviour(g, ui));
    updates(g, ui);
}

/// Language, style and appearance.
fn look(g: &mut Gui, ui: &mut Ui) {
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

    let mut style = g.settings.style;
    row(ui, tr("Style"), tr("Classic looks like the first Macintosh: black and white, with a pixel font."), |ui| {
        w::segmented(ui, &mut style, &[(UiStyle::Standard, tr("Standard")), (UiStyle::Classic, tr("Classic Macintosh"))]);
    });
    if style != g.settings.style {
        g.settings.style = style;
        g.apply_style();
        g.save_settings();
    }

    let mut theme = g.settings.theme;
    row(ui, tr("Appearance"), "", |ui| {
        w::segmented(ui, &mut theme, &[(Theme::System, tr("System")), (Theme::Light, tr("Light")), (Theme::Dark, tr("Dark"))]);
    });
    if theme != g.settings.theme {
        g.settings.theme = theme;
        crate::apply_theme(ui.ctx(), theme);
        g.save_settings();
    }
}

/// Login, menu bar and notifications.
fn behaviour(g: &mut Gui, ui: &mut Ui) {
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
        w::note(
            ui,
            C::yellow(),
            tr("Launch at login is switched off in System Settings"),
            tr("Turn MacPilot on in System Settings → General → Login Items."),
        );
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

    let mut notes = g.settings.notifications;
    row(
        ui,
        tr("Notifications"),
        tr("Only about real problems: an app stuck in a loop, an almost full disk, a hot battery or an app draining it."),
        |ui| {
            w::switch(ui, &mut notes, tr("Notifications"));
        },
    );
    if notes != g.settings.notifications {
        g.settings.notifications = notes;
        if notes {
            crate::mac::init_notifications();
        }
        g.save_settings();
    }
}

/// What MacPilot may read, the folders it never opens, and the macOS permissions.
fn privacy(g: &mut Gui, ui: &mut Ui) {
    let mut on = g.settings.file_access;
    group(ui, |ui| {
        row(ui, tr("Read my files"), tr("Disk and Cleanup read the names, sizes and dates of your files — never their contents."), |ui| {
            w::switch(ui, &mut on, tr("Read my files"));
        });
    });
    if on != g.settings.file_access {
        if on { g.grant_file_access() } else { g.revoke_file_access() }
    }

    if g.settings.file_access {
        ui.add_space(w::sp::S);
        ui.label(RichText::new(tr("Don't scan")).semibold());
        ui.label(RichText::new(tr("Switch on the folders MacPilot must never open — not even to measure them.")).callout().color(C::dim(ui)));
        ui.add_space(w::sp::XS);
        w::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if crate::access_view::folder_checks(g, ui) {
                g.exclusions_changed();
            }
        });
        ui.add_space(w::sp::S);
    }

    ui.add_space(w::sp::S);
    ui.label(RichText::new(tr("macOS permissions")).semibold());
    ui.add_space(w::sp::XS);
    group(ui, |ui| permissions(g, ui));
}

fn permissions(g: &Gui, ui: &mut Ui) {
    row(ui, tr("Full Disk Access"), tr("Lets MacPilot see other apps' data too (Docker, virtual machines). Optional."), |ui| {
        if ui.button(tr("Open…")).clicked() {
            macpilot::open_full_disk_access_settings();
        }
        if g.full_disk_access {
            w::badge(ui, tr("granted"), C::green());
        } else {
            w::badge(ui, tr("not granted"), C::dim(ui));
        }
    });
    row(
        ui,
        tr("Files and Folders"),
        tr("What you allowed when macOS asked about Desktop, Documents and Downloads. Change or take it back there."),
        |ui| {
            if ui.button(tr("Open…")).clicked() {
                macpilot::open_files_and_folders_settings();
            }
        },
    );
}

fn disk(g: &mut Gui, ui: &mut Ui) {
    group(ui, |ui| disk_rows(g, ui));
    group(ui, |ui| auto_clean(g, ui));
}

/// Scheduled cleanup: off unless switched on here.
fn auto_clean(g: &mut Gui, ui: &mut Ui) {
    let mut days = g.settings.auto_clean_days;
    let mut hint = tr(
        "Caches of apps that are not running, logs and previews go to the Trash on their own. Nothing is erased: the space is freed when you empty the Trash.",
    )
    .to_string();
    if days > 0 && !g.settings.file_access {
        hint = format!("{hint} {}", tr("Waits until you allow MacPilot to look at your files."));
    } else if days > 0 {
        let next = g.settings.auto_clean_last + days * 86_400;
        hint = format!("{hint} {}", trf("Next: {0}.", &[&macpilot::fmt::date(next.max(macpilot::disk::now_unix()))]));
    }
    row(ui, tr("Clean up on a schedule"), &hint, |ui| {
        w::segmented(ui, &mut days, &[(0, tr("Never")), (7, tr("Weekly")), (30, tr("Monthly"))]);
    });
    if days != g.settings.auto_clean_days {
        // The first run is a whole period away, not right now.
        if g.settings.auto_clean_days == 0 {
            g.settings.auto_clean_last = macpilot::disk::now_unix();
        }
        g.settings.auto_clean_days = days;
        g.save_settings();
    }
}

fn disk_rows(g: &mut Gui, ui: &mut Ui) {
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
        w::segmented(ui, &mut stale, &[(90, tr("3 months")), (180, tr("6 months")), (365, tr("1 year")), (730, tr("2 years"))]);
    });
    if stale != g.settings.stale_days {
        g.settings.stale_days = stale;
        g.stale_days = stale;
        g.stale_dirty = true;
        g.save_settings();
    }

    let mut junk = g.settings.junk_days;
    row(ui, tr("Inactive project after"), tr("Build folders of projects not changed for this long are pre-selected for removal."), |ui| {
        w::segmented(ui, &mut junk, &[(7, tr("1 week")), (30, tr("1 month")), (90, tr("3 months")), (365, tr("1 year"))]);
    });
    if junk != g.settings.junk_days {
        g.settings.junk_days = junk;
        g.junk_dirty = true;
        g.save_settings();
    }

    let mut mb = g.settings.dupes_min_mb;
    row(ui, tr("Duplicates: ignore files smaller than"), "", |ui| {
        w::segmented(ui, &mut mb, &[(1, "1 MB"), (10, "10 MB"), (100, "100 MB")]);
    });
    if mb != g.settings.dupes_min_mb {
        g.settings.dupes_min_mb = mb;
        g.save_settings();
    }
}

fn updates(g: &mut Gui, ui: &mut Ui) {
    let Some(_) = macpilot::update::repo() else { return };
    ui.add_space(w::sp::M);
    if let Some(u) = g.update.clone() {
        let can_install = macpilot::update::replaceable_app().is_some();
        w::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(trf("MacPilot {0} is available", &[&u.version])).semibold().color(C::green()));
                    let text = if g.update_installing {
                        tr("Downloading and checking the new version…").to_string()
                    } else if can_install {
                        trf(
                            "You have {0}. MacPilot downloads the new version, checks it, replaces itself and opens again.",
                            &[&env!("CARGO_PKG_VERSION")],
                        )
                    } else {
                        trf("You have {0}. Download the new version and replace the app in Applications.", &[&env!("CARGO_PKG_VERSION")])
                    };
                    ui.label(RichText::new(text).callout().color(C::dim(ui)));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if g.update_installing {
                        ui.spinner();
                    } else if can_install {
                        if ui.add(egui::Button::new(RichText::new(tr("Update and relaunch")).color(egui::Color32::WHITE)).fill(C::accent())).clicked()
                        {
                            g.install_update();
                        }
                        if ui.link(tr("Release page")).clicked() {
                            let _ = std::process::Command::new("open").arg(&u.url).spawn();
                        }
                    } else if ui.add(egui::Button::new(RichText::new(tr("Download")).color(egui::Color32::WHITE)).fill(C::accent())).clicked() {
                        let _ = std::process::Command::new("open").arg(&u.url).spawn();
                    }
                });
            });
        });
        ui.add_space(w::sp::S);
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
    let mut check = false;
    group(ui, |ui| {
        row(ui, tr("Check for updates"), &hint, |ui| {
            w::switch(ui, &mut on, tr("Check for updates"));
            check = ui.add_enabled(!g.update_checking, egui::Button::new(tr("Check now"))).clicked();
        });
    });
    if check {
        g.check_updates();
    }
    if on != g.settings.check_updates {
        g.settings.check_updates = on;
        g.save_settings();
    }
}
