//! Zurg arena: setup 0x42B300 and frame routine 0x42B3A0.
//! Ports entrance, attack timing, hit recovery, ring exclusion, death and completion.
//! Still unverified visually. Lighting and the nearest-light marker are omitted.
//! Projectile origin follows animated node 1 through the same transform as the renderer
//! (0x43C1C0). The viewer supplies the original animations to Creatures.
use crate::{
    boxes::Boxes,
    creatures::{Creatures, ParticleRequest},
    game::{Player, World},
    script::{Common, Script, sin_table},
    specials::Specials,
};

pub struct Level12 {
    common: Common,
    phase: u8,
    started: bool,
    health: i32,
    hurt: i32,
    cycle: i32,
    retaliating: bool,
    taunt: i32,
    intro_voice: i32,
    fall_speed: i32,
}
impl Default for Level12 {
    fn default() -> Self {
        Self {
            common: Common::default(),
            phase: 0,
            started: false,
            health: 29,
            hurt: 0,
            cycle: 0x104,
            retaliating: false,
            taunt: 300,
            intro_voice: 0x78,
            fall_speed: 0,
        }
    }
}
impl std::ops::Deref for Level12 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}
impl std::ops::DerefMut for Level12 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Script for Level12 {
    fn step(&mut self, _: &World, _: &mut Specials, player: &mut Player, _: &Boxes) {
        // During the two scenes the original keeps Buzz on the arena's safe ledge.
        if matches!(self.phase, 1 | 3) && player.pos_obj()[1] > -70000 {
            player.pos = glam::Vec3::from([
                -0x1da7c as f32 / 32.0,
                -0x12bd0 as f32 / 32.0,
                0xf699 as f32 / 32.0,
            ]);
        }
    }

    fn step_creatures(
        &mut self,
        _: &World,
        _: &mut Specials,
        player: &Player,
        cr: &mut Creatures,
        _: i32,
    ) {
        if cr.slots.is_empty() {
            return;
        }
        self.clock = self.clock.wrapping_add(1);
        if !std::mem::replace(&mut self.started, true) {
            let c = &mut cr.slots[0];
            self.health = c.health;
            c.flags |= 0x800;
            c.pos[1] -= 0x28000;
            c.pos[2] += 0x10000;
            c.home[1] = c.pos[1];
            c.angles[1] = 0xc00;
            c.rec.turn = 0;
        }
        self.sounds.push((0x67, [0; 3]));
        if self.phase > 1 {
            self.hurt -= 1;
            let c = &mut cr.slots[0];
            if self.hurt < 0 {
                self.hurt = 0;
                c.rec.state = 6;
                c.lit = false;
            } else {
                c.lit = self.clock & 1 != 0;
            }
        }
        let buzz = player.pos_obj();
        if self.phase == 0 && buzz[0] >= -0x18b2a {
            self.phase = 1;
            let at = cr.slots[0].pos;
            self.look_at(at, buzz, 300, 0x10);
            if let Some(l) = self.look.as_mut() {
                l.at[1] -= 0x4000;
                l.from = [at[0] - 0x3000, at[1] - 0x5000, at[2]];
            }
            self.voices.push(0xd8);
        }
        if self.phase == 1 {
            if self.intro_voice > 0 {
                self.intro_voice -= 1;
                if self.intro_voice == 0 {
                    self.voices.push(0xb9);
                }
            }
            let ticks = self.look_ticks();
            let c = &mut cr.slots[0];
            if ticks == 0 {
                c.flags |= 0xc;
                c.rec.turn = 10;
                c.rec.state = 6;
                self.phase = 2;
            } else {
                c.pos[1] += 0x200;
                c.home[1] = c.pos[1];
                if let Some(l) = self.look.as_mut() {
                    l.from[1] += 0x220;
                    l.from[0] -= 0x20 + if ticks < 0xb4 { 0x150 } else { 0 };
                    l.at[1] = c.pos[1] - 0x4000;
                }
            }
        }
        if self.phase == 2 {
            self.boss_hud = true;
            cr.boss.focus = Some(cr.slots[0].pos);
            self.taunt -= 1;
            if self.taunt < 0 {
                self.taunt = self.rand() * 2 + 400;
                let voice = 0xba + (self.rand() & 3) as u16;
                self.voices.push(voice);
            }
            if cr.slots[0].health != self.health {
                if self.rand() & 3 == 0 {
                    self.voices.push(0xd9);
                }
                self.health = cr.slots[0].health;
                self.hurt = 60;
                cr.slots[0].rec.state = 4;
                if self.health < 10 {
                    self.phase = 3;
                    let c = &mut cr.slots[0];
                    c.flags &= !0xc;
                    c.rec.decel = [0x20; 2];
                    let at = c.pos;
                    let yaw = c.angles[1];
                    self.look_at(at, buzz, 300, 0x10);
                    if let Some(l) = self.look.as_mut() {
                        l.at[1] -= 0x4000;
                        l.from = [
                            at[0] + sin_table(yaw),
                            at[1] - 0x4000,
                            at[2] + sin_table(yaw + 0x400),
                        ];
                    }
                    self.cycle = 30;
                    cr.animate_slot(0, 2, 0x19, [-64; 2]);
                    self.voices.push(0xc0);
                } else {
                    let voice = 0xbe + (self.rand() & 1) as u16;
                    self.voices.push(voice);
                    self.cycle = 0;
                    self.retaliating = true;
                    cr.animate_slot(0, 1, 2, [-32; 2]);
                }
            }
            let previous = self.cycle;
            self.cycle -= 1;
            if self.cycle < 0 {
                self.cycle = 302;
                let direction =
                    cr.slots[0].angles[1] + if self.rand() < 0x80 { -0x200 } else { 0x200 };
                let c = &mut cr.slots[0];
                c.vel[0] = sin_table(direction) >> 3;
                c.vel[2] = sin_table(direction + 0x400) >> 3;
            } else {
                if previous > 200 && self.cycle <= 200 {
                    cr.animate_slot(0, 0, 0x18, [-64; 2]);
                }
                let shot = match self.cycle {
                    146 | 126 => 1,
                    138 => 2,
                    _ => 0,
                };
                if shot != 0 {
                    let c = &cr.slots[0];
                    let yaw = c.angles[1];
                    let pos = cr.node_point(0, 1, [-250.0, -250.0, 0.0]).unwrap_or(c.pos);
                    let lob = !self.retaliating && (c.health > 19 || shot != 2);
                    let (vel, gravity, typ) = if lob {
                        (
                            [
                                sin_table(yaw - 0x80) >> 2,
                                0x400,
                                sin_table(yaw + 0x400) >> 2,
                            ],
                            0x100,
                            0x6c,
                        )
                    } else {
                        ([0, -2, 0], yaw << 2, 0x6d)
                    };
                    self.particles.push(ParticleRequest {
                        pos,
                        vel,
                        gravity,
                        angle: 0,
                        spin: 0,
                        typ,
                    });
                    self.sounds.push((0x9d, c.pos));
                }
                if previous > 104 && self.cycle <= 104 {
                    self.retaliating = false;
                    cr.animate_slot(0, 1, 2, [-32; 2]);
                }
            }
        }
        if self.phase == 3 {
            let c = &mut cr.slots[0];
            c.target_facing =
                crate::boxes::angle12((-0xbd7c - c.pos[0]) as f32, (0x99 - c.pos[2]) as f32);
            let inside = c.pos[0] > -0x256d6
                && c.pos[0] < 0xe0aa
                && c.pos[2] > -0x1b3e9
                && c.pos[2] < 0x1b297;
            if inside {
                c.vel[0] = -sin_table(c.target_facing) >> 4;
                c.vel[2] = -sin_table(c.target_facing + 0x400) >> 4;
            } else {
                c.rec.turn = 0;
                c.angles[1] = (c.angles[1] + 16) & 0xfff;
                if c.pos[1] < 170000 {
                    c.pos[1] += self.fall_speed;
                    if c.pos[1] >= 170000 {
                        self.sounds.push((0x9f, [0; 3]));
                    }
                    self.fall_speed += 0x40;
                    c.home[1] = c.pos[1];
                }
                if c.pos[1] > 70000 {
                    self.sounds.push((0x9e, [0; 3]));
                    if self.every(4) && self.look_ticks() > 50 {
                        let angle = self.rand() << 4;
                        self.particles.push(ParticleRequest {
                            pos: [c.pos[0], 80000, c.pos[2]],
                            vel: [0, -0x2400, 0],
                            gravity: 0,
                            angle,
                            spin: 0x80,
                            typ: 0x70,
                        });
                    }
                }
            }
            let divisor = (self.fall_speed / 64).clamp(1, 8);
            if let Some(l) = self.look.as_mut() {
                l.at = [c.pos[0], c.pos[1] - 0x4000, c.pos[2]];
                l.from = [
                    c.pos[0] + sin_table(c.angles[1]) / divisor,
                    -0x188e0,
                    c.pos[2] + sin_table(c.angles[1] + 0x400) / divisor,
                ];
            }
            if self.look_ticks() == 0 {
                self.done = true;
                self.phase = 4;
            }
        }
        self.boss_bar = Some(((cr.slots[0].health - 9) * 54 / 20).clamp(0, 54));
        let c = &mut cr.slots[0];
        let dx = (-0xbd7c - c.pos[0]) >> 5;
        let dz = (0x99 - c.pos[2]) >> 5;
        if (dx as i64 * dx as i64 + dz as i64 * dz as i64) < 0x190000 {
            let angle = crate::boxes::angle12(dx as f32, dz as f32);
            c.pos[0] = -0xbd7c - (sin_table(angle) * 0x500 >> 9);
            c.pos[2] = 0x99 - (sin_table(angle + 0x400) * 0x500 >> 9);
        }
        self.tick_look();
    }
}
