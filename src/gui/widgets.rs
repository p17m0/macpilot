//! Shared widgets and styling.

use std::collections::VecDeque;

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, TextStyle, Ui, Vec2, WidgetInfo, WidgetType,
};
use macpilot::disk::DelSafety;
use macpilot::procs::Safety;
use macpilot::settings::{Palette, UiStyle};
use macpilot::tr;

static STYLE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static DARK: AtomicBool = AtomicBool::new(false);
static PALETTE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The colors of a palette in one mode (light or dark).
#[derive(Clone, Copy)]
pub struct Pal {
    pub accent: Color32,
    /// Page background.
    pub panel: Color32,
    /// Sidebar and status bar.
    pub bar: Color32,
    pub card: Color32,
    /// Borders, dividers, the track of meters and switches, the fill of plain buttons.
    pub track: Color32,
}

/// The colors of `p`. Tinted palettes carry a little of their hue into the backgrounds.
pub fn pal(p: Palette, dark: bool) -> Pal {
    let c = |v: [u8; 3]| Color32::from_rgb(v[0], v[1], v[2]);
    let make = |accent: [u8; 3], panel: [u8; 3], bar: [u8; 3], card: [u8; 3], track: [u8; 3]| Pal {
        accent: c(accent),
        panel: c(panel),
        bar: c(bar),
        card: c(card),
        track: c(track),
    };
    match (p, dark) {
        (Palette::Default, true) => make([64, 156, 255], [22, 23, 26], [28, 29, 33], [36, 38, 43], [55, 58, 64]),
        (Palette::Default, false) => make([64, 156, 255], [246, 247, 249], [236, 237, 240], [255, 255, 255], [222, 224, 228]),
        (Palette::Graphite, true) => make([152, 156, 168], [24, 24, 25], [30, 30, 31], [39, 39, 41], [58, 58, 61]),
        (Palette::Graphite, false) => make([104, 108, 120], [246, 246, 246], [236, 236, 237], [255, 255, 255], [222, 222, 224]),
        (Palette::Ocean, true) => make([45, 196, 208], [14, 24, 32], [18, 31, 41], [24, 40, 52], [40, 62, 78]),
        (Palette::Ocean, false) => make([0, 150, 170], [240, 247, 250], [226, 238, 243], [255, 255, 255], [206, 224, 231]),
        (Palette::Forest, true) => make([92, 200, 122], [17, 24, 19], [22, 31, 25], [29, 40, 32], [46, 62, 50]),
        (Palette::Forest, false) => make([46, 150, 82], [243, 248, 243], [230, 239, 231], [255, 255, 255], [210, 225, 212]),
        (Palette::Sunset, true) => make([255, 138, 76], [28, 21, 19], [36, 27, 24], [46, 35, 31], [72, 54, 47]),
        (Palette::Sunset, false) => make([232, 106, 44], [253, 247, 242], [246, 235, 226], [255, 255, 255], [236, 220, 207]),
        (Palette::Rose, true) => make([255, 110, 160], [28, 20, 25], [36, 26, 32], [46, 33, 41], [72, 51, 63]),
        (Palette::Rose, false) => make([226, 68, 124], [253, 245, 248], [247, 232, 238], [255, 255, 255], [238, 214, 224]),
        (Palette::Nord, true) => make([136, 192, 208], [46, 52, 64], [40, 45, 56], [59, 66, 82], [76, 86, 106]),
        (Palette::Nord, false) => make([94, 129, 172], [236, 239, 244], [229, 233, 240], [250, 251, 253], [216, 222, 233]),
        (Palette::Dracula, true) => make([189, 147, 249], [33, 34, 44], [40, 42, 54], [52, 55, 70], [68, 71, 90]),
        (Palette::Dracula, false) => make([124, 77, 224], [248, 247, 252], [237, 235, 246], [255, 255, 255], [222, 218, 238]),
        (Palette::Solarized, true) => make([42, 161, 152], [0, 43, 54], [0, 36, 46], [7, 54, 66], [38, 82, 94]),
        (Palette::Solarized, false) => make([38, 139, 210], [253, 246, 227], [238, 232, 213], [255, 251, 240], [221, 213, 190]),
        (Palette::Midnight, true) => make([10, 132, 255], [0, 0, 0], [9, 9, 11], [20, 20, 23], [44, 44, 49]),
        (Palette::Midnight, false) => make([0, 112, 240], [250, 250, 252], [240, 240, 244], [255, 255, 255], [224, 224, 230]),
    }
}

/// The colors of a style: the retro ones bring their own, the standard one has the palettes.
fn style_pal(style: UiStyle, palette: Palette, dark: bool) -> Pal {
    let c = Color32::from_rgb;
    let g = Color32::from_gray;
    match style {
        UiStyle::Win98 => Pal { accent: c(0, 0, 128), panel: g(192), bar: g(192), card: g(192), track: g(128) },
        UiStyle::WinXp => Pal { accent: c(49, 106, 197), panel: c(236, 233, 216), bar: c(214, 223, 247), card: g(255), track: c(172, 168, 153) },
        UiStyle::Nes => Pal { accent: c(216, 40, 0), panel: c(222, 222, 218), bar: c(190, 190, 186), card: c(240, 240, 236), track: g(120) },
        UiStyle::Ps1 => {
            Pal { accent: c(48, 105, 200), panel: c(206, 206, 210), bar: c(186, 186, 192), card: c(226, 226, 230), track: c(160, 160, 168) }
        }
        UiStyle::Ps2 => Pal { accent: c(80, 140, 255), panel: c(3, 5, 18), bar: c(7, 10, 30), card: c(11, 17, 46), track: c(34, 50, 108) },
        // The blue-gray of grouped tables, white groups.
        UiStyle::Ios6 => Pal { accent: c(36, 104, 224), panel: c(206, 211, 219), bar: c(178, 187, 201), card: g(255), track: c(160, 166, 176) },
        UiStyle::Ios7 => Pal { accent: c(0, 122, 255), panel: c(239, 239, 244), bar: c(247, 247, 247), card: g(255), track: c(206, 206, 211) },
        _ => pal(palette, dark),
    }
}

fn cur_pal(dark: bool) -> Pal {
    style_pal(style(), Palette::ALL[PALETTE.load(Ordering::Relaxed) as usize % Palette::ALL.len()], dark)
}

/// `a` moved towards `b` by `t` (0…1).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// The style in use.
pub fn style() -> UiStyle {
    UiStyle::ALL[STYLE.load(Ordering::Relaxed) as usize % UiStyle::ALL.len()]
}

/// The two-color styles — the Macintosh (black and white) and the Game Boy (dark green on light
/// green): ink on paper, 1-pixel lines, hard shadows, no other colors.
pub fn classic() -> bool {
    matches!(style(), UiStyle::Classic | UiStyle::GameBoy)
}

/// Corner radius in the current style: square in the boxy ones, barely rounded in Windows XP.
pub fn rad(r: u8) -> u8 {
    match style() {
        UiStyle::Win98 | UiStyle::Nes | UiStyle::Ps2 => 0,
        UiStyle::WinXp => r.min(3),
        UiStyle::Ios7 => r.min(5),
        _ => r,
    }
}

/// The two-tone edge of Windows 98: light from the top left. `raised` for buttons and panels,
/// sunken for fields, wells and pressed buttons.
pub fn bevel(p: &egui::Painter, r: Rect, raised: bool) {
    let (white, light, shade, black) = (Color32::WHITE, Color32::from_gray(223), Color32::from_gray(128), Color32::from_gray(10));
    let (tl_out, tl_in, br_out, br_in) = if raised { (white, light, black, shade) } else { (shade, black, white, light) };
    let edge = |r: Rect, tl: Color32, br: Color32| {
        let (l, t, rt, b) = (r.left() + 0.5, r.top() + 0.5, r.right() - 0.5, r.bottom() - 0.5);
        p.line_segment([Pos2::new(l, t), Pos2::new(rt, t)], Stroke::new(1.0, tl));
        p.line_segment([Pos2::new(l, t), Pos2::new(l, b)], Stroke::new(1.0, tl));
        p.line_segment([Pos2::new(l, b), Pos2::new(rt + 0.5, b)], Stroke::new(1.0, br));
        p.line_segment([Pos2::new(rt, t), Pos2::new(rt, b)], Stroke::new(1.0, br));
    };
    edge(r, tl_out, br_out);
    edge(r.shrink(1.0), tl_in, br_in);
}

/// What a style adds to a button after egui drew it: the bevel of Windows 98 (pressed in while
/// the mouse is down on it).
fn finish_button(ui: &Ui, r: &egui::Response) {
    // Menu items are frameless buttons: they stay flat.
    if style() == UiStyle::Win98 && ui.visuals().button_frame && ui.is_rect_visible(r.rect) {
        bevel(ui.painter(), r.rect, !r.is_pointer_button_down_on());
    }
}

/// A plain button in the current style. Use it instead of `ui.button`, which cannot draw bevels.
pub fn button(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    let r = ui.button(text);
    finish_button(ui, &r);
    r
}

/// A gray 50% dither, the only "gray" a 1-bit screen has: ink and paper pixels in a checkerboard.
pub fn dither(ui: &Ui, rect: Rect) {
    let (ink, paper) = (C::fg(), C::paper());
    let id = egui::Id::new(("dither", ink, paper));
    let tex: egui::TextureHandle = ui.ctx().data_mut(|d| d.get_temp(id)).unwrap_or_else(|| {
        let img = egui::ColorImage::new([2, 2], vec![ink, paper, paper, ink]);
        let opts = egui::TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..egui::TextureOptions::NEAREST };
        let t = ui.ctx().load_texture("dither", img, opts);
        ui.ctx().data_mut(|d| d.insert_temp(id, t.clone()));
        t
    });
    // One texture pixel per screen pixel.
    let k = ui.ctx().pixels_per_point() / 2.0;
    let uv = Rect::from_min_max(Pos2::new(rect.left() * k, rect.top() * k), Pos2::new(rect.right() * k, rect.bottom() * k));
    ui.painter().image(tex.id(), rect, uv, Color32::WHITE);
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
    /// Green, yellow, red, purple — as each style paints them.
    fn signal(i: usize, normal: [u8; 3]) -> Color32 {
        let own: Option<[[u8; 3]; 4]> = match style() {
            // The 16 colors of Windows.
            UiStyle::Win98 => Some([[0, 128, 0], [150, 100, 0], [200, 0, 0], [128, 0, 128]]),
            UiStyle::WinXp => Some([[61, 149, 61], [214, 138, 0], [211, 53, 29], [120, 80, 180]]),
            UiStyle::Nes => Some([[0, 136, 0], [188, 124, 0], [216, 40, 0], [104, 68, 252]]),
            // Triangle, the logo's yellow, circle, square.
            UiStyle::Ps1 => Some([[0, 160, 132], [214, 150, 0], [226, 60, 76], [206, 104, 170]]),
            UiStyle::Ios6 => Some([[58, 160, 40], [214, 138, 0], [200, 30, 30], [120, 80, 180]]),
            // The system colors of iOS 7.
            UiStyle::Ios7 => Some([[76, 217, 100], [255, 149, 0], [255, 59, 48], [88, 86, 214]]),
            _ => None,
        };
        let [r, g, b] = own.map_or(normal, |o| o[i]);
        Self::pick(r, g, b)
    }
    pub fn accent() -> Color32 {
        if classic() { Self::fg() } else { cur_pal(dark_now()).accent }
    }
    pub fn green() -> Color32 {
        Self::signal(0, [52, 199, 89])
    }
    pub fn yellow() -> Color32 {
        Self::signal(1, [255, 176, 32])
    }
    pub fn red() -> Color32 {
        Self::signal(2, [255, 69, 58])
    }
    pub fn purple() -> Color32 {
        Self::signal(3, [175, 82, 222])
    }
    /// Ink of the two-color styles: black on white (or white on black), the darkest green of the Game Boy.
    pub fn fg() -> Color32 {
        if style() == UiStyle::GameBoy {
            return Color32::from_rgb(15, 56, 15);
        }
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
    /// Paper of the two-color styles.
    pub fn paper() -> Color32 {
        if style() == UiStyle::GameBoy {
            return Color32::from_rgb(155, 188, 15);
        }
        if dark_now() { Color32::BLACK } else { Color32::WHITE }
    }
    /// The in-between shade of the two-color styles, for secondary text and pressed things.
    fn mid(dark: bool) -> Color32 {
        match (style(), dark) {
            (UiStyle::GameBoy, _) => Color32::from_rgb(48, 98, 48),
            (_, false) => Color32::from_gray(85),
            (_, true) => Color32::from_gray(175),
        }
    }

    pub fn dark(ui: &Ui) -> bool {
        ui.visuals().dark_mode
    }
    pub fn dim(ui: &Ui) -> Color32 {
        match (classic(), Self::dark(ui)) {
            (true, dark) => Self::mid(dark),
            (false, true) if style() == UiStyle::Ps2 => Color32::from_rgb(132, 150, 200),
            (false, false) if matches!(style(), UiStyle::Win98 | UiStyle::Nes) => Color32::from_gray(70),
            (false, true) => Color32::from_gray(150),
            (false, false) => Color32::from_gray(105),
        }
    }
    pub fn text(ui: &Ui) -> Color32 {
        if classic() {
            return Self::fg();
        }
        match (style(), Self::dark(ui)) {
            (UiStyle::Win98 | UiStyle::WinXp, _) => Color32::BLACK,
            (UiStyle::Ps2, _) => Color32::from_rgb(214, 226, 255),
            (_, true) => Color32::from_gray(235),
            (_, false) => Color32::from_gray(25),
        }
    }
    /// Sidebar and status bar.
    pub fn bg_bar(ui: &Ui) -> Color32 {
        match (classic(), Self::dark(ui)) {
            (true, _) => Self::paper(),
            (false, dark) => cur_pal(dark).bar,
        }
    }
    pub fn card(ui: &Ui) -> Color32 {
        match (classic(), Self::dark(ui)) {
            (true, _) => Self::paper(),
            (false, dark) => cur_pal(dark).card,
        }
    }
    pub fn track(ui: &Ui) -> Color32 {
        match (classic(), Self::dark(ui)) {
            (true, _) if style() == UiStyle::GameBoy => Color32::from_rgb(139, 172, 15),
            (true, true) => Color32::from_gray(70),
            (true, false) => Color32::from_gray(200),
            (false, dark) => cur_pal(dark).track,
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

fn fonts(style: UiStyle) -> egui::FontDefinitions {
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
    // The retro styles put their own typeface in front: what the machine itself used, or the
    // nearest thing macOS ships (all of them have Cyrillic).
    let file = |name: &str, path: &str| system_font(path).map(|b| (name.to_string(), egui::FontData::from_static(b)));
    // One face of Helvetica Neue (a collection: 0 regular, 1 bold, 7 light, 10 medium).
    let neue = |index: u32| {
        system_font("/System/Library/Fonts/HelveticaNeue.ttc")
            .map(|b| (format!("helvetica-neue-{index}"), egui::FontData { index, ..egui::FontData::from_static(b) }))
    };
    let pixel = |w: f32| Some((format!("pixel-{w}"), variant(PIXEL_FONT, &[(b"wght", w)])));
    let supplemental = "/System/Library/Fonts/Supplemental";
    for (family, list) in &mut front {
        if *family == FontFamily::Monospace {
            continue;
        }
        let body = *family == FontFamily::Proportional;
        let display = matches!(family, FontFamily::Name(n) if n.starts_with("display"));
        let own = match style {
            // Geneva for reading, as on the Macintosh; pixels for titles and names.
            UiStyle::Classic if body => file("geneva", "/System/Library/Fonts/Geneva.ttf").or_else(|| pixel(400.0)),
            UiStyle::Classic => pixel(700.0),
            UiStyle::GameBoy | UiStyle::Nes => pixel(if body { 400.0 } else { 700.0 }),
            UiStyle::Win98 if body => file("ms-sans", &format!("{supplemental}/Microsoft Sans Serif.ttf")),
            UiStyle::WinXp if body => file("tahoma", &format!("{supplemental}/Tahoma.ttf")),
            // Windows XP set its title bars in Trebuchet.
            UiStyle::WinXp if display => file("trebuchet-bold", &format!("{supplemental}/Trebuchet MS Bold.ttf")),
            UiStyle::Win98 | UiStyle::WinXp => file("tahoma-bold", &format!("{supplemental}/Tahoma Bold.ttf")),
            // iOS spoke Helvetica: bold in the glossy years, light from iOS 7 on.
            UiStyle::Ios6 => neue(if body { 0 } else { 1 }),
            UiStyle::Ios7 if body => neue(0),
            UiStyle::Ios7 if display => neue(7),
            UiStyle::Ios7 => neue(10),
            _ => None,
        };
        if let Some(f) = own {
            list.insert(0, f);
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

/// Apply a style (standard with its palette, or Classic) to both the light and the dark theme.
pub fn setup_style(ctx: &egui::Context, style: UiStyle, palette: Palette) {
    STYLE.store(UiStyle::ALL.iter().position(|s| *s == style).unwrap_or(0) as u8, Ordering::Relaxed);
    let classic = classic();
    PALETTE.store(Palette::ALL.iter().position(|p| *p == palette).unwrap_or(0) as u8, Ordering::Relaxed);
    ctx.set_fonts(fonts(style));
    ctx.all_styles_mut(|s| {
        // The pixel font looks right a little larger.
        let k = if matches!(style, UiStyle::GameBoy | UiStyle::Nes) { 1.08 } else { 1.0 };
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
                let gameboy = style == UiStyle::GameBoy;
                let shade = Color32::from_rgb(139, 172, 15);
                let (ink, paper) = match (gameboy, dark) {
                    (true, _) => (Color32::from_rgb(15, 56, 15), Color32::from_rgb(155, 188, 15)),
                    (false, true) => (Color32::WHITE, Color32::BLACK),
                    (false, false) => (Color32::BLACK, Color32::WHITE),
                };
                v.panel_fill = paper;
                v.window_fill = paper;
                v.extreme_bg_color = paper;
                v.faint_bg_color = match (gameboy, dark) {
                    (true, _) => shade,
                    (false, true) => Color32::from_gray(28),
                    (false, false) => Color32::from_gray(236),
                };
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
                let pressed = match (gameboy, dark) {
                    (true, _) => shade,
                    (false, true) => Color32::from_gray(90),
                    (false, false) => Color32::from_gray(170),
                };
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
                let pl = style_pal(style, palette, dark);
                let ink = if dark { Color32::WHITE } else { Color32::BLACK };
                v.panel_fill = pl.panel;
                // Text fields; striped table rows.
                v.extreme_bg_color = if dark { mix(pl.panel, pl.card, 0.6) } else { pl.card };
                v.faint_bg_color = if dark { mix(pl.panel, pl.card, 0.55) } else { mix(pl.panel, pl.bar, 0.6) };
                // Dialogs, menus and tooltips sit on the card color, with the palette's border.
                v.window_fill = pl.card;
                v.window_stroke = Stroke::new(1.0, pl.track);
                v.widgets.noninteractive.bg_fill = pl.card;
                v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, pl.track);
                v.hyperlink_color = pl.accent;
                v.selection.bg_fill = pl.accent.gamma_multiply(0.45);
                v.selection.stroke = Stroke::new(1.0, pl.accent);
                // Plain buttons take the palette's tint instead of a neutral gray.
                for (w, lift) in
                    [(&mut v.widgets.inactive, 0.0), (&mut v.widgets.hovered, 0.10), (&mut v.widgets.active, 0.20), (&mut v.widgets.open, 0.10)]
                {
                    w.weak_bg_fill = mix(pl.track, ink, lift);
                    w.bg_fill = mix(pl.track, ink, lift);
                    w.corner_radius = CornerRadius::same(rad(6));
                }
                // What each retro style does to plain egui widgets (buttons, menus, check boxes).
                let all = |v: &mut egui::Visuals, fill: Color32, stroke: Stroke| {
                    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                        w.weak_bg_fill = fill;
                        w.bg_fill = fill;
                        w.bg_stroke = stroke;
                        w.expansion = 0.0;
                    }
                };
                match style {
                    UiStyle::Win98 => {
                        // Gray faces with a dark edge (the bevel is painted on top, see `finish_button`);
                        // white fields; navy selection with white text; no hover effects.
                        all(v, Color32::from_gray(192), Stroke::new(1.0, Color32::from_gray(64)));
                        v.extreme_bg_color = Color32::WHITE;
                        v.faint_bg_color = Color32::from_gray(206);
                        v.selection.bg_fill = pl.accent;
                        v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
                        v.window_fill = Color32::from_gray(192);
                        v.window_stroke = Stroke::new(1.0, Color32::BLACK);
                        v.window_shadow = egui::epaint::Shadow { offset: [2, 2], blur: 0, spread: 0, color: Color32::from_black_alpha(120) };
                        v.popup_shadow = v.window_shadow;
                    }
                    UiStyle::WinXp => {
                        // Off-white buttons with a dark blue outline that glows orange under the mouse.
                        all(v, Color32::from_rgb(246, 245, 240), Stroke::new(1.0, Color32::from_rgb(0, 60, 116)));
                        v.widgets.hovered.bg_stroke = Stroke::new(2.0, Color32::from_rgb(229, 151, 0));
                        v.widgets.active.weak_bg_fill = Color32::from_rgb(226, 225, 218);
                        v.widgets.active.bg_fill = Color32::from_rgb(226, 225, 218);
                        v.faint_bg_color = Color32::from_rgb(244, 243, 234);
                        v.selection.bg_fill = pl.accent;
                        v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
                        v.window_stroke = Stroke::new(2.0, Color32::from_rgb(0, 84, 227));
                    }
                    UiStyle::Nes => {
                        // Chunky: thick dark outlines, hard shadows, red under the finger.
                        let dark_gray = Color32::from_gray(38);
                        all(v, Color32::from_rgb(204, 204, 200), Stroke::new(2.0, dark_gray));
                        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(228, 228, 224);
                        v.widgets.hovered.bg_fill = Color32::from_rgb(228, 228, 224);
                        v.window_stroke = Stroke::new(2.0, dark_gray);
                        v.window_shadow = egui::epaint::Shadow { offset: [4, 4], blur: 0, spread: 0, color: Color32::from_black_alpha(110) };
                        v.popup_shadow = v.window_shadow;
                        v.widgets.noninteractive.bg_stroke = Stroke::new(2.0, Color32::from_gray(120));
                    }
                    UiStyle::Ps1 => {
                        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                            w.corner_radius = CornerRadius::same(13);
                        }
                    }
                    UiStyle::Ios6 => {
                        // Light gray buttons with a darker rim.
                        all(v, Color32::from_rgb(238, 240, 243), Stroke::new(1.0, Color32::from_rgb(134, 141, 152)));
                        v.widgets.hovered.weak_bg_fill = Color32::WHITE;
                        v.widgets.hovered.bg_fill = Color32::WHITE;
                        v.widgets.active.weak_bg_fill = Color32::from_rgb(204, 210, 220);
                        v.widgets.active.bg_fill = Color32::from_rgb(204, 210, 220);
                        v.faint_bg_color = Color32::from_rgb(196, 202, 211);
                        v.window_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 18, spread: 0, color: Color32::from_black_alpha(110) };
                    }
                    UiStyle::Ios7 => {
                        // No button shapes: what can be pressed is set in the tint color.
                        all(v, Color32::TRANSPARENT, Stroke::NONE);
                        // (Not the pressed state: egui takes the color of strong text from it.)
                        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.open] {
                            w.fg_stroke.color = pl.accent;
                        }
                        v.widgets.active.fg_stroke.color = Color32::BLACK;
                        v.widgets.hovered.weak_bg_fill = pl.accent.gamma_multiply(0.10);
                        v.widgets.hovered.bg_fill = pl.accent.gamma_multiply(0.10);
                        v.widgets.active.weak_bg_fill = pl.accent.gamma_multiply(0.22);
                        v.widgets.active.bg_fill = pl.accent.gamma_multiply(0.22);
                        v.widgets.open.weak_bg_fill = pl.accent.gamma_multiply(0.10);
                        v.faint_bg_color = Color32::from_rgb(247, 247, 250);
                        v.window_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 30, spread: 0, color: Color32::from_black_alpha(50) };
                    }
                    UiStyle::Ps2 => {
                        // Blue glass with a glowing edge.
                        all(v, Color32::from_rgb(14, 24, 66), Stroke::new(1.0, Color32::from_rgb(52, 84, 190)));
                        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(24, 40, 104);
                        v.widgets.hovered.bg_fill = Color32::from_rgb(24, 40, 104);
                        v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(120, 180, 255));
                        v.window_stroke = Stroke::new(1.0, Color32::from_rgb(70, 110, 230));
                        v.window_shadow =
                            egui::epaint::Shadow { offset: [0, 0], blur: 24, spread: 0, color: Color32::from_rgba_unmultiplied(60, 110, 255, 90) };
                        v.popup_shadow = v.window_shadow;
                    }
                    _ => {}
                }
                v.window_corner_radius = CornerRadius::same(rad(12));
                v.menu_corner_radius = CornerRadius::same(rad(6));
                // Body text at full label contrast (egui's default is a mid grey, too faint in the dark theme).
                v.widgets.noninteractive.fg_stroke.color = match (style, dark) {
                    (UiStyle::Win98 | UiStyle::WinXp, _) => Color32::BLACK,
                    (UiStyle::Ps2, _) => Color32::from_rgb(206, 218, 250),
                    (_, true) => Color32::from_gray(225),
                    (_, false) => Color32::from_gray(30),
                };
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
    // Styles with a window title bar have the title on a row of its own; the sections start the row under it.
    let classic = title_is_bar();
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
                widths.0 = inline_title(ui, title);
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

/// Styles whose page title is a window title bar across the page.
fn title_is_bar() -> bool {
    matches!(style(), UiStyle::Classic | UiStyle::GameBoy | UiStyle::Win98 | UiStyle::WinXp | UiStyle::Nes | UiStyle::Ios6)
}

/// The page title next to the sections, for styles without a title bar; returns its width.
fn inline_title(ui: &mut Ui, title: &str) -> f32 {
    match style() {
        UiStyle::Ps1 => {
            // The four buttons of the controller, in their colors, after the title.
            let w = ui.label(RichText::new(title).large_title()).rect.width();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(88.0, 26.0), Sense::hover());
            let p = ui.painter();
            let at = |i: usize| Pos2::new(rect.left() + 12.0 + i as f32 * 21.0, rect.center().y + 1.0);
            let line = |c: Color32| Stroke::new(2.0, c);
            let (c0, r) = (at(0), 7.0);
            let tri: Vec<Pos2> =
                (0..3).map(|i| c0 + Vec2::angled(-std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 3.0) * (r + 1.0)).collect();
            p.add(egui::Shape::closed_line(tri, line(C::green())));
            p.circle_stroke(at(1), r, line(C::red()));
            let x = at(2);
            p.line_segment([x + Vec2::new(-r, -r) * 0.85, x + Vec2::new(r, r) * 0.85], line(C::accent()));
            p.line_segment([x + Vec2::new(r, -r) * 0.85, x + Vec2::new(-r, r) * 0.85], line(C::accent()));
            p.rect_stroke(Rect::from_center_size(at(3), Vec2::splat(r * 1.8)), 0, line(C::purple()), egui::StrokeKind::Middle);
            w + 96.0
        }
        // Thin, wide-set capitals in the blue of the browser.
        UiStyle::Ps2 => {
            let t = RichText::new(title.to_uppercase()).size(ty::LARGE_TITLE - 4.0).extra_letter_spacing(5.0).color(Color32::from_rgb(140, 196, 255));
            ui.label(t).rect.width()
        }
        // Big and light, as the headers of iOS 7.
        UiStyle::Ios7 => ui.label(RichText::new(title).large_title().size(ty::LARGE_TITLE + 6.0)).rect.width(),
        _ => ui.label(RichText::new(title).large_title()).rect.width(),
    }
}

/// A vertical or horizontal gradient between two colors, as a mesh.
fn gradient(p: &egui::Painter, r: Rect, from: Color32, to: Color32, horizontal: bool) {
    let mut m = egui::Mesh::default();
    let (c_lt, c_rt, c_lb, c_rb) = if horizontal { (from, to, from, to) } else { (from, from, to, to) };
    m.colored_vertex(r.left_top(), c_lt);
    m.colored_vertex(r.right_top(), c_rt);
    m.colored_vertex(r.left_bottom(), c_lb);
    m.colored_vertex(r.right_bottom(), c_rb);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 2, 3);
    p.add(egui::Shape::mesh(m));
}

/// The title bar of a window, as the style's machine drew it.
fn title_text(ui: &mut Ui, title: &str) {
    let display = |size: f32| FontId::new(size, FontFamily::Name(DISPLAY_BOLD.into()));
    match style() {
        UiStyle::Classic => {
            // The striped title bar of classic Mac windows: a close box on the left, the title in a white box.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
            let p = ui.painter();
            for i in 0..6 {
                let y = rect.top() + 6.0 + i as f32 * 3.5;
                p.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0, C::fg()));
            }
            let close = Rect::from_center_size(Pos2::new(rect.left() + 18.0, rect.top() + 14.75), Vec2::splat(13.0));
            p.rect_filled(close.expand2(Vec2::new(3.0, 5.0)), 0, C::paper());
            p.rect_stroke(close, 0, Stroke::new(1.0, C::fg()), egui::StrokeKind::Inside);
            let galley = p.layout_no_wrap(title.to_string(), display(ty::TITLE), C::fg());
            let bx = Rect::from_center_size(rect.center(), galley.size() + Vec2::new(24.0, 2.0));
            p.rect_filled(bx, 0, C::paper());
            p.galley(bx.center() - galley.size() / 2.0, galley, C::fg());
        }
        UiStyle::GameBoy => {
            // An ink band with the title in paper, like the header of a Game Boy menu.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 0, C::fg());
            p.rect_stroke(rect.shrink(3.0), 0, Stroke::new(1.0, C::paper()), egui::StrokeKind::Inside);
            p.text(rect.center(), egui::Align2::CENTER_CENTER, title, display(ty::TITLE), C::paper());
        }
        UiStyle::Win98 | UiStyle::WinXp => {
            let xp = style() == UiStyle::WinXp;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), if xp { 30.0 } else { 24.0 }), Sense::hover());
            let p = ui.painter();
            if xp {
                // Luna: a deep blue bar, lighter along the top edge, with rounded top corners.
                p.rect_filled(rect, CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, Color32::from_rgb(0, 84, 227));
                gradient(
                    p,
                    Rect::from_min_max(rect.left_top() + Vec2::new(6.0, 2.0), Pos2::new(rect.right() - 6.0, rect.top() + 9.0)),
                    Color32::from_rgb(64, 150, 255),
                    Color32::from_rgb(0, 84, 227),
                    false,
                );
                gradient(
                    p,
                    Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 8.0), rect.right_bottom()),
                    Color32::from_rgb(0, 84, 227),
                    Color32::from_rgb(0, 60, 180),
                    false,
                );
            } else {
                // Navy fading to light blue, left to right.
                gradient(p, rect, Color32::from_rgb(0, 0, 128), Color32::from_rgb(16, 132, 208), true);
            }
            let size = if xp { 15.0 } else { 13.0 };
            let pos = Pos2::new(rect.left() + 8.0, rect.center().y);
            if xp {
                p.text(pos + Vec2::splat(1.0), egui::Align2::LEFT_CENTER, title, display(size), Color32::from_rgb(10, 24, 131));
            }
            p.text(pos, egui::Align2::LEFT_CENTER, title, display(size), Color32::WHITE);
            // Minimize, maximize, close — for show.
            let side = rect.height() - if xp { 8.0 } else { 6.0 };
            for (i, kind) in ["close", "max", "min"].iter().enumerate() {
                let gap = if xp {
                    2.0
                } else if i == 0 {
                    0.0
                } else {
                    2.0 - i as f32 * 2.0 + 2.0
                };
                let x = rect.right() - 4.0 - side / 2.0 - i as f32 * (side + if xp { 2.0 } else { 0.0 }) - if !xp && i > 0 { 2.0 } else { 0.0 };
                let _ = gap;
                let b = Rect::from_center_size(Pos2::new(x, rect.center().y), Vec2::splat(side));
                let ink = if xp {
                    let fill = if i == 0 { Color32::from_rgb(218, 70, 38) } else { Color32::from_rgb(38, 110, 236) };
                    p.rect_filled(b, 3, fill);
                    p.rect_stroke(b, 3, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
                    Color32::WHITE
                } else {
                    p.rect_filled(b, 0, Color32::from_gray(192));
                    bevel(p, b, true);
                    Color32::BLACK
                };
                let g = b.shrink(side * 0.28);
                let pen = Stroke::new(if xp { 2.0 } else { 1.5 }, ink);
                match *kind {
                    "close" => {
                        p.line_segment([g.left_top(), g.right_bottom()], pen);
                        p.line_segment([g.right_top(), g.left_bottom()], pen);
                    }
                    "max" => {
                        p.rect_stroke(g, 0, Stroke::new(1.0, ink), egui::StrokeKind::Inside);
                        p.line_segment([g.left_top() + Vec2::new(0.0, 1.0), g.right_top() + Vec2::new(0.0, 1.0)], Stroke::new(2.0, ink));
                    }
                    _ => {
                        p.line_segment([g.left_bottom(), Pos2::new(g.center().x + 1.0, g.bottom())], Stroke::new(2.0, ink));
                    }
                }
            }
        }
        UiStyle::Ios6 => {
            // The navigation bar: glossy blue-gray, the title engraved in white.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::hover());
            let p = ui.painter();
            let mid = Pos2::new(rect.right(), rect.center().y);
            gradient(p, Rect::from_min_max(rect.left_top(), mid), Color32::from_rgb(182, 194, 211), Color32::from_rgb(140, 158, 184), false);
            gradient(
                p,
                Rect::from_min_max(Pos2::new(rect.left(), rect.center().y), rect.right_bottom()),
                Color32::from_rgb(128, 148, 176),
                Color32::from_rgb(108, 131, 162),
                false,
            );
            p.line_segment(
                [rect.left_top() + Vec2::new(0.0, 0.5), rect.right_top() + Vec2::new(0.0, 0.5)],
                Stroke::new(1.0, Color32::from_rgb(214, 222, 234)),
            );
            p.line_segment(
                [rect.left_bottom() - Vec2::new(0.0, 0.5), rect.right_bottom() - Vec2::new(0.0, 0.5)],
                Stroke::new(1.0, Color32::from_rgb(45, 60, 84)),
            );
            p.text(
                rect.center() - Vec2::new(0.0, 1.0),
                egui::Align2::CENTER_CENTER,
                title,
                display(ty::TITLE),
                Color32::from_rgba_unmultiplied(30, 44, 68, 190),
            );
            p.text(rect.center(), egui::Align2::CENTER_CENTER, title, display(ty::TITLE), Color32::WHITE);
        }
        UiStyle::Nes => {
            // The dark band of the console's front, with its red lettering and two red stripes.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 0, Color32::from_gray(34));
            let galley = p.layout_no_wrap(title.to_string(), display(ty::TITLE), Color32::from_rgb(232, 52, 12));
            let tx = rect.left() + 14.0;
            let after = tx + galley.size().x + 14.0;
            p.galley(Pos2::new(tx, rect.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
            for dy in [-5.0, 5.0] {
                let y = rect.center().y + dy;
                p.line_segment([Pos2::new(after, y), Pos2::new(rect.right() - 14.0, y)], Stroke::new(3.0, Color32::from_rgb(216, 40, 0)));
            }
        }
        _ => {
            ui.label(RichText::new(title).large_title());
        }
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
        let hard = |x: i8, c: Color32| egui::epaint::Shadow { offset: [x, x], blur: 0, spread: 0, color: c };
        match style() {
            // A raised panel: white along the top and left, a dark line under and to the right.
            UiStyle::Win98 => f.stroke(Stroke::new(1.0, Color32::WHITE)).corner_radius(CornerRadius::ZERO).shadow(hard(1, Color32::from_gray(64))),
            UiStyle::Nes => {
                f.stroke(Stroke::new(2.0, Color32::from_gray(38))).corner_radius(CornerRadius::ZERO).shadow(hard(4, Color32::from_black_alpha(70)))
            }
            UiStyle::Ps1 => f.stroke(Stroke::new(1.0, C::track(ui))).corner_radius(CornerRadius::same(16)),
            // A group of a grouped table: white, rounded, a gray rim and a light line under it.
            UiStyle::Ios6 => f
                .stroke(Stroke::new(1.0, Color32::from_rgb(150, 156, 166)))
                .corner_radius(CornerRadius::same(10))
                .shadow(hard(1, Color32::from_white_alpha(150))),
            // Flat, hairlines only.
            UiStyle::Ios7 => f.stroke(Stroke::new(0.5, C::track(ui))).corner_radius(CornerRadius::ZERO),
            UiStyle::Ps2 => f
                .stroke(Stroke::new(1.0, Color32::from_rgb(52, 84, 190)))
                .corner_radius(CornerRadius::ZERO)
                .shadow(egui::epaint::Shadow { offset: [0, 0], blur: 16, spread: 0, color: Color32::from_rgba_unmultiplied(50, 100, 255, 46) }),
            _ => f.stroke(Stroke::new(1.0, C::track(ui))).corner_radius(CornerRadius::same(rad(10))),
        }
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
        egui::Frame::new().fill(color.gamma_multiply(0.12)).corner_radius(rad(8)).inner_margin(egui::Margin::same(12))
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
    History,
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
        Icon::History => {
            // A clock.
            let paper = C::paper_on(c);
            if filled {
                p.circle_filled(m, 7.0 * u, c);
            } else {
                p.circle_stroke(m, 6.3 * u, s);
            }
            let hands = Stroke::new(1.5 * u, if filled { paper } else { c });
            p.line_segment([m, at(8.0, 4.4)], hands);
            p.line_segment([m, at(10.8, 9.6)], hands);
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
        p.rect_filled(rect, if classic() { 0 } else { rad(8) }, if classic() { C::fg() } else { C::accent() });
        if style() == UiStyle::Ps2 {
            p.rect_stroke(rect, 0, Stroke::new(1.0, Color32::from_rgb(150, 200, 255)), egui::StrokeKind::Inside);
        }
    } else if resp.hovered() {
        p.rect_filled(rect, if classic() { 0 } else { rad(8) }, C::track(ui).gamma_multiply(0.6));
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
            p.rect_filled(br, rad(9), C::red());
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

/// The pilot of the app icon: a Macintosh in aviator goggles on the 16×16 grid of
/// [`PIXEL_FRIEND`]. `o` is the goggles' strap and frame, `G` a lens, `w` its glint.
/// Keep in sync with `PILOT` in examples/make_icon.rs.
const PIXEL_PILOT: [&str; 16] = [
    "  ############  ",
    " #++++++++++++# ",
    " #+##########+# ",
    " #+#........#+# ",
    " #oooooooooooo# ",
    " #+#wGGoowGG#+# ",
    " #+#GGGooGGG#+# ",
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

/// The app logo — a small version of the app icon (the pixel pilot on a yellow tile).
/// In the Classic style it is 1-bit: the pilot alone, in ink on paper.
pub fn app_logo(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    let classic = classic();
    let ink = if classic { C::fg() } else { Color32::from_rgb(27, 34, 51) };
    let grid = if classic {
        rect
    } else {
        p.rect_filled(rect, size * 0.24, Color32::from_rgb(255, 200, 60));
        rect.shrink(size * 0.11)
    };
    let px = grid.width() / 16.0;
    for (y, row) in PIXEL_PILOT.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let color = match (ch, classic) {
                (' ', _) => continue,
                ('+' | '.' | 'w', true) => C::paper(),
                ('+', false) => Color32::from_rgb(255, 250, 236),
                ('.', false) => Color32::from_rgb(150, 222, 255),
                ('G', false) => Color32::from_rgb(255, 106, 61),
                ('w', false) => Color32::from_rgb(255, 236, 205),
                _ => ink,
            };
            let r = Rect::from_min_size(grid.min + Vec2::new(x as f32 * px, y as f32 * px), Vec2::splat(px));
            // Whole pixels, no anti-aliased seams between them.
            p.rect_filled(r.expand(0.25), 0, color);
        }
    }
}

/// Segmented control.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    let classic = classic();
    let win98 = style() == UiStyle::Win98;
    let track = if classic {
        C::paper()
    } else if win98 {
        Color32::TRANSPARENT
    } else {
        C::track(ui)
    };
    let frame = egui::Frame::new().fill(track).corner_radius(if classic { 0 } else { rad(8) }).inner_margin(egui::Margin::same(2));
    let ios7 = style() == UiStyle::Ios7;
    let frame = if classic {
        frame.stroke(Stroke::new(1.0, C::fg()))
    } else if ios7 {
        // Outlined in the tint; the chosen segment is filled with it.
        frame.fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, C::accent())).inner_margin(egui::Margin::same(1))
    } else {
        frame
    };
    frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = if ios7 { 0.0 } else { 2.0 };
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
                    // Windows 98: a row of buttons, the chosen one pressed in.
                    (false, true) if ios7 => (C::accent(), Color32::WHITE),
                    (false, false) if ios7 => (Color32::TRANSPARENT, C::accent()),
                    (false, true) if win98 => (Color32::from_gray(224), Color32::BLACK),
                    (false, false) if win98 => (Color32::from_gray(192), Color32::BLACK),
                    (false, true) => (C::card(ui), C::text(ui)),
                    // On the dark track of the NES the other options are light.
                    (false, false) if style() == UiStyle::Nes => (Color32::TRANSPARENT, Color32::from_gray(236)),
                    (false, false) => (Color32::TRANSPARENT, C::dim(ui)),
                };
                let b = egui::Button::new(RichText::new(*label).color(text))
                    .fill(fill)
                    .corner_radius(if classic { 0 } else { rad(6) })
                    .stroke(Stroke::NONE);
                let r = ui.add(b);
                if win98 {
                    bevel(ui.painter(), r.rect, !sel);
                }
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
    // Windows had check boxes, not switches.
    let windows = matches!(style(), UiStyle::Win98 | UiStyle::WinXp);
    let size = if classic() || windows { Vec2::new(18.0, 18.0) } else { Vec2::new(38.0, 22.0) };
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
    if windows {
        let xp = style() == UiStyle::WinXp;
        p.rect_filled(rect, rad(2), Color32::WHITE);
        if xp {
            p.rect_stroke(rect, 2, Stroke::new(1.0, Color32::from_rgb(28, 81, 128)), egui::StrokeKind::Inside);
        } else {
            bevel(p, rect, false);
        }
        if *on {
            let (c, pen) = (rect.center(), Stroke::new(2.2, if xp { Color32::from_rgb(33, 161, 33) } else { Color32::BLACK }));
            p.add(egui::Shape::line(vec![c + Vec2::new(-4.5, 0.0), c + Vec2::new(-1.5, 3.5), c + Vec2::new(4.5, -3.5)], pen));
        }
        return resp;
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    // Blue in the glossy years, green since iOS 7.
    let ios = matches!(style(), UiStyle::Ios6 | UiStyle::Ios7);
    let bg = match (*on, style()) {
        (true, UiStyle::Ios6) => Color32::from_rgb(0, 127, 234),
        (true, _) => C::green(),
        (false, _) if ios => Color32::from_rgb(229, 229, 234),
        (false, _) => C::track(ui),
    };
    let (round, knob) = if ios { (11, 9) } else { (rad(11), rad(9)) };
    p.rect_filled(rect, round, bg);
    let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    // A square knob in the boxy styles.
    let knob_rect = Rect::from_center_size(Pos2::new(x, rect.center().y), Vec2::splat(18.0));
    p.rect_filled(knob_rect, knob, Color32::WHITE);
    if ios {
        p.rect_stroke(rect, round, Stroke::new(1.0, Color32::from_black_alpha(40)), egui::StrokeKind::Inside);
        p.rect_stroke(knob_rect, knob, Stroke::new(1.0, Color32::from_black_alpha(60)), egui::StrokeKind::Outside);
    }
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
        p.rect_filled(rect, rad(8), C::card(ui));
        match style() {
            UiStyle::Win98 => bevel(p, rect, false),
            UiStyle::Nes => drop(p.rect_stroke(rect, 0, Stroke::new(2.0, Color32::from_gray(38)), egui::StrokeKind::Inside)),
            UiStyle::Ps2 => drop(p.rect_stroke(rect, 0, Stroke::new(1.0, Color32::from_rgb(40, 64, 150)), egui::StrokeKind::Inside)),
            _ => {}
        }
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
            if style() == UiStyle::Classic {
                dither(ui, r);
            } else {
                p.rect_filled(r, rad(2), C::track(ui));
            }
            let mut f = r;
            f.set_width(r.width() * ratio.clamp(0.0, 1.0));
            p.rect_filled(f, rad(2), color);
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
        // The empty part is "gray": a dither on the Macintosh, the light shade on the Game Boy.
        if style() == UiStyle::Classic {
            dither(ui, rect);
        } else {
            p.rect_filled(rect, 0, C::track(ui));
        }
        p.rect_filled(f, 0, C::fg());
        p.rect_stroke(rect, 0, Stroke::new(1.0, C::fg()), egui::StrokeKind::Inside);
        return resp;
    }
    if matches!(style(), UiStyle::Win98 | UiStyle::WinXp) && size.y >= 6.0 {
        // The progress bar of Windows: a sunken well filled with blocks.
        let xp = style() == UiStyle::WinXp;
        p.rect_filled(rect, rad(3), if xp { Color32::WHITE } else { Color32::from_gray(192) });
        if xp {
            p.rect_stroke(rect, 3, Stroke::new(1.0, Color32::from_gray(104)), egui::StrokeKind::Inside);
        } else {
            bevel(p, rect, false);
        }
        // Green in XP unless the color means something (yellow, red).
        let fill = if xp && color == C::accent() { Color32::from_rgb(46, 196, 62) } else { color };
        let inner = rect.shrink(2.0);
        let block = (inner.height() * 0.8).max(5.0);
        let mut x = inner.left();
        while x + 1.0 < inner.left() + inner.width() * ratio.clamp(0.0, 1.0) {
            let w = block.min(inner.right() - x);
            p.rect_filled(Rect::from_min_size(Pos2::new(x, inner.top()), Vec2::new(w, inner.height())), 0, fill);
            x += block + 2.0;
        }
        return resp;
    }
    let r = rad((size.y / 2.0) as u8);
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
        ui.painter().rect_filled(rect, rad(5), color.gamma_multiply(0.16));
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
    match style() {
        UiStyle::Classic | UiStyle::GameBoy => 6,
        // Round like the buttons of the console.
        UiStyle::Ps1 => 16,
        _ => rad(8),
    }
}

pub fn big_button(ui: &mut Ui, text: &str, color: Color32, enabled: bool) -> egui::Response {
    let label = RichText::new(text).semibold();
    let b = match style() {
        // The default button of the Macintosh: a thick rounded outline.
        UiStyle::Classic => egui::Button::new(label.color(C::fg())).fill(C::paper()).corner_radius(8).stroke(Stroke::new(3.0, C::fg())),
        UiStyle::GameBoy => egui::Button::new(label.color(C::paper())).fill(C::fg()).corner_radius(0).stroke(Stroke::new(2.0, C::fg())),
        // The default button of a dialog: a black frame around the bevel.
        UiStyle::Win98 => egui::Button::new(label.color(Color32::BLACK)).fill(Color32::from_gray(192)).corner_radius(0).stroke(Stroke::NONE),
        // Green, like Start — unless the color warns.
        UiStyle::WinXp => {
            let fill = if color == C::accent() { Color32::from_rgb(58, 150, 58) } else { color };
            egui::Button::new(label.color(Color32::WHITE)).fill(fill).corner_radius(3).stroke(Stroke::new(1.0, mix(fill, Color32::BLACK, 0.35)))
        }
        UiStyle::Nes => egui::Button::new(label.color(Color32::WHITE)).fill(color).corner_radius(0).stroke(Stroke::new(2.0, Color32::from_gray(38))),
        // A glossy button (the shine is painted on top, below).
        UiStyle::Ios6 => {
            egui::Button::new(label.color(Color32::WHITE)).fill(color).corner_radius(8).stroke(Stroke::new(1.0, mix(color, Color32::BLACK, 0.45)))
        }
        // Outlined in its color, like the buttons of the App Store.
        UiStyle::Ios7 => egui::Button::new(label.color(color)).fill(Color32::TRANSPARENT).corner_radius(5).stroke(Stroke::new(1.0, color)),
        UiStyle::Ps2 => egui::Button::new(label.color(Color32::WHITE))
            .fill(color.gamma_multiply(0.55))
            .corner_radius(0)
            .stroke(Stroke::new(1.0, mix(color, Color32::WHITE, 0.45))),
        _ => egui::Button::new(label.color(Color32::WHITE)).fill(color).corner_radius(button_radius()),
    };
    let r = ui.add_enabled(enabled, b.min_size(Vec2::new(0.0, 32.0)));
    finish_button(ui, &r);
    if style() == UiStyle::Win98 && enabled {
        ui.painter().rect_stroke(r.rect.expand(1.0), 0, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Outside);
    }
    if style() == UiStyle::Ios6 && ui.is_rect_visible(r.rect) {
        // The shine of the upper half.
        let top = Rect::from_min_max(r.rect.left_top() + Vec2::new(3.0, 1.5), Pos2::new(r.rect.right() - 3.0, r.rect.center().y));
        gradient(ui.painter(), top, Color32::from_white_alpha(95), Color32::from_white_alpha(30), false);
    }
    r
}

/// An action repeated on many cards: tinted with `color` instead of filled, so one screen
/// does not shout with a dozen bright buttons. A plain bordered button in the styles that have
/// no tints.
pub fn tinted_button(ui: &mut Ui, text: &str, color: Color32, enabled: bool) -> egui::Response {
    let b = match style() {
        UiStyle::Classic | UiStyle::GameBoy => egui::Button::new(RichText::new(text).semibold()).corner_radius(button_radius()),
        // The usual button of the system, with the text in the color.
        UiStyle::Win98 | UiStyle::WinXp | UiStyle::Nes | UiStyle::Ios6 => {
            egui::Button::new(RichText::new(text).semibold().color(color)).corner_radius(button_radius())
        }
        // Just the word, in its color.
        UiStyle::Ios7 => egui::Button::new(RichText::new(text).color(color)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE).corner_radius(5),
        _ => egui::Button::new(RichText::new(text).semibold().color(color))
            .fill(color.gamma_multiply(0.14))
            .stroke(Stroke::NONE)
            .corner_radius(button_radius()),
    };
    let r = ui.add_enabled(enabled, b.min_size(Vec2::new(0.0, 32.0)));
    finish_button(ui, &r);
    r
}

pub fn plain_button(ui: &mut Ui, text: &str) -> egui::Response {
    let r = ui.add(egui::Button::new(text).corner_radius(button_radius()).min_size(Vec2::new(0.0, 32.0)));
    finish_button(ui, &r);
    r
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
