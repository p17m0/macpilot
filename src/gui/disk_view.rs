//! Disk page: what takes space, and safe removal.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use eframe::egui::{self, Color32, FontId, Rect, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use macpilot::disk::{self, DelSafety};
use macpilot::{fmt, tr, trf};

use crate::icons;
use crate::widgets::{self as w, C, Level, Txt};
use crate::{Action, Confirm, DiskMode, Gui};

pub fn show(g: &mut Gui, ui: &mut Ui) {
    if !g.settings.file_access {
        crate::access_view::show(g, ui);
        return;
    }
    if !matches!(g.disk_mode, DiskMode::Dupes | DiskMode::Summary) {
        egui::Panel::right("disk_detail").resizable(true).default_size(360.0).min_size(300.0).frame(w::side_frame(ui)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| detail(g, ui));
        });
    }
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        w::centered(ui, |ui| {
            let mut scan = None;
            w::title_bar(ui, tr("Disk"), "", |ui| mode_switch(g, ui), |ui| scan = scan_actions(ui));
            if let Some(p) = scan {
                let p = p.unwrap_or_else(|| g.cwd.clone());
                g.start_scan(p);
            }
            header(g, ui);
            ui.add_space(w::sp::S);
            nav(g, ui);
            match g.disk_mode {
                DiskMode::Summary => summary(g, ui),
                DiskMode::List => list(g, ui),
                DiskMode::Map => map(g, ui),
                DiskMode::Big => big(g, ui),
                DiskMode::Stale => stale(g, ui),
                DiskMode::Dupes => dupes(g, ui),
            }
        });
    });
}

fn header(g: &mut Gui, ui: &mut Ui) {
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        // Summary draws the whole disk itself; the bar here would say the same thing twice.
        let disk = w::data_volume(&g.disks).filter(|_| g.disk_mode != DiskMode::Summary);
        if let Some((avail, total)) = disk {
            let used = total.saturating_sub(avail);
            let r = used as f32 / total.max(1) as f32;
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Startup disk")).headline());
                ui.label(RichText::new(trf("{0} free", &[&fmt::bytes(avail)])).headline().color(w::value_color(ui, r)));
            });
            ui.add_space(w::sp::XS);
            let width = ui.available_width();
            w::bar(ui, r, egui::vec2(width, 10.0), w::ratio_color(r));
            ui.add_space(2.0);
            ui.label(RichText::new(trf("{0} of {1} used", &[&fmt::bytes(used), &fmt::bytes(total)])).color(C::dim(ui)));
        }
        ui.add_space(w::sp::S);
        ui.horizontal_wrapped(|ui| match &g.scan {
            Some(sc) => {
                let files = sc.shared.files.load(Ordering::Relaxed);
                let bytes = sc.shared.bytes.load(Ordering::Relaxed);
                let errs = sc.shared.errors.load(Ordering::Relaxed);
                if let Some(at) = sc.cached_at {
                    let bytes = sc.dir(&sc.root).map(|d| d.size).unwrap_or(bytes);
                    ui.label(trf(
                        "{0}: {1}, {2} · last scan {3}",
                        &[&fmt::place(&sc.root), &fmt::bytes(bytes), &fmt::n(files, fmt::Noun::File), &fmt::ago(at)],
                    ));
                    if let Some(fresh) = &g.rescan {
                        ui.spinner();
                        let n = fresh.shared.files.load(Ordering::Relaxed);
                        ui.label(RichText::new(trf("updating… {0}", &[&fmt::n(n, fmt::Noun::File)])).color(C::dim(ui)));
                    }
                } else if let Some(t) = sc.finished_in {
                    ui.label(RichText::new("✔").color(C::green()));
                    ui.label(trf(
                        "Scanned {0}: {1}, {2}, {3} s",
                        &[&fmt::place(&sc.root), &fmt::bytes(bytes), &fmt::n(files, fmt::Noun::File), &format!("{:.1}", t.as_secs_f32())],
                    ));
                } else {
                    ui.spinner();
                    ui.label(trf("{0}: scanning… {1}, {2}", &[&fmt::place(&sc.root), &fmt::n(files, fmt::Noun::File), &fmt::bytes(bytes)]));
                    if g.scan_stalled {
                        ui.label(RichText::new(tr("· waiting for a macOS permission dialog")).color(C::yellow()));
                    }
                }
                if errs > 0 {
                    ui.label(RichText::new(trf("· no access: {0}", &[&fmt::n(errs, fmt::Noun::Folder)])).color(C::dim(ui)))
                        .on_hover_text(tr("To see everything: System Settings → Privacy & Security → Full Disk Access → add MacPilot."));
                }
            }
            None => {
                ui.label(tr("Not scanned yet"));
            }
        });
    });
}

fn nav(g: &mut Gui, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let browsing = matches!(g.disk_mode, DiskMode::List | DiskMode::Map);
        if browsing {
            let up = g.cwd.parent().map(|p| p.to_path_buf());
            if ui.add_enabled(up.is_some(), egui::Button::new(tr("‹ Up")).corner_radius(w::button_radius())).clicked() {
                if let Some(p) = up {
                    g.go(p);
                }
            }
            // Breadcrumbs: every part of the path is clickable.
            let mut acc = PathBuf::new();
            let home = macpilot::home();
            let mut crumbs: Vec<(String, PathBuf)> = Vec::new();
            for c in g.cwd.components() {
                acc.push(c);
                if acc == home {
                    crumbs.clear();
                    crumbs.push((format!("~ {}", tr("Home")), acc.clone()));
                    continue;
                }
                let name = match c {
                    std::path::Component::RootDir => "Macintosh HD".to_string(),
                    _ => c.as_os_str().to_string_lossy().to_string(),
                };
                crumbs.push((name, acc.clone()));
            }
            let n = crumbs.len();
            let mut go = None;
            for (i, (name, p)) in crumbs.into_iter().enumerate() {
                if i > 0 {
                    ui.label(RichText::new("›").color(C::dim(ui)));
                }
                let last = i + 1 == n;
                let text = if last { RichText::new(name).semibold() } else { RichText::new(name).color(C::accent()) };
                if ui.add(egui::Button::new(text).frame(false)).clicked() && !last {
                    go = Some(p);
                }
            }
            if let Some(p) = go {
                g.go(p);
            }
        }
    });
}

/// The scan buttons of the title bar (laid out right to left). Returns the folder to scan;
/// `Some(None)` is the folder being shown.
fn scan_actions(ui: &mut Ui) -> Option<Option<PathBuf>> {
    let mut out = None;
    if w::plain_button(ui, tr("Whole disk")).on_hover_text(tr("Scan everything from / (needs Full Disk Access)")).clicked() {
        out = Some(Some(PathBuf::from("/")));
    }
    if w::plain_button(ui, tr("Home folder")).clicked() {
        out = Some(Some(macpilot::home()));
    }
    if w::plain_button(ui, tr("⟳ Scan this folder")).clicked() {
        out = Some(None);
    }
    out
}

/// From the whole picture to the details: Summary, List, Map, then the special searches.
fn mode_switch(g: &mut Gui, ui: &mut Ui) {
    let before = g.disk_mode;
    w::segmented(
        ui,
        &mut g.disk_mode,
        &[
            (DiskMode::Summary, tr("Summary")),
            (DiskMode::List, tr("List")),
            (DiskMode::Map, tr("Map")),
            (DiskMode::Big, tr("Large files")),
            (DiskMode::Stale, tr("Not used")),
            (DiskMode::Dupes, tr("Duplicates")),
        ],
    );
    if before != g.disk_mode && g.disk_mode == DiskMode::Dupes && g.dupes.is_none() {
        g.start_dupes();
    }
}

fn bar_color(s: u64) -> Color32 {
    if s >= 10_000_000_000 {
        C::red()
    } else if s >= 1_000_000_000 {
        C::yellow()
    } else {
        C::accent()
    }
}

fn heads(h: &mut egui_extras::TableRow, names: &[&str]) {
    for t in names {
        h.col(|ui| {
            ui.label(RichText::new(*t).semibold().color(C::dim(ui)));
        });
    }
}

fn list(g: &mut Gui, ui: &mut Ui) {
    if let Some(e) = &g.entries_err {
        ui.add_space(w::sp::XL);
        if disk::is_excluded(&g.cwd) {
            ui.label(RichText::new(e).color(C::dim(ui)));
            if ui.button(tr("Privacy settings…")).clicked() {
                g.open_privacy();
            }
        } else {
            ui.label(RichText::new(trf("Could not open the folder: {0}", &[e])).color(C::red()));
        }
        return;
    }
    let total = g.dir_total().max(1);
    if !g.scan.as_ref().is_some_and(|s| s.covers(&g.cwd)) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("This folder is outside the scanned area — folder sizes are unknown.")).color(C::yellow()));
            if ui.button(tr("Scan it")).clicked() {
                let p = g.cwd.clone();
                g.start_scan(p);
            }
        });
    }
    let mut open: Option<PathBuf> = None;
    let mut select: Option<PathBuf> = None;
    let mut trash: Option<PathBuf> = None;
    TableBuilder::new(ui)
        .striped(true)
        .sense(Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(160.0).clip(true))
        .column(Column::exact(76.0))
        .column(Column::exact(124.0))
        .column(Column::exact(108.0))
        .column(Column::exact(108.0))
        .column(Column::exact(92.0))
        .header(24.0, |mut h| heads(&mut h, &[tr("Name"), tr("Size"), tr("Share"), tr("Modified"), tr("Opened"), tr("Removal")]))
        .body(|body| {
            body.rows(28.0, g.entries.len(), |mut row| {
                let e = &g.entries[row.index()];
                row.set_selected(g.disk_sel.as_ref() == Some(&e.path));
                let (safety, _) = disk::deletion_safety(&e.path);
                row.col(|ui| {
                    if e.name.ends_with(".app") && !e.is_link {
                        icons::app(ui, &e.path, 18.0);
                    } else {
                        w::file_icon(ui, e.is_dir, false, e.is_link);
                    }
                    let t = RichText::new(&e.name);
                    ui.label(if e.is_dir { t.semibold() } else { t });
                });
                row.col(|ui| match e.size {
                    Some(s) => {
                        ui.label(RichText::new(fmt::bytes(s)).color(w::size_color(ui, s)));
                    }
                    None => {
                        ui.spinner();
                    }
                });
                row.col(|ui| {
                    let r = e.size.map(|s| s as f32 / total as f32).unwrap_or(0.0);
                    w::bar(ui, r, egui::vec2(70.0, 7.0), bar_color(e.size.unwrap_or(0)));
                    ui.label(RichText::new(fmt::pct(r * 100.0)).caption().color(C::dim(ui)));
                });
                row.col(|ui| w::time_cell(ui, e.mtime));
                row.col(|ui| w::time_cell(ui, e.used));
                row.col(|ui| {
                    w::del_badge(ui, safety);
                });
                let resp = row.response();
                if resp.clicked() {
                    select = Some(e.path.clone());
                }
                if resp.double_clicked() && e.is_dir && !e.is_link {
                    open = Some(e.path.clone());
                }
                let p = e.path.clone();
                let is_dir = e.is_dir && !e.is_link;
                resp.context_menu(|ui| {
                    if is_dir && ui.button(tr("Open")).clicked() {
                        open = Some(p.clone());
                        ui.close();
                    }
                    if ui.button(tr("Show in Finder")).clicked() {
                        macpilot::trash::reveal_in_finder(&p);
                        ui.close();
                    }
                    ui.separator();
                    if ui.add_enabled(safety != DelSafety::Blocked, egui::Button::new(RichText::new(tr("Move to Trash…")).color(C::red()))).clicked()
                    {
                        trash = Some(p.clone());
                        ui.close();
                    }
                });
            });
        });
    if let Some(p) = select {
        g.disk_sel = Some(p);
    }
    if let Some(p) = open {
        g.go(p);
    }
    if let Some(p) = trash {
        g.disk_sel = Some(p.clone());
        ask_trash(g, &p);
    }
}

fn big(g: &mut Gui, ui: &mut Ui) {
    let root = g.scan.as_ref().map(|s| fmt::path(&s.root)).unwrap_or_default();
    ui.label(RichText::new(trf("Files of 50 MB and more in {0}: {1}", &[&root, &g.big.len()])).color(C::dim(ui)));
    let mut select = None;
    let mut goto = None;
    TableBuilder::new(ui)
        .striped(true)
        .sense(Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(86.0))
        .column(Column::remainder().at_least(300.0).clip(true))
        .column(Column::exact(120.0))
        .column(Column::exact(96.0))
        .header(24.0, |mut h| heads(&mut h, &[tr("Size"), tr("File"), tr("Opened"), tr("Removal")]))
        .body(|body| {
            body.rows(28.0, g.big.len(), |mut row| {
                let (s, p) = &g.big[row.index()];
                row.set_selected(g.disk_sel.as_ref() == Some(p));
                row.col(|ui| {
                    ui.label(RichText::new(fmt::bytes(*s)).color(w::size_color(ui, *s)));
                });
                row.col(|ui| {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    ui.label(RichText::new(name).semibold());
                    ui.label(RichText::new(fmt::path(p.parent().unwrap_or(Path::new("")))).color(C::dim(ui)));
                });
                row.col(|ui| w::time_cell(ui, std::fs::symlink_metadata(p).ok().map(|m| disk::used_time(&m))));
                row.col(|ui| {
                    w::del_badge(ui, disk::deletion_safety(p).0);
                });
                let r = row.response();
                if r.clicked() {
                    select = Some(p.clone());
                }
                if r.double_clicked() {
                    goto = Some(p.clone());
                }
            });
        });
    if let Some(p) = select {
        g.disk_sel = Some(p);
    }
    if let Some(p) = goto {
        reveal_in_list(g, p);
    }
}

fn reveal_in_list(g: &mut Gui, p: PathBuf) {
    if let Some(parent) = p.parent() {
        g.disk_mode = DiskMode::List;
        g.go(parent.to_path_buf());
        g.disk_sel = Some(p);
    }
}

// ---------------------------------------------------------------------------
// Map (treemap)
// ---------------------------------------------------------------------------

/// Squarified treemap: tiles close to squares, area proportional to size.
fn squarify(sizes: &[f64], rect: Rect) -> Vec<Rect> {
    let mut out = vec![Rect::NOTHING; sizes.len()];
    let total: f64 = sizes.iter().sum();
    if total <= 0.0 || rect.area() <= 0.0 {
        return out;
    }
    let scale = rect.area() as f64 / total;
    let areas: Vec<f64> = sizes.iter().map(|s| s * scale).collect();
    let mut rest = rect;
    let mut i = 0;
    let worst = |sum: f64, max: f64, min: f64, side: f64| {
        let (s2, w2) = (sum * sum, side * side);
        (w2 * max / s2).max(s2 / (w2 * min))
    };
    while i < areas.len() {
        let side = rest.width().min(rest.height()) as f64;
        if side <= 0.0 {
            break;
        }
        let mut end = i + 1;
        let mut sum = areas[i];
        let mut best = worst(sum, areas[i], areas[i], side);
        while end < areas.len() {
            let ns = sum + areas[end];
            let wv = worst(ns, areas[i], areas[end], side);
            if wv > best {
                break;
            }
            best = wv;
            sum = ns;
            end += 1;
        }
        let thick = (sum / side) as f32;
        let horizontal = rest.width() >= rest.height();
        let mut pos = if horizontal { rest.top() } else { rest.left() };
        for k in i..end {
            let len = (areas[k] / thick as f64) as f32;
            out[k] = if horizontal {
                Rect::from_min_size(egui::pos2(rest.left(), pos), egui::vec2(thick, len))
            } else {
                Rect::from_min_size(egui::pos2(pos, rest.top()), egui::vec2(len, thick))
            };
            pos += len;
        }
        if horizontal {
            rest.min.x += thick;
        } else {
            rest.min.y += thick;
        }
        i = end;
    }
    out
}

fn tile_color(name: &str, is_dir: bool, dark: bool) -> Color32 {
    if w::classic() {
        // Shades of gray, like a 1-bit screen with dither patterns.
        let h = name.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
        let g = [255u8, 225, 200, 175][(h % 4) as usize];
        return if dark { Color32::from_gray(255 - g) } else { Color32::from_gray(g) };
    }
    if !is_dir {
        return if dark { Color32::from_rgb(70, 74, 82) } else { Color32::from_rgb(190, 194, 202) };
    }
    let mut h: u32 = 2_166_136_261;
    for b in name.bytes() {
        h = (h ^ b as u32).wrapping_mul(16_777_619);
    }
    let (s, v) = if dark { (0.45, 0.62) } else { (0.40, 0.86) };
    egui::ecolor::Hsva::new((h % 360) as f32 / 360.0, s, v, 1.0).into()
}

fn map(g: &mut Gui, ui: &mut Ui) {
    let items: Vec<(usize, u64)> = g.entries.iter().enumerate().filter_map(|(i, e)| e.size.filter(|s| *s > 0).map(|s| (i, s))).collect();
    let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
    if items.is_empty() {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            tr("Empty, or still being measured…"),
            FontId::proportional(w::ty::HEADLINE),
            C::dim(ui),
        );
        return;
    }
    let sizes: Vec<f64> = items.iter().map(|(_, s)| *s as f64).collect();
    let tiles = squarify(&sizes, rect.shrink(1.0));
    let total: u64 = items.iter().map(|(_, s)| s).sum();
    let dark = C::dark(ui);
    let (mut open, mut select) = (None, None);
    for ((idx, size), tile) in items.iter().zip(tiles) {
        if !tile.is_positive() {
            continue;
        }
        let e = &g.entries[*idx];
        let r = tile.shrink(1.5);
        let resp = ui.interact(r, ui.id().with(("tile", &e.path)), Sense::click());
        let spoken = format!("{}, {}", e.name, fmt::bytes(*size));
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &spoken));
        let mut color = tile_color(&e.name, e.is_dir, dark);
        if resp.hovered() {
            color = color.gamma_multiply(1.15);
        }
        let p = ui.painter();
        p.rect_filled(r, 5, color);
        if g.disk_sel.as_ref() == Some(&e.path) {
            p.rect_stroke(r, 5, egui::Stroke::new(2.5, Color32::WHITE), egui::StrokeKind::Inside);
        }
        if r.width() > 60.0 && r.height() > 34.0 {
            let clip = p.with_clip_rect(r.shrink(4.0));
            let tc = Color32::from_gray(18);
            clip.text(r.left_top() + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, &e.name, w::semibold_font(w::ty::BODY), tc);
            let sub = format!("{} · {}", fmt::bytes(*size), fmt::pct0((*size as f64 / total as f64 * 100.0) as f32));
            clip.text(
                r.left_top() + egui::vec2(8.0, 23.0),
                egui::Align2::LEFT_TOP,
                sub,
                egui::FontId::proportional(w::ty::CAPTION),
                tc.gamma_multiply(0.8),
            );
        }
        let hint = if e.is_dir {
            format!("{}\n{}\n{}", e.name, fmt::bytes(*size), tr("Double-click to open"))
        } else {
            format!("{}\n{}", e.name, fmt::bytes(*size))
        };
        let resp = resp.on_hover_text(hint);
        if resp.clicked() {
            select = Some(e.path.clone());
        }
        if resp.double_clicked() && e.is_dir && !e.is_link {
            open = Some(e.path.clone());
        }
    }
    if let Some(p) = select {
        g.disk_sel = Some(p);
    }
    if let Some(p) = open {
        g.go(p);
    }
}

// ---------------------------------------------------------------------------
// Not used lately
// ---------------------------------------------------------------------------

/// Where the whole disk goes (volumes, folders outside home, what MacPilot cannot see) and what changed lately.
fn summary(g: &mut Gui, ui: &mut Ui) {
    g.measure_outside();
    let info = g.space.lock().unwrap().clone();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let Some(v) = info.volumes else {
            w::waiting(ui, tr("Reading the disk…"));
            return;
        };
        let home = g
            .scan
            .as_ref()
            .filter(|s| s.root == macpilot::home() && (s.done() || s.cached_at.is_some()))
            .and_then(|s| s.dir(&s.root))
            .map(|d| d.size);
        let outside: u64 = info.outside.iter().filter_map(|o| o.2).sum();
        let outside_done = !info.outside.is_empty() && info.outside.iter().all(|o| o.2.is_some());
        let hidden = home.map(|h| v.data.saturating_sub(h + outside));

        // One bar for the whole disk.
        let parts: Vec<(Color32, String, u64, String)> = [
            (C::accent(), tr("Your home folder").to_string(), home.unwrap_or(0), tr("Everything the scan found in your folders.").to_string()),
            (
                C::yellow(),
                tr("Apps and other folders").to_string(),
                outside,
                if outside_done { tr("/Applications, /Library, /opt, Homebrew and temporary files.").into() } else { tr("measuring…").into() },
            ),
            (
                HIDDEN,
                tr("Not visible to MacPilot").to_string(),
                hidden.unwrap_or(0),
                if g.full_disk_access {
                    tr("System areas that only macOS can read.").into()
                } else {
                    tr("Mostly data of other apps (Docker and virtual machines can take tens of GB). Give Full Disk Access to see it.").into()
                },
            ),
            (C::purple(), tr("macOS").to_string(), v.macos, tr("The system itself, updates and recovery. It cannot be cleaned.").to_string()),
            (
                Color32::from_rgb(142, 142, 160),
                tr("Swap and sleep image").to_string(),
                v.vm,
                tr("Memory that did not fit into RAM. It shrinks after a restart.").to_string(),
            ),
        ]
        .into_iter()
        .filter(|p| p.2 > 0)
        .collect();
        w::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new(trf("{0} of {1} used · {2} free", &[&fmt::bytes(v.total - v.free), &fmt::bytes(v.total), &fmt::bytes(v.free)]))
                    .semibold()
                    .size(w::ty::HEADLINE),
            );
            ui.add_space(w::sp::S);
            // Pointing at a part of the bar lights up its line in the legend, and the other way round.
            let hover_id = ui.id().with("summary_hover");
            let was: Option<usize> = ui.data(|d| d.get_temp(hover_id)).flatten();
            let mut now: Option<usize> = None;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 5, C::track(ui));
            let mut x = rect.left();
            for (i, (color, _, size, _)) in parts.iter().enumerate() {
                let wdt = rect.width() * *size as f32 / v.total.max(1) as f32;
                // Thin gaps between the parts, as in macOS's storage bar.
                let seg = Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2((wdt - 2.0).max(1.0), rect.height()));
                if resp.hover_pos().is_some_and(|h| h.x >= x && h.x < x + wdt) {
                    now = Some(i);
                }
                let dim = was.is_some_and(|h| h != i);
                paint_part(p, seg, *color, dim, ui);
                x += wdt;
            }
            ui.add_space(w::sp::S);
            let free = (C::track(ui), tr("Free").to_string(), v.free, String::new());
            for (i, (color, label, size, hint)) in parts.iter().chain(std::iter::once(&free)).enumerate() {
                let row = ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                    paint_part(ui.painter(), dot.shrink(1.0), *color, false, ui);
                    fixed(ui, 230.0, RichText::new(label).semibold());
                    fixed(ui, 80.0, RichText::new(fmt::bytes(*size)));
                    // Long hints are cut to the row; the full text is on hover.
                    ui.add(egui::Label::new(RichText::new(hint).callout().color(C::dim(ui))).truncate()).on_hover_text(hint);
                });
                if row.response.contains_pointer() {
                    now = Some(i);
                }
                if was == Some(i) {
                    ui.painter().rect_filled(row.response.rect.expand(2.0), 4, C::track(ui).gamma_multiply(0.5));
                }
            }
            if now != was {
                ui.ctx().request_repaint();
            }
            ui.data_mut(|d| d.insert_temp(hover_id, now));
            if hidden.is_some_and(|h| h > 5_000_000_000) && !g.full_disk_access {
                ui.add_space(w::sp::S);
                if ui.button(tr("Open Full Disk Access settings…")).clicked() {
                    macpilot::open_full_disk_access_settings();
                }
            }
            if let Some(p) = info.purgeable.filter(|p| *p > 1_000_000_000) {
                ui.add_space(w::sp::S);
                ui.label(
                    RichText::new(trf(
                        "macOS can free about {0} more by itself when something needs the space (snapshots, caches, files that are also in iCloud).",
                        &[&fmt::bytes(p)],
                    ))
                    .callout()
                    .color(C::dim(ui)),
                );
            }
        });
        snapshots(g, ui, &info.snapshots);

        ui.add_space(w::sp::L);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("What changed in your home folder")).section());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let before = g.changes_days;
                w::segmented(ui, &mut g.changes_days, &[(1, tr("1 day")), (7, tr("7 days")), (30, tr("30 days"))]);
                if before != g.changes_days {
                    g.changes = None;
                }
            });
        });
        ui.add_space(w::sp::S);
        w::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            match &g.changes {
                None => {
                    ui.label(RichText::new(tr("Waiting for the scan…")).color(C::dim(ui)));
                }
                Some((None, _)) => {
                    let since = macpilot::space::first_summary().map(fmt::date).unwrap_or_else(|| tr("today").into());
                    let text = trf(
                        "MacPilot keeps a small daily summary of your home folder (history starts {0}). Changes appear from the next day.",
                        &[&since],
                    );
                    ui.add(egui::Label::new(RichText::new(text).color(C::dim(ui))).wrap());
                }
                Some((Some(at), list)) => {
                    ui.label(RichText::new(trf("Compared with {0}.", &[&fmt::date(*at)])).color(C::dim(ui)));
                    ui.add_space(w::sp::XS);
                    if list.is_empty() {
                        ui.label(tr("Nothing grew or shrank by more than 300 MB."));
                    }
                    let mut open = None;
                    for c in list.iter().take(12) {
                        let d = c.delta();
                        ui.horizontal(|ui| {
                            let (sign, color) = if d > 0 { ("+", C::red()) } else { ("−", C::green()) };
                            fixed(ui, 90.0, RichText::new(format!("{sign}{}", fmt::bytes(d.unsigned_abs()))).semibold().color(color));
                            // The folder's name first, where it is in grey: the part that matters is never cut off.
                            let name = c.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| fmt::path(&c.path));
                            let r = ui.add(egui::Label::new(RichText::new(name).semibold()).sense(Sense::click()));
                            let r = r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Open in the list"));
                            if r.clicked() {
                                open = Some(c.path.clone());
                            }
                            if let Some(parent) = c.path.parent() {
                                ui.add(egui::Label::new(RichText::new(fmt::path(parent)).callout().color(C::dim(ui))).truncate());
                            }
                            ui.label(RichText::new(format!("{} → {}", fmt::bytes(c.old), fmt::bytes(c.new))).callout().color(C::dim(ui)));
                        });
                    }
                    if let Some(p) = open {
                        g.disk_mode = DiskMode::List;
                        if p.is_dir() {
                            g.go(p);
                        } else if let Some(parent) = p.parent() {
                            g.go(parent.to_path_buf());
                        }
                    }
                }
            }
        });
    });
}

/// Marker of the part MacPilot cannot see: drawn hatched, an unknown rather than a warning.
const HIDDEN: Color32 = Color32::from_rgb(1, 2, 3);

/// One part of the disk bar (or its legend dot). The hidden part is hatched.
fn paint_part(p: &egui::Painter, r: Rect, color: Color32, dim: bool, ui: &Ui) {
    let fade = |c: Color32| if dim { c.gamma_multiply(0.35) } else { c };
    if color != HIDDEN {
        p.rect_filled(r, 2, fade(color));
        return;
    }
    let ink = fade(C::dim(ui));
    p.rect_filled(r, 2, fade(C::track(ui)));
    let clip = p.with_clip_rect(r.intersect(p.clip_rect()));
    let step = 5.0;
    let mut x = r.left() - r.height();
    while x < r.right() {
        clip.line_segment([egui::pos2(x, r.bottom()), egui::pos2(x + r.height(), r.top())], egui::Stroke::new(1.5, ink));
        x += step;
    }
}

/// A left-aligned label in a column of fixed width.
fn fixed(ui: &mut Ui, width: f32, text: RichText) {
    ui.allocate_ui_with_layout(egui::vec2(width, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(width);
        ui.add(egui::Label::new(text).truncate());
    });
}

fn stale(g: &mut Gui, ui: &mut Ui) {
    if !g.scan_ready() {
        w::waiting(ui, tr("Wait for the scan to finish to see what has not been used lately."));
        return;
    }
    crate::clean_view::partial_note(g, ui);
    let Some(scan) = &g.scan else { return };
    let root = fmt::place(&scan.root);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Not opened or changed for more than")).color(C::dim(ui)));
        let before = g.stale_days;
        w::segmented(ui, &mut g.stale_days, &[(90, tr("3 months")), (180, tr("6 months")), (365, tr("1 year")), (730, tr("2 years"))]);
        if before != g.stale_days {
            g.stale_dirty = true;
            g.settings.stale_days = g.stale_days;
            g.save_settings();
            g.refresh_stale();
        }
    });
    ui.add_space(w::sp::XS);
    let total: u64 = g.stale.iter().map(|i| i.size).sum();
    let checked: Vec<&disk::StaleItem> = g.stale.iter().filter(|i| g.stale_checked.contains(&i.path)).collect();
    let (checked_n, checked_size) = (checked.len(), checked.iter().map(|i| i.size).sum::<u64>());
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(trf("{0}: found {1}", &[&root, &fmt::n(g.stale.len() as u64, fmt::Noun::Item)])).color(C::dim(ui)));
                ui.label(RichText::new(fmt::bytes(total)).metric().color(C::yellow()));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if checked_n > 0 {
                    trf("Move {0} to Trash · {1}", &[&checked_n, &fmt::bytes(checked_size)])
                } else {
                    tr("Select items to remove").into()
                };
                if w::big_button(ui, &label, C::red(), checked_n > 0).clicked() {
                    let items: Vec<(PathBuf, u64, String)> =
                        g.stale.iter().filter(|i| g.stale_checked.contains(&i.path)).map(|i| (i.path.clone(), i.size, fmt::age_in(i.used))).collect();
                    ask_trash_many(g, items, tr("not used for {0}"));
                }
                if w::plain_button(ui, tr("Select none")).clicked() {
                    g.stale_checked.clear();
                }
                if w::plain_button(ui, tr("Select safe")).on_hover_text(tr("Caches and data that is recreated automatically")).clicked() {
                    g.stale_checked = g.stale.iter().filter(|i| i.safety == DelSafety::Safe).map(|i| i.path.clone()).collect();
                }
            });
        });
        ui.label(
            RichText::new(tr("Not listed: photos, video and music, app data, cloud folders, system and protected places, and single files inside developer tools. “Opened” means the last time a file was read or changed."))
                .caption()
                .color(C::dim(ui)),
        );
    });
    ui.add_space(w::sp::S);
    if g.stale.is_empty() {
        w::empty(ui, tr("Nothing found — you use everything 👍"));
        return;
    }
    let (mut select, mut toggle, mut goto) = (None, None, None);
    TableBuilder::new(ui)
        .striped(true)
        .sense(Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(28.0))
        .column(Column::remainder().at_least(260.0).clip(true))
        .column(Column::exact(86.0))
        .column(Column::exact(140.0))
        .column(Column::exact(96.0))
        .header(24.0, |mut h| heads(&mut h, &["", tr("Item"), tr("Size"), tr("Not used for"), tr("Removal")]))
        .body(|body| {
            body.rows(44.0, g.stale.len(), |mut row| {
                let it = &g.stale[row.index()];
                row.set_selected(g.disk_sel.as_ref() == Some(&it.path));
                row.col(|ui| {
                    let mut on = g.stale_checked.contains(&it.path);
                    if ui.checkbox(&mut on, "").changed() {
                        toggle = Some(it.path.clone());
                    }
                });
                row.col(|ui| name_with_parent(ui, &it.path, it.is_dir));
                row.col(|ui| {
                    ui.label(RichText::new(fmt::bytes(it.size)).color(w::size_color(ui, it.size)));
                });
                row.col(|ui| {
                    ui.label(RichText::new(fmt::age(it.used)).color(w::age_color(ui, it.used))).on_hover_text(format!(
                        "{}: {}\n{}: {}",
                        tr("Opened"),
                        fmt::date(it.used),
                        tr("Modified"),
                        fmt::date(it.modified)
                    ));
                });
                row.col(|ui| {
                    w::del_badge(ui, it.safety).on_hover_text(&it.why);
                });
                let r = row.response();
                if r.clicked() {
                    select = Some(it.path.clone());
                }
                if r.double_clicked() {
                    goto = Some(it.path.clone());
                }
            });
        });
    if let Some(p) = toggle {
        if !g.stale_checked.remove(&p) {
            g.stale_checked.insert(p);
        }
    }
    if let Some(p) = select {
        g.disk_sel = Some(p);
    }
    if let Some(p) = goto {
        reveal_in_list(g, p);
    }
}

pub fn name_with_parent(ui: &mut Ui, p: &Path, is_dir: bool) {
    if p.extension().is_some_and(|e| e == "app") {
        icons::app(ui, p, 18.0);
    } else {
        w::file_icon(ui, is_dir, false, false);
    }
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        ui.label(RichText::new(name).semibold());
        ui.label(RichText::new(fmt::path(p.parent().unwrap_or(Path::new("")))).caption().color(C::dim(ui)));
    });
}

// ---------------------------------------------------------------------------
// Duplicates
// ---------------------------------------------------------------------------

fn dupes(g: &mut Gui, ui: &mut Ui) {
    let Some(ds) = g.dupes.clone() else {
        ui.add_space(w::sp::XL);
        if w::big_button(ui, tr("Find duplicates"), C::accent(), true).clicked() {
            g.start_dupes();
        }
        return;
    };
    if !ds.done() {
        let stage = ds.stage.load(Ordering::Relaxed);
        let text = if stage == 0 {
            trf("Listing files… {0}", &[&fmt::count(ds.files_seen.load(Ordering::Relaxed))])
        } else {
            trf("Comparing contents… {0} read", &[&fmt::bytes(ds.bytes_hashed.load(Ordering::Relaxed))])
        };
        w::waiting(ui, &text);
        return;
    }
    let groups = ds.groups.lock().unwrap().clone();
    // Default copy to keep: the oldest one (first in the group).
    for gr in &groups {
        if let Some(first) = gr.files.first() {
            let key = first.path.clone();
            g.dupes_keep.entry(key).or_insert_with(|| first.path.clone());
        }
    }
    let wasted: u64 = groups.iter().map(|gr| gr.wasted()).sum();
    let to_remove: Vec<(PathBuf, u64)> = groups
        .iter()
        .filter(|gr| gr.files.first().is_some_and(|f| g.dupes_selected.contains(&f.path)))
        .flat_map(|gr| {
            let keep = gr.files.first().and_then(|f| g.dupes_keep.get(&f.path)).cloned();
            gr.files.iter().filter(move |f| Some(&f.path) != keep.as_ref()).map(move |f| (f.path.clone(), gr.size))
        })
        .filter(|(p, _)| disk::deletion_safety(p).0 != DelSafety::Blocked)
        .collect();
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(trf("{1}: {0}", &[&fmt::n(groups.len() as u64, fmt::Noun::DupGroup), &fmt::place(&ds.root)])).color(C::dim(ui)),
                );
                ui.label(RichText::new(trf("{0} can be freed", &[&fmt::bytes(wasted)])).metric().color(C::yellow()));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let size: u64 = to_remove.iter().map(|(_, s)| s).sum();
                if w::big_button(
                    ui,
                    &trf("Remove {0} · {1}", &[&fmt::n(to_remove.len() as u64, fmt::Noun::Copy), &fmt::bytes(size)]),
                    C::red(),
                    !to_remove.is_empty(),
                )
                .clicked()
                {
                    let items = to_remove.iter().map(|(p, s)| (p.clone(), *s, String::new())).collect();
                    ask_trash_many(g, items, "");
                }
                if w::plain_button(ui, tr("⟳ Search again")).clicked() {
                    g.start_dupes();
                }
                if w::plain_button(ui, tr("Select none")).clicked() {
                    g.dupes_selected.clear();
                }
                if w::plain_button(ui, tr("Select groups outside projects"))
                    .on_hover_text(tr("Copies inside git projects are part of the code; removing them could break the project."))
                    .clicked()
                {
                    g.dupes_selected = groups.iter().filter(|gr| !gr.in_project).filter_map(|gr| gr.files.first().map(|f| f.path.clone())).collect();
                }
            });
        });
        ui.label(
            RichText::new(trf("Tick the groups to clean. One copy of each file is kept (the oldest, unless you choose another). Files are compared by content, not by name. Not searched: ~/Library, hidden folders, app bundles and files under {0} MB.", &[&g.settings.dupes_min_mb]))
                .caption()
                .color(C::dim(ui)),
        );
    });
    ui.add_space(w::sp::S);
    if groups.is_empty() {
        w::empty(ui, tr("No duplicates found."));
        return;
    }
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for gr in groups.iter().take(300) {
            let Some(first) = gr.files.first() else { continue };
            let key = first.path.clone();
            let name = first.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            w::card(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let mut on = g.dupes_selected.contains(&key);
                    if ui.checkbox(&mut on, "").on_hover_text(tr("Remove the other copies of this file")).changed() {
                        if on {
                            g.dupes_selected.insert(key.clone());
                        } else {
                            g.dupes_selected.remove(&key);
                        }
                    }
                    ui.label(RichText::new(&name).semibold());
                    ui.label(
                        RichText::new(format!("{} × {}", fmt::n(gr.files.len() as u64, fmt::Noun::Copy), fmt::bytes(gr.size))).color(C::dim(ui)),
                    );
                    if gr.in_project {
                        w::badge(ui, tr("in a project"), C::yellow())
                            .on_hover_text(tr("Copies inside git projects are part of the code; removing them could break the project."));
                    }
                });
                for f in &gr.files {
                    ui.horizontal(|ui| {
                        let keep = g.dupes_keep.get(&key) == Some(&f.path);
                        if ui.radio(keep, "").on_hover_text(tr("Keep this copy")).clicked() {
                            g.dupes_keep.insert(key.clone(), f.path.clone());
                        }
                        let text = RichText::new(fmt::path(&f.path));
                        ui.label(if keep { text.semibold() } else { text.color(C::dim(ui)) });
                        ui.label(RichText::new(fmt::date(f.modified)).caption().color(C::dim(ui)));
                        if keep {
                            w::badge(ui, tr("keep"), C::green());
                        }
                        if ui.small_button("↗").on_hover_text(tr("Show in Finder")).clicked() {
                            macpilot::trash::reveal_in_finder(&f.path);
                        }
                    });
                }
            });
            ui.add_space(w::sp::S);
        }
    });
}

// ---------------------------------------------------------------------------
// Details and removal
// ---------------------------------------------------------------------------

fn detail(g: &mut Gui, ui: &mut Ui) {
    let Some(path) = g.disk_sel.clone() else {
        ui.label(RichText::new(fmt::place(&g.cwd)).title());
        ui.label(RichText::new(format!("{} · {}", fmt::bytes(g.dir_total()), fmt::n(g.entries.len() as u64, fmt::Noun::Item))).color(C::dim(ui)));
        ui.add_space(w::sp::L);
        w::card(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(tr("How to use")).semibold());
            ui.add_space(w::sp::XS);
            for line in [
                tr("• Click to select, double-click to open a folder."),
                tr("• “Map” shows what takes space as tiles."),
                tr("• “Not used” lists what you have not opened for months."),
                tr("• “Duplicates” finds identical files."),
                tr("• Removal always goes to the Trash."),
            ] {
                ui.label(line);
            }
        });
        ui.add_space(w::sp::S);
        legend(ui);
        return;
    };
    let entry = g.entries.iter().find(|e| e.path == path).cloned();
    let md = std::fs::symlink_metadata(&path).ok();
    let is_dir = md.as_ref().is_some_and(|m| m.is_dir());
    let scan_st = g.scan.as_ref().and_then(|s| s.size_of(&path));
    let size = entry.as_ref().and_then(|e| e.size).or(scan_st.map(|s| s.size)).or_else(|| md.as_ref().filter(|m| !m.is_dir()).map(disk::alloc_size));
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());

    ui.horizontal(|ui| {
        if name.ends_with(".app") {
            icons::app(ui, &path, 32.0);
        } else {
            w::file_icon(ui, is_dir, false, false);
        }
        ui.add(egui::Label::new(RichText::new(&name).title()).wrap());
    });
    ui.label(RichText::new(fmt::path(&path)).color(C::dim(ui)));
    ui.add_space(w::sp::S);
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(size.map(fmt::bytes).unwrap_or(tr("measuring…").into())).metric().color(w::size_color(ui, size.unwrap_or(0))));
        if let Some(n) = entry.as_ref().and_then(|e| e.files).or(scan_st.filter(|_| is_dir).map(|s| s.files)) {
            ui.label(RichText::new(fmt::n(n, fmt::Noun::File)).color(C::dim(ui)));
        }
    });
    ui.add_space(w::sp::S);
    let (mtime, used) = match (&scan_st, &md) {
        (Some(st), _) if is_dir => (Some(st.modified), Some(st.used)),
        (_, Some(m)) => (Some(m.mtime()), Some(disk::used_time(m))),
        _ => (None, None),
    };
    let created = md.as_ref().and_then(|m| m.created().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64);
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let row = |ui: &mut Ui, k: &str, t: Option<i64>| {
            if let Some(t) = t.filter(|t| *t > 0) {
                ui.horizontal(|ui| {
                    ui.add_sized([120.0, 18.0], egui::Label::new(RichText::new(k).color(C::dim(ui))));
                    ui.label(RichText::new(fmt::date(t)).semibold());
                    ui.label(RichText::new(format!("· {}", fmt::ago(t))).color(w::age_color(ui, t)));
                });
            }
        };
        row(ui, tr("Modified"), mtime);
        row(ui, tr("Opened"), used);
        row(ui, tr("Created"), created);
        if is_dir {
            ui.label(RichText::new(tr("For a folder: the latest dates among everything inside.")).caption().color(C::dim(ui)));
        }
    });
    if let Some(u) = used.filter(|u| *u > 0 && disk::now_unix() - *u >= 180 * 86_400) {
        let media = disk::is_media(&path) || scan_st.is_some_and(|st| is_dir && st.media * 2 >= st.size);
        ui.add_space(w::sp::S);
        let text = if media {
            tr("Photos, video and music are valuable even when not opened, so they are never suggested for removal.")
        } else {
            tr("Probably no longer needed. If you are sure, move it to the Trash — it can be restored.")
        };
        w::note(ui, C::yellow(), &trf("Not used for {0}", &[&fmt::age_in(u)]), text);
    }
    ui.add_space(w::sp::S);
    let (safety, why) = disk::deletion_safety(&path);
    let title = match safety {
        DelSafety::Safe => tr("✔ Safe to remove"),
        DelSafety::Careful => tr("⚠ Remove with care"),
        DelSafety::Blocked => tr("⛔ Protected from removal"),
    };
    w::note(ui, w::del_color(safety), title, &why);
    ui.add_space(w::sp::M);
    ui.horizontal_wrapped(|ui| {
        if is_dir && w::plain_button(ui, tr("Open")).clicked() {
            g.disk_mode = DiskMode::List;
            g.go(path.clone());
        }
        if w::plain_button(ui, tr("Show in Finder")).clicked() {
            macpilot::trash::reveal_in_finder(&path);
        }
        if w::big_button(ui, tr("Move to Trash…"), C::red(), safety != DelSafety::Blocked).on_disabled_hover_text(&why).clicked() {
            ask_trash(g, &path);
        }
    });
    if is_dir {
        ui.add_space(w::sp::S);
        let r = ui.button(tr("Exclude from MacPilot")).on_hover_text(tr("MacPilot will never open this folder again. Undo in Settings → Privacy."));
        if r.clicked() {
            g.settings.set_excluded(&path, true);
            g.disk_sel = None;
            g.exclusions_changed();
        }
    }
}

fn legend(ui: &mut Ui) {
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Removal labels")).semibold());
        ui.add_space(w::sp::XS);
        for (s, t) in [
            (DelSafety::Safe, tr("caches, logs, build folders — recreated automatically")),
            (DelSafety::Careful, tr("your files and app data — think before removing")),
            (DelSafety::Blocked, tr("system, ~/Library, keychains, ~/.ssh, .git, parts of apps")),
        ] {
            ui.horizontal_wrapped(|ui| {
                w::del_badge(ui, s);
                ui.label(RichText::new(t).color(C::dim(ui)));
            });
        }
    });
}

pub fn ask_trash(g: &mut Gui, path: &Path) {
    let (safety, why) = disk::deletion_safety(path);
    if safety == DelSafety::Blocked {
        g.toast(trf("Protected: {0}", &[&why]), Level::Danger);
        return;
    }
    let st = g.scan.as_ref().and_then(|s| s.size_of(path)).unwrap_or_else(|| disk::measure(path));
    let mut lines = vec![
        (fmt::path(path), Level::Info),
        (
            trf("Size: {0}", &[&fmt::bytes(st.size)])
                + &if st.files > 1 { format!(" · {}", fmt::n(st.files, fmt::Noun::File)) } else { String::new() },
            Level::Info,
        ),
        (why, if safety == DelSafety::Safe { Level::Ok } else { Level::Warn }),
        (tr("It goes to the Trash and can be restored until the Trash is emptied.").into(), Level::Ok),
    ];
    let running: Vec<String> = g
        .snap
        .procs
        .iter()
        .filter(|p| p.exe.as_deref().is_some_and(|e| e.starts_with(path)) || p.cwd.as_deref().is_some_and(|c| c.starts_with(path)))
        .map(|p| p.name.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .take(5)
        .collect();
    if !running.is_empty() {
        lines.push((trf("Running from here right now: {0} — quit them first.", &[&running.join(", ")]), Level::Warn));
    }
    g.confirm =
        Some(Confirm::new(tr("Move to the Trash?"), lines, Action::Trash { paths: vec![path.to_path_buf()], size: st.size }, tr("Move to Trash")));
}

/// Confirm removal of several items. `note_fmt` (with `{0}`) is shown after each item.
pub fn ask_trash_many(g: &mut Gui, items: Vec<(PathBuf, u64, String)>, note_fmt: &str) {
    if items.is_empty() {
        return;
    }
    let size: u64 = items.iter().map(|i| i.1).sum();
    let careful = items.iter().filter(|i| disk::deletion_safety(&i.0).0 == DelSafety::Careful).count();
    let mut lines = vec![(format!("{}, {}", fmt::n(items.len() as u64, fmt::Noun::Item), fmt::bytes(size)), Level::Info)];
    for (p, s, note) in items.iter().take(10) {
        let extra = if note.is_empty() || note_fmt.is_empty() { String::new() } else { format!(", {}", note_fmt.replace("{0}", note)) };
        lines.push((format!("• {} — {}{extra}", fmt::path(p), fmt::bytes(*s)), Level::Info));
    }
    if items.len() > 10 {
        lines.push((trf("…and {0} more", &[&(items.len() - 10)]), Level::Info));
    }
    if careful > 0 {
        lines.push((trf("{0} of them are your own data (label “careful”): make sure you do not need them.", &[&careful]), Level::Warn));
    }
    lines.push((tr("Everything goes to the Trash and can be restored until the Trash is emptied.").into(), Level::Ok));
    let mut c =
        Confirm::new(tr("Move to the Trash?"), lines, Action::Trash { paths: items.into_iter().map(|i| i.0).collect(), size }, tr("Move to Trash"));
    c.danger = careful > 0;
    g.confirm = Some(c);
}

/// Local Time Machine snapshots: they hold on to deleted files, so the disk does not get freer.
fn snapshots(g: &mut Gui, ui: &mut Ui, list: &[String]) {
    if list.is_empty() {
        return;
    }
    ui.add_space(w::sp::L);
    let mut delete: Option<Vec<String>> = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Time Machine snapshots on this disk")).section());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if list.len() > 1 && ui.button(tr("Delete all…")).clicked() {
                delete = Some(list.to_vec());
            }
        });
    });
    ui.add_space(w::sp::S);
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new(tr(
                "Hourly copies macOS keeps on the disk for a day. They hold on to files you deleted, so the space is not freed at once. macOS removes them by itself when space runs low; deleting them here frees it now.",
            ))
            .callout()
            .color(C::dim(ui)),
        );
        ui.add_space(w::sp::S);
        for (i, name) in list.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                match macpilot::space::snapshot_time(name) {
                    Some(t) => {
                        fixed(ui, 230.0, RichText::new(fmt::ago(t)).semibold());
                        ui.label(RichText::new(name).callout().color(C::dim(ui)));
                    }
                    None => {
                        ui.label(RichText::new(name).semibold());
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Delete…")).clicked() {
                        delete = Some(vec![name.clone()]);
                    }
                });
            });
        }
    });
    if let Some(dates) = delete {
        let lines = vec![
            (trf("Snapshots: {0}", &[&dates.len()]), Level::Info),
            (tr("Your files stay as they are. What goes away is the possibility to return to the state of that hour.").into(), Level::Warn),
            (tr("This cannot be undone. macOS may ask for the administrator password.").into(), Level::Danger),
        ];
        let mut c = Confirm::new(tr("Delete the snapshots?"), lines, Action::DeleteSnapshots(dates), tr("Delete"));
        c.danger = true;
        g.confirm = Some(c);
    }
}
