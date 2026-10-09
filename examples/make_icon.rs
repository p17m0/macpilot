//! Generates the app icon: `cargo run --example make_icon --features dev-tools -- assets/icon.png`
//!
//! A macOS-style squircle with a pixel-art Macintosh in aviator goggles: the pilot of your Mac,
//! drawn on the same 16×16 grid as the little computer of the empty states (`widgets::PIXEL_PILOT`).

#[derive(Clone, Copy)]
struct Rgba(f32, f32, f32, f32);

fn over(dst: Rgba, src: Rgba) -> Rgba {
    let a = src.3 + dst.3 * (1.0 - src.3);
    if a <= 0.0 {
        return Rgba(0.0, 0.0, 0.0, 0.0);
    }
    let mix = |d: f32, s: f32| (s * src.3 + d * dst.3 * (1.0 - src.3)) / a;
    Rgba(mix(dst.0, src.0), mix(dst.1, src.1), mix(dst.2, src.2), a)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

/// Signed distance to a superellipse ("squircle") of half-size `h`, exponent 5 like Apple's icons.
fn squircle(x: f32, y: f32, h: f32) -> f32 {
    let n = 5.0;
    let v = (x.abs() / h).powf(n) + (y.abs() / h).powf(n);
    (v.powf(1.0 / n) - 1.0) * h
}

/// Coverage 0..1 from a signed distance, with a one-pixel soft edge.
fn cover(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

/// The pilot on a 16×16 grid: `#` ink, `+` the case, `.` the screen, `o` the goggles' strap and
/// frame, `G` a lens, `w` its glint. Keep in sync with `PIXEL_PILOT` in src/gui/widgets.rs.
const PILOT: [&str; 16] = [
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

/// The grid cell under a point, if any.
fn cell(x: f32, y: f32, origin: (f32, f32), px: f32) -> Option<u8> {
    let (gx, gy) = (((x - origin.0) / px).floor(), ((y - origin.1) / px).floor());
    if !(0.0..16.0).contains(&gx) || !(0.0..16.0).contains(&gy) {
        return None;
    }
    Some(PILOT[gy as usize].as_bytes()[gx as usize]).filter(|c| *c != b' ')
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or("assets/icon.png".into());
    let size = 1024u32;
    let f = size as f32;
    let c = f / 2.0;
    let half = 412.0; // 824 px body, Apple's macOS icon grid
    let cy = c - 8.0; // body is slightly above center, shadow below
    let mut img = image::RgbaImage::new(size, size);

    // The pilot: whole pixels of 38 px, centered in the body.
    let px = 38.0;
    let origin = (c - 8.0 * px, cy - 8.0 * px + 6.0);
    let ink = [0.106, 0.133, 0.200];

    let ss = 3;
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let fx = x as f32 + (sx as f32 + 0.5) / ss as f32;
                    let fy = y as f32 + (sy as f32 + 0.5) / ss as f32;
                    let mut col = Rgba(0.0, 0.0, 0.0, 0.0);

                    // Soft shadow under the body.
                    let ds = squircle(fx - c, fy - (cy + 14.0), half - 6.0);
                    let shadow = (1.0 - (ds / 28.0).clamp(0.0, 1.0)).powi(2) * 0.35;
                    if ds < 28.0 {
                        col = over(col, Rgba(0.0, 0.0, 0.08, shadow));
                    }

                    let d = squircle(fx - c, fy - cy, half);
                    let body = cover(d);
                    if body > 0.0 {
                        // Sunny yellow, warmer towards the bottom, with a soft glow behind the pilot.
                        let t = (fy - (cy - half)) / (2.0 * half);
                        let mut rgb = lerp3([1.0, 0.87, 0.32], [1.0, 0.66, 0.16], t);
                        let glow = (1.0 - (((fx - c).powi(2) + (fy - cy).powi(2)).sqrt() / (half * 1.05))).clamp(0.0, 1.0);
                        rgb = lerp3(rgb, [1.0, 0.95, 0.62], glow * 0.45);
                        // Glossy top highlight and a thin inner rim.
                        let hl = (1.0 - ((fy - (cy - half)) / (half * 0.9))).clamp(0.0, 1.0).powi(2) * 0.22;
                        rgb = lerp3(rgb, [1.0, 1.0, 1.0], hl);
                        if d > -6.0 {
                            rgb = lerp3(rgb, [1.0, 1.0, 1.0], 0.22);
                        }
                        col = over(col, Rgba(rgb[0], rgb[1], rgb[2], body));

                        // The pilot's hard pixel shadow, down and to the right.
                        if cell(fx - 16.0, fy - 16.0, origin, px).is_some() {
                            col = over(col, Rgba(0.55, 0.25, 0.0, 0.30));
                        }
                        if let Some(ch) = cell(fx, fy, origin, px) {
                            let gy = (fy - origin.1) / (16.0 * px);
                            let rgb = match ch {
                                b'+' => lerp3([1.0, 0.985, 0.93], [0.95, 0.91, 0.82], gy),
                                b'.' => lerp3([0.70, 0.93, 1.0], [0.48, 0.80, 1.0], ((gy - 0.19) / 0.44).clamp(0.0, 1.0)),
                                b'G' => lerp3([1.0, 0.52, 0.25], [0.95, 0.30, 0.20], ((gy - 0.3125) / 0.125).clamp(0.0, 1.0)),
                                b'w' => [1.0, 0.93, 0.80],
                                _ => ink,
                            };
                            col = over(col, Rgba(rgb[0], rgb[1], rgb[2], body));
                        }
                    }
                    acc[0] += col.0 * col.3;
                    acc[1] += col.1 * col.3;
                    acc[2] += col.2 * col.3;
                    acc[3] += col.3;
                }
            }
            let k = (ss * ss) as f32;
            let a = acc[3] / k;
            if a > 0.0 {
                let to8 = |v: f32| ((v / acc[3]).clamp(0.0, 1.0) * 255.0).round() as u8;
                img.put_pixel(x, y, image::Rgba([to8(acc[0]), to8(acc[1]), to8(acc[2]), (a * 255.0).round() as u8]));
            }
        }
    }
    img.save(&out).expect("save icon");
    println!("wrote {out}");
}
