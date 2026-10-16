//! Level 10 (start-up 0x425B60, frame script 0x425F60).
//!
//! - Spinners: specials 0–3 turn about X at a speed that rises and falls on a cycle (the wind
//!   fans); 7–15 about Y at the arena fans' power; 6 always, 4 / 5 once their vents are open.
//! - Air shafts (level.dat lists 0, 1 and 10, bottom / top pairs; 1 and 10 only once their
//!   vents are stomped open: movers 4 / 5): Buzz in one rises (0x80 a tick, to 0x1000) with
//!   steam at both ends. The arena fans (list 8) lift him while their power is up.
//! - Wind (list 2) while the wind fans run fast: Buzz in front is blown along +x.
//! - Two lifts: movers 1 and 0 run path programs 0x4F3688 / 0x4F36DC (specials 0x34 / 0x37),
//!   their groups of movers (0x4F3614 / 0x4F3628) and specials (0x4F35B4 / 0x4F35D0 in
//!   full, 0x4F35EC / 0x4F3600 a quarter) following; cables 0x4B / 0x4D stretch.
//! - Three stomp switches (surfaces 0x20–0x22) shift three markers (specials 0x12–0x14,
//!   along list 9); all three in the middle opens the way (mover 0x13 goes, the barrier
//!   0x50 / 0x51 shrinks away, the lights blink, jingle −5).
//! - Sparks (lists 4–6) crackle every 120 ticks.
//! - Characters: Mr Potato Head (7: the grappling hook), Hamm (6, list 0x1D), the hint
//!   character (0x17, 0x26), the mice finder (slot 0, list 0x1E: five mice, token 1).
//! - The runaway token (slot 0x16, list 0x21): token 2 runs along list 14 after 150 ticks;
//!   catch it before the clock (140) runs down to 100; going into the middle (x ±4500,
//!   z ±2500 level units, at any height) loses it.
//! - The spider (slot 8, list 0x1F; token 4) with its routine 0x425700: Buzz reaching its
//!   floor starts it; the arena fans run on a random cycle.
//!
//! Not ported: the lifts standing still while Buzz climbs onto them (0x559E80), the lights
//! (list 13, 0x44F200), the electric beams in rooms 7 and 12 (lists 11 / 12 / 58).

use crate::boxes::Boxes;
use crate::creatures::{Creatures, ParticleRequest};
use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::game::{Player, World};
use crate::script::{near, sin_table, touched, Common, PathRunner, Script};
use crate::specials::Specials;
use glam::Vec3;

const INFO_TABLE: u32 = 0x4F37A4;
const HINTS: u32 = 0x4F37D8;
const BOSS_TEXT: u32 = 0x4F2F90;
const FINDER_ASK: u32 = 0x4F2FE0;
const FINDER_THANKS: u32 = 0x4F3058;
const POTATO_TEXTS: [u32; 3] = [0x4F30A0, 0x4F3120, 0x4F31EC];
const RACE_ASK: u32 = 0x4F3284;
const RACE_HURRY: u32 = 0x4F32D8;

const FINDER: (usize, i16) = (0, 0x1E);
const HAMM: (usize, i16) = (6, 0x1D);
const POTATO: usize = 7;
const SPIDER: (usize, i16) = (8, 0x1F);
const RACE: (usize, i16) = (0x16, 0x21);
const HINT: (usize, i16) = (0x17, 0x26);
/// The runaway token's path and special.
const TRACK: i16 = 14;
const TOKEN_SPECIAL: i32 = 0x6E;

/// A lift: lead mover, its special, program, group movers, full and quarter followers,
/// cable, sound special and its start / moving flags.
struct LiftDef {
    mover: i32,
    special: i32,
    program: u32,
    group: &'static [i32],
    full: &'static [i32],
    quarter: &'static [i32],
    cable: i32,
    sounder: i32,
    flags: (u16, u16),
}

const LIFTS: [LiftDef; 2] = [
    LiftDef {
        mover: 1,
        special: 0x34,
        program: 0x4F3688,
        group: &[1, 2, 3, 6, 7, 8, 9, 10],
        full: &[0x18, 0x1C, 0x1D, 0x1E, 0x1F, 0x31, 0x34, 0x35, 0x36, 0x3A, 0x3B, 0x3C, 0x4A],
        quarter: &[0x1A, 0x24, 0x25, 0x26, 0x27, 0x30, 0x40, 0x41, 0x42],
        cable: 0x4B,
        sounder: 0x18,
        flags: (1, 2),
    },
    LiftDef {
        mover: 0,
        special: 0x37,
        program: 0x4F36DC,
        group: &[0, 0xB, 0xC, 0xD, 0xE, 0xF, 0x10, 0x11],
        full: &[0x19, 0x20, 0x21, 0x22, 0x23, 0x33, 0x37, 0x38, 0x39, 0x3D, 0x3E, 0x3F, 0x4C],
        quarter: &[0x1B, 0x28, 0x29, 0x2A, 0x2B, 0x32, 0x43, 0x44, 0x45],
        cable: 0x4D,
        sounder: 0x19,
        flags: (0x100, 0x200),
    },
];

pub struct Level10 {
    lifts: [PathRunner; 2],
    path_flags: u16,
    /// 0x52FC14: bit 1 vent 4 open, 2 vent 5 open, 4 the switches solved, 8 the lights on.
    opened: u32,
    /// The wind fans: cycle phase (0x52FC28), speed (0x52FC38), angle (0x52FC54).
    wind_phase: i32,
    wind: i32,
    wind_angle: i32,
    /// The arena fans' angle (0x52FD00) and the others' (0x52FC6C).
    fan_angle: i32,
    spin_angle: i32,
    /// Sparks: the 120-tick beat (0x52FCD0), which third crackles (0x52FCEC), their wobble
    /// (0x52FC68).
    beat: i32,
    third: usize,
    wobble: i32,
    /// The switches' markers (0x52FC50 / 0x52FC30 / 0x52FCD8), the stomp latch (0x52FC24,
    /// later the lights' blink) and the barrier's height (0x52FC3C).
    marks: [usize; 3],
    latch: i32,
    barrier: i32,
    /// The arena fans' power (0x52FC34), its ramp (0x52FCFC) and period (0x52FC18).
    power: i32,
    ramp: i32,
    period: i32,
    /// The runaway token: its delay (0x52FC44) and position along the track (0x52FCDC,
    /// 16.16 segments).
    /// A jingle to play (the switches solved), passed on with the creatures.
    jingle: Option<[i32; 3]>,
    placed: bool,
    delay: i32,
    along: i32,
    common: Common,
}

impl Level10 {
    pub fn new(exe: Option<&Exe>) -> Level10 {
        let load = |va: u32| exe.map(|e| PathRunner::load(e, va)).unwrap_or_default();
        Level10 {
            lifts: [load(LIFTS[0].program), load(LIFTS[1].program)],
            path_flags: 0,
            opened: 0,
            wind_phase: 0,
            wind: 0,
            wind_angle: 0,
            fan_angle: 0,
            spin_angle: 0,
            beat: 0,
            third: 0,
            wobble: 0,
            marks: [0, 2, 0],
            latch: 1,
            barrier: 0x1000,
            power: 0,
            ramp: 0x3000,
            period: 0x7FFF,
            delay: 0,
            along: 0,
            jingle: None,
            placed: false,
            // 0x503A36: level 10's missing item (4) and gift (bit 0x10, the grappling hook).
            common: Common { given: [false, false, false, true, false], item: 4, ..Default::default() },
        }
    }

    pub fn set_story(&mut self, exe: &Exe, lists: &[PointList]) {
        self.story.hints = (0..5).filter_map(|k| exe.at(HINTS + 4 * k, 4).ok().map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect();
        let mut texts = vec![crate::script::HAMM_THANKS, crate::script::HAMM_ASK, crate::script::ALL_TOKENS, BOSS_TEXT, FINDER_ASK, FINDER_THANKS, RACE_ASK, RACE_HURRY];
        texts.extend(POTATO_TEXTS);
        self.load_story(exe, lists, &texts, Some(INFO_TABLE));
    }

    fn list(&self, k: i16) -> Vec<[i32; 3]> {
        self.story.lists.get(&k).cloned().unwrap_or_default()
    }

    /// The start-up's specials: the switch markers, the lights, the runaway token at the
    /// start of its track.
    fn start(&mut self, specials: &mut Specials) {
        self.place_marks(specials);
        self.lights(specials, false);
        if let Some(&p) = self.list(TRACK).first() {
            specials.set_pos(TOKEN_SPECIAL, p);
        }
    }

    /// The light pairs (0x46 / 0x47, 0x48 / 0x49, 0x4E / 0x4F): `on` shows the first of each.
    fn lights(&self, specials: &mut Specials, on: bool) {
        for (a, b) in [(0x46, 0x47), (0x48, 0x49), (0x4E, 0x4F)] {
            let (show, hide) = if on { (a, b) } else { (b, a) };
            specials.set_scale(show, [0x1000; 3]);
            specials.set_scale(hide, [0; 3]);
        }
    }

    /// 0x425680: the switch markers on list 9 (three points each).
    fn place_marks(&self, specials: &mut Specials) {
        let pts = self.list(9);
        for (k, &m) in self.marks.iter().enumerate() {
            if let Some(&p) = pts.get(3 * k + m) {
                specials.set_pos(0x12 + k as i32, p);
            }
        }
    }

    /// Buzz (object units) inside the shaft from `bottom` up to `top` (level units) with
    /// radius² `r2` (eighths of level units) and the height scaled by `scale` / 4096.
    fn in_shaft(buzz: [i32; 3], bottom: [i32; 3], top: [i32; 3], r2: i32, scale: i32) -> bool {
        let d = [0, 1, 2].map(|k| (bottom[k] >> 3) - (buzz[k] >> 8));
        let h = (((bottom[1] >> 3) - (top[1] >> 3)) - 0xC0) * scale >> 12;
        d[0] * d[0] + d[2] * d[2] < r2 && d[1] > 0 && d[1] < h
    }

    /// 0x425EB0: steam (kind 4: up, sound 0x8C) or wind (kind 1: along +x) at `at`.
    fn puff(&mut self, at: [i32; 3], kind: usize, n: i32) {
        let (every, typ, vel) = if kind == 4 { (7, 0x4F, [0, -0x100, 0]) } else { (16, 0x4E, [0x100, 0, 0]) };
        if kind == 4 {
            self.common.sounds.push((0x8C, at));
        }
        if self.every(every) {
            let angle = 0xFFF - ((self.clock as i32 & 0x3F) * 0x40);
            self.common.particles.push(ParticleRequest { pos: at, vel: vel.map(|v| v * n), gravity: 0, angle, spin: -0x100, typ });
        }
    }

    /// The air shafts of list `k`: Buzz in one rises; steam at both ends when he's near.
    fn shafts(&mut self, player: &mut Player, k: i16) {
        let pts = self.list(k);
        let buzz = player.pos_obj();
        for pair in pts.chunks_exact(2) {
            let (bottom, top) = (pair[0], pair[1]);
            let lo = bottom.map(|v| v * 32);
            if near(lo, buzz, 0x580) {
                self.puff(lo, 4, 5);
            }
            if Self::in_shaft(buzz, bottom, top, 0x2400, 0x1000) {
                player.updraft(0x80, -0x1000);
            }
            let hi = [top[0] * 32, (top[1] + 0x5DC) * 32, top[2] * 32];
            if near(hi, buzz, 0x580) {
                self.puff(hi, 4, 5);
            }
        }
    }

    /// The spinners and the cycles behind them.
    fn spinners(&mut self, specials: &mut Specials) {
        self.fan_angle += self.power * 0x80 / 0x1000;
        self.spin_angle = (self.spin_angle + 0x80) & 0xFFF;
        self.wind_phase = (self.wind_phase + 6) & 0xFFF;
        if self.wind_phase < 0xA01 {
            if self.wind < 0x80 {
                self.wind += 2;
            }
        } else if self.wind > 0 {
            self.wind -= 2;
        }
        self.wind_angle += self.wind;
        for k in 0..4 {
            specials.turn_from_own(k, [self.wind_angle + 0x300 * k, 0, 0]);
        }
        for (n, k) in (7..0x10).enumerate() {
            specials.turn_from_own(k, [0, self.fan_angle + 0x300 * n as i32, 0]);
        }
        let a = self.spin_angle;
        if self.opened & 1 != 0 {
            specials.turn_from_own(4, [0, a, 0]);
        }
        specials.turn_from_own(6, [0, a + 0x300, 0]);
        if self.opened & 2 != 0 {
            specials.turn_from_own(5, [0, a + 0x600, 0]);
        }
    }

    /// Sparks crackling on lists 4–6: every 120 ticks the next third bursts (0x45, sound
    /// 0x8B); the others fizz (0x44).
    fn sparks(&mut self, buzz: [i32; 3]) {
        self.beat += 1;
        let burst = self.beat > 0x78;
        if burst {
            self.beat -= 0x78;
            self.third = (self.third + 1) % 3;
        }
        for (list, mode, offsets) in [(4i16, 0x15usize, &[[0, -0x800, 0]][..]), (5, 0x16, &[[-0x800, 0, 0], [-0x2800, 0, 0]][..]), (6, 0x17, &[[0x800, 0, 0], [0x2800, 0, 0]][..])] {
            let pts = self.list(list);
            for p in pts.iter().skip(self.third).step_by(3) {
                let at = p.map(|v| v * 32);
                if !near(at, buzz, 0x480) {
                    continue;
                }
                if burst {
                    for o in offsets {
                        self.common.spawns.push(([at[0] + o[0], at[1] + o[1], at[2] + o[2]], 0x45, 2, 0, None));
                    }
                    self.common.sounds.push((0x8B, at));
                }
                let w = self.wobble;
                let at = if list == 4 {
                    [at[0] + (sin_table(w) >> 3), at[1], at[2] + (sin_table(w + 0x400) >> 3)]
                } else {
                    [at[0], at[1] + (sin_table(w) >> 3), at[2] + (sin_table(w + 0x400) >> 3)]
                };
                let r = self.common.rand();
                self.wobble += 0x100 + (r & 0x7F);
                let life = (self.common.rand() & 0xF) + 0x20;
                self.common.spawns.push((at, 0x44, mode, 0, Some(life)));
            }
        }
    }

    /// The wind (list 2): near a fan running fast enough, its noise and puffs; Buzz in front
    /// of it (within 0x9000 across, along its length) is blown along +x, harder nearer.
    fn winds(&mut self, player: &mut Player) {
        if self.wind <= 0x20 {
            return;
        }
        let pts = self.list(2);
        let buzz = player.pos_obj();
        for (n, pair) in pts.chunks_exact(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            let at = a.map(|v| v * 32);
            if !near(at, buzz, 0x580) {
                continue;
            }
            self.common.sounds.push((0x8C, at));
            if self.wind > 0x40 || self.wind_phase < 0xA00 {
                self.puff(at, 1, if n < 2 { 5 } else { 0xC });
            }
            let d = [0, 1, 2].map(|k| (a[k] >> 3) - (buzz[k] >> 8));
            let len = ((a[0] >> 3) - (b[0] >> 3)) - 0x20;
            if d[1] * d[1] + d[2] * d[2] < 0x9000 && d[0] < 0 && len < d[0] {
                let s = self.wind;
                player.push_by([s * 0x20 - s * d[0] * 0x20 / len, 0, 0]);
            }
        }
    }

    /// The arena fans (list 8): while their power is up, Buzz over one is lifted, higher the
    /// more power.
    fn fans(&mut self, player: &mut Player) {
        if self.power == 0 {
            return;
        }
        let pts = self.list(8);
        let buzz = player.pos_obj();
        for pair in pts.chunks_exact(2) {
            if Self::in_shaft(buzz, pair[0], pair[1], 0x6400, self.power) && player.vel[1] > -0x800 {
                player.updraft((self.power * 0x80 >> 12) + 0x80, -0x800);
                break;
            }
        }
    }

    /// The lifts: each runs its program; its group and specials follow.
    fn lifts(&mut self, world: &World, specials: &mut Specials) {
        let lists = std::mem::take(&mut self.common.story.lists);
        for (k, l) in LIFTS.iter().enumerate() {
            let Some(before) = world.mover_pos(l.mover) else { continue };
            let mut flags = self.path_flags;
            let mut rand = || self.common.rand();
            self.lifts[k].step(world, specials, l.mover, l.special, &mut flags, &lists, &mut rand);
            self.path_flags = flags;
            let moved = world.mover_pos(l.mover).map_or(Vec3::ZERO, |p| p - before);
            for &m in l.group.iter().filter(|&&m| m != l.mover) {
                world.move_mover(m, moved);
            }
            let (Some(at), Some(home)) = (specials.pos_obj(l.special), specials.home_obj(l.special)) else { continue };
            let d = [0, 1, 2].map(|i| at[i] - home[i]);
            for (ids, div) in [(l.full, 1), (l.quarter, 4)] {
                for &s in ids {
                    if let Some(h) = specials.home_obj(s) {
                        specials.set_pos(s, [0, 1, 2].map(|i| (h[i] + d[i] / div) >> 5));
                    }
                }
            }
            specials.set_scale(l.cable, [0x1000, ((d[1] + 0x4EE80) as i64 * 0x1000 / 0x4EE80) as i32, 0x1000]);
            let at = specials.pos_obj(l.sounder).unwrap_or(at);
            if self.path_flags & l.flags.0 != 0 {
                self.common.sounds.push((0x90, at));
            }
            if self.path_flags & l.flags.1 != 0 {
                self.common.sounds.push((0x8F, at));
            }
        }
        self.common.story.lists = lists;
    }

    /// Vents 4 and 5: stomped, each swings open (its special too) and its shaft runs.
    fn vents(&mut self, world: &World, specials: &mut Specials, player: &Player) {
        if player.stomp == 0 {
            return;
        }
        let on = player.floor_mover.map(|(m, _)| m);
        if self.opened & 1 == 0 && on == Some(4) {
            world.turn_mover(4, [0x180, 0, 0]);
            specials.turn_from_own(0x11, [0x180, 0, 0]);
            self.opened |= 1;
            self.put_out.push((0, 1));
        }
        if self.opened & 2 == 0 && on == Some(5) {
            world.turn_mover(5, [0, 0, 0x180]);
            specials.turn_from_own(0x10, [0, 0, 0x180]);
            self.opened |= 2;
            self.put_out.push((4, 1));
        }
    }

    /// The three switches (surfaces 0x20–0x22), then the barrier and the blinking lights.
    fn switches(&mut self, world: &World, specials: &mut Specials, player: &Player) {
        let stomped = player.on_floor && player.stomp != 0;
        if self.opened & 4 == 0 {
            if stomped && self.latch != 0 {
                let shift = match player.surface {
                    Some(0x20) => Some((0, 1)),
                    Some(0x21) => Some((1, 2)),
                    Some(0x22) => Some((2, 0)),
                    _ => None,
                };
                if let Some((down, up)) = shift {
                    self.marks[down] = self.marks[down].saturating_sub(1);
                    self.marks[up] = (self.marks[up] + 1).min(2);
                    self.latch = 0;
                    for k in [2, 3, 1] {
                        self.put_out.push((k, 1));
                    }
                }
                if self.marks == [1, 1, 1] {
                    self.opened |= 4;
                    world.set_mover(0x13, false);
                    self.jingle = Some(player.pos_obj());
                    self.lights(specials, true);
                }
            }
            if self.opened & 4 == 0 {
                if player.stomp == 0 {
                    self.latch = 1;
                }
                self.place_marks(specials);
                return;
            }
        }
        if self.barrier != 0 {
            self.barrier -= 0x20;
            if self.barrier < 0 {
                self.barrier = 0;
                specials.set_scale(0x50, [0; 3]);
                specials.set_scale(0x51, [0; 3]);
            } else {
                specials.set_scale(0x50, [0x1000, self.barrier, 0x1000]);
                specials.set_scale(0x51, [0x1000, self.barrier, 0x1000]);
            }
        }
        self.latch = (self.latch + 1) & 0x3F;
        let on = self.latch < 0x29;
        if on != (self.opened & 8 != 0) {
            self.opened ^= 8;
            self.lights(specials, on);
        }
        self.place_marks(specials);
    }

    /// The spider (0x52FC4C = `cr.boss.state`): Buzz standing on its floor starts it; after
    /// the talk its fight script runs; the fans' power runs on a random cycle; once it's
    /// gone, 120 ticks on, token 4.
    fn spider(&mut self, player: &Player, cr: &mut Creatures) {
        let y = player.pos_obj()[1];
        let b = &mut cr.boss;
        match b.state {
            0 => {
                if player.on_floor && y > -0x1C2509 && y < -0x1AC1C9 && cr.slots.get(SPIDER.0).is_some_and(|c| c.id == 0x1F) {
                    b.state = 1;
                    b.last = cr.slots[SPIDER.0].health;
                    self.talk(SPIDER.0, SPIDER.1, BOSS_TEXT, None, None);
                }
            }
            1 => {
                if self.cutscene.is_none() {
                    b.state = 2;
                    b.spin = 0x190;
                    b.wave = 5;
                    let c = &mut cr.slots[SPIDER.0];
                    c.set_script(0x1C / 2);
                    c.rec.face = 0;
                    c.timer = 0;
                }
            }
            2 => {
                self.ramp += 0x20;
                if self.ramp > self.period {
                    self.ramp = 0;
                    self.period = (self.common.rand() + 0x140) * 0x40;
                    self.power = 0;
                } else {
                    self.power = match self.ramp {
                        0..0x1000 => self.ramp,
                        0x1000..0x2000 => 0x1000,
                        0x2000..0x3000 => 0x2FFF - self.ramp,
                        _ => 0,
                    };
                }
                if cr.slots[SPIDER.0].id == 0 {
                    self.boss_bar = Some(0);
                    cr.boss.state = 3;
                } else {
                    self.boss_bar = Some(cr.boss.bar);
                    self.boss_hud = y > -0x1C2509;
                }
            }
            3..0x78 => {
                b.state += 1;
                self.power = (self.power - 0x20).max(0);
                self.boss_bar = Some(0);
            }
            0x78 => {
                if !self.given[4] {
                    self.given[4] = true;
                    self.reveals.push(4);
                }
                b.state = 200;
            }
            _ => self.power = (self.power - 0x20).max(0),
        }
        let b = &mut cr.boss;
        b.phase = self.power;
        if b.hum > 0 {
            b.hum -= 1;
            if self.power != 0 {
                let at = cr.slots[SPIDER.0].pos;
                self.common.sounds.push((0x8C, at));
            }
        }
    }

    /// The runaway token (0x52F2F8 = `race`, its clock `laps`).
    fn runaway(&mut self, specials: &mut Specials, player: &Player, cr: &mut Creatures) {
        let pts = self.list(TRACK);
        if self.tokens & 4 != 0 {
            self.laps = 100;
            self.race = 0;
            self.delay = 0;
        }
        if touched(cr, RACE.0) && self.tokens & 4 == 0 {
            let text = if self.race == 0 {
                self.race = 1;
                RACE_ASK
            } else {
                RACE_HURRY
            };
            self.talk(RACE.0, RACE.1, text, None, None);
        }
        if self.race == 1 && self.cutscene.is_none() {
            self.laps = 0x8C;
            self.delay = 1;
            self.along = 0;
            self.reveals.push(2);
            self.race = 2;
        }
        if self.race >= 2 {
            let b = player.pos_obj();
            if b[0] > -0x23560 && b[0] < 0x24820 && b[2] > -0x13A40 && b[2] < 0x13EC0 {
                self.laps = 99;
            }
            if self.every(64) {
                self.laps -= 1;
            }
            if self.laps < 100 {
                self.laps = 100;
                self.race = 0;
                self.hides.push(2);
                if let Some(&p) = pts.first() {
                    specials.set_pos(TOKEN_SPECIAL, p);
                    self.token_moves.push((2, p));
                }
                self.delay = 0;
            }
        }
        if self.delay != 0 && pts.len() >= 2 {
            self.delay += 1;
            if self.delay > 0x96 {
                self.delay = 1000;
                self.along += 0x5D2;
                let last = pts.len() as i32 - 2;
                if self.along >> 16 > last {
                    self.along = pts.len() as i32 * 0x10000 - 0x10001;
                }
                let (i, f) = ((self.along >> 16) as usize, (self.along & 0xFFFF) as i64);
                let (a, c) = (pts[i], pts[(i + 1).min(pts.len() - 1)]);
                let p = [0, 1, 2].map(|k| a[k] + ((c[k] - a[k]) as i64 * f >> 16) as i32);
                specials.set_pos(TOKEN_SPECIAL, p);
                self.token_moves.push((2, p));
            }
        }
    }
}

impl std::ops::Deref for Level10 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}

impl std::ops::DerefMut for Level10 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Script for Level10 {
    fn step(&mut self, world: &World, specials: &mut Specials, player: &mut Player, _boxes: &Boxes) {
        if self.clock == 0 {
            self.start(specials);
        }
        self.clock = self.clock.wrapping_add(1);
        self.sparks(player.pos_obj());
        self.spinners(specials);
        self.fans(player);
        if self.opened & 1 != 0 {
            self.shafts(player, 10);
        }
        if self.opened & 2 != 0 {
            self.shafts(player, 1);
        }
        self.shafts(player, 0);
        self.winds(player);
        self.switches(world, specials, player);
        self.lifts(world, specials);
        self.vents(world, specials, player);
    }

    fn step_creatures(&mut self, _world: &World, specials: &mut Specials, player: &Player, cr: &mut Creatures, coins: i32) {
        // 0x425B60: the spider starts 0x10000 along x from its home.
        if !std::mem::replace(&mut self.placed, true) && let Some(c) = cr.slots.get_mut(SPIDER.0).filter(|c| c.id == 0x1F) {
            c.pos[0] += 0x10000;
        }
        if let Some(at) = self.jingle.take() {
            cr.boss.jingles.push((-5, at));
        }
        self.boss_hud = false;
        self.spider(player, cr);
        let list = if self.item < 1 { 0x25 } else { 0x20 };
        // Level 10's gift (0x503A37): the grappling hook.
        self.potato(cr, POTATO, list, POTATO_TEXTS, [0, 0xE00], 0x10);
        self.hamm(cr, HAMM.0, HAMM.1, None, 0, coins);
        self.hint_character(cr, HINT.0, HINT.1);
        if touched(cr, FINDER.0) && cr.sheep >= 0 {
            if cr.sheep == 5 {
                self.talk(FINDER.0, FINDER.1, FINDER_THANKS, Some(1), Some([0x4BE, 0xE00]));
                cr.sheep = -1;
            } else {
                self.talk(FINDER.0, FINDER.1, FINDER_ASK, None, Some([0x4BE, 0xE00]));
            }
        }
        self.runaway(specials, player, cr);
    }
}
