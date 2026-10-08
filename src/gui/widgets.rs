//! Shared widgets and styling.

use std::collections::VecDeque;

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, TextStyle, Ui, Vec2, WidgetInfo, WidgetType,
};
use macpilot::disk::DelSafety;
use macpilot::procs::Safety;
use macpilot::settings::UiStyle;
use macpilot::tr;

static CLASSIC: AtomicBool = AtomicBool::new(false);
static DARK: AtomicBool = AtomicBool::new(false);

pub fn classic() -> bool {
    CLASSIC.load(Ordering::Relaxed)
}

/// Remember the current light/dark mode for colors that are asked for without a `Ui`.
pub fn begin_frame(ctx: &egui::Context) {
    DARK.store(ctx.theme() == egui::Theme::Dark, Ordering::Relaxed);
}

fn dark_now() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Palette. The standard colors, or black and white in the Classic style.
pub struct C;

impl C {
    fn pick(r: u8, g: u8, b: u8) -> Color32 {
        if classic() { Self::fg() } else { Color32::from_rgb(r, g, b) }
    }
    pub fn accent() -> Color32 {
        Self::pick(64, 156, 255)
    }
    pub fn green() -> Color32 {
        Self::pick(52, 199, 89)
    }
    pub fn yellow() -> Color32 {
        Self::pick(255, 176, 32)
    }
    pub fn red() -> Color32 {
        Self::pick(255, 69, 58)
    }
    pub fn purple() -> Color32 {
        Self::pick(175, 82, 222)
    }
    /// Ink of the Classic style: black on white, or white on black.
    pub fn fg() -> Color32 {
        if dark_now() { Color32::WHITE } else { Color32::BLACK }
    }
    /// What is cut out of a filled icon of color `c`: the background it sits on.
    pub fn paper_on(c: Color32) -> Color32 {
        if classic() {
            return if c == Self::fg() { Self::paper() } else { Self::fg() };
        }
        // Selected icons are white on the accent: their cut-outs take the accent.
        if c == Color32::WHITE { Self::accent() } else { Color32::WHITE }
    }
    /// Paper of the Classic style.
    pub fn paper() -> Color32 {
        if dark_now() { Color32::BLACK } else { Color32::WHITE }
    }

    pub fn dark(ui: &Ui) -> bool {
        ui.visuals().dark_mode
    }
    pub fn dim(ui: &Ui) -> Color32 {
        match (classic(), Self::dark(ui)) {
            (true, false) => Color32::from_gray(85),
            (true, true) => Color32::from_gray(175),
            (false, true) => Color32::from_gray(150),
            (false, false) => Color32::from_gray(105),
        }
    }
    pub fn text(ui: &Ui) -> Color32 {
        if classic() {
            return Self::fg();
        }
        if Self::dark(ui) { Color32::from_gray(235) } else { Color32::from_gray(25) }
    }
    /// Sidebar and status bar.
    pub fn bg_bar(ui: &Ui) -> Color32 {
        match (classic(), Self::dark(ui)) {
            (true, _) => Self::paper(),
            (false, true) => Color32::from_rgb(28, 29, 33),
            (false, false) => Color32::from_rgb(236, 237, 240),
        }
    }
    pub fn card(ui: &Ui) -> Color32 {
        match (classic(), Self::dark(ui)) {
            (true, _) => Self::paper(),
            (false, true) => Color32::from_rgb(36, 38, 43),
            (false, false) => Color32::WHITE,
        }
    }
    pub fn track(ui: &Ui) -> Color32 {
        match (classic(), Self::dark(ui)) {
            (true, true) => Color32::from_gray(70),
            (true, false) => Color32::from_gray(200),
            (false, true) => Color32::from_rgb(55, 58, 64),
            (false, false) => Color32::from_rgb(222, 224, 228),
        }
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
            Level::Ok => C::green(),
            Level::Warn => C::yellow(),
            Level::Danger => C::red(),
        }
    }
}

/// The Classic pixel font (Pixelify Sans, SIL Open Font License — see assets/fonts).
const PIXEL_FONT: &[u8] = include_bytes!("../../assets/fonts/PixelifySans.ttf");

/// Type scale, after the macOS Human Interface Guidelines.
pub mod ty {
    /// Secondary lines: bundle ids, paths, labels under numbers.
    pub const CAPTION: f32 = 11.0;
    /// Small print that still has to be read: descriptions in cards.
    pub const CALLOUT: f32 = 12.0;
    pub const BODY: f32 = 13.0;
    /// Card titles.
    pub const HEADLINE: f32 = 15.0;
    /// Section titles inside a page.
    pub const SECTION: f32 = 17.0;
    /// The name of the selected thing in a side panel.
    pub const TITLE: f32 = 20.0;
    /// Big numbers.
    pub const METRIC: f32 = 26.0;
    /// Page titles.
    pub const LARGE_TITLE: f32 = 26.0;
}

/// Text in SF Pro Text Semibold (bold pixels in Classic).
const SEMIBOLD: &str = "semibold";
/// SF Pro Display Bold, for page titles.
const DISPLAY_BOLD: &str = "display-bold";
/// SF Pro Display Semibold, for titles and big numbers.
const DISPLAY_SEMIBOLD: &str = "display-semibold";

/// Text roles. egui's `strong()` only changes the color; these change the weight too.
pub trait Txt {
    fn semibold(self) -> Self;
    fn caption(self) -> Self;
    fn callout(self) -> Self;
    fn headline(self) -> Self;
    fn section(self) -> Self;
    fn title(self) -> Self;
    fn metric(self) -> Self;
    fn large_title(self) -> Self;
}

impl Txt for RichText {
    fn semibold(self) -> Self {
        self.strong().family(FontFamily::Name(SEMIBOLD.into()))
    }
    fn caption(self) -> Self {
        self.size(ty::CAPTION)
    }
    fn callout(self) -> Self {
        self.size(ty::CALLOUT)
    }
    fn headline(self) -> Self {
        self.semibold().size(ty::HEADLINE)
    }
    fn section(self) -> Self {
        self.semibold().size(ty::SECTION)
    }
    fn title(self) -> Self {
        self.strong().family(FontFamily::Name(DISPLAY_SEMIBOLD.into())).size(ty::TITLE)
    }
    fn metric(self) -> Self {
        self.strong().family(FontFamily::Name(DISPLAY_SEMIBOLD.into())).size(ty::METRIC)
    }
    fn large_title(self) -> Self {
        self.strong().family(FontFamily::Name(DISPLAY_BOLD.into())).size(ty::LARGE_TITLE)
    }
}

pub fn semibold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// A system font read once and kept for the whole run: every weight below shares the same bytes.
fn system_font(path: &str) -> Option<&'static [u8]> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    type Fonts = HashMap<String, Option<&'static [u8]>>;
    static CACHE: OnceLock<Mutex<Fonts>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().unwrap();
    *c.entry(path.to_string()).or_insert_with(|| std::fs::read(path).ok().map(|b| &*Box::leak(b.into_boxed_slice())))
}

/// `bytes` at the given variation coordinates (`wght`, `opsz`…).
fn variant(bytes: &'static [u8], coords: &[(&[u8; 4], f32)]) -> egui::FontData {
    use egui::epaint::text::{FontTweak, VariationCoords};
    let tweak = FontTweak { coords: VariationCoords::new(coords.iter().map(|(t, v)| (**t, *v))), ..Default::default() };
    egui::FontData::from_static(bytes).tweak(tweak)
}

fn fonts(classic: bool) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    // Family → the fonts to put in front of egui's own (which stay as a fallback for missing glyphs).
    let mut front: Vec<(FontFamily, Vec<(String, egui::FontData)>)> = Vec::new();
    let named = |n: &str| FontFamily::Name(n.into());
    if let Some(sf) = system_font("/System/Library/Fonts/SFNS.ttf") {
        // SF is one variable font: `opsz` picks Text (small sizes, open spacing) or Display (large, tight).
        // Its default is Display, which is too tight for 13 pt.
        front.push((FontFamily::Proportional, vec![("sf-text".into(), variant(sf, &[(b"wght", 400.0), (b"opsz", 17.0)]))]));
        front.push((named(SEMIBOLD), vec![("sf-text-semibold".into(), variant(sf, &[(b"wght", 590.0), (b"opsz", 17.0)]))]));
        front.push((named(DISPLAY_SEMIBOLD), vec![("sf-display-semibold".into(), variant(sf, &[(b"wght", 590.0), (b"opsz", 28.0)]))]));
        front.push((named(DISPLAY_BOLD), vec![("sf-display-bold".into(), variant(sf, &[(b"wght", 700.0), (b"opsz", 28.0)]))]));
    } else if let Some(h) = system_font("/System/Library/Fonts/Helvetica.ttc") {
        for f in [FontFamily::Proportional, named(SEMIBOLD), named(DISPLAY_SEMIBOLD), named(DISPLAY_BOLD)] {
            front.push((f, vec![("helvetica".into(), egui::FontData::from_static(h))]));
        }
    } else {
        for f in [named(SEMIBOLD), named(DISPLAY_SEMIBOLD), named(DISPLAY_BOLD)] {
            front.push((f, Vec::new()));
        }
    }
    if let Some(mono) = system_font("/System/Library/Fonts/SFNSMono.ttf") {
        // SF Mono defaults to its lightest weight.
        front.push((FontFamily::Monospace, vec![("sf-mono".into(), variant(mono, &[(b"wght", 400.0)]))]));
    }
    if classic {
        for (family, list) in &mut front {
            let w = match family {
                FontFamily::Proportional => 400.0,
                FontFamily::Monospace => continue,
                _ => 700.0,
            };
            list.insert(0, (format!("pixel-{w}"), variant(PIXEL_FONT, &[(b"wght", w)])));
        }
    }
    for (family, list) in front {
        let names = fonts.families.entry(family).or_insert_with(|| fallback.clone());
        for (i, (name, data)) in list.into_iter().enumerate() {
            names.insert(i, name.clone());
            fonts.font_data.insert(name, data.into());
        }
    }
    fonts
}

/// Apply a style (standard or Classic) to both the light and the dark theme.
pub fn setup_style(ctx: &egui::Context, style: UiStyle) {
    let classic = style == UiStyle::Classic;
    CLASSIC.store(classic, Ordering::Relaxed);
    ctx.set_fonts(fonts(classic));
    ctx.all_styles_mut(|s| {
        // The pixel font looks right a little larger.
        let k = if classic { 1.08 } else { 1.0 };
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(ty::CAPTION * k)),
            (TextStyle::Body, FontId::proportional(ty::BODY * k)),
            (TextStyle::Button, FontId::proportional(ty::BODY * k)),
            (TextStyle::Heading, FontId::new(ty::LARGE_TITLE * k, FontFamily::Name(DISPLAY_BOLD.into()))),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = if classic { Vec2::new(12.0, 4.0) } else { Vec2::new(10.0, 5.0) };
        s.spacing.interact_size.y = 26.0;
    });
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let dark = theme == egui::Theme::Dark;
        ctx.style_mut_of(theme, |s| {
            let v = &mut s.visuals;
            if classic {
                let (ink, paper) = if dark { (Color32::WHITE, Color32::BLACK) } else { (Color32::BLACK, Color32::WHITE) };
                v.panel_fill = paper;
                v.window_fill = paper;
                v.extreme_bg_color = paper;
                v.faint_bg_color = if dark { Color32::from_gray(28) } else { Color32::from_gray(236) };
                v.window_stroke = Stroke::new(1.0, ink);
                v.window_corner_radius = CornerRadius::ZERO;
                v.menu_corner_radius = CornerRadius::ZERO;
                // The hard drop shadow of classic Mac windows and menus.
                v.window_shadow = egui::epaint::Shadow { offset: [3, 3], blur: 0, spread: 0, color: ink };
                v.popup_shadow = egui::epaint::Shadow { offset: [2, 2], blur: 0, spread: 0, color: ink };
                v.selection.bg_fill = ink;
                v.selection.stroke = Stroke::new(1.0, paper);
                v.hyperlink_color = ink;
                v.override_text_color = Some(ink);
                // Pressed and open widgets turn gray. (egui also takes the color of strong text from the
                // pressed state, so it has to stay ink.)
                let pressed = if dark { Color32::from_gray(90) } else { Color32::from_gray(170) };
                for (w, filled) in [
                    (&mut v.widgets.noninteractive, false),
                    (&mut v.widgets.inactive, false),
                    (&mut v.widgets.hovered, false),
                    (&mut v.widgets.active, true),
                    (&mut v.widgets.open, true),
                ] {
                    w.bg_fill = if filled { pressed } else { paper };
                    w.weak_bg_fill = if filled { pressed } else { paper };
                    w.bg_stroke = Stroke::new(1.0, ink);
                    w.fg_stroke = Stroke::new(1.0, ink);
                    w.corner_radius = CornerRadius::same(4);
                    w.expansion = 0.0;
                }
                v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, ink);
                v.widgets.hovered.bg_stroke = Stroke::new(2.0, ink);
            } else {
                *v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
                v.panel_fill = if dark { Color32::from_rgb(22, 23, 26) } else { Color32::from_rgb(246, 247, 249) };
                v.extreme_bg_color = if dark { Color32::from_rgb(30, 31, 35) } else { v.extreme_bg_color };
                v.faint_bg_color = if dark { Color32::from_rgb(30, 32, 36) } else { Color32::from_rgb(240, 241, 244) };
                let accent = Color32::from_rgb(64, 156, 255);
                v.selection.bg_fill = accent.gamma_multiply(0.45);
                v.selection.stroke = Stroke::new(1.0, accent);
                for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
                    w.corner_radius = CornerRadius::same(6);
                }
                v.window_corner_radius = CornerRadius::same(12);
                // Body text at full label contrast (egui's default is a mid grey, too faint in the dark theme).
                v.widgets.noninteractive.fg_stroke.color = if dark { Color32::from_gray(225) } else { Color32::from_gray(30) };
            }
        });
    }
}

/// Keeps pages readable on wide windows: content is centered and at most this wide.
const PAGE_MAX: f32 = 1180.0;

pub fn page_frame(ui: &Ui) -> egui::Frame {
    egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(egui::Margin { left: 24, right: 24, top: 20, bottom: 16 })
}

/// Lay out a page in a centered column of at most [`PAGE_MAX`] width.
pub fn centered<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_rect_before_wrap();
    let w = avail.width().min(PAGE_MAX);
    let rect = Rect::from_min_size(Pos2::new(avail.center().x - w / 2.0, avail.top()), Vec2::new(w, avail.height()));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), add).inner
}

pub fn side_frame(ui: &Ui) -> egui::Frame {
    if classic() {
        return egui::Frame::new().fill(C::paper()).inner_margin(egui::Margin::same(16)).stroke(Stroke::new(1.0, C::fg()));
    }
    egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(egui::Margin::same(16))
}

/// Spacing scale: everything is a multiple of 4.
pub mod sp {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

/// The width a page has while its side panel is open. Pages whose panel is there only on some of
/// their sections lay the title bar out for this width on all of them, so the sections do not
/// move when one of them is chosen. `shown` is the panel's width when it is open (remembered for
/// the sections without it); `full` is the width of the page as it is now.
pub fn width_with_panel(ui: &Ui, panel: &'static str, shown: Option<f32>, default: f32, full: f32) -> f32 {
    let id = egui::Id::new(("panel_width", panel));
    match shown {
        Some(w) => {
            ui.data_mut(|d| d.insert_temp(id, w));
            full.min(PAGE_MAX)
        }
        None => (full - ui.data(|d| d.get_temp(id)).unwrap_or(default)).min(PAGE_MAX),
    }
}

/// The top of every page, always in the same place: the title, the page's sections (a segmented
/// control) next to it, and the page's own actions on the right. A subtitle goes underneath.
pub fn title_bar(ui: &mut Ui, title: &str, subtitle: &str, sections: impl FnOnce(&mut Ui), actions: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    title_bar_for(ui, width, title, subtitle, sections, actions);
}

/// [`title_bar`] laid out as if the page were `width` wide (see [`width_with_panel`]).
pub fn title_bar_for(ui: &mut Ui, width: f32, title: &str, subtitle: &str, sections: impl FnOnce(&mut Ui), actions: impl FnOnce(&mut Ui)) {
    // Classic has the title on a striped row of its own; the sections start the row under it.
    let classic = classic();
    if classic {
        title_text(ui, title);
    }
    // In a narrow window the three parts do not fit one row: the actions, and then the
    // sections too, move to rows of their own. Their widths are known from the last frame.
    let id = ui.id().with(("title_bar", title));
    let known: Option<(f32, f32, f32)> = ui.data(|d| d.get_temp(id));
    let (title_w, sections_w, actions_w) = known.unwrap_or_default();
    let avail = width.min(ui.available_width());
    let gap = sp::M * 2.0;
    let sections_below = title_w + sections_w + if classic { 0.0 } else { gap } > avail;
    // A page without actions has nothing to move.
    let actions_below = actions_w > 0.0 && (sections_below || title_w + sections_w + actions_w + gap * 2.0 > avail);
    let mut sections = Some(sections);
    let mut actions = Some(actions);
    let mut widths = (0.0, sections_w, actions_w);
    let measure = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| ui.scope(|ui| add(ui)).response.rect.width();
    let first_row = !classic || !sections_below || !actions_below;
    if first_row {
        ui.horizontal(|ui| {
            if !classic {
                widths.0 = ui.label(RichText::new(title).large_title()).rect.width();
                ui.add_space(sp::M);
            }
            if !sections_below {
                let f = sections.take().unwrap();
                let mut f = Some(f);
                widths.1 = measure(ui, &mut |ui| (f.take().unwrap())(ui));
            }
            if !actions_below {
                let f = actions.take().unwrap();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut f = Some(f);
                    widths.2 = measure(ui, &mut |ui| (f.take().unwrap())(ui));
                });
            }
        });
    }
    if let Some(f) = sections.take() {
        if first_row {
            ui.add_space(sp::S);
        }
        // Wider than the page itself (the smallest window): the row scrolls sideways, and a
        // scroll bar says so.
        let out = egui::ScrollArea::horizontal()
            .id_salt(id.with("sections"))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| ui.horizontal(|ui| f(ui)));
        widths.1 = out.content_size.x;
    }
    if let Some(f) = actions.take() {
        ui.add_space(sp::S);
        let mut f = Some(f);
        ui.horizontal(|ui| widths.2 = measure(ui, &mut |ui| (f.take().unwrap())(ui)));
    }
    if widths != (title_w, sections_w, actions_w) {
        ui.data_mut(|d| d.insert_temp(id, widths));
        if known.is_none() {
            // The first frame of a page is laid out blind: draw it again before it is seen.
            ui.ctx().request_discard("title bar measured");
        }
        ui.ctx().request_repaint();
    }
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(C::dim(ui)));
    }
    ui.add_space(sp::M);
}

/// Page title with an optional subtitle.
pub fn header(ui: &mut Ui, title: &str, subtitle: &str) {
    title_bar(ui, title, subtitle, |_| {}, |_| {});
}

fn title_text(ui: &mut Ui, title: &str) {
    if classic() {
        // The striped title bar of classic Mac windows, with the title in a white box.
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
        let p = ui.painter();
        for i in 0..6 {
            let y = rect.top() + 6.0 + i as f32 * 3.5;
            p.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0, C::fg()));
        }
        let galley = p.layout_no_wrap(title.to_string(), FontId::new(ty::TITLE, FontFamily::Name(DISPLAY_BOLD.into())), C::fg());
        let bx = Rect::from_center_size(rect.center(), galley.size() + Vec2::new(24.0, 2.0));
        p.rect_filled(bx, 0, C::paper());
        p.galley(bx.center() - galley.size() / 2.0, galley, C::fg());
    } else {
        ui.label(RichText::new(title).large_title());
    }
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame(ui).show(ui, |ui| ui.with_layout(egui::Layout::top_down(egui::Align::Min), add).inner).inner
}

/// A card: white (or dark) with a thin border, or a Classic box with a hard shadow.
pub fn card_frame(ui: &Ui) -> egui::Frame {
    let f = egui::Frame::new().fill(C::card(ui)).inner_margin(egui::Margin::same(16));
    if classic() {
        f.stroke(Stroke::new(1.0, C::fg())).corner_radius(CornerRadius::ZERO).shadow(egui::epaint::Shadow {
            offset: [2, 2],
            blur: 0,
            spread: 0,
            color: C::fg(),
        })
    } else {
        f.stroke(Stroke::new(1.0, C::track(ui))).corner_radius(CornerRadius::same(10))
    }
}

/// The sidebar.
pub fn pane_frame(ui: &Ui) -> egui::Frame {
    if classic() {
        return egui::Frame::new().fill(C::paper()).inner_margin(egui::Margin::symmetric(12, 16)).stroke(Stroke::new(1.0, C::fg()));
    }
    egui::Frame::new().fill(C::bg_bar(ui)).inner_margin(egui::Margin::symmetric(12, 16))
}

/// Colored hint box (warnings, explanations).
pub fn note(ui: &mut Ui, color: Color32, title: &str, text: &str) {
    let frame = if classic() {
        egui::Frame::new().fill(C::paper()).stroke(Stroke::new(1.0, C::fg())).inner_margin(egui::Margin::same(12))
    } else {
        egui::Frame::new().fill(color.gamma_multiply(0.12)).corner_radius(8).inner_margin(egui::Margin::same(12))
    };
    frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if !title.is_empty() {
            ui.label(RichText::new(title).semibold().color(color));
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
    Battery,
    Disk,
    Clean,
    Apps,
    Startup,
    Settings,
}

/// Sidebar icons, drawn on a 16-unit grid with one stroke weight (no icon font needed).
/// `filled` is the selected variant, like the `.fill` symbols of macOS.
pub fn paint_icon(p: &egui::Painter, r: Rect, icon: Icon, c: Color32, filled: bool) {
    use std::f32::consts::{PI, TAU};
    let u = r.width() / 16.0;
    let s = Stroke::new(1.5 * u, c);
    let m = r.center();
    let at = |x: f32, y: f32| r.left_top() + Vec2::new(x * u, y * u);
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(at(x0, y0), at(x1, y1));
    let boxed = |rr: Rect, radius: f32| {
        if filled {
            p.rect_filled(rr, radius * u, c);
        } else {
            p.rect_stroke(rr, radius * u, s, egui::StrokeKind::Middle);
        }
    };
    let arc = |center: Pos2, radius: f32, from: f32, to: f32| -> Vec<Pos2> {
        (0..=24).map(|i| center + Vec2::angled(from + (to - from) * i as f32 / 24.0) * radius).collect()
    };
    match icon {
        Icon::Overview => {
            // A gauge, like the app icon: an open arc with a needle.
            let c0 = at(8.0, 9.5);
            if filled {
                p.circle_filled(c0, 7.0 * u, c);
                p.add(egui::Shape::line(arc(c0, 4.6 * u, PI * 0.75, PI * 2.25), Stroke::new(1.5 * u, C::paper_on(c))));
                p.line_segment([c0, c0 + Vec2::angled(-PI * 0.25) * 4.0 * u], Stroke::new(1.6 * u, C::paper_on(c)));
            } else {
                p.add(egui::Shape::line(arc(c0, 6.5 * u, PI * 0.75, PI * 2.25), s));
                p.line_segment([c0, c0 + Vec2::angled(-PI * 0.25) * 4.2 * u], s);
                p.circle_filled(c0, 1.4 * u, c);
            }
        }
        Icon::Procs => {
            // Activity bars.
            for (i, h) in [6.0, 10.0, 13.0, 8.0].iter().enumerate() {
                let x = 2.5 + i as f32 * 3.4;
                let bar = rect(x, 14.5 - h, x + 2.2, 14.5);
                if filled || i == 2 {
                    p.rect_filled(bar, 0.8 * u, c);
                } else {
                    p.rect_stroke(bar, 0.8 * u, Stroke::new(1.2 * u, c), egui::StrokeKind::Inside);
                }
            }
        }
        Icon::Battery => {
            let body = rect(1.0, 4.5, 13.5, 11.5);
            boxed(body, 2.2);
            p.rect_filled(rect(14.3, 6.6, 15.4, 9.4), 0.5 * u, c);
            let level = rect(2.8, 6.3, 8.6, 9.7);
            p.rect_filled(level, 0.8 * u, if filled { C::paper_on(c) } else { c });
        }
        Icon::Disk => {
            // A drive, as in Finder's sidebar.
            let body = rect(1.0, 4.0, 15.0, 12.0);
            boxed(body, 2.0);
            let ink = if filled { C::paper_on(c) } else { c };
            p.line_segment([at(1.8, 9.0), at(14.2, 9.0)], Stroke::new(1.2 * u, ink));
            p.circle_filled(at(12.3, 10.6), 0.9 * u, ink);
        }
        Icon::Clean => {
            // A sparkle, and a small one.
            let star = |c0: Pos2, a: f32, b: f32| {
                vec![
                    c0 + Vec2::new(0.0, -a),
                    c0 + Vec2::new(b, -b),
                    c0 + Vec2::new(a, 0.0),
                    c0 + Vec2::new(b, b),
                    c0 + Vec2::new(0.0, a),
                    c0 + Vec2::new(-b, b),
                    c0 + Vec2::new(-a, 0.0),
                    c0 + Vec2::new(-b, -b),
                ]
            };
            let big = star(at(7.0, 9.0), 6.2 * u, 1.6 * u);
            if filled {
                // Not convex: fill it as four triangles around the middle.
                let c0 = at(7.0, 9.0);
                for i in 0..8 {
                    p.add(egui::Shape::convex_polygon(vec![c0, big[i], big[(i + 1) % 8]], c, Stroke::NONE));
                }
            } else {
                p.add(egui::Shape::closed_line(big, s));
            }
            let small = star(at(13.0, 3.5), 2.6 * u, 0.8 * u);
            let c1 = at(13.0, 3.5);
            for i in 0..8 {
                p.add(egui::Shape::convex_polygon(vec![c1, small[i], small[(i + 1) % 8]], c, Stroke::NONE));
            }
        }
        Icon::Apps => {
            // Four app tiles.
            for (x, y) in [(1.5, 1.5), (9.0, 1.5), (1.5, 9.0), (9.0, 9.0)] {
                boxed(rect(x, y, x + 5.5, y + 5.5), 1.6);
            }
        }
        Icon::Startup => {
            let c0 = at(8.0, 8.8);
            if filled {
                p.circle_filled(c0, 7.2 * u, c);
                let ink = Stroke::new(1.6 * u, C::paper_on(c));
                p.add(egui::Shape::line(arc(c0, 4.0 * u, -PI / 2.0 + 0.7, -PI / 2.0 + TAU - 0.7), ink));
                p.line_segment([c0 - Vec2::new(0.0, 5.0 * u), c0 - Vec2::new(0.0, 1.0 * u)], ink);
            } else {
                p.add(egui::Shape::line(arc(c0, 6.0 * u, -PI / 2.0 + 0.6, -PI / 2.0 + TAU - 0.6), s));
                p.line_segment([at(8.0, 1.0), at(8.0, 8.0)], s);
            }
        }
        Icon::Settings => {
            // A gear: eight teeth around a ring.
            for i in 0..8 {
                let d = Vec2::angled(i as f32 / 8.0 * TAU);
                p.line_segment([m + d * 4.6 * u, m + d * 7.0 * u], Stroke::new(2.4 * u, c));
            }
            if filled {
                p.circle_filled(m, 5.4 * u, c);
                p.circle_filled(m, 2.0 * u, C::paper_on(c));
            } else {
                p.circle_stroke(m, 4.6 * u, s);
                p.circle_stroke(m, 1.8 * u, Stroke::new(1.2 * u, c));
            }
        }
    }
}

pub fn nav_item(ui: &mut Ui, selected: bool, icon: Icon, label: &str, badge: Option<usize>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    let p = ui.painter();
    // Selected: filled with the accent, or inverted (white on black) in Classic.
    let on_sel = if classic() { C::paper() } else { Color32::WHITE };
    if selected {
        p.rect_filled(rect, if classic() { 0 } else { 8 }, if classic() { C::fg() } else { C::accent() });
    } else if resp.hovered() {
        p.rect_filled(rect, if classic() { 0 } else { 8 }, C::track(ui).gamma_multiply(0.6));
    }
    let fg = if selected { on_sel } else { C::text(ui) };
    let ir = Rect::from_center_size(Pos2::new(rect.left() + 20.0, rect.center().y), Vec2::splat(18.0));
    paint_icon(p, ir, icon, if selected { on_sel } else { C::accent() }, selected);
    p.text(
        Pos2::new(rect.left() + 38.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        if selected { semibold_font(ty::BODY) } else { FontId::proportional(ty::BODY) },
        fg,
    );
    if let Some(n) = badge {
        let br = Rect::from_center_size(Pos2::new(rect.right() - 16.0, rect.center().y), Vec2::new(22.0, 18.0));
        if classic() {
            p.rect_filled(br, 0, if selected { C::paper() } else { C::fg() });
            p.text(br.center(), egui::Align2::CENTER_CENTER, n.to_string(), semibold_font(ty::CAPTION), if selected { C::fg() } else { C::paper() });
        } else {
            p.rect_filled(br, 9, C::red());
            p.text(br.center(), egui::Align2::CENTER_CENTER, n.to_string(), semibold_font(ty::CAPTION), Color32::WHITE);
        }
    }
    // Painted by hand, so tell VoiceOver what it is.
    let spoken = match badge {
        Some(n) => format!("{label}, {n}"),
        None => label.to_string(),
    };
    resp.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, &spoken));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The app logo — a small version of the app icon (gauge with a colored arc and a sparkle).
/// In the Classic style it is 1-bit: an ink square with the gauge cut out in paper.
pub fn app_logo(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    let classic = classic();
    let (body, done, todo, mid, mark) = if classic {
        let (ink, paper) = (C::fg(), C::paper());
        (ink, paper, paper, paper, paper)
    } else {
        (
            Color32::from_rgb(80, 92, 245),
            Color32::from_rgb(70, 225, 140),
            Color32::from_white_alpha(70),
            Color32::from_rgb(255, 205, 70),
            Color32::WHITE,
        )
    };
    p.rect_filled(rect.shrink(size * 0.02), if classic { size * 0.08 } else { size * 0.24 }, body);
    let m = rect.center() + Vec2::new(0.0, size * 0.06);
    let rr = size * 0.27;
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        (0..=16)
            .map(|i| {
                let t = std::f32::consts::PI * (0.75 + 1.5 * (from + (to - from) * i as f32 / 16.0));
                m + Vec2::new(t.cos() * rr, t.sin() * rr)
            })
            .collect()
    };
    let w = size * 0.1;
    // Classic has no gray: the rest of the arc is a thin line instead of a faint one.
    let rest = if classic { Stroke::new(w * 0.35, todo) } else { Stroke::new(w, todo) };
    p.add(egui::Shape::line(arc(0.64, 1.0), rest));
    p.add(egui::Shape::line(arc(0.0, 0.32), Stroke::new(w, done)));
    p.add(egui::Shape::line(arc(0.32, 0.64), Stroke::new(w, mid)));
    let na = std::f32::consts::PI * (0.75 + 1.5 * 0.64);
    p.line_segment([m, m + Vec2::new(na.cos(), na.sin()) * rr * 0.8], Stroke::new(size * 0.06, mark));
    p.circle_filled(m, size * 0.07, mark);
    // Sparkle.
    let c = rect.left_top() + Vec2::new(size * 0.78, size * 0.22);
    let (a, b) = (size * 0.15, size * 0.035);
    // Two slim diamonds make a four-point star (each one is convex).
    for (dx, dy) in [(b, a), (a, b)] {
        let d = vec![c + Vec2::new(0.0, -dy), c + Vec2::new(dx, 0.0), c + Vec2::new(0.0, dy), c + Vec2::new(-dx, 0.0)];
        p.add(egui::Shape::convex_polygon(d, mark, Stroke::NONE));
    }
}

/// Segmented control.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    let classic = classic();
    let track = if classic { C::paper() } else { C::track(ui) };
    let frame = egui::Frame::new().fill(track).corner_radius(if classic { 0 } else { 8 }).inner_margin(egui::Margin::same(2));
    let frame = if classic { frame.stroke(Stroke::new(1.0, C::fg())) } else { frame };
    frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        // Inside a right-aligned row egui lays items out from the right: add them reversed,
        // so the options always read in the given order.
        let rtl = ui.layout().prefer_right_to_left();
        ui.horizontal(|ui| {
            let ordered: Vec<&(T, &str)> = if rtl { options.iter().rev().collect() } else { options.iter().collect() };
            for (v, label) in ordered {
                let sel = *value == *v;
                let (fill, text) = match (classic, sel) {
                    (true, true) => (C::fg(), C::paper()),
                    (true, false) => (C::paper(), C::fg()),
                    (false, true) => (C::card(ui), C::text(ui)),
                    (false, false) => (Color32::TRANSPARENT, C::dim(ui)),
                };
                let b =
                    egui::Button::new(RichText::new(*label).color(text)).fill(fill).corner_radius(if classic { 0 } else { 6 }).stroke(Stroke::NONE);
                let r = ui.add(b);
                r.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, sel, *label));
                if r.clicked() && !sel {
                    *value = *v;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// On/off switch (a classic checkbox in the Classic style). `label` is what VoiceOver reads.
pub fn switch(ui: &mut Ui, on: &mut bool, label: &str) -> egui::Response {
    let size = if classic() { Vec2::new(18.0, 18.0) } else { Vec2::new(38.0, 22.0) };
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let state = *on;
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, state, label));
    let p = ui.painter();
    if classic() {
        p.rect_filled(rect, 0, C::paper());
        p.rect_stroke(rect, 0, Stroke::new(1.0, C::fg()), egui::StrokeKind::Inside);
        if *on {
            let r = rect.shrink(3.0);
            p.line_segment([r.left_top(), r.right_bottom()], Stroke::new(1.5, C::fg()));
            p.line_segment([r.right_top(), r.left_bottom()], Stroke::new(1.5, C::fg()));
        }
        return resp;
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let bg = if *on { C::green() } else { C::track(ui) };
    p.rect_filled(rect, 11, bg);
    let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    p.circle_filled(Pos2::new(x, rect.center().y), 9.0, Color32::WHITE);
    resp
}

/// Fill of a meter: the accent while all is well, yellow and red only when something needs attention.
pub fn ratio_color(r: f32) -> Color32 {
    if r >= 0.9 {
        C::red()
    } else if r >= 0.75 {
        C::yellow()
    } else {
        C::accent()
    }
}

/// Color of a measured value: plain text while all is well (color is kept for what needs attention).
pub fn value_color(ui: &Ui, r: f32) -> Color32 {
    if r >= 0.75 { ratio_color(r) } else { C::text(ui) }
}

/// Meter in the sidebar: title, value, and a sparkline or a bar.
pub fn side_meter(ui: &mut Ui, title: &str, value: &str, ratio: f32, hist: Option<&VecDeque<f32>>) {
    let size = Vec2::new(ui.available_width(), 44.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, format!("{title}: {value}")));
    let color = ratio_color(ratio);
    let value_color = value_color(ui, ratio);
    let p = ui.painter();
    if classic() {
        p.rect_filled(rect, 0, C::paper());
        p.rect_stroke(rect, 0, Stroke::new(1.0, C::fg()), egui::StrokeKind::Inside);
    } else {
        p.rect_filled(rect, 8, C::card(ui));
    }
    p.text(rect.left_top() + Vec2::new(10.0, 6.0), egui::Align2::LEFT_TOP, title, FontId::proportional(ty::CAPTION), C::dim(ui));
    p.text(rect.left_top() + Vec2::new(10.0, 20.0), egui::Align2::LEFT_TOP, value, semibold_font(ty::BODY), value_color);
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
    ui.add_space(sp::XS);
}

pub fn paint_sparkline(ui: &Ui, h: &VecDeque<f32>, rect: Rect, color: Color32) {
    let p = ui.painter();
    // The baseline spans the whole width from the start, so a short history does not look cut off.
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, color.gamma_multiply(0.35)));
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
        p.rect_filled(r, 4, C::purple());
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
    let mut f = rect;
    f.set_width((rect.width() * ratio.clamp(0.0, 1.0)).max(if ratio > 0.0 { 2.0 } else { 0.0 }));
    if classic() {
        p.rect_filled(rect, 0, C::paper());
        p.rect_filled(f, 0, C::fg());
        p.rect_stroke(rect, 0, Stroke::new(1.0, C::fg()), egui::StrokeKind::Inside);
        return resp;
    }
    let r = (size.y / 2.0) as u8;
    p.rect_filled(rect, r, C::track(ui));
    p.rect_filled(f, r, color);
    resp
}

pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), semibold_font(ty::CAPTION), color);
    let size = galley.size() + Vec2::new(12.0, 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, text));
    if classic() {
        ui.painter().rect_stroke(rect, 0, Stroke::new(1.0, C::fg()), egui::StrokeKind::Inside);
    } else {
        ui.painter().rect_filled(rect, 5, color.gamma_multiply(0.16));
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

pub fn safety_color(s: Safety) -> Color32 {
    match s {
        Safety::User => C::green(),
        Safety::System => C::yellow(),
        Safety::Critical => C::red(),
        Safety::Own => C::accent(),
    }
}

pub fn safety_badge(ui: &mut Ui, s: Safety) -> egui::Response {
    badge(ui, s.label(), safety_color(s))
}

pub fn del_color(s: DelSafety) -> Color32 {
    match s {
        DelSafety::Safe => C::green(),
        DelSafety::Careful => C::yellow(),
        DelSafety::Blocked => C::red(),
    }
}

pub fn del_badge(ui: &mut Ui, s: DelSafety) -> egui::Response {
    badge(ui, s.label(), del_color(s))
}

pub fn cpu_color(ui: &Ui, c: f32) -> Color32 {
    if c >= 80.0 {
        C::red()
    } else if c >= 25.0 {
        C::yellow()
    } else if c >= 1.0 {
        C::text(ui)
    } else {
        C::dim(ui)
    }
}

pub fn mem_color(ui: &Ui, m: u64) -> Color32 {
    if m >= 4_000_000_000 {
        C::red()
    } else if m >= 1_500_000_000 {
        C::yellow()
    } else if m >= 30_000_000 {
        C::text(ui)
    } else {
        C::dim(ui)
    }
}

/// Sizes are information, not alarms: only 10 GB and more stand out.
pub fn size_color(ui: &Ui, s: u64) -> Color32 {
    if s >= 10_000_000_000 {
        C::yellow()
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
        C::yellow()
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

/// Wrapped text where `commands` in backticks are set in SF Mono on a faint background, as in docs.
pub fn text_with_code(ui: &mut Ui, text: &str, color: Color32) {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    let body = ui.style().text_styles[&TextStyle::Body].clone();
    for (i, part) in text.split('`').enumerate() {
        let format = if i % 2 == 1 {
            TextFormat { font_id: FontId::monospace(body.size - 1.0), color: C::text(ui), background: C::track(ui), ..Default::default() }
        } else {
            TextFormat { font_id: body.clone(), color, ..Default::default() }
        };
        job.append(part, 0.0, format);
    }
    job.wrap.max_width = ui.available_width();
    ui.label(job);
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

/// The main action of a page: a filled pill, or a Classic default button (inverted, thick border).
/// Corner radius of buttons in the current style.
pub fn button_radius() -> u8 {
    if classic() { 6 } else { 8 }
}

pub fn big_button(ui: &mut Ui, text: &str, color: Color32, enabled: bool) -> egui::Response {
    let b = if classic() {
        egui::Button::new(RichText::new(text).semibold().color(C::paper())).fill(C::fg()).corner_radius(6).stroke(Stroke::new(2.0, C::fg()))
    } else {
        egui::Button::new(RichText::new(text).semibold().color(Color32::WHITE)).fill(color).corner_radius(8)
    };
    ui.add_enabled(enabled, b.min_size(Vec2::new(0.0, 32.0)))
}

/// An action repeated on many cards: tinted with `color` instead of filled, so one screen
/// does not shout with a dozen bright buttons. A plain bordered button in Classic.
pub fn tinted_button(ui: &mut Ui, text: &str, color: Color32, enabled: bool) -> egui::Response {
    let b = if classic() {
        egui::Button::new(RichText::new(text).semibold()).corner_radius(button_radius())
    } else {
        egui::Button::new(RichText::new(text).semibold().color(color))
            .fill(color.gamma_multiply(0.14))
            .stroke(Stroke::NONE)
            .corner_radius(button_radius())
    };
    ui.add_enabled(enabled, b.min_size(Vec2::new(0.0, 32.0)))
}

pub fn plain_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(text).corner_radius(button_radius()).min_size(Vec2::new(0.0, 32.0)))
}

/// Checkbox header row helper: "select all / none".
pub fn waiting(ui: &mut Ui, text: &str) {
    ui.add_space(sp::XXL);
    ui.vertical_centered(|ui| {
        ui.spinner();
        ui.label(RichText::new(text).color(C::dim(ui)));
    });
}

/// A small pixel-art computer with a smile, for empty and "all clean" states — a nod to the
/// first Macintosh, drawn on a 16×16 grid. `#` is ink, `+` a lighter tint, `.` the screen.
const PIXEL_FRIEND: [&str; 16] = [
    "  ############  ",
    " #++++++++++++# ",
    " #+##########+# ",
    " #+#........#+# ",
    " #+#..#..#..#+# ",
    " #+#..#..#..#+# ",
    " #+#........#+# ",
    " #+#.#....#.#+# ",
    " #+#..####..#+# ",
    " #+#........#+# ",
    " #+##########+# ",
    " #++++++++++++# ",
    " #+++++++###++# ",
    " #++++++++++++# ",
    "  ############  ",
    "   ##########   ",
];

pub fn pixel_friend(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let px = size / 16.0;
    let (ink, tint, screen) =
        if classic() { (C::fg(), C::paper(), C::paper()) } else { (C::accent(), C::accent().gamma_multiply(0.18), C::card(ui)) };
    let p = ui.painter();
    for (y, row) in PIXEL_FRIEND.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let color = match ch {
                '#' => ink,
                '+' => tint,
                '.' => screen,
                _ => continue,
            };
            let r = Rect::from_min_size(rect.min + Vec2::new(x as f32 * px, y as f32 * px), Vec2::splat(px));
            // Whole pixels, no anti-aliased seams between them.
            p.rect_filled(r.expand(0.25), 0, color);
        }
    }
}

pub fn empty(ui: &mut Ui, text: &str) {
    ui.add_space(sp::XXL);
    ui.vertical_centered(|ui| {
        pixel_friend(ui, 64.0);
        ui.add_space(sp::M);
        ui.label(RichText::new(text).size(ty::HEADLINE).color(C::dim(ui)));
    });
}

pub fn yes_word() -> &'static str {
    tr("yes")
}
