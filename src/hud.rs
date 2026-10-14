//! The in-game HUD (0x49FD40, drawn each frame): lives, the battery meter, coins, and the
//! spin and laser charge meters, each sliding in while something shows it and out after.
//!
//! - Each element k has a show timer (0x52C824 + 2k, ticks) and a slide (0x52F2E0 + 2k, 0 to
//!   0x400): 0x49FC60 moves the slide 0x20 a tick towards 0x400 while the timer runs (counting
//!   it down) and back to 0 after; the element is drawn offset by sin(slide) / 256 − 64
//!   (0 = in place, −64 = off screen), and not at all at slide 0.
//! - Level start shows lives, batteries and coins for 180 ticks (0x414720); collecting or
//!   losing them shows theirs (0x4A0F80, 0x4071E0); the meters show while charging.
//! - Sprites come from the table 0x557500 (`sprite_table`): entries 0–49 from 0x4EC0A4, then
//!   the level's own from 0x4F7284[level]. Positions are in a 512 × 256 screen (0x493F40,
//!   0x494820) or 320 × 256 (0x4942D0, 0x493DC0, 0x4946A0); sizes scale by s / 4096.
//!
//! Elements: 0 lives, 1 batteries, 2 coins, 3 spin meter, 4 laser meter.

use crate::formats::exe::Exe;
use crate::pickups::SpriteDef;
use crate::scene::{self, Batch, Blend, GpuVertex};

pub const LIVES: usize = 0;
pub const HEALTH: usize = 1;
pub const COINS: usize = 2;
pub const SPIN: usize = 3;
pub const LASER: usize = 4;
pub const SHEEP: usize = 5;
pub const LAPS: usize = 6;
pub const BOSS: usize = 7;
const ELEMENTS: usize = 10;

/// Ticks an element stays up when something shows it (0xB4).
pub const SHOW_TICKS: i32 = 0xB4;

/// The numbers the HUD shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Status {
    pub lives: i32,
    pub health: i32,
    pub coins: i32,
    /// 0x53C83C (spin charge / super spin) and the laser's 0x53C620 / 0x53C840.
    pub spin_charge: i32,
    pub laser: i32,
    pub laser_charge: i32,
    /// 0x52B7D8: Bo Peep's sheep found (−1 when done).
    pub sheep: i32,
    /// 0x52F2F8 and 0x52AD64: the race's state and Buzz's laps.
    pub race: i32,
    pub laps: i32,
    /// 0x52B7E0: a boss's health bar (0..54), shown while `boss_on`.
    pub boss: i32,
    pub boss_on: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Hud {
    timer: [i32; ELEMENTS],
    slide: [i32; ELEMENTS],
    /// 0x52AD5F–0x52AD63: counters running modulo 4, 8, 16, 32 and 64 (blinks and pulses).
    clock: u32,
    /// 0x830CA0: the coin icon's spin (0..24, two ticks a frame).
    coin_frame: i32,
}

/// One HUD sprite to draw: sprite, cell, top-left in a `width` × 256 screen, size in those
/// pixels, colour (0x80 = full).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub sprite: usize,
    pub cell: usize,
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub width: f32,
    pub color: [u8; 3],
}

/// The sprite table 0x557500 as 0x447D40 builds it for `level` (128 entries).
pub fn sprite_table(exe: &Exe, level: usize) -> Vec<Option<SpriteDef>> {
    let read = |p: u32| -> Option<SpriteDef> {
        let d = exe.at(p, 8).ok()?;
        let cells = (0..40).map_while(|k| exe.at(p + 8 + 2 * k, 2).ok().map(|c| [c[0], c[1]])).collect();
        Some(SpriteDef { texture: i16::from_le_bytes([d[0], d[1]]), size: [d[2], d[3]], cells })
    };
    let ptr = |va: u32| exe.at(va, 4).ok().map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let mut out: Vec<Option<SpriteDef>> = (0..0x32).map(|k| ptr(0x4EC0A4 + 4 * k).filter(|&p| p != 0).and_then(read)).collect();
    if let Some(list) = ptr(0x4F7284 + 4 * level as u32) {
        for k in 0..(0x80 - 0x32) {
            match ptr(list + 4 * k) {
                Some(p) if p != u32::MAX && p != 0 => out.push(read(p)),
                _ => break,
            }
        }
    }
    out.resize(0x80, None);
    out
}

impl Hud {
    /// Shows element `k` for `ticks`.
    pub fn show(&mut self, k: usize, ticks: i32) {
        self.timer[k] = self.timer[k].max(ticks);
    }

    /// Level start (0x414720): lives, batteries and coins up, already in place.
    pub fn level_start(&mut self) {
        *self = Hud::default();
        for k in [LIVES, HEALTH, COINS] {
            self.timer[k] = SHOW_TICKS;
        }
        self.slide[LIVES] = 0x400;
    }

    /// One tick: the meters' own show rules (top of 0x49FD40), the slides (0x49FC60), clocks.
    pub fn tick(&mut self, s: &Status) {
        if s.spin_charge != 0 {
            self.timer[SPIN] = 0x78;
        }
        if s.laser != 0 {
            self.timer[LASER] = 0x3C;
        }
        if s.sheep > 0 {
            self.timer[SHEEP] = 5;
        }
        if (1..3).contains(&s.race) {
            self.timer[LAPS] = 5;
        }
        if s.boss_on {
            self.timer[BOSS] = 0x5A;
        }
        for k in 0..ELEMENTS {
            if self.timer[k] == 0 {
                self.slide[k] = (self.slide[k] - 0x20).max(0);
            } else {
                self.slide[k] = (self.slide[k] + 0x20).min(0x400);
                if self.timer[k] < 1000 {
                    self.timer[k] -= 1;
                }
            }
        }
        self.clock = self.clock.wrapping_add(1);
        self.coin_frame = (self.coin_frame + 1) % 0x18;
    }

    /// 0x49FC60's offset: None when hidden.
    fn offset(&self, k: usize) -> Option<i32> {
        let s = self.slide[k];
        (s != 0).then(|| ((((s & 0xFFF) as f64 / 4096.0 * std::f64::consts::TAU).sin() * 16384.0) as i32 >> 8) - 0x40)
    }

    fn counter(&self, bits: u32) -> u32 {
        self.clock % (1 << bits)
    }

    /// What 0x49FD40 draws this frame.
    pub fn quads(&self, s: &Status, sprites: &[Option<SpriteDef>]) -> Vec<Quad> {
        let mut q = Vec::new();
        let size = |sp: usize| sprites.get(sp).and_then(|d| d.as_ref()).map_or([0.0; 2], |d| [d.size[0] as f32, d.size[1] as f32]);
        // A sprite at (x, y) scaled by (sx, sy) / 4096, in a `w`-wide screen.
        let mut put = |w: f32, x: i32, y: i32, sp: usize, cell: usize, color: [u8; 3], sx: i32, sy: i32| {
            let [pw, ph] = size(sp);
            q.push(Quad {
                sprite: sp,
                cell,
                pos: [x as f32, y as f32],
                size: [(pw as i32 * sx >> 12) as f32, (ph as i32 * sy >> 12) as f32],
                width: w,
                color,
            });
        };
        const WIDE: f32 = 512.0;
        const NARROW: f32 = 320.0;
        let grey = [0x80; 3];
        let white = [0xFF; 3];
        let digit = |n: i32| (n.clamp(0, 9) + 0x1A) as usize;
        if let Some(off) = self.offset(LIVES) {
            put(NARROW, 0x10, off + 0x10, 10, 0, grey, 0x800, 0x800);
            put(WIDE, 0x3C, off + 0x20, 0x14, digit(s.lives), white, 0x1000, 0x1000);
        }
        // 6: laps to go (a big digit, sprite 3) beside the flag icon (sprite 0x1A), pulsing on
        // the last one.
        if let Some(off) = self.offset(LAPS) {
            let left = (3 - s.laps).clamp(0, 3);
            let c = if s.laps == 2 {
                let u = self.counter(6) as i32;
                ((if u > 0x1F { 0x3F - u } else { u }) * 4 + 0x14) as u8
            } else {
                0x80
            };
            put(WIDE, 0xFA, off + 0x18, 3, left as usize, [c; 3], 0x1000, 0x1000);
            put(NARROW, 0x8C, off + 0x10, 0x1A, 0, [c; 3], 0x1000, 0x1000);
        }
        // 5: the sheep count beside its icon (sprite 0x32, the level's own), blinking at five,
        // left of the batteries' place by however far they are in.
        if let Some(off) = self.offset(SHEEP) {
            let shift = (self.offset(HEALTH).unwrap_or(-0x40) + 0x40) / 2;
            let pulse = if s.sheep == 5 {
                let u = (self.counter(5) as i32 - 0x10) & 0x1F;
                (if u > 0xF { 0x1F - u } else { u }) * 4 + 0x50
            } else {
                0x80
            };
            if s.sheep != 5 || self.counter(5) < 0x10 {
                put(WIDE, 0x1DA - (shift << 9) / 0x140, off + 0x20, 0x14, digit(s.sheep), white, 0x1000, 0x1000);
            }
            put(NARROW, 0x110 - shift, off + 0x10, 0x32, 0, [pulse as u8; 3], 0x1000, 0x1000);
        }
        if let Some(off) = self.offset(HEALTH) {
            put(NARROW, 0x110, off + 0x10, 0xB, 0, grey, 0x800, 0x800);
            // The charge column: 2·health + 2 pixels, red to yellow; blinking when low.
            let mut h = s.health.max(0) * 2 + 2;
            if h < 5 && self.counter(4) < 8 {
                h = 0;
            }
            put(WIDE, 0x1DC, off - h + 0x2F, 6, 0, [0x80, (h * 6) as u8, 0], 0x5000, h << 12);
            put(NARROW, 0x128, off + 0x10, 0xC, 0, grey, 0x1000, 0x1000);
        }
        if let Some(off) = self.offset(COINS) {
            let y = 0xE0 - off;
            put(WIDE, 0x1A, y, 0x10, (self.coin_frame / 2) as usize, grey, 0xCCD, 0x800);
            put(WIDE, 0x33, y, 0x14, digit(s.coins / 10), white, 0x1000, 0x1000);
            put(WIDE, 0x3F, y, 0x14, digit(s.coins % 10), white, 0x1000, 0x1000);
        }
        let spin_off = self.offset(SPIN);
        if let Some(off) = spin_off {
            put(NARROW, 0x110, 0xDE - off, 0xE, 0, grey, 0x800, 0x800);
            let c = s.spin_charge;
            let len = if c > 0 { c / 2 } else if c < -0x77 { -(c + 0x78) / 6 } else { 0 };
            let full = c > 0x3B || c < -0x77;
            let color = if self.counter(3) < 4 && full { [0, 0, 0] } else { [0x80, 0x80, 0] };
            put(WIDE, 0x1B6, 0xDA - off, 6, 0, color, len * 0x1838, 0x3000);
            put(NARROW, 0x110, 0xD8 - off, 0xD, 0, grey, 0x1000, 0x1000);
        }
        if let Some(off) = self.offset(LASER) {
            // Beside the spin meter when that is up.
            let shift = (spin_off.unwrap_or(-0x40) + 0x40) / 2;
            put(NARROW, 0x110 - shift, 0xE0 - off, 0xF, 0, grey, 0x800, 0x800);
            let c = s.laser_charge;
            let blink = c == 0x40 && self.counter(3) < 4;
            let color = if blink { [0, (c * 2 - 0x80) as u8, 0] } else { [0x80, (c * 2) as u8, 0] };
            put(WIDE, 0x1B6 - (shift << 9) / 0x140, 0xDA - off, 6, 0, color, c * 0x1644 / 2, 0x3000);
            put(NARROW, 0x110 - shift, 0xD8 - off, 0xD, 0, grey, 0x1000, 0x1000);
        }
        // 7: a boss's health bar at the top (sprite 6 stretched, red to yellow) in its frame
        // (sprite 0xD); blinking when nearly empty.
        if let Some(off) = self.offset(BOSS) {
            let v = s.boss.clamp(0, 0x36);
            if v >= 0xB || self.counter(4) < 8 {
                put(WIDE, 0xE4, off + 0x14, 6, 0, [0x80, (v * 2) as u8, 0], v << 12, 0x4000);
            }
            put(WIDE, 0xE0, off + 0x10, 0xD, 0, grey, 0x2000, 0x2000);
        }
        // 0x4B8DD0 puts each normal-mode sprite at the front of the list it draws from, so the
        // last one queued is drawn first: meter bars end up over their frames.
        q.reverse();
        q
    }
}

/// The quads as triangles in screen space (x, y in 0..1 of the window, y down; z 0), for a
/// pass that maps them straight to the screen. `texture(slot)` gives a `texNN` slot's index
/// and pixel size.
pub fn batches(quads: &[Quad], sprites: &[Option<SpriteDef>], texture: &dyn Fn(i16) -> Option<(usize, [f32; 2])>) -> Vec<Batch> {
    let mut out = Vec::new();
    for q in quads {
        let Some(def) = sprites.get(q.sprite).and_then(|d| d.as_ref()) else { continue };
        if q.size[0] <= 0.0 || q.size[1] <= 0.0 {
            continue;
        }
        let Some((tex, [tw, th])) = texture(def.texture) else { continue };
        let [cu, cv] = def.cells.get(q.cell).copied().unwrap_or([0, 0]);
        let (u0, v0) = (cu as f32 / tw, cv as f32 / th);
        let (u1, v1) = ((cu as f32 + def.size[0] as f32) / tw, (cv as f32 + def.size[1] as f32) / th);
        let (x0, y0) = (q.pos[0] / q.width, q.pos[1] / 256.0);
        let (x1, y1) = ((q.pos[0] + q.size[0]) / q.width, (q.pos[1] + q.size[1]) / 256.0);
        let color = [0xFF, q.color[0], q.color[1], q.color[2]];
        let v = |x: f32, y: f32, u: f32, w: f32| GpuVertex { pos: [x, y, 0.0], uv: [u, w], color };
        let (a, b, c, d) = (v(x0, y0, u0, v0), v(x1, y0, u1, v0), v(x1, y1, u1, v1), v(x0, y1, u0, v1));
        scene::push(&mut out, Some(tex), Blend::Alpha, [a, b, c]);
        scene::push(&mut out, Some(tex), Blend::Alpha, [a, c, d]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_slide_in_and_out() {
        let mut h = Hud::default();
        assert_eq!(h.offset(COINS), None);
        h.show(COINS, 10);
        h.tick(&Status::default());
        assert_eq!(h.slide[COINS], 0x20);
        for _ in 0..12 {
            h.tick(&Status::default());
        }
        // Up while the timer runs (10 ticks), then back down.
        assert_eq!(h.timer[COINS], 0);
        assert_eq!(h.slide[COINS], 0x20 * 7);
        for _ in 0..40 {
            h.tick(&Status::default());
        }
        assert_eq!(h.offset(COINS), None);
        let mut h = Hud::default();
        h.level_start();
        assert_eq!(h.offset(LIVES), Some(0));
        h.tick(&Status { spin_charge: 5, ..Default::default() });
        assert!(h.offset(SPIN).is_some());
    }
}
