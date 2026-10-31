//! Level 9, a boss arena (start-up 0x424390, frame script 0x424490). The boss is slot 0
//! (type 54, 26 health) with six flails (slots 1–6, type 55) spinning round it; slots 7–11
//! are the minions it calls up, dead at the start and never respawning by themselves.
//!
//! - It's kept within 0x1068 (later 0x1388) of the middle; everything bobs on a base height
//!   (0x52FBCC + sin >> 3). The flails turn at 4 a tick, each knocking Buzz back from its
//!   tip (local 0, 0x96, 0x5AA) within 0x4B × 256.
//! - Buzz coming in past z −0x1BB58: a look-at scene (0x168 ticks) while it rises to its
//!   height and taunts (0xD5), then the fight (speed 0x10).
//! - Each hit (sound 0x83): its health goes back to 26 − round, it can't be hurt for 120 ticks
//!   (in the fight: 600, with a look-at scene of 0x168 while the round's flail calls up the
//!   next pair of minions from 0x4F2F58 — they grow in at the flail, which then bursts:
//!   fireballs, 0x85, taunt 0xD6). When both minions are beaten the next round starts; after
//!   six it chases Buzz itself (state 3, turn 2, speed 0x14). Below 11 health it dies
//!   (look-at 0xF0) and the level ends.
//! - Every so often (when its cool-down allows) with Buzz within 400 × 256 it fires a beam
//!   at the ground in front of it towards him (±0x200), sparks and sound 0x87 where it lands;
//!   Buzz within 30 × 256 of that is hurt (0x84, taunt 0xD7).
//! - The camera turns to the nearer live minion, else to the boss; its bar shows in the
//!   fight ((health − 10) × 54 / 16).

use crate::boxes::Boxes;
use crate::creatures::Creatures;
use crate::game::{Player, World};
use crate::script::{near, sin_table, Common, Script};
use crate::specials::Specials;

const BOSS: usize = 0;
const FLAILS: std::ops::RangeInclusive<usize> = 1..=6;
/// The minion pairs called up each round (0x4F2F5C…).
const PAIRS: [(usize, usize); 6] = [(8, 8), (7, 7), (8, 9), (10, 7), (11, 8), (11, 7)];

/// The beam's heading (0x424490): towards Buzz, at most 0x200 off its facing. The turn is
/// wrapped by 0xFFF, not 0x1000, so a turn just left of ahead comes out one unit further right.
pub fn beam_heading(facing: i32, to_buzz: i32) -> i32 {
    let mut d = (to_buzz - facing) & 0xFFF;
    if d > 0x800 {
        d -= 0xFFF;
    }
    (d.clamp(-0x200, 0x200) + facing) & 0xFFF
}

pub struct Level9 {
    /// 0x52FC04: 0 waiting, 1 rising, 2 fighting, 3 chasing, 4 dying, 5 over.
    phase: i32,
    /// 0x52FBB8 health seen; 0x52FBD4 round; 0x52FBF4 pair index; 0x52FBC8 / 0x52FBC4 the pair.
    health: i32,
    round: usize,
    pair_at: usize,
    pair: (usize, usize),
    /// 0x52FBC0 the after-hit count; 0x52FBF0 the pair's been set never to come back.
    hurt: i32,
    pair_fixed: bool,
    /// 0x52FBCC base height, 0x52FC08 its bob phase, 0x52FC0C the flails' turn, 0x52FBEC
    /// their animation phase; 0x52FC00 the beam's cool-down; 0x52FBF8 the taunt timer;
    /// 0x52FBFC the hurt-voice timer; 0x52FBE8 the blink.
    base: i32,
    bob: i32,
    turn: i32,
    phase_anim: i32,
    cool: i32,
    taunt: i32,
    ouch: i32,
    blink: bool,
    started: bool,
    /// This round's flail has called its minions up / burst.
    called: bool,
    burst: bool,
    common: Common,
}

impl Level9 {
    pub fn new() -> Level9 {
        Level9 {
            phase: 0,
            health: 26,
            round: 1,
            pair_at: 0,
            pair: (7, 7),
            hurt: 0,
            pair_fixed: false,
            base: -0x58000,
            bob: 0,
            turn: 0,
            phase_anim: 0,
            cool: 0xC8,
            taunt: 0x78,
            ouch: 0,
            blink: false,
            started: false,
            called: false,
            burst: false,
            common: Common::default(),
        }
    }

    /// 0x424390's creature set-up: the minions dead for good, nothing drops a coin, the boss
    /// can't move yet.
    fn start(&mut self, cr: &mut Creatures) {
        for i in 7..=11 {
            if let Some(c) = cr.slots.get_mut(i) {
                c.respawn = 10000;
                c.health = 0;
            }
        }
        for c in cr.slots.iter_mut().take(12) {
            c.flags |= 0x800;
        }
        if let Some(c) = cr.slots.get_mut(BOSS) {
            self.health = c.health;
            c.rec.speed = 0;
        }
    }
}

impl Default for Level9 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for Level9 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}

impl std::ops::DerefMut for Level9 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Script for Level9 {
    fn step(&mut self, _world: &World, _specials: &mut Specials, _player: &mut Player, _boxes: &Boxes) {}

    fn step_creatures(&mut self, world: &World, _specials: &mut Specials, player: &Player, cr: &mut Creatures, _coins: i32) {
        if cr.slots.len() < 12 || (!self.started && cr.slots[BOSS].id != 54) {
            return;
        }
        if !std::mem::replace(&mut self.started, true) {
            self.start(cr);
        }
        self.clock = self.clock.wrapping_add(1);
        let buzz = player.pos_obj();
        // Kept within reach of the middle.
        let reach = if self.phase < 3 { 0x1068 } else { 0x1388 };
        let c = &mut cr.slots[BOSS];
        let (x, z) = (c.pos[0] >> 5, c.pos[2] >> 5);
        if x * x + z * z > reach * reach {
            let a = crate::boxes::angle12(x as f32, z as f32);
            c.pos[0] = sin_table(a) * reach >> 9;
            c.pos[2] = sin_table(a + 0x400) * reach >> 9;
        }
        // Once a pair has been called (0x52FBF4 > 0): no sideways speed while a scene holds
        // play (0x52B816 bit 1) and both kept within 0x15E0 of the middle.
        if self.pair_at > 0 {
            let frozen = self.look.is_some();
            for m in [self.pair.0, self.pair.1] {
                let c = &mut cr.slots[m];
                if frozen {
                    c.vel[0] = 0;
                    c.vel[2] = 0;
                }
                let (x, z) = (c.pos[0] >> 5, c.pos[2] >> 5);
                if x * x + z * z > 0x15E0 * 0x15E0 {
                    let a = crate::boxes::angle12(x as f32, z as f32);
                    c.pos[0] = sin_table(a) * 0x15E0 >> 9;
                    c.pos[2] = sin_table(a + 0x400) * 0x15E0 >> 9;
                }
            }
        }
        let at = cr.slots[BOSS].pos;
        // The flails: round the boss, turning; their tips knock Buzz back.
        let mut tips = [[0; 3]; 7];
        let mut a = self.turn;
        for (k, i) in FLAILS.enumerate() {
            let f = &mut cr.slots[i];
            f.pos[0] = at[0];
            f.pos[2] = at[2];
            f.angles[1] = a;
            f.target_facing = a;
            a = (a + 0x2AB) & 0xFFF;
            let s = (sin_table(f.angles[1] + 0x800), sin_table(f.angles[1] + 0xC00));
            let tip = [f.pos[0] + (0x5AA * s.0 >> 14), f.pos[1] + 0x96, f.pos[2] + (0x5AA * s.1 >> 14)];
            tips[k + 1] = tip;
            if f.health == 1 && f.id != 0 && near(buzz, tip, 0x4B) {
                cr.hurts.push((crate::boxes::angle12((buzz[0] - tip[0]) as f32, (buzz[2] - tip[2]) as f32), 1));
            }
        }
        // Everything bobs on the base height.
        let y = (sin_table(self.bob) >> 3) + self.base;
        for i in 0..=6 {
            cr.slots[i].pos[1] = y;
            cr.slots[i].target[1] = y;
        }
        self.turn = (self.turn + 4) & 0xFFF;
        self.bob = (self.bob + 0x20) & 0xFFF;
        self.phase_anim = (self.phase_anim + 0x4000) % 0x180000;
        self.blink = !self.blink;
        // A hit.
        let c = &mut cr.slots[BOSS];
        if c.health != 0 && c.health != self.health && self.phase < 4 {
            self.common.sounds.push((0x83, c.pos));
            if self.round < 6 {
                c.health = 0x1A - self.round as i32;
            }
            if self.pair_at < 12 {
                self.pair = PAIRS[self.pair_at / 2];
                self.pair_at += 2;
            }
            self.health = c.health;
            c.rec.state = 4;
            if self.phase == 2 {
                c.rec.speed = 0;
                self.hurt = 600;
                self.look_at(at, buzz, 0x168, 0x10);
            } else {
                self.hurt = 0x78;
            }
            if c.health < 0xB {
                self.look_at(at, buzz, 0xF0, 0x10);
                if let Some(l) = self.look.as_mut() {
                    l.from[1] -= 0x8000;
                }
                cr.slots[BOSS].rec.speed = 0;
                self.cool = 10000;
                self.phase = 4;
                self.hurt = 0;
            }
        }
        let c = &mut cr.slots[BOSS];
        self.hurt -= 1;
        if self.hurt < 1 {
            self.hurt = 0;
            c.rec.state = 7;
            c.lit = false;
            if self.phase == 2 && self.round == 7 {
                self.phase = 3;
                c.rec.turn = 2;
                c.rec.speed = 0x14;
            }
        } else {
            c.lit = self.blink;
        }
        // The arrival.
        if buzz[2] > -0x1BB58 && self.phase == 0 {
            self.phase = 1;
            self.base = -0x38000;
            self.look_at(at, buzz, 0x168, 0x10);
            if let Some(l) = self.look.as_mut() {
                l.from = [buzz[0], buzz[1] - 0x8000, buzz[2]];
            }
        }
        if self.phase == 1 {
            if self.taunt > 0 {
                self.taunt -= 1;
                if self.taunt == 0 {
                    self.voices.push(0xD5);
                }
            }
            if let Some(l) = self.look.as_mut() {
                l.at = at;
                l.from[1] -= 0x80;
            }
            if self.base < -0x8000 {
                self.base = (self.base + 0x280).min(-0x8000);
            }
            if self.look_ticks() == 0 {
                self.phase = 2;
                cr.slots[BOSS].rec.speed = 0x10;
            }
        }
        // Calling up the round's minions.
        let (p0, p1) = self.pair;
        if self.phase == 2 && self.hurt != 0 {
            // Held at 600 (out of reach) until the pair are beaten.
            if self.hurt > 0x1E0 {
                self.hurt = 600;
            }
            if self.look_ticks() == 0 {
                if !self.pair_fixed {
                    if self.hurt >= 0x1A4 {
                        self.pair_fixed = true;
                        cr.slots[p0].respawn = 10000;
                        cr.slots[p1].respawn = 10000;
                    }
                }
                if self.pair_fixed && cr.slots[p0].health <= 0 && cr.slots[p1].health <= 0 {
                    self.hurt = 0x78;
                    self.pair_fixed = false;
                    cr.slots[BOSS].rec.speed = 0x10;
                    self.round += 1;
                    self.called = false;
                    self.burst = false;
                }
            } else {
                self.cool = 200;
                let flail = self.round.min(6);
                let tip = tips[flail];
                if let Some(l) = self.look.as_mut() {
                    l.at = tip;
                    let f = cr.slots[flail].angles[1];
                    l.from = [tip[0] + sin_table(f + 0x400) * 3, tip[1], tip[2] + sin_table(f - 0x800) * 3];
                }
                if self.look_ticks() < 300 && !self.called {
                    self.called = true;
                    for m in [p0, p1] {
                        cr.restart(m);
                        let c = &mut cr.slots[m];
                        c.scale = 1;
                    }
                    let at = cr.slots[p0].pos;
                    self.common.sounds.push((0x86, at));
                }
                if cr.slots[flail].health == 1 {
                    for m in [p0, p1] {
                        let c = &mut cr.slots[m];
                        c.pos = [tip[0], tip[1] + 0x1000, tip[2]];
                        c.vel = [0; 3];
                        if c.id != 0xE {
                            c.timer = 0x1CC;
                        }
                    }
                    if self.look_ticks() < 0x3C && !self.burst {
                        self.burst = true;
                        for _ in 0..4 {
                            let spin = self.common.rand() - 0x80;
                            self.common.spawns.push((tip, 0x23, 0xE, spin, None));
                        }
                        self.common.sounds.push((0x85, tip));
                        self.voices.push(0xD6);
                        cr.kill_slot(flail);
                    }
                }
            }
        }
        // The minions grow in.
        for m in 7..=11 {
            let c = &mut cr.slots[m];
            if c.scale < 0x1000 {
                c.scale = (c.scale + 0xC).min(0x1000);
            }
        }
        // Height: up to Buzz when he's close in the chase, else back to −0x8000.
        if self.phase == 3 && near(buzz, at, 200) {
            if self.base < buzz[1] {
                self.base = (self.base + 0x80).min(buzz[1]);
            }
            self.cool = (self.cool - 1).max(0);
        } else {
            if self.base > -0x8000 {
                self.base = (self.base - 0x80).max(-0x8000);
            }
            match self.phase {
                4 => {
                    if let Some(l) = self.look.as_mut() {
                        l.from[1] += 0x80;
                    }
                    if self.look_ticks() < 0x78 {
                        cr.kill_slot(BOSS);
                        self.phase = 5;
                    }
                    self.cool = (self.cool - 1).max(0);
                }
                5 => {
                    if self.look_ticks() == 0 && !self.done {
                        self.done = true;
                        self.phase = 6;
                    }
                }
                2 | 3 => self.cool = (self.cool - 1).max(0),
                _ => {}
            }
        }
        self.ouch = (self.ouch - 1).max(0);
        // The beam.
        if self.cool == 0 && self.phase >= 2 && self.phase < 4 && near(buzz, at, 400) {
            let c = &cr.slots[BOSS];
            let f = c.angles[1] + 0x800;
            let from = [at[0] + (-400 * sin_table(f) >> 14), at[1], at[2] + (-400 * sin_table(f + 0x400) >> 14)];
            let to_buzz = crate::boxes::angle12((buzz[0] - from[0]) as f32, (buzz[2] - from[2]) as f32);
            let u = beam_heading(c.angles[1], to_buzz);
            let delta = [sin_table(u) << 2, 96000, sin_table(u + 0x400) << 2];
            let o = glam::Vec3::from(from.map(|v| v as f32 / 32.0));
            let dv = glam::Vec3::from(delta.map(|v| v as f32 / 32.0));
            let t = world.sweep(o, dv, 1.0).map_or(1.0, |h| h.t);
            let hit = [0, 1, 2].map(|k| from[k] + (delta[k] as f32 * t) as i32);
            cr.ribbons.push((from, hit, [0, 0x80, 0], 0x40));
            self.common.sounds.push((0x87, hit));
            if self.every(2) {
                self.common.spawns.push((hit, 4, 4, 0, None));
            }
            if self.every(4) {
                self.common.spawns.push((hit, 0x46, 2, 0, None));
            }
            if near(buzz, hit, 0x1E) {
                if player.hurt_timer == 0 {
                    self.common.sounds.push((0x84, at));
                    if self.ouch == 0 {
                        self.ouch = 0x4B0;
                    }
                    self.voices.push(0xD7);
                }
                cr.hurts.push((crate::boxes::angle12((buzz[0] - hit[0]) as f32, (buzz[2] - hit[2]) as f32), 3));
            }
        }
        // The camera: the nearer live minion, else the boss.
        if self.phase == 2 || self.phase == 3 {
            let alive = |i: usize| cr.slots[i].health > 0 && cr.slots[i].id != 0;
            let dist = |i: usize| {
                let d = [(buzz[0] - cr.slots[i].pos[0]) >> 8, (buzz[2] - cr.slots[i].pos[2]) >> 8];
                d[0] * d[0] + d[1] * d[1]
            };
            let pick = [p0, p1].into_iter().filter(|&i| alive(i)).min_by_key(|&i| dist(i));
            cr.boss.focus = Some(match pick {
                Some(i) => [cr.slots[i].pos[0], cr.slots[i].pos[1] - 0x2000, cr.slots[i].pos[2]],
                None => [at[0], at[1] - 0x4000, at[2]],
            });
            self.boss_hud = true;
        } else {
            self.boss_hud = false;
        }
        self.boss_bar = Some(((cr.slots[BOSS].health - 10) * 0x36 / 16).max(0));
        self.tick_look();
    }
}
