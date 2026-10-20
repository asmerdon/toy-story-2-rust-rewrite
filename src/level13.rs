//! Airport Infiltration: setup 0x42C930, frame 0x42CA60, Prospector 0x42BE60.
//! Conveyor floors, tilting belts, travelling luggage/springs, fan wind, characters,
//! timed weights and Prospector challenge. Dynamic lighting remains a renderer gap.
use crate::{
    boxes::Boxes,
    creatures::{Creatures, ParticleRequest},
    formats::{exe::Exe, leveldat::PointList},
    game::{JumpPhase, Player, Sound, World},
    script::{Common, Script, Tilt, near, sin_table, touched},
    specials::Specials,
};
use glam::Vec3;

struct Luggage {
    mover: i32,
    path: Vec<[i32; 3]>,
    models: [i32; 2],
    yaw: i32,
    progress: i32,
    speed: i32,
    previous: Option<[i32; 2]>,
    hint: bool,
}
impl Luggage {
    fn target(&self) -> Option<[i32; 3]> {
        let k = (self.progress / 4096) as usize;
        let (a, b) = (self.path.get(k)?, self.path.get(k + 1)?);
        Some(std::array::from_fn(|i| {
            a[i] + (b[i] - a[i]) * (self.progress & 4095) / 4096
        }))
    }
}
pub struct Level13 {
    common: Common,
    luggage: Vec<Luggage>,
    tilts: [Tilt; 2],
    wind_phase: i32,
    wind: i32,
    fans: [i32; 2],
    spring: (i32, i32),
    boss_phase: i32,
    health: i32,
    hurt: i32,
    taunt: i32,
    shot: i32,
    started: bool,
}
impl Level13 {
    pub fn new(
        world: &World,
        specials: &mut Specials,
        lists: &[PointList],
        exe: Option<&Exe>,
    ) -> Self {
        let mut s = Self {
            common: Common {
                given: [false, false, false, true, false],
                show_collectables: Some(false),
                ..Default::default()
            },
            luggage: Vec::new(),
            tilts: Default::default(),
            wind_phase: 0,
            wind: 0,
            fans: [0; 2],
            spring: (0, 0),
            boss_phase: 0,
            health: 29,
            hurt: 0,
            taunt: 200,
            shot: 0,
            started: false,
        };
        if let Some(e) = exe {
            // The hints go first: load_story reads their texts.
            s.story.hints = (0..5)
                .filter_map(|i| {
                    e.at(0x4F46AC + i * 4, 4)
                        .ok()
                        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                })
                .collect();
            s.load_story(
                e,
                lists,
                &[
                    crate::script::HAMM_ASK,
                    crate::script::HAMM_THANKS,
                    crate::script::ALL_TOKENS,
                    0x4F4268,
                    0x4F42B8,
                    0x4F4384,
                    0x4F419C,
                    0x4F4210,
                    0x4F40B4,
                    0x4F4124,
                    0x4F414C,
                    0x4F4068,
                ],
                None,
            );
        } else {
            s.story.lists = lists.iter().map(|l| (l.kind, l.points.clone())).collect();
        }
        world.turn_mover(4, [0, -0x400, 0]);
        // Arguments checked in the executable: two carts, two paired cases, one spring case.
        for (mover, path, models, yaw, progress) in [
            (8, 2, [0x15, 0x14], 0x400, 0),
            (10, 4, [0x19, 0x18], 0x400, 0),
            (2, 6, [0xD, 0xC], -0x400, 0x8000),
            (5, 7, [0xF, 0xE], -0xC00, 0),
            (1, 5, [0xB, 0xA], -0x638, 0),
        ] {
            let cart = Luggage {
                mover,
                path: lists
                    .iter()
                    .find(|l| l.kind == path)
                    .map(|l| l.points.clone())
                    .unwrap_or_default(),
                models,
                yaw,
                progress,
                speed: 0,
                previous: None,
                hint: true,
            };
            if let Some(pos) = cart.target() {
                world.place_mover(mover, Vec3::from(pos.map(|v| v as f32)));
                world.turn_mover(mover, [0, yaw, 0]);
                for id in models {
                    specials.set_pos(id, pos);
                    specials.turn_from_own(id, [0, yaw, 0]);
                }
            }
            s.luggage.push(cart);
        }
        for id in 0x38..=0x3C {
            specials.set_scale(id, [0; 3]);
        }
        s
    }
    fn bounce(&mut self, player: &mut Player, model: i32) {
        player.stomp = 0;
        player.start_jump(-0xC00, JumpPhase::Released);
        player.sounds.push(Sound {
            id: 0x1C,
            voice: false,
        });
        self.spring = (model, -0x200);
    }
    fn carts(&mut self, world: &World, specials: &mut Specials, player: &mut Player) {
        for k in 0..self.luggage.len() {
            let cart = &mut self.luggage[k];
            if cart.path.len() < 2 {
                continue;
            }
            let Some(old) = world.mover_pos(cart.mover) else {
                continue;
            };
            let old_obj = old.to_array().map(|v| (v * 32.0) as i32);
            if cart.progress > (cart.path.len() as i32 - 2) * 4096 {
                cart.progress = 0;
                if let Some(target) = cart.target() {
                    world.place_mover(cart.mover, Vec3::from(target.map(|v| v as f32)));
                }
            } else if let Some(target) = cart.target() {
                if cart.speed < 125 {
                    cart.speed += 4;
                }
                if cart.speed > 62 && cart.previous == Some([old_obj[0], old_obj[2]]) {
                    cart.speed = -125;
                }
                cart.previous = Some([old_obj[0], old_obj[2]]);
                let delta = std::array::from_fn::<_, 3, _>(|i| target[i] * 32 - old_obj[i]);
                if delta.iter().all(|d| d.abs() < 0x4000) {
                    cart.progress += 4096;
                }
                let dir = Vec3::from(delta.map(|v| v as f32)).normalize_or_zero();
                // 0x451FD0 normalizes to 4096; 0x487900 divides by 1024.
                world.move_mover(cart.mover, dir * (cart.speed as f32 / 8.0));
            }
            for id in cart.models {
                specials.set_pos(id, old_obj.map(|v| v >> 5));
            }
            if cart.yaw != 0x400
                && player.stomp != 0
                && player.floor_mover.is_some_and(|(m, _)| m == cart.mover)
            {
                cart.hint = false;
                let id = cart.models[0];
                self.bounce(player, id);
            }
        }
    }
    fn challenge(&mut self, specials: &mut Specials, cr: &mut Creatures) {
        if touched(cr, 5) {
            if self.race == 0 {
                self.voices.push(0xB6);
                self.talk(5, 0x1E, 0x4F40B4, None, None);
                self.collect_reset = true;
                self.collected = 0;
                self.show_collectables = Some(true);
                self.race = 1;
                for id in 0x38..=0x3C {
                    specials.set_scale(id, [0x1000; 3]);
                }
            } else if self.collected < 5 {
                self.talk(5, 0x1E, 0x4F4124, None, None);
            } else if self.race != 3 {
                self.voices.push(0xB7);
                self.talk(5, 0x1E, 0x4F414C, Some(2), None);
                self.race = 3;
            }
        }
        if self.race == 1 && self.cutscene.is_none() {
            self.race = 2;
            self.laps = 0xA0;
        }
        if self.race == 2 && self.every(64) {
            self.laps -= 1;
            if self.laps < 100 {
                self.laps = 100;
                self.race = 0;
                self.show_collectables = Some(false);
                for id in 0x38..=0x3C {
                    specials.set_scale(id, [0; 3]);
                }
            }
        }
    }
    fn boss(&mut self, player: &Player, cr: &mut Creatures) {
        const SLOT: usize = 0x20;
        let Some(c) = cr.slots.get_mut(SLOT) else {
            return;
        };
        if !std::mem::replace(&mut self.started, true) {
            self.health = c.health;
            c.rec.zone[0] = 90;
        }
        if self.boss_phase == 0
            && player.on_floor
            && player.pos_obj()[1] < -0x3660E
            && near(player.pos_obj(), c.pos, 300)
        {
            self.boss_phase = 1;
            self.talk(SLOT, 0x20, 0x4F4068, None, None);
        }
        if self.boss_phase == 1 && self.cutscene.is_none() {
            self.boss_phase = 2;
            let c = &mut cr.slots[SLOT];
            c.set_script(0x1C / 2);
            c.rec.face = 0;
        }
        if self.boss_phase == 2 {
            self.boss_hud = true;
            self.taunt -= 1;
            if self.taunt < 0 {
                self.taunt = self.rand() * 2 + 400;
                self.voices.push(0xC3);
            }
            if cr.slots[SLOT].health != self.health {
                self.health = cr.slots[SLOT].health;
                self.hurt = 60;
                let r = self.rand() & 3;
                self.voices.push(0xC4 + if r == 3 { 0 } else { r as u16 });
            }
            self.hurt = (self.hurt - 1).max(0);
            let c = &mut cr.slots[SLOT];
            c.rec.state = if self.hurt > 0 { 4 } else { 6 };
            c.lit = self.hurt > 0 && self.clock & 1 != 0;
            self.boss_bar = Some((c.health - 9) * 54 / 20);
            if c.health < 10 {
                c.set_script(0x5A / 2);
                c.rec.state = 4;
                c.flags &= !0x108;
                c.rec.speed = 0;
                let pos = c.pos;
                self.sounds.push((0xFFFE, pos));
                self.boss_phase = 3;
                self.boss_bar = Some(0);
                self.shot = 0;
            } else {
                if c.anim == 4 && near(c.pos, player.pos_obj(), 200) {
                    c.rec.speed = 0;
                    cr.animate_slot(SLOT, 3, 9, [-48, -48]);
                    self.shot = 44;
                    self.voices.push(0xC2);
                }
                let c = &mut cr.slots[SLOT];
                if c.anim == 3 && c.time >> 16 > 21 {
                    c.set_script(0x20 / 2);
                    c.rec.speed = 16;
                    c.flags &= !0xC;
                }
                if self.shot > 0 {
                    self.shot -= 1;
                    if self.shot == 0 {
                        let a = c.angles[1];
                        self.particles.push(ParticleRequest {
                            pos: [
                                c.pos[0] + (sin_table(a + 0x400) >> 3),
                                c.pos[1] - 0x800,
                                c.pos[2] + (sin_table(a - 0x800) >> 3),
                            ],
                            vel: [sin_table(a) >> 2, 0, sin_table(a + 0x400) >> 2],
                            gravity: 0,
                            angle: (0x7FF - a) & 0xFFF,
                            spin: 0,
                            typ: 0x68,
                        });
                        self.sounds.push((0xA6, c.pos));
                    }
                }
            }
        } else if (3..120).contains(&self.boss_phase) {
            self.boss_phase += 1;
        } else if self.boss_phase == 120 {
            self.given[4] = true;
            self.reveals.push(4);
            self.boss_phase = 200;
        }
    }
}
impl std::ops::Deref for Level13 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}
impl std::ops::DerefMut for Level13 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}
impl Script for Level13 {
    fn step(&mut self, world: &World, specials: &mut Specials, player: &mut Player, _: &Boxes) {
        self.clock = self.clock.wrapping_add(1);
        if matches!(world.room_at(player.pos), Some(2 | 4 | 5)) {
            self.tilts[0].step(world, specials, player, 3, 4, 0, 2, -0xE0, 0xE0, 0x80);
            self.tilts[1].step(world, specials, player, 4, 5, -0x400, 2, -0xE0, 0xE0, 0x80);
            world.turn_mover(6, world.mover_turn(3));
            world.turn_mover(7, world.mover_turn(4));
        }
        self.wind_phase = (self.wind_phase + 6) & 0xFFF;
        self.wind = if self.wind_phase <= 0x800 {
            (self.wind + 2).min(128)
        } else {
            (self.wind - 2).max(0)
        };
        self.fans[0] -= self.wind * 3;
        self.fans[1] -= (128 - self.wind) * 3;
        for k in 0..4 {
            specials.turn_from_own(k, [0, 0, self.fans[k as usize % 2]]);
        }
        if world.room_at(player.pos) == Some(4) {
            let buzz = player.pos_obj();
            if let Some(points) = self.story.lists.get(&1) {
                for (k, pair) in points.chunks_exact(2).enumerate() {
                    let power = if k % 2 == 0 {
                        128 - self.wind
                    } else {
                        self.wind
                    };
                    let d = std::array::from_fn::<_, 3, _>(|i| pair[0][i] / 8 - buzz[i] / 256);
                    let length = (pair[0][0] - pair[1][0]) / 8 - 32;
                    if power > 32
                        && d[1] * d[1] + d[2] * d[2] < 0x10000
                        && d[0] > 0
                        && d[0] < length
                    {
                        player.push_by([power * d[0] * 32 / length - power * 32, 0, 0]);
                    }
                }
            }
        }
        for (m, id, reward) in [(0, 9, 0), (14, 0x1C, 1)] {
            if player.stomp != 0 && player.floor_mover.is_some_and(|(i, _)| i == m) {
                self.bounce(player, id);
                self.put_out.push((reward, 1));
            }
        }
        if self.spring.1 != 0 {
            self.spring.1 = (self.spring.1 + 32).min(0);
            let yaw = specials.angles(self.spring.0).unwrap_or_default()[1];
            specials.set_angles(self.spring.0, [0, yaw, self.spring.1]);
        }
        self.carts(world, specials, player);
        for id in 0x10..0x14 {
            specials.set_scale(
                id,
                if world.room_at(player.pos) == Some(3) {
                    [4096; 3]
                } else {
                    [0; 3]
                },
            );
        }
        let force = match player.surface {
            Some(0x20) => [0, 0, 0x400],
            Some(0x21) => [0x400, 0, 0],
            Some(0x22) => [-0x200, 0, 0x376],
            Some(0x23) => [-0x292, 0, -0x310],
            Some(0x24) => [-0x400, 0, 0],
            _ => [0; 3],
        };
        if force != [0; 3] {
            player.push_by(force);
        }
    }
    fn step_creatures(
        &mut self,
        _: &World,
        specials: &mut Specials,
        player: &Player,
        cr: &mut Creatures,
        coins: i32,
    ) {
        self.potato(
            cr,
            8,
            0x1F,
            [0x4F4268, 0x4F42B8, 0x4F4384],
            [-1, 0xE00],
            0x10,
        );
        self.hamm(cr, 6, 0x1D, None, 0, coins);
        self.hint_character(cr, 0x21, 0x22);
        self.finder(cr, 7, 0x21, 0x4F419C, 0x4F4210, None, 1);
        self.call(cr, 5, 0xB5, true);
        self.challenge(specials, cr);
        self.boss(player, cr);
    }
}
