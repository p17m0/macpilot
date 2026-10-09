//! Settings page.

use eframe::egui::{self, RichText, Ui};
use macpilot::i18n::Lang;
use macpilot::settings::{Palette, Theme, UiStyle};
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
    let title_text = RichText::new(title).semibold();
    row_titled(ui, title, title_text, hint, add);
}

/// A row whose title is styled by the caller. The controls take the room they need (known from
/// the last frame); the text wraps in what is left, so neither runs into the other.
fn row_titled(ui: &mut Ui, key: &str, title: RichText, hint: &str, add: impl FnOnce(&mut Ui)) {
    if !FIRST_ROW.replace(false) {
        let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter().hline(r.x_range(), r.center().y, egui::Stroke::new(1.0, C::track(ui)));
    }
    ui.add_space(w::sp::S);
    let id = ui.id().with(("settings row", key));
    let known: Option<f32> = ui.data(|d| d.get_temp(id));
    ui.horizontal(|ui| {
        ui.set_min_height(32.0);
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() - known.unwrap_or(330.0) - w::sp::L).max(160.0));
            ui.label(title);
            if !hint.is_empty() {
                ui.label(RichText::new(hint).callout().color(C::dim(ui)));
            }
        });
        let controls = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.scope(add).response.rect.width()).inner;
        if known != Some(controls) {
            ui.data_mut(|d| d.insert_temp(id, controls));
            if known.is_none() {
                // The first frame is laid out blind: draw it again before it is seen.
                ui.ctx().request_discard("settings row measured");
            }
            ui.ctx().request_repaint();
        }
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
                if macpilot::update::repo().is_some() {
                    g.version_line(ui, "MacPilot ");
                    ui.label(RichText::new("MIT License").callout().color(C::dim(ui)));
                } else {
                    ui.label(RichText::new(format!("MacPilot {} · MIT License", env!("CARGO_PKG_VERSION"))).color(C::dim(ui)));
                }
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
    row(ui, tr("Style"), tr("The look of the whole app: the standard one, or after a classic computer or console."), |ui| {
        egui::ComboBox::from_id_salt("style").selected_text(style.name()).width(180.0).show_ui(ui, |ui| {
            for st in UiStyle::ALL {
                ui.selectable_value(&mut style, st, st.name());
            }
        });
    });
    if style != g.settings.style {
        g.settings.style = style;
        g.apply_style();
        g.save_settings();
    }

    // The other styles bring their own colors.
    if g.settings.style == UiStyle::Standard {
        let mut palette = g.settings.palette;
        row(ui, tr("Colors"), palette.name(), |ui| palette_picker(ui, &mut palette));
        if palette != g.settings.palette {
            g.settings.palette = palette;
            g.apply_style();
            g.save_settings();
        }
    }

    // Light or dark is part of most retro styles.
    if g.settings.style.fixed_dark().is_none() {
        let mut theme = g.settings.theme;
        row(ui, tr("Appearance"), "", |ui| {
            w::segmented(ui, &mut theme, &[(Theme::System, tr("System")), (Theme::Light, tr("Light")), (Theme::Dark, tr("Dark"))]);
        });
        if theme != g.settings.theme {
            g.settings.theme = theme;
            crate::apply_theme(ui.ctx(), theme, g.settings.style);
            g.save_settings();
        }
    }
}

/// A row of swatches, one per palette: its background with a dot of its accent.
fn palette_picker(ui: &mut Ui, value: &mut Palette) {
    let dark = C::dark(ui);
    ui.spacing_mut().item_spacing.x = 6.0;
    // The row is laid out from the right: add the swatches reversed, so they read in order.
    for p in Palette::ALL.into_iter().rev() {
        let pl = w::pal(p, dark);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::click());
        let resp = resp.on_hover_text(p.name()).on_hover_cursor(egui::CursorIcon::PointingHand);
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, *value == p, p.name()));
        let painter = ui.painter();
        let inner = rect.shrink(3.0);
        painter.rect_filled(inner, 7, pl.panel);
        painter.rect_stroke(inner, 7, egui::Stroke::new(1.0, pl.track), egui::StrokeKind::Inside);
        painter.circle_filled(inner.center(), 6.0, pl.accent);
        if *value == p {
            painter.rect_stroke(rect, 9, egui::Stroke::new(2.0, C::accent()), egui::StrokeKind::Inside);
        } else if resp.hovered() {
            painter.rect_stroke(rect, 9, egui::Stroke::new(1.0, C::dim(ui)), egui::StrokeKind::Inside);
        }
        if resp.clicked() {
            *value = p;
        }
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
        if w::button(ui, tr("Login Items settings…")).clicked() {
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
        if w::button(ui, tr("Open…")).clicked() {
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
            if w::button(ui, tr("Open…")).clicked() {
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
    // What it would take right now, so the schedule is not switched on blindly.
    if g.settings.file_access {
        g.preview_auto_clean();
        let status = match g.auto_preview {
            Some((0, _)) => tr("Nothing to clean up right now.").to_string(),
            Some((n, size)) => format!("{}, {}", macpilot::fmt::bytes(size), macpilot::fmt::n(n as u64, macpilot::fmt::Noun::Item)),
            None => tr("measuring…").to_string(),
        };
        let mut run = false;
        row(ui, tr("It would take now"), &status, |ui| {
            let has = g.auto_preview.is_some_and(|p| p.0 > 0);
            run = ui.add_enabled(has, egui::Button::new(tr("Run now…"))).clicked();
        });
        if run {
            g.ask_auto_clean_now();
        }
    }
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
    let (mut install, mut open_page) = (false, false);
    let update = g.update.clone();
    group(ui, |ui| {
        // The new version, when there is one, is the first row of the same box.
        if let Some(u) = &update {
            let can_install = macpilot::update::replaceable_app().is_some();
            let text = if g.update_installing {
                tr("Downloading and checking the new version…").to_string()
            } else if can_install {
                trf("You have {0}. MacPilot downloads the new version, checks it, replaces itself and opens again.", &[&env!("CARGO_PKG_VERSION")])
            } else {
                trf("You have {0}. Download the new version and replace the app in Applications.", &[&env!("CARGO_PKG_VERSION")])
            };
            let title = RichText::new(trf("MacPilot {0} is available", &[&u.version])).semibold().color(C::green());
            row_titled(ui, "update", title, &text, |ui| {
                if g.update_installing {
                    ui.spinner();
                    return;
                }
                let label = if can_install { tr("Update and relaunch") } else { tr("Download") };
                if w::big_button(ui, label, C::accent(), true).clicked() {
                    install = can_install;
                    open_page = !can_install;
                }
                if can_install && w::plain_button(ui, tr("Release page")).clicked() {
                    open_page = true;
                }
            });
        }
        row(ui, tr("Check for updates"), &hint, |ui| {
            w::switch(ui, &mut on, tr("Check for updates"));
            check = ui.add_enabled(!g.update_checking, egui::Button::new(tr("Check now"))).clicked();
        });
    });
    if install {
        g.install_update();
    }
    if let Some(u) = update.filter(|_| open_page) {
        let _ = std::process::Command::new("open").arg(&u.url).spawn();
    }
    if check {
        g.check_updates();
    }
    if on != g.settings.check_updates {
        g.settings.check_updates = on;
        g.save_settings();
    }
}
