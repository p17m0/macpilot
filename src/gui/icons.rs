//! App icons from macOS as egui textures. Loaded on the main thread as rows become visible,
//! a few per frame, and kept for the session. Classic style gets them in grey.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, ColorImage, Rect, Sense, TextureHandle, TextureOptions, Ui, Vec2};

use crate::mac;
use crate::widgets::{self as w, C};

/// Pixels per side: sharp at 32 pt on Retina, mipmapped for the small sizes.
const PX: usize = 64;
/// Icons made per frame, so a long list does not stall the first frame.
const PER_FRAME: usize = 6;

/// `None` path: the generic Unix executable icon.
type Key = (Option<PathBuf>, bool);

#[derive(Default)]
struct Cache {
    icons: HashMap<Key, Option<TextureHandle>>,
    pass: u64,
    made: usize,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// `None` while waiting for its turn; `Some(None)` when macOS gave no icon.
fn texture(ctx: &egui::Context, path: Option<&Path>) -> Option<Option<TextureHandle>> {
    let classic = w::classic();
    let key = (path.map(Path::to_path_buf), classic);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(t) = c.icons.get(&key) {
            return Some(t.clone());
        }
        let pass = ctx.cumulative_pass_nr();
        if c.pass != pass {
            c.pass = pass;
            c.made = 0;
        }
        if c.made >= PER_FRAME {
            ctx.request_repaint();
            return None;
        }
        c.made += 1;
        let tex = mac::icon_rgba(path, PX).map(|mut px| {
            if classic {
                for p in px.chunks_exact_mut(4) {
                    let l = (0.30 * p[0] as f32 + 0.59 * p[1] as f32 + 0.11 * p[2] as f32) as u8;
                    p[..3].fill(l);
                }
            }
            let img = ColorImage::from_rgba_premultiplied([PX, PX], &px);
            let opts = TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..TextureOptions::LINEAR };
            ctx.load_texture(format!("icon:{key:?}"), img, opts)
        });
        c.icons.insert(key, tex.clone());
        Some(tex)
    })
}

/// The icon of the app bundle at `path`, `size` points square. A plain tile when it has none.
pub fn app(ui: &mut Ui, path: &Path, size: f32) {
    paint(ui, Some(path), size);
}

/// The icon of a process: its app's when it belongs to one, otherwise the Unix executable icon.
pub fn process(ui: &mut Ui, app: Option<&Path>, size: f32) {
    paint(ui, app, size);
}

fn paint(ui: &mut Ui, path: Option<&Path>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    match texture(ui.ctx(), path) {
        Some(Some(t)) => {
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(t.id(), rect, uv, Color32::WHITE);
        }
        Some(None) => {
            ui.painter().rect_filled(rect.shrink(size * 0.1), size * 0.22, C::purple());
        }
        None => {}
    }
}
