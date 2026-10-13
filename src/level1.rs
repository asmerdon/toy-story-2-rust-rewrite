//! Level 1 (Andy's house): the level's start-up (0x4171D0) and per-frame script (0x417680),
//! the parts that don't need creatures yet. Rooms are the room-map numbers (`0x54DEA0`).
//!
//! - The basement door (special 4) is box 3 of the level's push list: once Buzz pushes it
//!   over its edge, it tips open (up to 0x200 about X, 0x10 per tick) and its collision swaps
//!   from mover 1 (closed) to mover 14 (open).
//! - Stepping onto the basement shelf (a box around x 8988–9822, z −11300 to −10058, y 0–576)
//!   unrolls a plank: special 0x15 grows along x (0x40 per tick) and drags 0x16 with it, and
//!   mover 8 starts colliding.
//! - Special 0xE (a fan) always turns, 0x20 per tick about Y.
//! - Room 2: three spinning tops (specials 9, 0x11, 0x12; 0x417510) and two pop-up platforms
//!   (specials 10, 0x10 with movers 12, 13; 0x417380) that sink and rise, losing their
//!   collision while down.
//! - Room 4: mover 3 is a trampoline: landing on it bounces Buzz (−0x980, −0xC00 from a stomp).
//!
//! Not ported yet: the creature parts (Bo Peep and her sheep, the RC race, Hamm's coins, the
//! falling toys of room 1), the lift of room 2 (special 6 and pole 8), the spring of room 3,
//! the tilting platform of room 5 and Mr. Potato Head's shield in room 6.

use crate::boxes::Boxes;
use crate::game::{JumpPhase, Player, Sound, World};
use crate::specials::Specials;
use glam::Vec3;

pub struct Level1 {
    /// 0x52F500: door tilt.
    door: i32,
    /// 0x52F5A0: plank growth (0 = not started).
    plank: i32,
    /// 0x52F5CC: fan angle.
    fan: i32,
    /// 0x52F4F4 / 0x52F568: tops' angle and phase; 0x52F5A4 / 0x52F588: pop-ups'.
    tops: (i32, i32),
    pops: (i32, i32),
    /// Rest positions (object units) of the tops and pop-ups, read at start-up.
    home: Vec<(i32, [i32; 3])>,
}

/// Sine table value (round(16384 · sin)) of a 12-bit angle, as 0x4FE788 holds it.
fn sin_table(a: i32) -> i32 {
    ((a & 0xFFF) as f64 / 4096.0 * std::f64::consts::TAU).sin().mul_add(16384.0, 0.0).round() as i32
}

/// Within `r` (in 256-object-unit steps) of Buzz (0x49F400).
fn near(p: [i32; 3], buzz: [i32; 3], r: i32) -> bool {
    let d = [0, 1, 2].map(|k| (p[k] - buzz[k]) >> 8);
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < r * r
}

impl Level1 {
    /// 0x4171D0's part that sets up objects.
    pub fn new(world: &World, specials: &mut Specials) -> Level1 {
        let home = [9, 0x11, 0x12, 10, 0x10].iter().filter_map(|&id| specials.pos_obj(id).map(|p| (id, p))).collect();
        specials.set_scale(0x15, [0, 0x1000, 0x1000]);
        specials.scale_about(0x16, 0x15);
        world.set_mover(8, false);
        world.set_mover(14, false);
        Level1 { door: 0, plank: 0, fan: 0, tops: (0, 0), pops: (0, 0), home }
    }

    fn home(&self, id: i32) -> Option<[i32; 3]> {
        self.home.iter().find(|(i, _)| *i == id).map(|(_, p)| *p)
    }

    /// One tick of 0x417680.
    pub fn tick(&mut self, world: &World, specials: &mut Specials, player: &mut Player, boxes: &Boxes) {
        let room = world.room_at(player.pos);
        let buzz = player.pos_obj();

        // The basement door: box 3 going over its edge.
        if boxes.going_over(3) {
            if self.door == 0 {
                world.set_mover(14, true);
                world.set_mover(1, false);
            }
            self.door = (self.door + 0x10).min(0x200);
            specials.turn_from_own(4, [self.door, 0, 0]);
        }

        // The basement shelf's plank.
        if self.plank == 0 {
            let [x, y, z] = buzz;
            if (-0x58481..-0x4E942).contains(&z) && (0x46377..0x4CBB6).contains(&x) && (1..0x4800).contains(&y) && player.coyote != 0 {
                world.set_mover(8, true);
                self.plank = 8;
            }
        } else if self.plank < 0x1000 {
            self.plank = (self.plank + 0x40).min(0x1000);
            specials.set_scale(0x15, [self.plank, 0x1000, 0x1000]);
            specials.scale_about(0x16, 0x15);
        }

        // Room 4: the trampoline (mover 3).
        if room == Some(4)
            && player.on_floor
            && let Some((3, n)) = player.floor_mover
            && n.y < -0.75
        {
            let vy = if player.stomp != 0 {
                player.stomp = 0;
                -0xC00
            } else {
                -0x980
            };
            player.start_jump(vy, JumpPhase::Released);
            player.sounds.push(Sound { id: 0x1C, voice: false });
        }

        // Room 2: tops and pop-ups.
        if room == Some(2) {
            self.tops.0 = (self.tops.0 + 0x100) & 0xFFF;
            self.tops.1 = (self.tops.1 + 0x40) & 0x1FFF;
            for (k, id) in [9, 0x11, 0x12].into_iter().enumerate() {
                self.top(specials, player, id, self.tops.1 + [0, 0xAAA, 0x1555][k], self.tops.0 + 0x200 * k as i32);
            }
            self.pops.0 = (self.pops.0 + 0x80) & 0xFFF;
            self.pops.1 = (self.pops.1 + 0x40) & 0x1FFF;
            for (k, (id, mover)) in [(10, 0xC), (0x10, 0xD)].into_iter().enumerate() {
                self.pop(world, specials, player, id, mover, self.pops.1 + 0x1000 * k as i32, self.pops.0 + 0x200 * k as i32);
            }
        }

        // The fan.
        self.fan = (self.fan + 0x20) & 0xFFF;
        specials.turn_from_own(0xE, [0, self.fan, 0]);
    }

    /// 0x417510: a top that spins and squashes in a cycle, near Buzz.
    fn top(&self, specials: &mut Specials, player: &mut Player, id: i32, phase: i32, angle: i32) {
        let Some(p) = self.home(id) else { return };
        if !near(p, player.pos_obj(), 0x280) {
            return;
        }
        let mut v = phase & 0x1FFF;
        if v >= 0x800 {
            if v < 0x1000 {
                v = 0x800;
            } else if v < 0x1800 {
                v = 0x1800 - v;
            } else {
                if v - 0x40 < 0x1800 {
                    player.sounds_at.push((Sound { id: 0x24, voice: false }, Vec3::from(p.map(|c| c as f32 / 32.0))));
                }
                v = 0;
            }
        }
        specials.turn_from_own(id, [0, angle & 0xFFF, 0]);
        specials.set_scale(id, [0x1000, (sin_table(v + 0x400) >> 3) + 0xC00, 0x1000]);
    }

    /// 0x417380: a platform that sinks 512 units and rises again, near Buzz; its collision goes
    /// off on the way down (phase 0x400) and back on on the way up (0x1400, sound 0x25).
    #[allow(clippy::too_many_arguments)]
    fn pop(&self, world: &World, specials: &mut Specials, player: &mut Player, id: i32, mover: i32, phase: i32, angle: i32) {
        let Some(p) = self.home(id) else { return };
        if !near(p, player.pos_obj(), 0x280) {
            return;
        }
        let v = phase & 0x1FFF;
        let h = if v < 0x800 {
            if v - 0x40 < 0x400 && v >= 0x400 {
                world.set_mover(mover, false);
            }
            v
        } else if v < 0x1000 {
            0x800
        } else if v < 0x1800 {
            if v - 0x40 < 0x1400 && v >= 0x1400 {
                world.set_mover(mover, true);
                player.sounds_at.push((Sound { id: 0x25, voice: false }, Vec3::from(p.map(|c| c as f32 / 32.0))));
            }
            0x1800 - v
        } else {
            0
        };
        specials.turn_from_own(id, [0, 0, angle & 0xFFF]);
        let y = (p[1] >> 5) - (sin_table(h + 0x400) >> 6) + 0x100;
        specials.set_pos(id, [p[0] >> 5, y, p[2] >> 5]);
    }
}
