//! Level 6, a boss arena (start-up 0x41FFB0, frame script 0x420060). The boss is creature slot
//! 0 (type 48, 20 health, hovering), moved 0x80000 along x at the start, facing 0xC00.
//!
//! - Buzz crossing x 0x31F9E: a 300-tick look-at scene while the boss flies in (x −0x800 a
//!   tick, banking), then the fight.
//! - Fighting: a 600-tick cycle. For its first 300 it hangs over Buzz (y cos(lift) × 3 −
//!   0x10B13 above home), from 0x226 sucking (sound 0x65 at its target, a taunt 0xD0 every 400
//!   ticks, dust 0x7B / 0x7C under its sides every 6 ticks) and dropping things (particle 0xC)
//!   on Buzz every 64 ticks when he's near; for the rest it flies off to one side
//!   (±0x40000, swapping each cycle). The camera turns to it and its bar shows.
//! - A hit: 120 ticks of a look-at scene while it can't be hurt (sound 0x66 near the end); at
//!   10 health or below it dies (300-tick scene, sinking, fireballs) and the level ends.
//! - Outside the arena (x −0x390AF…0x390AF, z −0x21F9E…0x21F9E) its lift phase rises.
//!
//! Not ported: the music change, the camera's drift during the scenes.

use crate::boxes::Boxes;
use crate::creatures::{Creatures, ParticleRequest};
use crate::game::{Player, World};
use crate::script::{near, sin_table, Common, Script};
use crate::specials::Specials;

const BOSS: usize = 0;

pub struct Level6 {
    /// 0x52F9A4: 0 waiting, 1 flying in, 2 fighting, 3 dying, 4 over.
    phase: i32,
    /// 0x52F98C: health last seen; 0x52F990 the after-hit count; 0x52F9A0 the cycle.
    health: i32,
    hurt: i32,
    cycle: i32,
    /// 0x52F9B8: the side it flies to; 0x52F994 its lift phase; 0x52F998 taunt timer;
    /// 0x52F9B0 voice timer and 0x52F9B4 the camera yaw it last saw.
    side: i32,
    lift: i32,
    taunt: i32,
    voice: i32,
    yaw: i32,
    started: bool,
    common: Common,
}

impl Level6 {
    pub fn new() -> Level6 {
        Level6 { phase: 0, health: 20, hurt: 0, cycle: 0x12C, side: 0x40000, lift: 0, taunt: 0x96, voice: 0, yaw: 0, started: false, common: Common::default() }
    }
}

impl Default for Level6 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for Level6 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}

impl std::ops::DerefMut for Level6 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Script for Level6 {
    fn step(&mut self, _world: &World, _specials: &mut Specials, _player: &mut Player, _boxes: &Boxes) {}

    fn step_creatures(&mut self, _world: &World, _specials: &mut Specials, player: &Player, cr: &mut Creatures, _coins: i32) {
        self.clock = self.clock.wrapping_add(1);
        let Some(c0) = cr.slots.get_mut(BOSS) else { return };
        if !std::mem::replace(&mut self.started, true) {
            c0.flags |= 0x800;
            c0.pos[0] += 0x80000;
            c0.angles[1] = 0xC00;
            self.health = c0.health;
        }
        let buzz = player.pos_obj();
        let c = &mut cr.slots[BOSS];
        // A hit.
        if c.health != 0 && c.health != self.health {
            self.health = c.health;
            c.rec.state = 4;
            self.cycle = 299;
            let at = c.pos;
            if c.health < 0xB {
                self.hurt = 300;
                self.look_at(at, buzz, 300, 0x10);
                self.phase = 3;
            } else {
                self.hurt = 0x78;
                self.look_at(at, buzz, 0x78, 0x10);
            }
        }
        let c = &mut cr.slots[BOSS];
        self.hurt -= 1;
        if self.hurt < 0 {
            self.hurt = 0;
            c.rec.state = 7;
            c.lit = false;
        } else if c.id != 0 {
            if let Some(l) = self.look.as_mut() {
                l.at = c.pos;
            }
            if self.hurt < 0x5A {
                self.common.sounds.push((0x66, c.pos));
            }
            // Lit (+0x34, light 0x2000) on alternate ticks (0x52F9A8).
            c.lit = self.clock & 1 != 0;
            c.light = [0x2000; 3];
        }
        // Its lift: rising while it's outside the arena.
        let inside = c.pos[0] > -0x390AF && c.pos[0] < 0x390AF && c.pos[2] > -0x21F9E && c.pos[2] < 0x21F9E;
        self.lift = if inside { (self.lift - 0x80).max(0) } else { (self.lift + 0x80).min(0x800) };
        if buzz[0] < 0x31F9E && self.phase == 0 {
            self.phase = 1;
            let at = c.pos;
            self.look_at(at, buzz, 300, 0x10);
            if let Some(l) = self.look.as_mut() {
                l.from = [at[0] - 0x48000, at[1] - 0x8000, at[2]];
            }
        }
        if self.phase == 1 || self.phase == 2 {
            let c = &cr.slots[BOSS];
            if self.hurt == 0 {
                self.common.sounds.push((0x61, c.pos));
            }
            let yaw = player.facing;
            if self.voice == 0 && near(buzz, c.pos, 400) {
                let d = ((self.yaw - yaw) & 0xFFF) - if (self.yaw - yaw) & 0xFFF > 0x800 { 0x1000 } else { 0 };
                if d.abs() > 0x40 {
                    let r = self.rand();
                    self.voices.push(0x62 + (r & 1) as u16);
                    self.voice = 100;
                }
            } else {
                self.voice = (self.voice - 1).max(0);
            }
            self.yaw = yaw;
        }
        if self.phase == 1 {
            let c = &mut cr.slots[BOSS];
            if self.look_ticks() < 0x1E {
                c.flags |= 4;
            }
            let i = (self.look_ticks() - 0x1E).max(0);
            c.angles[2] = ((sin_table(i * 0x14 + 0x400) as u16 as i32 >> 3) - 0x800) & 0xFFF;
            c.pos[0] -= 0x800;
            if self.look_ticks() == 0 {
                self.phase = 2;
            }
        }
        if self.phase == 2 {
            self.cycle -= 1;
            if self.cycle < 1 {
                self.cycle = 600;
                self.side = -self.side;
            }
            let c = &cr.slots[BOSS];
            cr.boss.focus = Some(c.pos);
            self.boss_hud = true;
            if self.every(64) && self.hurt == 0 && near(buzz, c.pos, 400) {
                let (v, p) = (c.vel, c.pos);
                cr.particles.push(ParticleRequest { pos: p, vel: [v[0] * 2, 0, v[2] * 2], gravity: 0x80, angle: 0, spin: 0, typ: 0xC });
            }
            let lift = sin_table(self.lift + 0x400) * 3;
            let c = &mut cr.slots[BOSS];
            if self.cycle < 0x12D {
                c.target = [self.side, lift + 0x4000 + c.home[1], 0];
            } else {
                c.target = [buzz[0], lift - 0x10B13, buzz[2]];
                if self.cycle < 0x226 {
                    let t = c.target;
                    self.common.sounds.push((0x65, t));
                    self.taunt -= 1;
                    if self.taunt < 0 {
                        self.taunt = 400;
                        self.voices.push(0xD0);
                    }
                    if self.every(6) {
                        let f = c.angles[1];
                        for a in [-0x80, 0x80] {
                            let at = [c.pos[0] + sin_table(f + a) * 4, c.pos[1], c.pos[2] + sin_table(f + a + 0x400) * 4];
                            self.common.spawns.push((at, 0x7B, 2, 0, None));
                            let r = self.common.rand();
                            self.common.spawns.push((at, 0x7C, 1, r, None));
                        }
                    }
                }
            }
        }
        if self.phase == 3 {
            let c = &mut cr.slots[BOSS];
            c.target[1] -= 0x280;
            c.pos[1] -= 0x280;
            if let Some(l) = self.look.as_mut() {
                l.from[1] -= 0x80;
            }
            if c.health < 0xB && self.look_ticks() < 0x78 && c.id != 0 && c.hurt == 0 {
                cr.kill_slot(BOSS);
            }
            if self.look_ticks() == 0 {
                self.done = true;
                self.phase = 4;
            }
        }
        // Its lean (flag 4, from the end of the fly-in): easing towards half its turn while
        // it can be hurt; after a hit spinning 0x40 a tick, and once beaten tumbling too.
        let c = &mut cr.slots[BOSS];
        if c.flags & 4 != 0 {
            if self.hurt == 0 {
                let mut d = (c.angles[1] - c.target_facing) & 0xFFF;
                if d > 0x800 {
                    d -= 0x1000;
                }
                let z = c.angles[2] as i16 as i32;
                c.angles[2] = (z - (((z - d / 2) >> 5) as i16 as i32)) as i16 as i32;
            } else {
                c.angles[2] = (c.angles[2] + 0x40) & 0xFFF;
                if self.phase == 3 {
                    c.angles[0] = (c.angles[0] + 0x24) & 0xFFF;
                }
            }
        }
        // Exhaust from its sides; fireballs once it's beaten.
        let c = &cr.slots[BOSS];
        if c.active && c.id != 0 {
            if self.every(2) {
                for x in [500, -500] {
                    let f = c.angles[1] + 0x800;
                    let at = [c.pos[0] + (x * sin_table(f + 0x400) >> 14), c.pos[1] - 400, c.pos[2] - (x * sin_table(f) >> 14)];
                    let spin = (self.common.rand() & 0x1F) - 0x10;
                    self.common.spawns.push((at, 0x11, 2, spin, None));
                }
            }
            if c.health < 0xB && self.every(7) {
                let spin = self.common.rand() - 0x80;
                self.common.spawns.push((c.pos, 0x23, 0xE, spin, None));
            }
        }
        self.boss_bar = Some(((cr.slots[BOSS].health - 10) * 0x36 / 10).max(0));
        self.tick_look();
    }
}
