//! Battery page: charge and power draw, health, a history chart, the apps using energy and what keeps the Mac awake.

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Stroke, Ui, Vec2};
use macpilot::battery::{self, Battery, RATED_CYCLES, Sample};
use macpilot::{fmt, tr, trf};

use crate::icons;
use crate::overview::stat_card;
use crate::widgets::{self as w, C, Txt};
use crate::{Gui, Page, ProcView, Sel};

/// Minutes as "1 h 25 min".
fn minutes(m: u32) -> String {
    fmt::duration(m as u64 * 60)
}

/// Short text for the sidebar meter and the status line: ("27%", "1 h 8 min left").
pub fn short_state(b: &Battery) -> (String, String) {
    let pct = fmt::pct0(b.percent as f32);
    let state = if b.charging {
        b.time_to_full.map(|m| trf("{0} to full", &[&minutes(m)])).unwrap_or_else(|| tr("charging").into())
    } else if b.plugged {
        if b.fully_charged { tr("charged").into() } else { tr("on power adapter").into() }
    } else {
        b.time_to_empty.map(|m| trf("{0} left", &[&minutes(m)])).unwrap_or_else(|| tr("on battery").into())
    };
    (pct, state)
}

/// Line for the menu bar menu.
pub fn menu_line(b: &Battery) -> String {
    let (pct, state) = short_state(b);
    trf("Battery: {0} · {1}", &[&pct, &state])
}

fn power_text(b: &Battery) -> (String, String) {
    if b.charging && b.watts > 0.5 {
        let adapter = b.adapter_watts.map(|a| trf("{0} adapter", &[&fmt::watts(a as f32)])).unwrap_or_default();
        (format!("+{}", fmt::watts(b.watts)), format!("{} {}", tr("charging"), adapter).trim().to_string())
    } else if b.plugged {
        let adapter = b.adapter_watts.map(|a| trf("{0} adapter", &[&fmt::watts(a as f32)])).unwrap_or_else(|| tr("on power adapter").into());
        (tr("from the adapter").into(), adapter)
    } else {
        (fmt::watts(-b.watts), tr("drawn from the battery now").into())
    }
}

pub fn show(g: &mut Gui, ui: &mut Ui) {
    let info = g.power.lock().unwrap().clone();
    egui::CentralPanel::default().frame(w::page_frame(ui)).show(ui, |ui| {
        w::centered(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let Some(b) = info.battery().cloned() else {
                    w::header(ui, tr("Battery"), "");
                    w::empty(ui, tr("This Mac has no battery."));
                    return;
                };
                let (pct, state) = short_state(&b);
                w::header(ui, tr("Battery"), &format!("{pct} · {state}"));

                ui.columns(4, |cols| {
                    let low = 1.0 - b.percent as f32 / 100.0;
                    let sub = if b.low_power_mode { tr("Low Power Mode is on").to_string() } else { state.clone() };
                    stat_card(&mut cols[0], tr("Charge"), pct.clone(), sub, low, None);
                    let (value, sub) = power_text(&b);
                    let heavy = if !b.plugged && -b.watts > 25.0 { 0.8 } else { 0.1 };
                    stat_card(&mut cols[1], tr("Power"), value, sub, heavy, None);
                    let health = b.health_pct();
                    let condition = match b.condition.as_deref() {
                        Some("Good" | "Normal") | None => tr("normal").to_string(),
                        Some(other) => other.to_string(),
                    };
                    stat_card(
                        &mut cols[2],
                        tr("Health"),
                        health.map(|h| format!("{h}%")).unwrap_or("—".into()),
                        trf("{0} · {1} of {2} cycles", &[&condition, &b.cycles, &RATED_CYCLES]),
                        if b.needs_service() { 0.95 } else { 0.1 },
                        None,
                    );
                    let hot = if b.temperature >= 40.0 {
                        0.95
                    } else if b.temperature >= 35.0 {
                        0.8
                    } else {
                        0.1
                    };
                    stat_card(&mut cols[3], tr("Temperature"), fmt::celsius(b.temperature), tr("of the battery").into(), hot, None);
                });

                ui.add_space(w::sp::M);
                advice(g, ui, &b);

                ui.add_space(w::sp::L);
                history(g, ui);

                ui.add_space(w::sp::L);
                energy_users(g, ui, &b);

                ui.add_space(w::sp::L);
                awake(g, ui, &info.blockers);
            });
        });
    });
}

fn advice(g: &mut Gui, ui: &mut Ui, b: &Battery) {
    let mut any = false;
    if b.needs_service() {
        let h = b.health_pct().map(|h| format!("{h}%")).unwrap_or_default();
        w::note(
            ui,
            C::red(),
            tr("The battery needs service"),
            &trf("Its maximum capacity is {0} of new. Below 80% macOS recommends replacing it; the Mac lasts noticeably less on a charge.", &[&h]),
        );
        any = true;
    }
    if b.temperature >= 40.0 {
        w::note(
            ui,
            C::yellow(),
            &trf("The battery is hot: {0}", &[&fmt::celsius(b.temperature)]),
            tr("Heat wears batteries fastest. Heavy apps, charging on a soft surface or in the sun make it worse."),
        );
        any = true;
    }
    if b.cycles >= RATED_CYCLES * 9 / 10 {
        w::note(
            ui,
            C::yellow(),
            &trf("{0} charge cycles", &[&b.cycles]),
            &trf("The battery is rated for {0} cycles. Expect it to hold less charge from now on.", &[&RATED_CYCLES]),
        );
        any = true;
    }
    if !b.plugged && b.percent <= 20 && !b.low_power_mode {
        w::note(
            ui,
            C::yellow(),
            tr("Low battery"),
            tr("Low Power Mode makes the charge last longer: macOS slows the processor a little and dims the screen."),
        );
        if w::button(ui, tr("Battery settings…")).clicked() {
            battery::open_battery_settings();
        }
        any = true;
    }
    if let Some(d) = battery::drain_per_hour(&recent(&g.battery_hist, 7)) {
        let mut text = trf("On battery your Mac uses about {0} of charge per hour.", &[&fmt::pct0(d)]);
        if d > 0.1 {
            text += " ";
            text += &trf("A full charge lasts about {0} with your usual use.", &[&fmt::duration((100.0 / d * 3600.0) as u64)]);
        }
        ui.label(RichText::new(text).color(C::dim(ui)));
        any = true;
    }
    if !any {
        ui.label(RichText::new(tr("The battery is in good shape.")).color(C::dim(ui)));
    }
}

fn recent(h: &[Sample], days: i64) -> Vec<Sample> {
    let cutoff = macpilot::disk::now_unix() - days * 86_400;
    h.iter().filter(|s| s.ts >= cutoff).copied().collect()
}

/// Local hour of a unix time.
fn local_tm(ts: i64) -> libc::tm {
    let t = ts as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm
}

fn history(g: &mut Gui, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Charge history")).section());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            w::segmented(ui, &mut g.battery_days, &[(1, tr("24 hours")), (7, tr("7 days"))]);
        });
    });
    ui.add_space(w::sp::S);
    let days = g.battery_days;
    let samples = recent(&g.battery_hist, days);
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if samples.len() < 2 {
            ui.label(
                RichText::new(tr("MacPilot records the charge every 5 minutes while it runs. The chart fills up over the next hours."))
                    .color(C::dim(ui)),
            );
            return;
        }
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 170.0), egui::Sense::hover());
        let plot = Rect::from_min_max(rect.min + Vec2::new(34.0, 6.0), rect.max - Vec2::new(4.0, 20.0));
        let now = macpilot::disk::now_unix();
        let t0 = now - days * 86_400;
        let x = |ts: i64| plot.left() + (ts - t0) as f32 / (days * 86_400) as f32 * plot.width();
        let y = |p: u32| plot.bottom() - p as f32 / 100.0 * plot.height();
        let p = ui.painter();
        let dim = C::dim(ui);
        for level in [0, 50, 100] {
            p.line_segment([Pos2::new(plot.left(), y(level)), Pos2::new(plot.right(), y(level))], Stroke::new(1.0, C::track(ui)));
            p.text(
                Pos2::new(plot.left() - 6.0, y(level)),
                egui::Align2::RIGHT_CENTER,
                format!("{level}%"),
                egui::FontId::proportional(w::ty::CAPTION),
                dim,
            );
        }
        // Time ticks: every 6 hours, or every day at midnight.
        let step = if days == 1 { 6 * 3600 } else { 86_400 };
        let tm = local_tm(t0);
        let offset = tm.tm_gmtoff;
        let mut tick = ((t0 + offset) / step + 1) * step - offset;
        while tick < now {
            let tt = local_tm(tick);
            let label = if days == 1 { format!("{:02}:00", tt.tm_hour) } else { format!("{:02}.{:02}", tt.tm_mday, tt.tm_mon + 1) };
            p.line_segment([Pos2::new(x(tick), plot.top()), Pos2::new(x(tick), plot.bottom())], Stroke::new(1.0, C::track(ui).gamma_multiply(0.6)));
            p.text(Pos2::new(x(tick), plot.bottom() + 4.0), egui::Align2::CENTER_TOP, label, egui::FontId::proportional(w::ty::CAPTION), dim);
            tick += step;
        }
        // Plugged-in stretches are shaded; gaps (the app was not running) break the line.
        for win in samples.windows(2) {
            let (a, b) = (win[0], win[1]);
            if b.ts - a.ts > 20 * 60 {
                continue;
            }
            if a.plugged {
                p.rect_filled(
                    Rect::from_min_max(Pos2::new(x(a.ts), plot.top()), Pos2::new(x(b.ts), plot.bottom())),
                    0,
                    C::green().gamma_multiply(0.10),
                );
            }
            let color = if a.percent <= 20 {
                C::red()
            } else if a.plugged {
                C::green()
            } else {
                C::accent()
            };
            p.line_segment([Pos2::new(x(a.ts), y(a.percent)), Pos2::new(x(b.ts), y(b.percent))], Stroke::new(2.0, color));
        }
        // Hover: the value at that time.
        if let Some(pos) = resp.hover_pos() {
            let ts = t0 + ((pos.x - plot.left()) / plot.width() * (days * 86_400) as f32) as i64;
            if let Some(s) = samples.iter().min_by_key(|s| (s.ts - ts).abs()).filter(|s| (s.ts - ts).abs() < 20 * 60) {
                let tt = local_tm(s.ts);
                p.circle_filled(Pos2::new(x(s.ts), y(s.percent)), 3.5, C::text(ui));
                let what = if s.plugged { tr("on power adapter").to_string() } else { fmt::watts(-s.watts) };
                resp.on_hover_text(format!("{:02}:{:02} · {} · {}", tt.tm_hour, tt.tm_min, fmt::pct0(s.percent as f32), what));
            }
        }
        ui.horizontal(|ui| {
            legend(ui, C::accent(), tr("on battery"));
            legend(ui, C::green(), tr("on power adapter"));
        });
    });
}

fn legend(ui: &mut Ui, c: Color32, text: &str) {
    w::dot(ui, c);
    ui.label(RichText::new(text).caption().color(C::dim(ui)));
    ui.add_space(w::sp::S);
}

fn energy_users(g: &mut Gui, ui: &mut Ui, b: &Battery) {
    ui.label(RichText::new(tr("Using energy now")).section());
    let mut groups: Vec<_> = g.snap.groups().into_iter().filter(|gr| gr.power.is_some_and(|p| p >= 0.05)).collect();
    groups.sort_by(|a, b| b.power.unwrap_or(0.0).total_cmp(&a.power.unwrap_or(0.0)));
    groups.truncate(8);
    ui.label(
        RichText::new(if b.plugged {
            tr("Processor and graphics work of your apps.")
        } else {
            tr("Processor and graphics work of your apps. The display, Wi-Fi and macOS itself use the rest of the battery power.")
        })
        .color(C::dim(ui)),
    );
    ui.add_space(w::sp::S);
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if groups.is_empty() {
            ui.label(RichText::new(tr("Your apps are idle.")).color(C::dim(ui)));
            return;
        }
        let max = groups.iter().filter_map(|g| g.power).fold(0.1f32, f32::max);
        let mut open = None;
        for gr in &groups {
            let watts = gr.power.unwrap_or(0.0);
            ui.horizontal(|ui| {
                let name = ui
                    .allocate_ui_with_layout(Vec2::new(220.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        icons::process(ui, gr.app.as_deref(), 18.0);
                        ui.add(egui::Label::new(RichText::new(&gr.label).color(C::text(ui))).truncate().sense(egui::Sense::click()))
                    })
                    .inner;
                if name.clicked() {
                    open = Some(gr.key.clone());
                }
                let (bar, text) = if watts >= 4.0 {
                    (C::red(), C::red())
                } else if watts >= 1.0 {
                    (C::yellow(), C::yellow())
                } else {
                    (C::accent(), C::text(ui))
                };
                w::bar(ui, watts / max, Vec2::new((ui.available_width() - 90.0).max(60.0), 8.0), bar);
                ui.label(RichText::new(fmt::watts(watts)).color(text));
            });
        }
        if let Some(key) = open {
            g.view = ProcView::Apps;
            g.sel = Some(Sel::Group(key));
            g.go_page(Page::Procs);
        }
    });
}

fn awake(g: &mut Gui, ui: &mut Ui, blockers: &[battery::SleepBlocker]) {
    ui.label(RichText::new(tr("Keeping the Mac awake")).section());
    ui.label(RichText::new(tr("While these apps ask for it, the Mac does not sleep on its own and the battery keeps draining.")).color(C::dim(ui)));
    ui.add_space(w::sp::S);
    w::card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if blockers.is_empty() {
            ui.label(RichText::new(tr("Nothing — the Mac can sleep when idle.")).color(C::dim(ui)));
            return;
        }
        for bl in blockers {
            let name = blocker_name(g, bl);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&name).semibold());
                if bl.display {
                    w::badge(ui, tr("keeps the display on"), C::yellow());
                }
                ui.label(RichText::new(trf("for {0}", &[&fmt::duration(bl.seconds)])).color(C::dim(ui)));
                if let Some(why) = reason_text(&bl.reason) {
                    ui.label(RichText::new(format!("· {why}")).caption().color(C::dim(ui)));
                }
            });
        }
    });
}

/// A readable reason, or nothing when the app only gave a technical name.
fn reason_text(r: &str) -> Option<String> {
    let l = r.to_lowercase();
    if l.contains("video") {
        return Some(tr("playing video").into());
    }
    if l.contains("audio") || l.contains("sound") {
        return Some(tr("playing sound").into());
    }
    if l.contains("download") {
        return Some(tr("downloading").into());
    }
    // "Electron", "com.apple.something…" say nothing to a person.
    let technical = r.is_empty() || r == "Electron" || (!r.contains(' ') && r.contains('.'));
    (!technical).then(|| r.to_string())
}

/// App name for a sleep blocker (the process may be a helper of an app).
pub fn blocker_name(g: &Gui, bl: &battery::SleepBlocker) -> String {
    g.snap
        .get(bl.pid)
        .map(|p| p.app_name().unwrap_or_else(|| p.name.clone()))
        .unwrap_or_else(|| if bl.name.is_empty() { format!("PID {}", bl.pid) } else { bl.name.clone() })
}
