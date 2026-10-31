//! Level 3, the slime boss (start-up 0x41A8A0, frame script 0x41AA10). The slime is creature
//! slot 0 (SLIME, 99 health: hits never kill it, the script puts its health back).
//!
//! - Buzz coming within 0x17C (× 256, flat) of it: a look-at scene (300 ticks) on it, which
//!   grows it to full size (sound 0x46, jingle −4); then its fight script (behaviour + 0x12).
//! - Fighting: a taunt (voice 0xD1) every r + 0x708 ticks; near it (0x32 × 256) Buzz is hurt
//!   (mode 3). Each hit flashes it and shrinks it by 0x300; shrunk to 0x200 it has lost a
//!   round: the next is 0x800 bigger (up to 0x3800, four rounds), its blobs pop, it can't be
//!   hurt while it regrows during a longer look-at scene (from 180 ticks, + 40 each round),
//!   and from the third round a pickup appears. At the last it dies (look-at 0x1A4 ticks),
//!   special 0 drops and bounces (sound 0x45) and the level ends.
//! - Its size drives its model's scale and its hit ellipsoid (offset −12000 − size, radius
//!   size / 16); its script's timer fires blobs (particle 0x3F, sound 0xD). The bar shows
//!   the rounds left.
//!
//! Not ported: the light and fog effects, the dripping, the camera shake.

use crate::boxes::Boxes;
use crate::creatures::{Creatures, ParticleRequest};
use crate::game::{Player, World};
use crate::script::{sin_table, Common, Script};
use crate::specials::Specials;

const SLIME: usize = 0;

pub struct Level3 {
    /// 0x52F754: 0 waiting, 1 meeting, 999 fighting, 1000 dying, 0x3E9… over.
    phase: i32,
    /// 0x52F704 size, 0x52F75C its target, 0x52F6F0 this round's full size.
    size: i32,
    target: i32,
    full: i32,
    /// 0x52F730: its health, put back after each hit.
    health: i32,
    /// 0x52F758: the regrowing scene's length; 0x52F71C the taunt timer; 0x52F710 flash.
    regrow: i32,
    taunt: i32,
    flash: i32,
    /// The scene's ticks last frame (0x52F70C), for the crossings.
    last_look: i32,
    /// Special 0's drop after death: height and speed (0x52F6F4 / 0x52F6FC).
    drop: (i32, i32),
    /// The lid's wobble phase (0x52F6F8, 13 bits).
    wobble: i32,
    /// Set the slime up on the first frame (0x41A8A0 changes its slot).
    started: bool,
    common: Common,
}

impl Level3 {
    pub fn new(specials: &mut Specials) -> Level3 {
        specials.set_scale(0x32, [0; 3]);
        specials.set_scale(0, [0x1388; 3]);
        Level3 {
            phase: 0,
            size: 1,
            target: 0x1000,
            full: 0x1000,
            health: 99,
            regrow: 0xB4,
            taunt: 0xB4,
            flash: 0,
            last_look: 0,
            drop: (0, 0),
            wobble: 0,
            started: false,
            common: Common::default(),
        }
    }

    fn start(&mut self, specials: &mut Specials, cr: &mut Creatures) {
        let Some(c) = cr.slots.get_mut(SLIME) else { return };
        c.flags |= 0x800;
        c.angles[1] = 0x800;
        c.target_facing = 0x800;
        c.pos[2] += 0x10000;
        self.health = c.health;
        // The pickup that appears in the third round waits hidden.
        self.pickups.push((0, false));
        specials.set_pos(0, [c.pos[0] >> 5, (c.pos[1] - 0x2AF8) >> 5, c.pos[2] >> 5]);
        self.drop.0 = c.pos[1] - 0x2AF8;
    }
}

impl std::ops::Deref for Level3 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}

impl std::ops::DerefMut for Level3 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Script for Level3 {
    fn step(&mut self, _world: &World, _specials: &mut Specials, _player: &mut Player, _boxes: &Boxes) {}

    fn step_creatures(&mut self, _world: &World, specials: &mut Specials, player: &Player, cr: &mut Creatures, _coins: i32) {
        if !std::mem::replace(&mut self.started, true) {
            self.start(specials, cr);
        }
        self.clock = self.clock.wrapping_add(1);
        if cr.slots.get(SLIME).is_none_or(|c| c.id != 0x10) && self.phase < 1000 {
            return;
        }
        let buzz = player.pos_obj();
        let s = cr.slots[SLIME].pos;
        let look = self.look_ticks();
        let last = self.last_look;
        let crossed = |t: i32| look < t && last >= t;
        if self.phase == 0 {
            let (dx, dz) = ((buzz[0] - s[0]) >> 8, (buzz[2] - s[2]) >> 8);
            if dx * dx + dz * dz < 0x17C * 0x17C {
                self.phase = 1;
                self.look_at([s[0], s[1] - 11000, s[2]], buzz, 300, 0x10);
            }
        } else if self.phase == 1 {
            if crossed(0xF0) {
                self.sounds.push((0x46, s));
            }
            if look < 0x96 {
                if crossed(0x96) {
                    cr.boss.jingles.push((-4, s));
                }
                self.size -= (self.size - self.target) / 32;
            }
            if look == 0 {
                cr.slots[SLIME].set_script(0x12 / 2);
                self.phase = 999;
            }
        } else if self.phase == 999 {
            self.taunt -= 1;
            if self.taunt < 1 {
                self.taunt = self.rand() + 0x708;
                self.voices.push(0xD1);
            }
            let d = [0, 1, 2].map(|k| (buzz[k] - s[k]) >> 8);
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < 0x32 * 0x32 {
                cr.hurts.push((crate::boxes::angle12((buzz[0] - s[0]) as f32, (buzz[2] - s[2]) as f32), 3));
            }
            let c = &mut cr.slots[SLIME];
            if c.health != self.health {
                self.flash = 4;
                self.target -= 0x300;
                c.health = self.health;
                if self.target < 0x200 {
                    self.target = 0x200;
                    self.full += 0x800;
                    self.pop.push(0x3F);
                    if self.full > 0x2FFF {
                        specials.set_scale(0x32, [0x1000; 3]);
                        self.pickups.push((0, true));
                    }
                    let c = &mut cr.slots[SLIME];
                    if self.full > 0x37FF {
                        c.set_script(0xBC / 2);
                        self.full = 0x3800;
                        self.size = 1;
                        self.target = 1;
                        self.phase = 1000;
                        self.look_at([s[0], s[1] - 11000, s[2]], buzz, 0x1A4, 0x10);
                        c.flags &= !0x180;
                        self.drop = (s[1] - 1000, 0);
                    } else {
                        c.rec.state = 4;
                        let ticks = self.regrow;
                        self.look_at([s[0], s[1] - 14000, s[2]], buzz, ticks, 0x10);
                        c.set_script(0x9E / 2);
                        c.vel = [0; 3];
                        self.regrow += 0x28;
                    }
                }
            }
            let c = &mut cr.slots[SLIME];
            if self.phase == 999 {
                if self.target < self.full {
                    if c.rec.state == 4 {
                        if look < 0x78 {
                            if crossed(0x78) {
                                cr.boss.jingles.push((-4, s));
                            }
                            self.target += (self.full - self.target) / 32;
                        } else {
                            c.wait = 5;
                        }
                    } else {
                        self.target += 0x20;
                    }
                    self.target = self.target.min(self.full);
                }
                self.size -= (self.size - self.target) / 8;
                // Its script's timer: 1 fires a blob, 3 shakes the camera.
                let c = &mut cr.slots[SLIME];
                if c.timer == 1 {
                    cr.particles.push(ParticleRequest { pos: [c.pos[0], c.pos[1] - 10000, c.pos[2]], vel: [0, -2, 0], gravity: 0, angle: c.angles[1] << 2, spin: 0x80, typ: 0x3F });
                    cr.sounds.push((0xD, c.pos));
                }
                c.timer = 0;
            }
        } else if self.phase == 1000 {
            if look < 0xB4 {
                self.phase = 0x3E9;
                self.drop.1 = -0xC00;
                cr.kill_slot(SLIME);
            }
        }
        if self.phase <= 1000 {
            // Special 0, the lid, rides on the slime's head: node 10 of its animated model,
            // 700 above, wobbling while it is big enough (0x4CCC70 / 0x43C1C0 / 0x4CCE30).
            self.wobble = (self.wobble + 0x80) & 0x1FFF;
            if self.size < 0x400 {
                self.wobble = 0;
            }
            let facing = cr.slots.get(SLIME).map_or(0, |c| c.angles[1]);
            specials.turn_from_own(0, [sin_table(self.wobble) >> 8, facing, sin_table(self.wobble >> 1) >> 8]);
            let head = cr.node_point(SLIME, 10, [0.0, -700.0, 0.0]).unwrap_or([s[0], s[1] - 0x2AF8, s[2]]);
            specials.set_pos(0, [head[0] >> 5, (head[1] >> 5) - 0x20, head[2] >> 5]);
        } else {
            // Special 0 drops and bounces.
            self.drop.1 += 0x60;
            self.drop.0 += self.drop.1;
            let floor = s[1] - 2000;
            if self.drop.0 > floor {
                self.drop.0 = floor;
                self.drop.1 = -(self.drop.1 >> 1);
                if self.drop.1.abs() > 0x80 {
                    self.sounds.push((0x45, s));
                }
            }
            specials.set_pos(0, [s[0] >> 5, self.drop.0 >> 5, s[2] >> 5]);
        }
        if self.phase == 0x3E9 && look == 0 {
            self.phase = 0x3EA;
        } else if self.phase >= 0x3EA && !self.done {
            self.phase += 1;
            if self.phase > 0x406 {
                self.done = true;
            }
        }
        // Its size: the model's scale and the hit ellipsoid.
        if self.phase < 0x3E9 && let Some(c) = cr.slots.get_mut(SLIME) {
            c.scale = self.size.max(1);
            c.hit_size = Some((-12000 - self.size, (self.size / 16).max(1)));
            c.lit = self.flash > 0;
        }
        self.flash = (self.flash - 1).max(0);
        self.boss_bar = Some(((0x3800 - self.full) >> 11) * 0x36 / 5);
        self.boss_hud = self.phase > 0x3E6;
        if self.phase > 0x3E6 {
            cr.boss.focus = Some([s[0], s[1] - 10000, s[2]]);
        }
        self.last_look = look;
        self.tick_look();
    }
}
