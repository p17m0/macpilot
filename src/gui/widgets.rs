//! Shared widgets and styling.

use std::collections::VecDeque;

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, TextStyle, Ui, Vec2};
use macpilot::disk::DelSafety;
use macpilot::procs::Safety;
use macpilot::tr;

/// Palette.
pub struct C;

impl C {
    pub const ACCENT: Color32 = Color32::from_rgb(64, 156, 255);
    pub const GREEN: Color32 = Color32::from_rgb(52, 199, 89);
    pub const YELLOW: Color32 = Color32::from_rgb(255, 176, 32);
    pub const RED: Color32 = Color32::from_rgb(255, 69, 58);
    pub const PURPLE: Color32 = Color32::from_rgb(175, 82, 222);

    pub fn dark(ui: &Ui) -> bool {
        ui.visuals().dark_mode
    }
    pub fn dim(ui: &Ui) -> Color32 {
        if Self::dark(ui) { Color32::from_gray(150) } else { Color32::from_gray(105) }
    }
    pub fn text(ui: &Ui) -> Color32 {
        if Self::dark(ui) { Color32::from_gray(235) } else { Color32::from_gray(25) }
    }
    pub fn bg_bar(ui: &Ui) -> Color32 {
        if Self::dark(ui) { Color32::from_rgb(28, 29, 33) } else { Color32::from_rgb(236, 237, 240) }
    }
    pub fn card(ui: &Ui) -> Color32 {
        if Self::dark(ui) { Color32::from_rgb(36, 38, 43) } else { Color32::WHITE }
    }
    pub fn track(ui: &Ui) -> Color32 {
        if Self::dark(ui) { Color32::from_rgb(55, 58, 64) } else { Color32::from_rgb(222, 224, 228) }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Ok,
    Warn,
    Danger,
}

impl Level {
    pub fn color(self, ui: &Ui) -> Color32 {
        match self {
            Level::Info => C::text(ui),
            Level::Ok => C::GREEN,
            Level::Warn => C::YELLOW,
            Level::Danger => C::RED,
        }
    }
}

pub fn setup_style(ctx: &egui::Context) {
    // Use the macOS system font when available; the bundled font stays as a fallback.
    let mut fonts = egui::FontDefinitions::default();
    for (name, path) in [("sf", "/System/Library/Fonts/SFNS.ttf"), ("helvetica", "/System/Library/Fonts/Helvetica.ttc")] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(name.into(), egui::FontData::from_owned(bytes).into());
            fonts.families.entry(FontFamily::Proportional).or_default().insert(0, name.into());
            break;
        }
    }
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, FontId::proportional(13.5)),
            (TextStyle::Heading, FontId::proportional(22.0)),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        s.visuals.selection.bg_fill = C::ACCENT.gamma_multiply(0.45);
        s.visuals.selection.stroke = Stroke::new(1.0, C::ACCENT);
        for w in [&mut s.visuals.widgets.inactive, &mut s.visuals.widgets.hovered, &mut s.visuals.widgets.active] {
            w.corner_radius = CornerRadius::same(6);
        }
        s.visuals.window_corner_radius = CornerRadius::same(12);
    });
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.visuals.panel_fill = Color32::from_rgb(22, 23, 26);
        s.visuals.extreme_bg_color = Color32::from_rgb(30, 31, 35);
        s.visuals.faint_bg_color = Color32::from_rgb(30, 32, 36);
    });
    ctx.style_mut_of(egui::Theme::Light, |s| {
        s.visuals.panel_fill = Color32::from_rgb(246, 247, 249);
        s.visuals.faint_bg_color = Color32::from_rgb(240, 241, 244);
    });
}

pub fn page_frame(ui: &Ui) -> egui::Frame {
    egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(egui::Margin { left: 18, right: 14, top: 14, bottom: 10 })
}

pub fn side_frame(ui: &Ui) -> egui::Frame {
    egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(egui::Margin::same(14))
}

/// Page title with an optional subtitle.
pub fn header(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(24.0).strong());
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(C::dim(ui)));
    }
    ui.add_space(8.0);
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(C::card(ui))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .stroke(Stroke::new(1.0, C::track(ui)))
        .show(ui, |ui| ui.with_layout(egui::Layout::top_down(egui::Align::Min), add).inner)
        .inner
}

/// Colored hint box (warnings, explanations).
pub fn note(ui: &mut Ui, color: Color32, title: &str, text: &str) {
    egui::Frame::new().fill(color.gamma_multiply(0.12)).corner_radius(8).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if !title.is_empty() {
            ui.label(RichText::new(title).strong().color(color));
        }
        if !text.is_empty() {
            ui.label(text);
        }
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Overview,
    Procs,
    Disk,
    Clean,
    Apps,
    Startup,
    Settings,
}

/// Simple vector icons (no icon font needed).
pub fn paint_icon(p: &egui::Painter, r: Rect, icon: Icon, c: Color32) {
    let s = Stroke::new(1.6, c);
    let m = r.center();
    let u = r.width() / 16.0;
    match icon {
        Icon::Overview => {
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let cc = m + Vec2::new(dx * 3.6 * u, dy * 3.6 * u);
                p.rect_stroke(Rect::from_center_size(cc, Vec2::splat(5.6 * u)), 1.5 * u, s, egui::StrokeKind::Middle);
            }
        }
        Icon::Procs => {
            for (i, h) in [5.0, 9.0, 12.0, 7.0].iter().enumerate() {
                let x = r.left() + (2.5 + i as f32 * 3.6) * u;
                p.line_segment([Pos2::new(x, r.bottom() - 2.0 * u), Pos2::new(x, r.bottom() - (2.0 + h) * u)], Stroke::new(2.2 * u, c));
            }
        }
        Icon::Disk => {
            p.circle_stroke(m, 6.5 * u, s);
            p.circle_filled(m, 1.6 * u, c);
            p.line_segment([m, m + Vec2::new(4.6 * u, -4.6 * u)], s);
        }
        Icon::Clean => {
            // A sparkle.
            let a = 6.5 * u;
            let b = 1.8 * u;
            let pts = [
                m + Vec2::new(0.0, -a),
                m + Vec2::new(b, -b),
                m + Vec2::new(a, 0.0),
                m + Vec2::new(b, b),
                m + Vec2::new(0.0, a),
                m + Vec2::new(-b, b),
                m + Vec2::new(-a, 0.0),
                m + Vec2::new(-b, -b),
            ];
            p.add(egui::Shape::closed_line(pts.to_vec(), s));
        }
        Icon::Apps => {
            p.rect_stroke(Rect::from_center_size(m, Vec2::splat(12.0 * u)), 3.0 * u, s, egui::StrokeKind::Middle);
            p.circle_filled(m, 2.2 * u, c);
        }
        Icon::Startup => {
            let rr = 5.8 * u;
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = -std::f32::consts::FRAC_PI_2 + 0.6 + i as f32 / 24.0 * (std::f32::consts::TAU - 1.2);
                    m + Vec2::new(t.cos() * rr, t.sin() * rr)
                })
                .collect();
            p.add(egui::Shape::line(pts, s));
            p.line_segment([m + Vec2::new(0.0, -7.2 * u), m + Vec2::new(0.0, -1.5 * u)], s);
        }
        Icon::Settings => {
            p.circle_stroke(m, 3.0 * u, s);
            for i in 0..8 {
                let t = i as f32 / 8.0 * std::f32::consts::TAU;
                let d = Vec2::new(t.cos(), t.sin());
                p.line_segment([m + d * 4.8 * u, m + d * 7.0 * u], Stroke::new(2.0 * u, c));
            }
        }
    }
}

pub fn nav_item(ui: &mut Ui, selected: bool, icon: Icon, label: &str, badge: Option<usize>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 8, C::ACCENT);
    } else if resp.hovered() {
        p.rect_filled(rect, 8, C::track(ui).gamma_multiply(0.6));
    }
    let fg = if selected { Color32::WHITE } else { C::text(ui) };
    let ir = Rect::from_center_size(Pos2::new(rect.left() + 20.0, rect.center().y), Vec2::splat(18.0));
    paint_icon(p, ir, icon, if selected { Color32::WHITE } else { C::ACCENT });
    p.text(Pos2::new(rect.left() + 38.0, rect.center().y), egui::Align2::LEFT_CENTER, label, FontId::proportional(14.0), fg);
    if let Some(n) = badge {
        let br = Rect::from_center_size(Pos2::new(rect.right() - 16.0, rect.center().y), Vec2::new(22.0, 18.0));
        p.rect_filled(br, 9, C::RED);
        p.text(br.center(), egui::Align2::CENTER_CENTER, n.to_string(), FontId::proportional(11.0), Color32::WHITE);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The app logo (same design as the app icon).
pub fn app_logo(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, size * 0.24, Color32::from_rgb(76, 100, 245));
    let m = rect.center() + Vec2::new(0.0, size * 0.05);
    let rr = size * 0.3;
    let pts: Vec<Pos2> = (0..=20)
        .map(|i| {
            let t = std::f32::consts::PI * (5.0 / 6.0) + i as f32 / 20.0 * std::f32::consts::PI * (4.0 / 3.0);
            m + Vec2::new(t.cos() * rr, t.sin() * rr)
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(size * 0.08, Color32::WHITE)));
    p.line_segment([m, m + Vec2::new(size * 0.17, -size * 0.17)], Stroke::new(size * 0.06, Color32::WHITE));
    p.circle_filled(m, size * 0.06, Color32::WHITE);
}

/// Segmented control.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    egui::Frame::new().fill(C::track(ui)).corner_radius(8).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (v, label) in options {
                let sel = *value == *v;
                let text = RichText::new(*label).color(if sel { C::text(ui) } else { C::dim(ui) });
                let b = egui::Button::new(text).fill(if sel { C::card(ui) } else { Color32::TRANSPARENT }).corner_radius(6).stroke(Stroke::NONE);
                if ui.add(b).clicked() && !sel {
                    *value = *v;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// iOS-style switch.
pub fn switch(ui: &mut Ui, on: &mut bool) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(38.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let p = ui.painter();
    let bg = if *on { C::GREEN } else { C::track(ui) };
    p.rect_filled(rect, 11, bg);
    let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    p.circle_filled(Pos2::new(x, rect.center().y), 9.0, Color32::WHITE);
    resp
}

pub fn ratio_color(r: f32) -> Color32 {
    if r >= 0.9 {
        C::RED
    } else if r >= 0.75 {
        C::YELLOW
    } else {
        C::GREEN
    }
}

/// Meter in the sidebar: title, value, and a sparkline or a bar.
pub fn side_meter(ui: &mut Ui, title: &str, value: &str, ratio: f32, hist: Option<&VecDeque<f32>>) {
    let size = Vec2::new(ui.available_width(), 44.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let color = ratio_color(ratio);
    let p = ui.painter();
    p.rect_filled(rect, 8, C::card(ui));
    p.text(rect.left_top() + Vec2::new(10.0, 6.0), egui::Align2::LEFT_TOP, title, FontId::proportional(10.5), C::dim(ui));
    p.text(rect.left_top() + Vec2::new(10.0, 20.0), egui::Align2::LEFT_TOP, value, FontId::proportional(13.0), color);
    match hist {
        Some(h) => {
            let r = Rect::from_min_size(Pos2::new(rect.right() - 62.0, rect.top() + 8.0), Vec2::new(52.0, 28.0));
            paint_sparkline(ui, h, r, color);
        }
        None => {
            let r = Rect::from_min_size(Pos2::new(rect.left() + 10.0, rect.bottom() - 7.0), Vec2::new(size.x - 20.0, 3.0));
            p.rect_filled(r, 2, C::track(ui));
            let mut f = r;
            f.set_width(r.width() * ratio.clamp(0.0, 1.0));
            p.rect_filled(f, 2, color);
        }
    }
    ui.add_space(4.0);
}

pub fn paint_sparkline(ui: &Ui, h: &VecDeque<f32>, rect: Rect, color: Color32) {
    let p = ui.painter();
    if h.len() < 2 {
        return;
    }
    let n = 60usize;
    let step = rect.width() / (n - 1) as f32;
    let start = n - h.len();
    let pts: Vec<Pos2> = h
        .iter()
        .enumerate()
        .map(|(i, v)| Pos2::new(rect.left() + (start + i) as f32 * step, rect.bottom() - (v / 100.0).clamp(0.0, 1.0) * rect.height()))
        .collect();
    for w in pts.windows(2) {
        let poly = vec![w[0], w[1], Pos2::new(w[1].x, rect.bottom()), Pos2::new(w[0].x, rect.bottom())];
        p.add(egui::Shape::convex_polygon(poly, color.gamma_multiply(0.15), Stroke::NONE));
    }
    p.add(egui::Shape::line(pts, Stroke::new(1.5, color)));
}

/// Folder / app / file icon.
pub fn file_icon(ui: &mut Ui, is_dir: bool, is_app: bool, is_link: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(18.0, 16.0), Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    if is_app {
        let r = Rect::from_center_size(c, Vec2::splat(14.0));
        p.rect_filled(r, 4, C::PURPLE);
        p.circle_stroke(c, 3.5, Stroke::new(1.5, Color32::WHITE));
    } else if is_dir {
        let body = Rect::from_min_max(Pos2::new(rect.left() + 1.0, rect.top() + 4.0), Pos2::new(rect.right() - 1.0, rect.bottom() - 1.0));
        let tab = Rect::from_min_size(Pos2::new(rect.left() + 1.0, rect.top() + 2.0), Vec2::new(7.0, 4.0));
        let col = if is_link { C::dim(ui) } else { Color32::from_rgb(90, 170, 250) };
        p.rect_filled(tab, 1.5, col.gamma_multiply(0.8));
        p.rect_filled(body, 2.5, col);
    } else {
        let r = Rect::from_center_size(c, Vec2::new(11.0, 14.0));
        p.rect_filled(r, 2, C::track(ui));
        p.rect_stroke(r, 2, Stroke::new(1.0, C::dim(ui)), egui::StrokeKind::Inside);
    }
}

pub fn dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
    ui.painter().rect_filled(rect.shrink(1.0), 3, color);
}

pub fn bar(ui: &mut Ui, ratio: f32, size: Vec2, color: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    let r = (size.y / 2.0) as u8;
    p.rect_filled(rect, r, C::track(ui));
    let mut f = rect;
    f.set_width((rect.width() * ratio.clamp(0.0, 1.0)).max(if ratio > 0.0 { 2.0 } else { 0.0 }));
    p.rect_filled(f, r, color);
    resp
}

pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(11.0), color);
    let size = galley.size() + Vec2::new(12.0, 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, 5, color.gamma_multiply(0.16));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

pub fn safety_color(s: Safety) -> Color32 {
    match s {
        Safety::User => C::GREEN,
        Safety::System => C::YELLOW,
        Safety::Critical => C::RED,
    }
}

pub fn safety_badge(ui: &mut Ui, s: Safety) -> egui::Response {
    badge(ui, s.label(), safety_color(s))
}

pub fn del_color(s: DelSafety) -> Color32 {
    match s {
        DelSafety::Safe => C::GREEN,
        DelSafety::Careful => C::YELLOW,
        DelSafety::Blocked => C::RED,
    }
}

pub fn del_badge(ui: &mut Ui, s: DelSafety) -> egui::Response {
    badge(ui, s.label(), del_color(s))
}

pub fn cpu_color(ui: &Ui, c: f32) -> Color32 {
    if c >= 80.0 {
        C::RED
    } else if c >= 25.0 {
        C::YELLOW
    } else if c >= 1.0 {
        C::text(ui)
    } else {
        C::dim(ui)
    }
}

pub fn mem_color(ui: &Ui, m: u64) -> Color32 {
    if m >= 2_000_000_000 {
        C::RED
    } else if m >= 500_000_000 {
        C::YELLOW
    } else if m >= 30_000_000 {
        C::text(ui)
    } else {
        C::dim(ui)
    }
}

pub fn size_color(ui: &Ui, s: u64) -> Color32 {
    if s >= 10_000_000_000 {
        C::RED
    } else if s >= 1_000_000_000 {
        C::YELLOW
    } else if s >= 100_000_000 {
        C::text(ui)
    } else {
        C::dim(ui)
    }
}

/// Age color: fresh, 6+ months, 1+ year.
pub fn age_color(ui: &Ui, ts: i64) -> Color32 {
    let days = (macpilot::disk::now_unix() - ts) / 86_400;
    if days >= 365 {
        C::YELLOW
    } else if days >= 180 {
        Color32::from_rgb(220, 190, 120)
    } else if days <= 7 {
        C::text(ui)
    } else {
        C::dim(ui)
    }
}

/// "3 days ago" cell with the exact date on hover.
pub fn time_cell(ui: &mut Ui, ts: Option<i64>) {
    match ts.filter(|t| *t > 0) {
        Some(t) => {
            ui.label(RichText::new(macpilot::fmt::ago(t)).color(age_color(ui, t))).on_hover_text(macpilot::fmt::date(t));
        }
        None => {
            ui.label(RichText::new("—").color(C::dim(ui)));
        }
    }
}

/// Label–value row in a details card.
pub fn kv(ui: &mut Ui, k: &str, v: impl Into<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.add_sized([120.0, 18.0], egui::Label::new(RichText::new(k).color(C::dim(ui))));
        ui.label(v.into());
    });
}

/// The data volume (APFS "Data"), or the root volume.
pub fn data_volume(disks: &sysinfo::Disks) -> Option<(u64, u64)> {
    let l = disks.list();
    l.iter()
        .find(|d| d.mount_point() == std::path::Path::new("/System/Volumes/Data"))
        .or_else(|| l.iter().find(|d| d.mount_point() == std::path::Path::new("/")))
        .map(|d| (d.available_space(), d.total_space()))
}

pub fn big_button(ui: &mut Ui, text: &str, color: Color32, enabled: bool) -> egui::Response {
    let b = egui::Button::new(RichText::new(text).strong().color(Color32::WHITE)).fill(color).corner_radius(8).min_size(Vec2::new(0.0, 32.0));
    ui.add_enabled(enabled, b)
}

pub fn plain_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(text).corner_radius(8).min_size(Vec2::new(0.0, 32.0)))
}

/// Checkbox header row helper: "select all / none".
pub fn waiting(ui: &mut Ui, text: &str) {
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        ui.spinner();
        ui.label(RichText::new(text).color(C::dim(ui)));
    });
}

pub fn empty(ui: &mut Ui, text: &str) {
    ui.add_space(30.0);
    ui.vertical_centered(|ui| ui.label(RichText::new(text).size(15.0).color(C::dim(ui))));
}

pub fn yes_word() -> &'static str {
    tr("yes")
}
