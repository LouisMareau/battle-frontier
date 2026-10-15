//! Everything drawn over the 3D view with egui: the plates over each creature (HP, MP,
//! status, stat stages, locks), the round queue, the message log, timing rings and pop-ups.

use std::collections::{HashMap, VecDeque};

use egui::{epaint::PathStroke, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, TextureHandle, Vec2};
use rules::battle::{Battle, Side, Uid, WeatherKind};
use rules::moves::Stat;

use crate::stage::Stage;

pub const PANEL: Color32 = Color32::from_rgba_premultiplied(14, 19, 30, 220);
pub const PANEL_2: Color32 = Color32::from_rgba_premultiplied(26, 35, 52, 235);
pub const TEXT: Color32 = Color32::from_rgb(238, 242, 248);
pub const MUTED: Color32 = Color32::from_rgb(154, 166, 184);
pub const ACCENT: Color32 = Color32::from_rgb(255, 213, 74);
pub const PLAYER: Color32 = Color32::from_rgb(95, 178, 255);
pub const FOE: Color32 = Color32::from_rgb(255, 122, 107);
pub const GOOD: Color32 = Color32::from_rgb(87, 217, 119);
pub const MID: Color32 = Color32::from_rgb(242, 200, 75);
pub const LOW: Color32 = Color32::from_rgb(240, 96, 77);
pub const MP: Color32 = Color32::from_rgb(106, 167, 255);
pub const BLOCK: Color32 = Color32::from_rgb(143, 208, 255);

pub fn hex32(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// A small coloured type label (the type's colour, sRGB hex); returns its width.
pub fn type_pill(p: &Painter, at: Pos2, color: u32, text: &str, size: f32, crossed: bool) -> f32 {
    let g = p.layout_no_wrap(text.to_string(), FontId::proportional(size), Color32::WHITE);
    let r = Rect::from_min_size(at, Vec2::new(g.size().x + 8.0, g.size().y + 2.0));
    let mut fill = hex32(color);
    if crossed {
        fill = fill.gamma_multiply(0.35);
    }
    p.rect_filled(r, 3.0, fill);
    p.galley(r.min + Vec2::new(4.0, 1.0), g, Color32::WHITE);
    if crossed {
        p.line_segment([Pos2::new(r.left() + 2.0, r.center().y), Pos2::new(r.right() - 2.0, r.center().y)], Stroke::new(1.5, Color32::WHITE));
    }
    r.width()
}

pub struct Pop {
    pub uid: Uid,
    pub text: String,
    pub color: Color32,
    pub size: f32,
    pub t0: f64,
    pub lane: u32,
}

pub struct Cue {
    pub uid: Uid,
    pub beat: f64,
    pub window: f64,
    pub block: bool,
    pub assist: bool,
}

#[derive(Default)]
pub struct Hud {
    pub shown: HashMap<Uid, (i32, i32)>,
    pub log: VecDeque<String>,
    pub prompt: Option<(String, Color32)>,
    pub banner: Option<(String, Color32, f64)>,
    pub pops: Vec<Pop>,
    pub cues: Vec<Cue>,
    pub charge: Option<(Uid, f64, f64)>,
    pub pointer: Option<Uid>,
    pub icons: HashMap<String, TextureHandle>,
    pub active: Option<Uid>,
}

impl Hud {
    pub fn message(&mut self, text: &str) {
        self.log.push_back(text.to_string());
        while self.log.len() > 5 {
            self.log.pop_front();
        }
    }

    pub fn pop(&mut self, uid: Uid, text: impl Into<String>, color: Color32, size: f32, now: f64) {
        let lane = self.pops.iter().filter(|p| p.uid == uid && now - p.t0 < 450.0).map(|p| p.lane + 1).max().unwrap_or(0) % 4;
        self.pops.push(Pop { uid, text: text.into(), color, size, t0: now, lane });
    }

    pub fn banner(&mut self, text: impl Into<String>, color: Color32, until: f64) {
        self.banner = Some((text.into(), color, until));
    }

    pub fn feedback(&mut self, uid: Uid, success: bool, delta: Option<f64>, overheld: bool, block: bool, now: f64) {
        if success {
            let perfect = delta.map(|d| d.abs() < 40.0).unwrap_or(false);
            let text = if block { "Blocked!" } else if perfect { "Perfect!" } else { "Nice!" };
            self.pop(uid, text, if block { BLOCK } else { Color32::from_rgb(255, 243, 107) }, 18.0, now);
        } else if let Some(d) = delta {
            let text = if overheld { "Held too long" } else if d < 0.0 { "Early" } else { "Late" };
            self.pop(uid, text, Color32::from_rgb(255, 155, 138), 14.0, now);
        } else if !block {
            self.pop(uid, "Missed the beat", Color32::from_rgb(255, 155, 138), 14.0, now);
        }
    }

    pub fn draw(&mut self, ctx: &egui::Context, battle: &Battle, stage: &Stage, now: f64) {
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("hud")));
        let screen = ctx.content_rect();
        // plates: the foes' over their heads, yours under your creatures' feet
        for side in [Side::Foe, Side::Player] {
            for &u in battle.field(side) {
                let at = if side == Side::Player { stage.feet(u) } else { stage.head(u, 0.0) };
                if let Some(at) = at {
                    self.plate(&p, at, battle, u);
                }
            }
        }
        if let Some(u) = self.pointer {
            if let Some(at) = stage.head(u, 0.15) {
                let y = at.y - 64.0 + ((now / 300.0).sin() as f32 + 1.0) * 4.0;
                let tri = vec![Pos2::new(at.x - 11.0, y), Pos2::new(at.x + 11.0, y), Pos2::new(at.x, y + 16.0)];
                p.add(egui::Shape::convex_polygon(tri, ACCENT, Stroke::new(2.0, Color32::BLACK)));
            }
        }
        self.queue_bar(&p, screen, battle);
        // the log, bottom left
        let mut y = screen.bottom() - 16.0;
        let n = self.log.len();
        for (i, line) in self.log.iter().enumerate().rev() {
            let g = p.layout(line.clone(), FontId::proportional(13.0), TEXT, 430.0);
            let h = g.size().y + 6.0;
            let r = Rect::from_min_size(Pos2::new(screen.left() + 16.0, y - h), Vec2::new(444.0, h));
            p.rect_filled(r, 6.0, PANEL);
            let alpha = if i + 1 == n { 1.0 } else { 0.62 };
            p.galley(r.min + Vec2::new(9.0, 3.0), g, TEXT.gamma_multiply(alpha));
            y -= h + 2.0;
        }
        if let Some((text, color)) = &self.prompt {
            let g = p.layout_no_wrap(text.clone(), FontId::proportional(15.0), *color);
            let r = Rect::from_center_size(Pos2::new(screen.center().x, screen.top() + 108.0), g.size() + Vec2::new(28.0, 12.0));
            p.rect_filled(r, r.height() / 2.0, Color32::from_black_alpha(180));
            p.galley(r.min + Vec2::new(14.0, 6.0), g, *color);
        }
        self.pops.retain(|x| now - x.t0 < 900.0);
        for x in &self.pops {
            let Some(at) = stage.head(x.uid, -0.6) else { continue };
            let t = ((now - x.t0) / 900.0) as f32;
            let alpha = 1.0 - ((t - 0.6).max(0.0) / 0.4);
            let pos = at + Vec2::new(0.0, -t * 40.0 - x.lane as f32 * 22.0);
            let font = FontId::proportional(x.size);
            p.text(pos + Vec2::new(0.0, 2.0), Align2::CENTER_CENTER, &x.text, font.clone(), Color32::from_black_alpha((230.0 * alpha) as u8));
            p.text(pos, Align2::CENTER_CENTER, &x.text, font, x.color.gamma_multiply(alpha));
        }
        // timing cues: the shrinking ring meets the fixed one at the beat
        self.cues.retain(|c| now < c.beat + c.window + 120.0);
        for c in &self.cues {
            let Some(at) = stage.head(c.uid, -1.0) else { continue };
            let left = c.beat - now;
            let hot = left.abs() <= c.window;
            let color = if c.block { BLOCK } else { ACCENT };
            let base = 28.0;
            p.circle_stroke(at, base, Stroke::new(3.0, if hot { color } else { Color32::from_white_alpha(220) }));
            if hot {
                p.circle_stroke(at, base + 4.0, Stroke::new(4.0, color.gamma_multiply(0.4)));
            }
            if c.assist && left <= 650.0 {
                let k = (left / 650.0).max(0.0) as f32;
                p.circle_stroke(at, base * (1.0 + k * 2.2), Stroke::new(3.0, color));
            }
        }
        if let Some((u, f, sweet)) = self.charge {
            if let Some(at) = stage.head(u, -1.0) {
                let r = 33.0;
                p.circle_stroke(at, r, Stroke::new(8.0, Color32::from_black_alpha(130)));
                arc(&p, at, r, 0.0, sweet.min(0.25) as f32, Stroke::new(8.0, Color32::from_rgba_unmultiplied(120, 255, 120, 220)));
                let hot = f >= 1.0 && f <= 1.0 + sweet;
                arc(&p, at, r, 0.0, f.min(1.25) as f32, Stroke::new(8.0, if hot { Color32::from_rgb(125, 255, 125) } else { ACCENT }));
            }
        }
        if let Some((text, color, until)) = &self.banner {
            if now < *until {
                let g = p.layout(text.clone(), FontId::proportional(24.0), *color, screen.width() - 64.0);
                let r = Rect::from_center_size(Pos2::new(screen.center().x, screen.top() + screen.height() * 0.3), g.size() + Vec2::new(44.0, 20.0));
                p.rect_filled(r, 14.0, Color32::from_black_alpha(170));
                p.galley(r.min + Vec2::new(22.0, 10.0), g, *color);
            }
        }
        // weather, screens, time of day, under the queue
        let mut bits = vec![];
        if let Some(w) = battle.weather {
            bits.push(if w.kind == WeatherKind::Rain { format!("Rain ({})", w.rounds) } else { format!("Harsh sun ({})", w.rounds) });
        }
        for s in [Side::Player, Side::Foe] {
            let st = battle.side(s);
            let who = if s == Side::Player { "Your" } else { "Foe" };
            if st.physical_screen > 0 {
                bits.push(format!("{who} Phys. screen ({})", st.physical_screen));
            }
            if st.special_screen > 0 {
                bits.push(format!("{who} Spec. screen ({})", st.special_screen));
            }
        }
        bits.push(battle.period.name().to_string());
        let g = p.layout_no_wrap(bits.join(" · "), FontId::proportional(11.0), MUTED);
        let r = Rect::from_center_size(Pos2::new(screen.center().x, screen.top() + 66.0), g.size() + Vec2::new(16.0, 4.0));
        p.rect_filled(r, 6.0, PANEL);
        p.galley(r.min + Vec2::new(8.0, 2.0), g, MUTED);
    }

    fn queue_bar(&self, p: &Painter, screen: Rect, battle: &Battle) {
        let preview = battle.queue_preview();
        if preview.round == 0 {
            return;
        }
        let label = p.layout_no_wrap(format!("Round {}", preview.round), FontId::proportional(11.0), ACCENT);
        let next = p.layout_no_wrap("Next".into(), FontId::proportional(11.0), MUTED);
        let w = 20.0 + label.size().x + 6.0 + preview.now.len() as f32 * 38.0 + 18.0 + next.size().x + 6.0 + preview.next.len() as f32 * 31.0;
        let r = Rect::from_center_size(Pos2::new(screen.center().x, screen.top() + 30.0), Vec2::new(w.min(screen.width() - 32.0), 44.0));
        p.rect_filled(r, 10.0, PANEL);
        let mut x = r.left() + 10.0;
        p.galley(Pos2::new(x, r.center().y - label.size().y / 2.0), label.clone(), ACCENT);
        x += label.size().x + 6.0;
        for &u in &preview.now {
            self.icon(p, Pos2::new(x, r.center().y - 17.0), 34.0, battle, u, Some(u) == self.active);
            x += 38.0;
        }
        p.line_segment([Pos2::new(x + 2.0, r.top() + 8.0), Pos2::new(x + 2.0, r.bottom() - 8.0)], Stroke::new(1.0, Color32::from_white_alpha(40)));
        x += 10.0;
        p.galley(Pos2::new(x, r.center().y - next.size().y / 2.0), next.clone(), MUTED);
        x += next.size().x + 6.0;
        for &u in &preview.next {
            self.icon(p, Pos2::new(x, r.center().y - 14.0), 28.0, battle, u, false);
            x += 31.0;
            if x > r.right() - 28.0 {
                break;
            }
        }
    }

    fn icon(&self, p: &Painter, at: Pos2, size: f32, battle: &Battle, u: Uid, active: bool) {
        let r = Rect::from_min_size(at, Vec2::splat(size));
        let side = battle.get(u).side;
        p.rect_filled(r, 7.0, Color32::from_white_alpha(14));
        let sp = &battle.dex.species[battle.mon(u).species].key;
        if let Some(t) = self.icons.get(sp) {
            p.image(t.id(), r.shrink(1.0), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), if size < 30.0 { Color32::from_white_alpha(150) } else { Color32::WHITE });
        }
        p.rect_stroke(r, 7.0, Stroke::new(2.0, if side == Side::Player { PLAYER } else { FOE }), StrokeKind::Inside);
        if active {
            p.rect_stroke(r.expand(2.0), 8.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
        }
        if battle.get(u).cast.is_some() {
            p.circle_filled(r.right_bottom() - Vec2::splat(4.0), 5.0, ACCENT);
        }
    }

    fn plate(&self, p: &Painter, at: Pos2, battle: &Battle, u: Uid) {
        let b = battle.get(u);
        let m = battle.mon(u);
        let player = b.side == Side::Player;
        let (hp, mp) = self.shown.get(&u).copied().unwrap_or((m.hp, m.mp));
        let w = 150.0;
        let mut chips: Vec<(String, bool)> = vec![];
        if let Some(s) = m.status {
            chips.push((s.label().to_string(), false));
        }
        for s in Stat::ALL {
            let v = b.stages[s.idx()];
            if v != 0 {
                chips.push((format!("{} {}{}", s.short(), if v > 0 { "+" } else { "" }, v), v > 0));
            }
        }
        for (cond, text, good) in [
            (b.confusion > 0, "Confused", false), (b.seeded_by.is_some(), "Seeded", false), (b.protect, "Protected", true),
            (b.charging.is_some(), "Charging", true), (b.recharge, "Recharging", false), (b.stunned > 0, "Staggered", false), (b.drowsy > 0, "Drowsy", false),
        ] {
            if cond {
                chips.push((text.into(), good));
            }
        }
        let hp_h = if player { 12.0 } else { 7.0 };
        let mp_h = if player { 10.0 } else { 5.0 };
        let mut h = 4.0 + 15.0 + 3.0 + hp_h + 3.0 + mp_h + 5.0;
        if !chips.is_empty() {
            h += 14.0 * ((chips.len() as f32) / 3.0).ceil();
        }
        if b.cast.is_some() {
            h += 34.0;
        }
        let top = if player { at.y } else { at.y - h };
        let r = Rect::from_min_size(Pos2::new(at.x - w / 2.0, top), Vec2::new(w, h));
        p.rect_filled(r, 8.0, PANEL);
        p.rect_filled(Rect::from_min_max(Pos2::new(r.left() + 4.0, r.bottom() - 2.0), Pos2::new(r.right() - 4.0, r.bottom())), 1.0, if player { PLAYER } else { FOE });
        if Some(u) == self.active {
            p.rect_stroke(r, 8.0, Stroke::new(1.5, ACCENT), StrokeKind::Outside);
        }
        let mut y = r.top() + 4.0;
        let name = p.layout_no_wrap(m.name.clone(), FontId::proportional(12.0), TEXT);
        let name_w = name.size().x.min(70.0);
        p.galley(Pos2::new(r.left() + 6.0, y), name, TEXT);
        p.text(Pos2::new(r.left() + 8.0 + name_w, y + 1.0), Align2::LEFT_TOP, format!("Lv{}", m.level), FontId::proportional(10.5), MUTED);
        let mut x = r.right() - 6.0;
        for &t in battle.types(u).iter().rev() {
            let short = battle.dex.type_short(t);
            let g = p.layout_no_wrap(short.to_string(), FontId::proportional(8.5), Color32::WHITE);
            x -= g.size().x + 8.0;
            type_pill(p, Pos2::new(x, y + 1.0), battle.dex.type_color(t), short, 8.5, false);
            x -= 2.0;
        }
        y += 18.0;
        let ratio = (hp.max(0) as f32 / m.stats.hp as f32).clamp(0.0, 1.0);
        let bar = Rect::from_min_size(Pos2::new(r.left() + 6.0, y), Vec2::new(w - 12.0, hp_h));
        p.rect_filled(bar, hp_h / 2.0, Color32::from_white_alpha(30));
        p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * ratio, hp_h)), hp_h / 2.0, if ratio > 0.5 { GOOD } else if ratio > 0.2 { MID } else { LOW });
        if player {
            p.text(bar.center(), Align2::CENTER_CENTER, format!("{hp}/{}", m.stats.hp), FontId::proportional(9.5), Color32::WHITE);
        }
        y += hp_h + 3.0;
        let bar = Rect::from_min_size(Pos2::new(r.left() + 6.0, y), Vec2::new(w - 12.0, mp_h));
        p.rect_filled(bar, mp_h / 2.0, Color32::from_white_alpha(30));
        p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * (mp.max(0) as f32 / m.max_mp.max(1) as f32), mp_h)), mp_h / 2.0, MP);
        if player {
            p.text(bar.center(), Align2::CENTER_CENTER, format!("MP {mp}/{}", m.max_mp), FontId::proportional(8.5), Color32::WHITE);
        }
        y += mp_h + 4.0;
        if !chips.is_empty() {
            let mut cx = r.left() + 6.0;
            for (text, good) in chips {
                let g = p.layout_no_wrap(text, FontId::proportional(9.0), if good { Color32::from_rgb(159, 240, 176) } else { Color32::from_rgb(255, 176, 166) });
                if cx + g.size().x + 6.0 > r.right() - 4.0 {
                    cx = r.left() + 6.0;
                    y += 14.0;
                }
                let cr = Rect::from_min_size(Pos2::new(cx, y), g.size() + Vec2::new(6.0, 1.0));
                p.rect_filled(cr, 3.0, Color32::from_white_alpha(22));
                p.galley(cr.min + Vec2::new(3.0, 0.0), g.clone(), Color32::WHITE);
                cx += cr.width() + 3.0;
            }
            y += 14.0;
        }
        if let Some(c) = &b.cast {
            let mv = battle.dex.mv(m.moves[c.slot].id);
            p.text(Pos2::new(r.left() + 6.0, y), Align2::LEFT_TOP, format!("{} {}", mv.name, if c.wait > 0 { "in 2 turns" } else { "next turn" }), FontId::proportional(10.0), ACCENT);
            let mut lx = r.left() + 6.0;
            for l in &c.locks {
                lx += type_pill(p, Pos2::new(lx, y + 14.0), battle.dex.type_color(l.ty), battle.dex.type_short(l.ty), 10.0, l.broken) + 3.0;
            }
        }
    }
}

/// An arc from the top, clockwise, as a fraction of the circle.
pub fn arc(p: &Painter, center: Pos2, r: f32, from: f32, to: f32, stroke: Stroke) {
    if to <= from {
        return;
    }
    let n = ((to - from) * 64.0).ceil().max(2.0) as usize;
    let pts: Vec<Pos2> = (0..=n)
        .map(|k| {
            let a = (from + (to - from) * k as f32 / n as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            center + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(egui::Shape::line(pts, PathStroke::new(stroke.width, stroke.color)));
}

