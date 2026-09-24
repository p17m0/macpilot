//! Генерирует иконку приложения: `cargo run --example make_icon -- assets/icon.png`

fn main() {
    let out = std::env::args().nth(1).unwrap_or("assets/icon.png".into());
    let n = 1024u32;
    let mut img = image::RgbaImage::new(n, n);
    let ss = 4; // суперсэмплинг для сглаживания
    let f = n as f32;
    let (m, r) = (f * 0.09, f * 0.2); // отступ и радиус скругления (как у иконок macOS)
    let c = (f / 2.0, f * 0.54);
    let ring_r = f * 0.29;
    let ring_w = f * 0.07;
    let start = 150f32.to_radians(); // дуга 240°
    let sweep = 240f32.to_radians();
    let fill = 0.68; // заполнение «спидометра»
    for y in 0..n {
        for x in 0..n {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let px = x as f32 + (sx as f32 + 0.5) / ss as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / ss as f32;
                    // скруглённый квадрат
                    let qx = (px - f / 2.0).abs() - (f / 2.0 - m - r);
                    let qy = (py - f / 2.0).abs() - (f / 2.0 - m - r);
                    let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
                    if d > 0.0 {
                        continue;
                    }
                    let t = ((px - m) + (py - m)) / (2.0 * (f - 2.0 * m));
                    let mut col = [lerp(40.0, 120.0, t), lerp(120.0, 60.0, t), lerp(255.0, 220.0, t)];
                    // кольцо
                    let dx = px - c.0;
                    let dy = py - c.1;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let mut ang = dy.atan2(dx);
                    if ang < 0.0 {
                        ang += std::f32::consts::TAU;
                    }
                    let mut rel = ang - start;
                    if rel < 0.0 {
                        rel += std::f32::consts::TAU;
                    }
                    if (dist - ring_r).abs() < ring_w / 2.0 && rel <= sweep {
                        let a = if rel / sweep <= fill { 1.0 } else { 0.3 };
                        col = mix(col, [255.0, 255.0, 255.0], a);
                    }
                    // стрелка
                    let na = start + sweep * fill;
                    let (ux, uy) = (na.cos(), na.sin());
                    let along = dx * ux + dy * uy;
                    let across = (dx * uy - dy * ux).abs();
                    if along > -f * 0.02 && along < ring_r * 0.78 && across < f * 0.022 * (1.0 - along / (ring_r * 1.1)).max(0.25) {
                        col = [255.0, 255.0, 255.0];
                    }
                    if dist < f * 0.05 {
                        col = [255.0, 255.0, 255.0];
                    }
                    acc[0] += col[0];
                    acc[1] += col[1];
                    acc[2] += col[2];
                    acc[3] += 255.0;
                }
            }
            let k = (ss * ss) as f32;
            let a = acc[3] / k;
            if a > 0.0 {
                let s = acc[3] / 255.0;
                img.put_pixel(x, y, image::Rgba([(acc[0] / s) as u8, (acc[1] / s) as u8, (acc[2] / s) as u8, a as u8]));
            }
        }
    }
    img.save(out).unwrap();
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}
