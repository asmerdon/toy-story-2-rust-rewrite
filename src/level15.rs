//! Prospector Showdown: 0x42FAA0 setup, 0x42FC50 frame and three boss routines
//! 0x42F310 / 0x42F530 / 0x42F7B0. Intro, simultaneous fight and rescue ending.
//! Renderer lighting and the rotating camera/arena presentation remain separate work.
use crate::{
    boxes::{Boxes, angle12},
    creatures::{Creatures, ParticleRequest},
    game::{Player, World},
    script::{Common, Script, near, sin_table},
    specials::Specials,
};

pub struct Level15 {
    common: Common,
    initialized: bool,
    intro: i32,
    phase: [u8; 3],
    health: [i32; 3],
    hurt: [i32; 3],
    shot: [i32; 3],
    taunt: i32,
    defeated: usize,
    last: usize,
    ending: u8,
    platform: [i32; 3],
    platform_v: i32,
    wobble: i32,
    wobble_kind: usize,
}
impl Level15 {
    pub fn new(specials: &Specials) -> Self {
        Self {
            common: Common::default(),
            initialized: false,
            intro: 0,
            phase: [0; 3],
            health: [29; 3],
            hurt: [0; 3],
            shot: [0; 3],
            taunt: 200,
            defeated: 0,
            last: 0,
            ending: 0,
            platform: specials.pos_obj(0).unwrap_or_default(),
            platform_v: 0,
            wobble: 0,
            wobble_kind: 0,
        }
    }
    fn setup(&mut self, cr: &mut Creatures) {
        for (k, pos) in [
            [-0x32C6D, -0x92D6, -0x14F5],
            [-0x327CC, -0x92DA, -0x45CD],
            [-0x32BBF, -0x92DB, -0x8251],
        ]
        .into_iter()
        .enumerate()
        {
            let c = &mut cr.slots[k];
            self.health[k] = c.health;
            c.pos = pos;
            c.angles[1] = 0x400;
            c.target_facing = 0x400;
            c.health = 0;
            c.respawn = 10000;
        }
        for c in cr.slots.iter_mut().take(5).skip(3) {
            c.respawn = 10000;
        }
    }
    fn platform(&mut self, specials: &mut Specials) {
        self.platform_v = (self.platform_v + 0x200).min(0xC00);
        let mut p = specials.pos_obj(0).unwrap_or(self.platform);
        p[1] = (p[1] + self.platform_v).min(self.platform[1]);
        for id in [0, 3] {
            specials.set_pos(id, p.map(|v| v >> 5));
        }
        for id in [1, 4] {
            specials.set_pos(id, p.map(|v| v >> 7));
        }
        if self.wobble > 0 {
            self.wobble = (self.wobble - 256).max(0);
            let a = match self.wobble_kind {
                0 => [sin_table(self.wobble) >> 7, 0, 0],
                1 => [0, 0, -sin_table(self.wobble) >> 8],
                2 => [-sin_table(self.wobble) >> 7, 0, 0],
                _ => [0, 0, -sin_table((self.wobble >> 1) + 0x400) >> 4],
            };
            for id in 0..if self.wobble_kind == 3 { 2 } else { 4 } {
                specials.set_angles(id, a);
            }
        }
    }
    fn fight(&mut self, cr: &mut Creatures, player: &Player) {
        for k in 0..3 {
            if self.phase[k] != 2 {
                continue;
            }
            if cr.slots[k].health != self.health[k] {
                self.health[k] = cr.slots[k].health;
                self.hurt[k] = 60;
                if k == 1 {
                    self.sounds.push((0xC1, cr.slots[k].pos));
                }
                if k == 2 {
                    let r = self.rand() & 3;
                    self.voices.push(0xC4 + if r == 3 { 0 } else { r as u16 });
                }
            }
            self.hurt[k] = (self.hurt[k] - 1).max(0);
            let c = &mut cr.slots[k];
            c.rec.state = if self.hurt[k] > 0 {
                4
            } else if k == 1 {
                7
            } else {
                6
            };
            c.lit = self.hurt[k] > 0 && self.clock & 1 != 0;
            if c.health < 10 {
                self.phase[k] = 3;
                self.defeated += 1;
                self.last = k;
                self.shot[k] = 0;
                c.set_script(if k == 1 { 0x68 } else { 0x5A });
                c.rec.state = 4;
                c.flags &= if k == 1 { !0x100 } else { !0x108 };
                c.rec.speed = 16;
                let pos = c.pos;
                self.sounds.push((0xFFFE, pos));
                continue;
            }
            if k == 1 {
                // The gunslinger's bytecode arms +0x8A; node 15 then 16 fire ten ticks apart.
                let timer = c.timer;
                if timer != 0 {
                    let fire = timer > 20 || timer <= 1;
                    c.timer = if timer > 20 { 9 } else { (timer - 1).max(0) };
                    if fire {
                        let pos = cr
                            .node_point(k, if timer > 20 { 15 } else { 16 }, [0.0; 3])
                            .unwrap_or(cr.slots[k].pos);
                        let facing = cr.slots[k].angles[1];
                        let b = player.pos_obj();
                        let aim = angle12((b[0] - pos[0]) as f32, (b[2] - pos[2]) as f32);
                        let a = if ((aim - facing + 0x100) & 0xFFF) > 0x200 {
                            facing
                        } else {
                            aim
                        };
                        self.particles.push(ParticleRequest {
                            pos,
                            vel: [sin_table(a) >> 2, 0x200, sin_table(a + 0x400) >> 2],
                            gravity: 0,
                            angle: 0,
                            spin: 0,
                            typ: 0x61,
                        });
                        self.sounds.push((0x56, pos));
                        if cr.slots[k].flags & 1 != 0 {
                            for _ in 0..5 {
                                let spin = self.rand() - 128;
                                self.spawns.push((pos, 100, 15, spin, None));
                            }
                        }
                    }
                }
            } else {
                let c = &mut cr.slots[k];
                if c.anim == if k == 0 { 1 } else { 4 } && near(c.pos, player.pos_obj(), 300) {
                    c.rec.speed = 0;
                    cr.animate_slot(
                        k,
                        3,
                        if k == 0 { 0x18 } else { 9 },
                        if k == 0 { [0x40; 2] } else { [-48; 2] },
                    );
                    self.shot[k] = if k == 0 { 63 } else { 44 };
                    if k == 2 {
                        self.voices.push(0xC2);
                    }
                }
                let c = &mut cr.slots[k];
                if c.anim == 3 && c.time >> 16 > if k == 0 { 46 } else { 21 } {
                    c.set_script(0x20);
                    c.rec.speed = 16;
                    c.flags &= !0xC;
                }
                if self.shot[k] > 0 {
                    self.shot[k] -= 1;
                    if self.shot[k] == 0 {
                        let a = c.angles[1];
                        let pos = c.pos;
                        if k == 0 {
                            let pos = cr.node_point(k, 4, [180.0, -150.0, -50.0]).unwrap_or(pos);
                            self.particles.push(ParticleRequest {
                                pos,
                                vel: [0, -1, 0],
                                gravity: a << 2,
                                angle: 0,
                                spin: 0,
                                typ: 0x66,
                            });
                            self.sounds.push((0xA7, pos));
                        } else {
                            self.particles.push(ParticleRequest {
                                pos: [
                                    pos[0] + (sin_table(a + 0x400) >> 3),
                                    pos[1] - 0x800,
                                    pos[2] + (sin_table(a - 0x800) >> 3),
                                ],
                                vel: [sin_table(a) >> 2, 0, sin_table(a + 0x400) >> 2],
                                gravity: 0,
                                angle: (0x7FF - a) & 0xFFF,
                                spin: 0,
                                typ: 0x68,
                            });
                            self.sounds.push((0xA6, pos));
                        }
                    }
                }
            }
        }
        self.taunt -= 1;
        if self.taunt < 0 && self.phase[2] == 2 {
            self.taunt = self.rand() * 2 + 400;
            let r = self.rand() % 6;
            self.voices.push(if r == 0 { 0xC3 } else { 199 + r as u16 });
        }
        for (a, b) in [(0, 1), (1, 2), (0, 2)] {
            let pa = cr.slots[a].pos;
            let pb = cr.slots[b].pos;
            if (0..3)
                .map(|i| ((pa[i] - pb[i]) >> 5) as i64)
                .map(|d| d * d)
                .sum::<i64>()
                < 0x40000
            {
                let angle = angle12((pa[0] - pb[0]) as f32, (pa[2] - pb[2]) as f32);
                cr.slots[b].pos[0] = pa[0] - sin_table(angle);
                cr.slots[b].pos[2] = pa[2] - sin_table(angle + 0x400);
            }
        }
        for c in cr.slots.iter_mut().take(3) {
            if c.pos[0] < -0x2BDD6 && c.vel[0] < 0 {
                c.pos[0] = -0x2BDD6;
            }
        }
        self.boss_hud = true;
        self.boss_bar =
            Some(((cr.slots.iter().take(3).map(|c| c.health).sum::<i32>() - 27) * 54 / 60).max(0));
    }
}
impl std::ops::Deref for Level15 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}
impl std::ops::DerefMut for Level15 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}
impl Script for Level15 {
    fn step(&mut self, _: &World, _: &mut Specials, _: &mut Player, _: &Boxes) {}
    fn step_creatures(
        &mut self,
        _: &World,
        specials: &mut Specials,
        player: &Player,
        cr: &mut Creatures,
        _: i32,
    ) {
        if cr.slots.len() < 5 {
            return;
        }
        self.clock = self.clock.wrapping_add(1);
        if !std::mem::replace(&mut self.initialized, true) {
            self.setup(cr);
        }
        if self.intro == 0 && player.pos_obj()[0] < -0x16BD9 {
            self.intro = 80;
            let at = self.platform;
            self.look_at(at, player.pos_obj(), 480, 56);
            if let Some(l) = self.look.as_mut() {
                l.from[1] -= 0x4000;
                l.at[1] -= 0x2000;
            }
        }
        if self.intro > 0 {
            self.intro = (self.intro + 1).min(1000);
            if self.intro == 151 {
                self.voices.push(0xCD);
            }
            if let Some(k) = [281, 336, 361, 431].iter().position(|&t| t == self.intro) {
                self.platform_v = -0xC00;
                self.wobble = 0x800;
                self.wobble_kind = k;
                if k == 3 {
                    for (i, vy) in [-0x900, -0xA00, -0x800].into_iter().enumerate() {
                        cr.slots[i].vel[1] = vy;
                        cr.slots[i].health = 29;
                    }
                    cr.slots[3].health = 0;
                    cr.slots[4].health = 0;
                    self.sounds.push((10, [0; 3]));
                } else {
                    let pair = match k {
                        0 => [-0x300, -0x400],
                        1 => [-0x400, -0x300],
                        _ => [-0x700, -0x780],
                    };
                    for i in 0..2 {
                        cr.slots[3 + i].vel[1] = pair[i];
                        if k == 2 {
                            cr.slots[3 + i].vel[0] = -0x400;
                            cr.slots[3 + i].home[1] = 0;
                        }
                    }
                    self.sounds.push((0x2F, [0; 3]));
                }
            }
            self.platform(specials);
            if self.phase == [0; 3] && self.look_ticks() <= 29 && self.intro >= 431 {
                self.phase = [2; 3];
                for (i, vy) in [-0x600, -0x800, -0x900].into_iter().enumerate() {
                    let c = &mut cr.slots[i];
                    c.health = 29;
                    c.set_script(0x1C);
                    c.rec.face = 0;
                    c.vel = [0x400, vy, 0];
                }
                self.sounds.push((14, [0; 3]));
                self.voices.push(199);
            }
        }
        if self.phase.iter().any(|&s| s > 0) {
            self.fight(cr, player);
            self.sounds.push((0xA8, [0; 3]));
        }
        if self.defeated == 3 && self.ending == 0 {
            self.ending = 1;
            let at = cr.slots[self.last].pos;
            self.look_at(at, player.pos_obj(), 120, 32);
            if let Some(l) = self.look.as_mut() {
                l.at[1] -= 0x2000;
                l.from[1] -= 0x8000;
            }
        } else if self.ending == 1 && self.look_ticks() == 0 {
            self.ending = 2;
            for (i, pos) in [[-0x4E4E0, -0xBC80, 0x14E0], [-0x46944, -0xB680, -0x1BEA]]
                .into_iter()
                .enumerate()
            {
                let c = &mut cr.slots[3 + i];
                c.pos = pos;
                c.health = 1;
                c.home[1] = pos[1];
                c.angles[1] = 0x400;
                c.target_facing = 0x400;
            }
            self.look_at(cr.slots[4].pos, player.pos_obj(), 300, 64);
            if let Some(l) = self.look.as_mut() {
                l.from = [l.at[0] + 0x3000, l.at[1] - 0x3000, l.at[2] - 0x1000];
                l.at[1] -= 0x2000;
            }
        } else if self.ending == 2 {
            if let Some(l) = self.look.as_mut() {
                l.from[2] += 32;
            } else {
                self.done = true;
                self.ending = 3;
            }
        }
    }
}
