//! Asking before MacPilot reads any files: shown on the Disk and Cleanup pages until the user agrees.

use eframe::egui::{self, RichText, Ui};
use macpilot::disk::PERSONAL;
use macpilot::{fmt, tr};

use crate::Gui;
use crate::widgets::{self as w, C, Txt};

/// Readable name of a personal folder key.
pub fn folder_name(key: &str) -> &'static str {
    match key {
        "Desktop" => tr("Desktop"),
        "Documents" => tr("Documents"),
        "Downloads" => tr("Downloads"),
        "Movies" => tr("Movies"),
        "Music" => tr("Music"),
        "Pictures" => tr("Pictures"),
        _ => tr("iCloud Drive"),
    }
}

/// One line of a folder list: icon, name, path, and whatever goes on the right.
fn folder_row(ui: &mut Ui, name: &str, path: &std::path::Path, right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        w::file_icon(ui, true, false, false);
        ui.label(name);
        ui.label(RichText::new(fmt::path(path)).callout().color(C::dim(ui)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
}

fn group_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).callout().semibold().color(C::dim(ui)));
    ui.add_space(2.0);
}

/// "Don't scan": the usual folders with a switch each (on = skipped), then the user's own folders
/// with a remove button and "Add folder…". Returns true when the list changed.
pub fn folder_checks(g: &mut Gui, ui: &mut Ui) -> bool {
    let mut changed = false;
    let home = macpilot::home();
    let standard: Vec<std::path::PathBuf> = PERSONAL.iter().map(|(_, rel)| home.join(rel)).collect();

    group_title(ui, tr("Standard folders"));
    for (i, (key, rel)) in PERSONAL.iter().enumerate() {
        if i > 0 {
            ui.separator();
        }
        let path = home.join(rel);
        let mut skip = g.settings.is_excluded(&path);
        let name = folder_name(key);
        folder_row(ui, name, &path, |ui| {
            if w::switch(ui, &mut skip, name).changed() {
                g.settings.set_excluded(&path, skip);
                changed = true;
            }
        });
    }

    ui.add_space(w::sp::M);
    group_title(ui, tr("Other folders"));
    let others: Vec<std::path::PathBuf> = g.settings.excluded_paths().into_iter().filter(|p| !standard.contains(p)).collect();
    if others.is_empty() {
        ui.label(RichText::new(tr("None yet. Add any folder MacPilot should never open.")).color(C::dim(ui)));
    }
    for (i, p) in others.iter().enumerate() {
        if i > 0 {
            ui.separator();
        }
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| fmt::path(p));
        folder_row(ui, &name, p, |ui| {
            if w::button(ui, tr("Remove")).on_hover_text(tr("Scan this folder again")).clicked() {
                g.settings.set_excluded(p, false);
                changed = true;
            }
        });
    }
    ui.add_space(w::sp::S);
    if w::button(ui, tr("Add folder…")).clicked() {
        for p in crate::mac::choose_folders(tr("Don't scan"), tr("MacPilot will never open the folders you choose.")) {
            g.settings.set_excluded(&p, true);
            changed = true;
        }
    }
    changed
}

pub fn show(g: &mut Gui, ui: &mut Ui) {
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        w::centered(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| ask(g, ui));
        });
    });
}

fn ask(g: &mut Gui, ui: &mut Ui) {
    ui.add_space(w::sp::XXL);
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Let MacPilot look at your files?")).title());
        ui.add_space(w::sp::S);
        ui.label(tr(
            "To show what takes space and what can be cleaned up, MacPilot reads the names, sizes and dates of the files in your home folder. It never opens what is inside them, sends nothing anywhere and removes nothing without your confirmation.",
        ));
        ui.add_space(w::sp::M);
        ui.label(RichText::new(tr("Don't scan")).semibold());
        ui.label(RichText::new(tr("Switch on the folders MacPilot must never open — not even to measure them.")).callout().color(C::dim(ui)));
        ui.add_space(w::sp::S);
        if folder_checks(g, ui) {
            g.save_settings();
            macpilot::disk::set_excluded(g.settings.excluded_paths());
        }
        ui.add_space(w::sp::M);
        w::note(
            ui,
            C::accent(),
            tr("What happens next"),
            tr(
                "macOS itself will ask separately about Desktop, Documents, Downloads and iCloud Drive — those are its standard dialogs. Allow the ones you want; MacPilot works with whatever you allow.",
            ),
        );
        ui.add_space(w::sp::M);
        ui.horizontal(|ui| {
            if w::big_button(ui, tr("Allow and scan"), C::accent(), true).clicked() {
                g.grant_file_access();
            }
            if ui.link(tr("You can change this any time in Settings → Privacy.")).clicked() {
                g.open_privacy();
            }
        });
    });
    ui.add_space(w::sp::M);
    ui.label(RichText::new(tr("Processes, Battery, Apps and Startup work without it.")).color(C::dim(ui)));
}
