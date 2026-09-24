//! Generates the app icon: `cargo run --example make_icon --features dev-tools -- assets/icon.png`
//!
//! A macOS-style squircle with a gauge (the "pilot" part) and a sparkle (the "cleaner" part).

use std::f32::consts::{PI, TAU};

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

fn main() {
    let out = std::env::args().nth(1).unwrap_or("assets/icon.png".into());
    let size = 1024u32;
    let f = size as f32;
    let c = f / 2.0;
    let half = 412.0; // 824 px body, Apple's macOS icon grid
    let cy = c - 8.0; // body is slightly above center, shadow below
    let mut img = image::RgbaImage::new(size, size);

    // Gauge geometry.
    let gc = (c, cy + 40.0);
    let ring_r = 250.0;
    let ring_w = 58.0;
    let start = PI * 0.75; // 135°, the arc opens at the bottom
    let sweep = PI * 1.5; // 270°
    let value = 0.64;
    let needle_a = start + sweep * value;

    let ss = 3;
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let px = x as f32 + (sx as f32 + 0.5) / ss as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / ss as f32;
                    let mut col = Rgba(0.0, 0.0, 0.0, 0.0);

                    // Soft shadow under the body.
                    let ds = squircle(px - c, py - (cy + 14.0), half - 6.0);
                    let shadow = (1.0 - (ds / 28.0).clamp(0.0, 1.0)).powi(2) * 0.35;
                    if ds < 28.0 {
                        col = over(col, Rgba(0.0, 0.0, 0.08, shadow));
                    }

                    let d = squircle(px - c, py - cy, half);
                    let body = cover(d);
                    if body > 0.0 {
                        // Diagonal gradient: bright azure → deep indigo → violet.
                        let t = ((px - (c - half)) + (py - (cy - half))) / (4.0 * half);
                        let base = if t < 0.5 {
                            lerp3([0.16, 0.58, 1.0], [0.29, 0.33, 0.96], t * 2.0)
                        } else {
                            lerp3([0.29, 0.33, 0.96], [0.50, 0.22, 0.88], (t - 0.5) * 2.0)
                        };
                        let mut rgb = base;
                        // Glossy top highlight.
                        let hl = (1.0 - ((py - (cy - half)) / (half * 1.1))).clamp(0.0, 1.0).powi(2) * 0.16;
                        rgb = lerp3(rgb, [1.0, 1.0, 1.0], hl);
                        // Thin inner rim.
                        if d > -6.0 {
                            rgb = lerp3(rgb, [1.0, 1.0, 1.0], 0.12);
                        }
                        col = over(col, Rgba(rgb[0], rgb[1], rgb[2], body));

                        let dx = px - gc.0;
                        let dy = py - gc.1;
                        let dist = (dx * dx + dy * dy).sqrt();
                        let mut ang = dy.atan2(dx);
                        if ang < 0.0 {
                            ang += TAU;
                        }
                        let mut rel = ang - start;
                        if rel < 0.0 {
                            rel += TAU;
                        }
                        // Dark dial face.
                        let face = cover(dist - (ring_r + ring_w / 2.0 + 18.0));
                        if face > 0.0 {
                            col = over(col, Rgba(0.05, 0.06, 0.20, face * 0.28));
                        }
                        // Track + colored value arc, with rounded caps.
                        let ring_d = (dist - ring_r).abs() - ring_w / 2.0;
                        let cap = |a: f32| {
                            let (ex, ey) = (gc.0 + a.cos() * ring_r, gc.1 + a.sin() * ring_r);
                            ((px - ex).powi(2) + (py - ey).powi(2)).sqrt() - ring_w / 2.0
                        };
                        let in_arc = rel <= sweep;
                        let arc_d = if in_arc { ring_d } else { cap(start).min(cap(start + sweep)) };
                        let a_arc = cover(arc_d);
                        if a_arc > 0.0 {
                            let near_start = cap(start) < cap(start + sweep);
                            // Round caps take the color of the arc end they belong to.
                            let tt = if in_arc {
                                (rel / sweep).clamp(0.0, 1.0)
                            } else if near_start {
                                0.0
                            } else {
                                1.0
                            };
                            let filled = if in_arc { tt <= value } else { near_start };
                            let rgb = if filled {
                                // green → yellow → orange along the arc
                                if tt < 0.5 {
                                    lerp3([0.20, 0.90, 0.55], [1.0, 0.86, 0.25], tt * 2.0)
                                } else {
                                    lerp3([1.0, 0.86, 0.25], [1.0, 0.55, 0.20], (tt - 0.5) * 2.0)
                                }
                            } else {
                                [1.0, 1.0, 1.0]
                            };
                            let alpha = if filled { 1.0 } else { 0.22 };
                            col = over(col, Rgba(rgb[0], rgb[1], rgb[2], a_arc * alpha));
                        }
                        // Ticks inside the ring.
                        for i in 0..=8 {
                            let a = start + sweep * i as f32 / 8.0;
                            let (ux, uy) = (a.cos(), a.sin());
                            let along = dx * ux + dy * uy;
                            let across = (dx * uy - dy * ux).abs();
                            let td = (across - 5.0).max((along - (ring_r - 52.0)).abs() - 14.0);
                            let at = cover(td);
                            if at > 0.0 {
                                col = over(col, Rgba(1.0, 1.0, 1.0, at * 0.55));
                            }
                        }
                        // Needle: a tapered bar with a round hub.
                        let (ux, uy) = (needle_a.cos(), needle_a.sin());
                        let along = dx * ux + dy * uy;
                        let across = (dx * uy - dy * ux).abs();
                        let len = ring_r - 40.0;
                        let width = lerp(15.0, 4.0, (along / len).clamp(0.0, 1.0));
                        let nd = (across - width).max(-along - 10.0).max(along - len);
                        let an = cover(nd);
                        if an > 0.0 {
                            col = over(col, Rgba(1.0, 1.0, 1.0, an));
                        }
                        let hub = cover(dist - 34.0);
                        if hub > 0.0 {
                            col = over(col, Rgba(1.0, 1.0, 1.0, hub));
                        }
                        let hub_in = cover(dist - 13.0);
                        if hub_in > 0.0 {
                            col = over(col, Rgba(0.32, 0.30, 0.93, hub_in));
                        }

                        // Sparkle (four-point star) at the top right: "clean".
                        let (sx0, sy0) = (c + 268.0, cy - 262.0);
                        let (qx, qy) = ((px - sx0).abs(), (py - sy0).abs());
                        let r_big = 86.0;
                        // Astroid-like star: |x|^p + |y|^p <= r^p with p < 1.
                        let p = 0.55;
                        let star = (qx / r_big).powf(p) + (qy / r_big).powf(p);
                        let sd = (star - 1.0) * 40.0;
                        let asd = cover(sd);
                        if asd > 0.0 {
                            col = over(col, Rgba(1.0, 1.0, 1.0, asd));
                        }
                        // Small companion sparkle.
                        let (sx1, sy1) = (c + 158.0, cy - 318.0);
                        let (qx, qy) = ((px - sx1).abs(), (py - sy1).abs());
                        let star2 = (qx / 34.0).powf(p) + (qy / 34.0).powf(p);
                        let asd2 = cover((star2 - 1.0) * 16.0);
                        if asd2 > 0.0 {
                            col = over(col, Rgba(1.0, 1.0, 1.0, asd2 * 0.85));
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
