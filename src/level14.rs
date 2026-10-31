//! Tarmac Trouble: setup 0x42E600 and frame 0x42E790. Rotating aeroplane,
//! three-press matching puzzle, floating token, wing race, luggage and Blacksmith.
//! Rain/lightning and dynamic lights remain renderer effects.
use crate::{
    boxes::{Boxes, angle12},
    creatures::{Creatures, ParticleRequest},
    formats::{exe::Exe, leveldat::PointList},
    game::{Player, World},
    script::{Common, Script, near, sin_table, touched},
    specials::Specials,
};
use glam::Vec3;

/// 0x42D710: one button sets its lamp and clears the preceding lamp.
pub fn switch_bits(bits: u8, button: usize) -> u8 {
    let set = 1u8 << button;
    let clear = 1u8 << ((button + 3) % 4);
    (bits & !clear) | set
}
pub struct Level14 {
    common: Common,
    bits: u8,
    presses: i32,
    pressed: Option<(i32, i32)>,
    entered: bool,
    solved: bool,
    token_drop: i32,
    float_phase: i32,
    plane_angle: i32,
    boss_phase: i32,
    health: i32,
    hurt: i32,
    shot: i32,
    started: bool,
}
impl Level14 {
    pub fn new(specials: &mut Specials, exe: Option<&Exe>, lists: &[PointList]) -> Self {
        let mut s = Self {
            common: Common {
                given: [false, false, false, true, false],
                ..Default::default()
            },
            bits: 0,
            presses: 3,
            pressed: None,
            entered: false,
            solved: false,
            token_drop: 0,
            float_phase: 0,
            plane_angle: 0,
            boss_phase: 0,
            health: 29,
            hurt: 0,
            shot: 0,
            started: false,
        };
        if let Some(e) = exe {
            // The hints go first: load_story reads their texts.
            s.story.hints = (0..5)
                .filter_map(|k| {
                    e.at(0x4F4C54 + 4 * k, 4)
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
                    0x4F47CC,
                    0x4F4844,
                    0x4F471C,
                    0x4F47A4,
                    0x4F46D0,
                ],
                Some(0x4F4C40),
            );
        }
        s.reset_puzzle(specials);
        s
    }
    fn lamps(&self, specials: &mut Specials) {
        for (bit, off, on) in [
            (7, 0x1F, 0x27),
            (6, 0x20, 0x28),
            (5, 0x21, 0x29),
            (4, 0x22, 0x2A),
            (3, 0x23, 0x2B),
            (2, 0x24, 0x2C),
            (1, 0x25, 0x33),
            (0, 0x26, 0x34),
        ] {
            let lit = self.bits & (1 << bit) != 0;
            specials.set_scale(off, if lit { [0; 3] } else { [4096; 3] });
            specials.set_scale(on, if lit { [4096; 3] } else { [0; 3] });
        }
        for id in 0x36..0x3A {
            specials.set_scale(
                id,
                if id == 0x36 + self.presses {
                    [4096; 3]
                } else {
                    [0; 3]
                },
            );
        }
    }
    fn reset_puzzle(&mut self, specials: &mut Specials) {
        // Generate the target by applying three effective moves to the starting lamps,
        // then swap the nibbles: always solvable in the three available presses.
        let mut chosen = None;
        for _ in 0..256 {
            let start = (self.rand() & 15) as u8;
            let mut target = start;
            let mut changes = 0;
            for _ in 0..3 {
                let next = switch_bits(target, (self.rand() & 3) as usize);
                changes += (next ^ target).count_ones();
                target = next;
            }
            if target != start && changes >= 6 {
                chosen = Some((target << 4) | start);
                break;
            }
        }
        // No random table (or a constant diagnostic table): a valid three-move puzzle.
        self.bits = chosen.unwrap_or(0xC3);
        self.presses = 3;
        self.pressed = None;
        for id in 0x4C..0x50 {
            specials.set_scale(id, [4096; 3]);
            specials.set_scale(id + 4, [0; 3]);
        }
        self.lamps(specials);
    }
    fn puzzle(&mut self, specials: &mut Specials, player: &Player) {
        if near(player.pos_obj(), [-0xD2AC0, -0xF9F7, -0x98D72], 500) {
            if !self.entered {
                self.reset_puzzle(specials);
                self.entered = true;
            }
            if !self.solved && self.bits >> 4 == self.bits & 15 && self.presses >= 0 {
                self.solved = true;
                let at = specials.pos_obj(0x74).unwrap_or_default();
                self.look_at(at, player.pos_obj(), 300, 32);
                if let Some(l) = self.look.as_mut() {
                    l.from[1] += 0x8000;
                }
                self.sounds.push((0xFFFB, player.pos_obj()));
            }
            if !self.solved {
                if self.presses <= 0 {
                    if self.presses == 0 {
                        self.bits = 0;
                        self.lamps(specials);
                    }
                    self.presses -= 1;
                    if self.presses < -60 {
                        self.reset_puzzle(specials);
                        self.sounds.push((0xFFFA, player.pos_obj()));
                    }
                } else if player.stomp == -40
                    && player.surface.is_some_and(|s| (0x20..0x24).contains(&s))
                {
                    let k = (player.surface.unwrap() - 0x20) as usize;
                    self.bits = switch_bits(self.bits, k);
                    self.presses -= 1;
                    let id = 0x4C + k as i32;
                    self.pressed = Some((id, 12));
                    specials.set_scale(id, [0; 3]);
                    specials.set_scale(id + 4, [4096; 3]);
                    for k in 0..4 {
                        self.put_out.push((k, 1));
                    }
                    self.lamps(specials);
                }
            }
        }
        if let Some((id, t)) = self.pressed {
            if t <= 1 {
                specials.set_scale(id, [4096; 3]);
                specials.set_scale(id + 4, [0; 3]);
                self.pressed = None;
            } else {
                self.pressed = Some((id, t - 1));
            }
        }
        if self.solved {
            self.token_drop = (self.token_drop + 128).min(48000);
        }
        for id in [3, 0x30, 0x42, 0x43, 0x44, 0x74, 0x31, 0x32] {
            if let Some(mut pos) = specials.home_obj(id) {
                let bob = sin_table(self.float_phase) / 2 + self.token_drop;
                pos[1] += if id == 0x74 {
                    sin_table(self.float_phase) / 4 + self.token_drop - 0xA000
                } else if id == 0x31 || id == 0x32 {
                    bob >> 2
                } else {
                    bob
                };
                let pos = pos.map(|v| v >> 5);
                specials.set_pos(id, pos);
                if id == 0x74 {
                    self.token_moves.push((0, pos));
                }
            }
        }
        for id in [0x30, 0x32] {
            specials.turn_from_own(id, [0, self.float_phase * 21, 0]);
        }
        self.float_phase = (self.float_phase + 33) & 0xFFF;
    }
    fn plane(&mut self, world: &World, specials: &mut Specials, player: &mut Player) {
        let before = world.mover_turn(0);
        self.plane_angle = (self.plane_angle + 1) & 0xFFF;
        let a = self.plane_angle;
        let s = sin_table(-a) as i64;
        let c = sin_table(a + 0x400) as i64;
        for id in (0..3).chain(4..0x1F).chain(0x54..0x57) {
            if let Some(p) = specials.home_obj(id) {
                let x = (p[0] as i64 * c - p[2] as i64 * s) >> 14;
                let z = (p[0] as i64 * s + p[2] as i64 * c) >> 14;
                specials.set_pos(id, [x as i32 >> 5, p[1] >> 5, z as i32 >> 5]);
                let (yaw, roll) = match id {
                    8 | 9 | 0x17 | 0x18 => (a, a * 0x327),
                    10 | 0x19 => (a + 0x400, a * -15),
                    11 | 0x1A => (a + 0x400, a * -21),
                    13 | 0x1C => (a + 700, a * -18),
                    _ => (a, 0),
                };
                specials.turn_from_own(id, [0, yaw, roll]);
                if [10, 11, 13].contains(&id) {
                    let pos = [x as i32, p[1] + 0xC00, z as i32];
                    if near(player.pos_obj(), pos, 80) {
                        let b = player.pos_obj();
                        player.hurt(angle12((b[0] - pos[0]) as f32, (b[2] - pos[2]) as f32), 3);
                    }
                }
            }
        }
        world.turn_mover(0, [0, a, 0]);
        if let (Some(pos), Some(old)) = (specials.get(0), world.mover_pos(0)) {
            let mut d = (Vec3::from(pos.pos) - old) * 0.75;
            d.y = 0.0;
            world.move_mover(0, d);
        }
        if player.floor_mover.is_some_and(|(m, _)| m == 0) {
            player.pos = world.ride_turn(0, before, [0, a, 0], player.pos);
        }
    }
    fn boss(&mut self, player: &Player, cr: &mut Creatures) {
        const K: usize = 46;
        if cr.slots.len() <= K {
            return;
        }
        if self.boss_phase == 0 && near(player.pos_obj(), cr.slots[K].pos, 300) {
            self.boss_phase = 1;
            self.talk(K, 1, 0x4F46D0, None, None);
        }
        if self.boss_phase == 1 && self.cutscene.is_none() {
            self.boss_phase = 2;
            cr.slots[K].set_script(0x1C / 2);
            cr.slots[K].rec.face = 0;
            self.health = cr.slots[K].health;
        }
        if self.boss_phase == 2 {
            self.boss_hud = true;
            let c = &mut cr.slots[K];
            if c.health != self.health {
                self.health = c.health;
                self.hurt = 60;
            }
            self.hurt = (self.hurt - 1).max(0);
            c.lit = self.hurt > 0 && self.clock & 1 != 0;
            c.rec.state = if self.hurt > 0 { 4 } else { 6 };
            self.boss_bar = Some((c.health - 9) * 54 / 20);
            if c.health < 10 {
                c.set_script(0x5A / 2);
                c.rec.state = 4;
                c.flags &= !0x108;
                c.rec.speed = 16;
                let pos = c.pos;
                self.sounds.push((0xFFFE, pos));
                self.boss_phase = 3;
                self.shot = 0;
            } else {
                if c.anim == 1 && near(c.pos, player.pos_obj(), 200) {
                    c.rec.speed = 0;
                    cr.animate_slot(K, 3, 0x18, [0x40; 2]);
                    self.shot = 63;
                }
                let c = &mut cr.slots[K];
                if c.anim == 3 && c.time >> 16 > 46 {
                    c.set_script(0x20 / 2);
                    c.rec.speed = 16;
                    c.flags &= !0xC;
                }
                if self.shot > 0 {
                    self.shot -= 1;
                    if self.shot == 0 {
                        let a = c.angles[1];
                        let fallback = c.pos;
                        let pos = cr
                            .node_point(K, 4, [180.0, -150.0, -50.0])
                            .unwrap_or(fallback);
                        self.particles.push(ParticleRequest {
                            pos,
                            vel: [0, -1, 0],
                            gravity: a << 2,
                            angle: 0,
                            spin: 0,
                            typ: 0x66,
                        });
                        self.sounds.push((0xA7, pos));
                    }
                }
            }
        } else if (3..120).contains(&self.boss_phase) {
            self.boss_phase += 1;
        } else if self.boss_phase == 120 {
            self.boss_phase = 200;
            self.given[4] = true;
            self.reveals.push(4);
        }
    }
}
impl std::ops::Deref for Level14 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}
impl std::ops::DerefMut for Level14 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}
impl Script for Level14 {
    fn step(&mut self, w: &World, s: &mut Specials, p: &mut Player, _: &Boxes) {
        self.clock = self.clock.wrapping_add(1);
        self.plane(w, s, p);
        self.puzzle(s, p);
        let t = self.clock as i32;
        let a = sin_table(t * 11);
        let turn = [
            0,
            sin_table(t * 17) * a / 0x200000,
            sin_table(t * 7) * a / 0x400000,
        ];
        for id in [0x45, 0x46] {
            s.turn_from_own(id, turn);
        }
        if self.race == 2 && (p.dying > 0 || p.surface == Some(0)) {
            self.laps = 99;
        }
    }
    fn step_creatures(
        &mut self,
        _: &World,
        s: &mut Specials,
        p: &Player,
        cr: &mut Creatures,
        coins: i32,
    ) {
        // 0x42E600 sets every creature's draw distance (+0x42, beside the flags at +0x40) to
        // 0x708 (normally 0x500, 0x406CD0). The port doesn't model draw distance; this must not
        // touch the flags: 0x708 there marked every creature touched (0x200: Hamm and the
        // race began at once), harmful (0x100) and chasing (8).
        self.started = true;
        self.hamm(cr, 39, 2, None, 0, coins);
        self.hint_character(cr, 47, 10);
        self.finder(cr, 40, 3, 0x4F47CC, 0x4F4844, None, 1);
        // Luggage pickups (0x42D620) share the finder count, but have no idle call.
        for k in 0..cr.slots.len() {
            if cr.slots[k].health == 0x66 && cr.slots[k].id == 0x3B && touched(cr, k) {
                cr.sheep += 1;
                let pos = cr.slots[k].pos;
                cr.slots[k].health = 0;
                cr.slots[k].respawn = 10000;
                self.sounds.push((0x1F, pos));
            }
        }
        self.call(cr, 38, 0xB5, true);
        if touched(cr, 38) && self.tokens & 4 == 0 {
            if self.race == 0 {
                self.voices.push(0xB6);
                self.talk(38, 4, 0x4F471C, Some(2), None);
                self.race = 1;
            } else {
                self.talk(38, 4, 0x4F47A4, None, None);
            }
        }
        if self.race == 1 && self.cutscene.is_none() {
            self.race = 2;
            self.laps = 170;
        }
        if self.race == 2 {
            if self.tokens & 4 != 0 {
                self.race = 0;
            } else {
                if self.every(64) {
                    self.laps -= 1;
                }
                if self.laps < 100 {
                    self.laps = 100;
                    self.race = 0;
                    self.hides.push(2);
                    self.given[2] = false;
                }
            }
        }
        if let (Some(pos), Some(c)) = (s.pos_obj(0), cr.slots.get_mut(43)) {
            c.pos[0] = pos[0];
            c.pos[2] = pos[2];
            c.home = pos;
        }
        self.boss(p, cr);
    }
}
