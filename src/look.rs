//! First-person look mode (0x4038E0, run instead of the follow camera while 0x50A13C ≠ 0).
//!
//! L1 on the ground (`Player::first_person` 3) puts the camera at Buzz's eyes; he can't walk,
//! the d-pad turns and tilts the view with a little acceleration, and the laser fires where he
//! looks. Creatures that can be targeted (record +0x1A ≠ 0) within 4096 units are listed; R1
//! cycles through them, putting a marker on the current one (particle 0x30, sound 0x29), and
//! holding R1 with the d-pad still swings the view onto it (sound 0x28). A dot (particle 8)
//! shows where the view meets the level or a creature (red over a creature). The helmet and
//! arms (special objects 0x2D–0x2F) sit at his eyes, breathing (sounds 0x13 / 0x14, faster
//! below two batteries). L1 again (state 5) puts the follow camera back behind him.
//!
//! Units as the game: object units, 12-bit angles, ticks.

use crate::creatures::Creatures;
use crate::game::{Player, World};
use crate::particles::Particles;
use crate::specials::Specials;
use glam::Vec3;

/// The first-person model: helmet, then the two arms.
pub const MODEL: [i32; 3] = [0x2D, 0x2E, 0x2F];
/// Eye height above the feet.
pub const EYE: i32 = 0x3000;

/// The pad as first person reads it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pad {
    /// D-pad bits (`game::pad`).
    pub dpad: u8,
    /// R1 held, and pressed this tick.
    pub r1: bool,
    pub r1_pressed: bool,
    /// L1 pressed this tick (leaves first person).
    pub l1_pressed: bool,
    /// Mouse motion this tick (12-bit angle units: turn, tilt down). Not in the original: a
    /// PC addition, applied straight to the view on top of the d-pad.
    pub mouse: [i32; 2],
}

#[derive(Debug, Clone, Default)]
pub struct FirstPerson {
    /// Turn and tilt speeds (0x50A130 / 0x50A138).
    yaw_speed: i32,
    pitch_speed: i32,
    /// Where the view is tilted to (camera +0xC) and the eased camera pitch and yaw (+0x20 /
    /// +0x22); positive pitch looks down.
    pitch: i32,
    pub cam_pitch: i32,
    pub cam_yaw: i32,
    /// Targeted creature slot (0x50A4FC), R1 lock (0x50A540), "just changed" (0x50A14C).
    pub target: Option<usize>,
    lock: bool,
    fresh: bool,
    /// The marker and dot particles (0x50A4DC / 0x50A538).
    marker: Option<usize>,
    dot: Option<usize>,
    /// The arms' lagging angles (0x50A28C pitch, 0x50A290 yaw) and the breathing phase
    /// (0x50A4C8).
    lag: [i32; 2],
    breath: i32,
    /// Sounds requested (flat).
    pub sounds: Vec<u16>,
    /// The level's hook points (`level.dat` list 59, level units); with hooks in hand they
    /// can be targeted too, as target 1000 + their index.
    pub hooks: Vec<[i32; 3]>,
}

/// Targets from here on are hook points (0x50A4FC ≥ 1000).
const HOOK: usize = 1000;

fn sin14(a: i32) -> i32 {
    ((a & 0xFFF) as f64 / 4096.0 * std::f64::consts::TAU).sin().mul_add(16384.0, 0.0).round() as i32
}

fn wrap(a: i32) -> i32 {
    let a = a & 0xFFF;
    if a > 0x7FF { a - 0x1000 } else { a }
}

impl FirstPerson {
    /// Where the eyes are (object units).
    pub fn eye(player: &Player) -> [i32; 3] {
        let p = player.pos_obj();
        [p[0], p[1] - EYE, p[2]]
    }

    /// The view direction in game space (+Y down).
    pub fn forward(&self) -> Vec3 {
        let (yaw, pitch) = (self.cam_yaw as f32, self.cam_pitch as f32);
        let r = |a: f32| a / 4096.0 * std::f32::consts::TAU;
        Vec3::new(r(yaw).sin() * r(pitch).cos(), r(pitch).sin(), r(yaw).cos() * r(pitch).cos())
    }

    /// 0x4038E0 for one tick. Hides the model again when first person is off.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(&mut self, p: &mut Player, creatures: &mut Creatures, particles: &mut Particles, specials: &mut Specials, world: &World, pad: Pad) {
        if p.first_person == 3 {
            self.sounds.push(0x2A);
            self.cam_pitch = 0;
            self.cam_yaw = p.facing;
            self.pitch = 0;
            self.lag = [0, p.facing];
            self.yaw_speed = 0;
            self.pitch_speed = 0;
            self.breath = 0x7F8;
            self.dot = None;
            self.marker = None;
            self.target = None;
            self.fresh = false;
            self.lock = false;
            p.first_person = 4;
        } else if p.first_person == 4 && pad.l1_pressed {
            p.first_person = 5;
            self.sounds.push(0x2B);
        }
        if p.first_person == 4 {
            self.look(p, creatures, particles, specials, world, pad);
        }
        if p.first_person == 5 {
            for k in [self.marker.take(), self.dot.take()].into_iter().flatten() {
                particles.slots[k].life = 1;
            }
            p.target = p.facing;
            p.first_person = 0;
        }
        if p.first_person == 0 {
            p.hook_target = None;
            for id in MODEL {
                specials.set_scale(id, [0; 3]);
            }
        }
    }

    /// A scene taking the camera (0x4020F0) ends first person at once: the marker and dot go,
    /// the look is off (0x50A13C = 0) and the helmet and arms are hidden.
    pub fn cancel(&mut self, p: &mut Player, particles: &mut Particles, specials: &mut Specials) {
        if p.first_person == 0 {
            return;
        }
        for k in [self.marker.take(), self.dot.take()].into_iter().flatten() {
            particles.slots[k].life = 1;
        }
        p.target = p.facing;
        p.first_person = 0;
        p.hook_target = None;
        for id in MODEL {
            specials.set_scale(id, [0; 3]);
        }
    }

    /// Where target `t` is (object units): a creature's middle or a hook point.
    fn target_at(&self, creatures: &Creatures, t: usize) -> [i32; 3] {
        match t.checked_sub(HOOK) {
            Some(k) => self.hooks.get(k).map_or([0; 3], |h| h.map(|v| v * 32)),
            None => creatures.ellipsoid_at(t).0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn look(&mut self, p: &mut Player, creatures: &mut Creatures, particles: &mut Particles, specials: &mut Specials, world: &World, pad: Pad) {
        let eye = Self::eye(p);
        let within = |at: [i32; 3]| {
            let d = [0, 1, 2].map(|k| ((at[k] - eye[k]) >> 5) as i64);
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < 0x100_0000
        };
        // With hooks in hand, the hook points in reach too.
        let hooks: Vec<usize> = if p.hook_ammo > 0 { (0..self.hooks.len()).filter(|&k| within(self.hooks[k].map(|v| v * 32))).map(|k| HOOK + k).collect() } else { Vec::new() };
        // The creatures that can be targeted.
        let mut list: Vec<usize> = (0..creatures.slots.len())
            .filter(|&i| {
                let c = &creatures.slots[i];
                if c.id == 0 || !c.active || c.health <= 0 || c.rec.state == 0 || c.hurt < 0 {
                    return false;
                }
                within(creatures.ellipsoid_at(i).0)
            })
            .collect();
        list.extend(hooks);
        let found = self.target.and_then(|t| list.iter().position(|&s| s == t));
        let advance = match found {
            Some(_) => pad.r1_pressed && self.lock,
            None => {
                self.target = None;
                self.fresh = true;
                self.lock = false;
                true
            }
        };
        if advance {
            if let Some(m) = self.marker {
                particles.slots[m].size = [8, 8];
            }
            let next = match (self.target, found) {
                (None, _) => list.first().copied(),
                (Some(_), Some(i)) => Some(list[(i + 1) % list.len()]),
                (Some(_), None) => None,
            };
            if next.is_some() {
                if next != self.target || found.is_some() {
                    self.sounds.push(0x29);
                }
                self.fresh = false;
            } else if let Some(m) = self.marker.take() {
                particles.slots[m].life = 1;
            }
            self.target = next;
        }
        if let Some(t) = self.target {
            let at = self.target_at(creatures, t);
            let m = match self.marker.filter(|&m| particles.slots[m].life > 0 && particles.slots[m].typ == 0x30) {
                Some(m) => Some(m),
                None => particles.spawn(at, 0x30, 2),
            };
            if let Some(m) = m {
                particles.slots[m].pos = at;
                particles.slots[m].life = 10000;
            }
            self.marker = m;
        }
        // Turning and tilting with a little acceleration.
        let accel = |speed: &mut i32, neg: bool, pos: bool| {
            let (step, lim) = if neg {
                (-1, -0x200)
            } else if pos {
                (1, 0x200)
            } else {
                (-speed.signum(), 0)
            };
            *speed += step * 0x20;
            if step < 0 && *speed < lim || step >= 0 && *speed > lim {
                *speed = lim;
            }
        };
        accel(&mut self.yaw_speed, pad.dpad & crate::game::pad::LEFT != 0, pad.dpad & crate::game::pad::RIGHT != 0);
        accel(&mut self.pitch_speed, pad.dpad & crate::game::pad::DOWN != 0, pad.dpad & crate::game::pad::UP != 0);
        if !self.lock && pad.r1 {
            self.lock = true;
        }
        let (mut turn, mut tilt) = (self.yaw_speed, self.pitch_speed);
        if pad.dpad & 0xF0 == 0 {
            if self.lock
                && let (Some(t), false) = (self.target, self.fresh)
            {
                self.sounds.push(0x28);
                let at = self.target_at(creatures, t);
                let d = [0, 1, 2].map(|k| (at[k] - eye[k]) >> 5);
                let a = crate::boxes::angle12(d[0] as f32, d[2] as f32);
                let h = (d[0] as f32).hypot(d[2] as f32);
                let up = crate::boxes::angle12(h, -d[1] as f32);
                self.yaw_speed = 0;
                self.pitch_speed = 0;
                turn = wrap(a - p.facing) * 4;
                tilt = wrap(up - self.pitch - 0x400) * 4;
            }
        } else {
            self.lock = false;
            self.fresh = false;
        }
        if pad.mouse != [0; 2] {
            self.lock = false;
            turn += pad.mouse[0] * 16;
            tilt += pad.mouse[1] * 16;
        }
        p.facing = (p.facing + turn / 16) & 0xFFF;
        let mut v = (self.pitch + 0x400 & 0xFFF) + tilt / 16;
        if !(200..=0x720).contains(&v) {
            v = v.clamp(200, 0x720);
            self.pitch_speed = 0;
        }
        self.pitch = (v - 0x400) & 0xFFF;
        self.cam_pitch = (self.cam_pitch + wrap(self.pitch - self.cam_pitch) / 8) & 0xFFF;
        self.cam_yaw = (self.cam_yaw + wrap(p.facing - self.cam_yaw) / 8) & 0xFFF;
        p.look = [self.cam_pitch, self.cam_yaw];
        // Locked on a hook point (0x50A540 with 0x50A4FC ≥ 1000): firing throws the hook.
        p.hook_target = match self.target {
            Some(t) if self.lock && t >= HOOK => self.hooks.get(t - HOOK).copied(),
            _ => None,
        };
        // Where the view meets something: the dot, red over a creature.
        let pc = wrap(self.cam_pitch);
        let o = [eye[0], eye[1] + 0x400, eye[2]];
        let mut delta = [
            ((sin14(self.cam_yaw) as i64 * sin14(0x400 - pc) as i64 * 4) >> 13) as i32,
            sin14(pc) << 3,
            ((sin14(self.cam_yaw + 0x400) as i64 * sin14(0x400 - pc) as i64 * 4) >> 13) as i32,
        ];
        let ov = Vec3::from(o.map(|v| v as f32 / 32.0));
        let dv = Vec3::from(delta.map(|v| v as f32 / 32.0));
        let mut bits = 0;
        if let Some(h) = world.sweep(ov, dv, 1.0) {
            delta = (dv * h.t * 32.0).to_array().map(|v| v as i32);
            bits |= 1;
        }
        bits |= creatures.laser_ray(o, &mut delta, 0, p.facing);
        let end = [0, 1, 2].map(|k| o[k] + delta[k]);
        let d = match self.dot.filter(|&d| particles.slots[d].life > 0 && particles.slots[d].typ == 8) {
            Some(d) => Some(d),
            None => particles.spawn(end, 8, 5),
        };
        if let Some(d) = d {
            let q = &mut particles.slots[d];
            q.pos = end;
            q.life = 10000;
            q.rgb = if bits < 2 { [0, 0xFF, 0] } else { [0xFF, 0, 0] };
        }
        self.dot = d;
        // The helmet and arms at the eyes, the arms lagging, breathing.
        for id in MODEL {
            specials.set_scale(id, [0x800; 3]);
            specials.set_pos(id, eye.map(|v| v >> 5));
        }
        self.lag[0] = (self.lag[0] - wrap(self.lag[0] - self.pitch) / 6) & 0xFFF;
        self.lag[1] = (self.lag[1] - wrap(self.lag[1] - p.facing) / 6) & 0xFFF;
        let bob = sin14(self.breath) >> 10;
        let half = bob / 2;
        specials.turn_from_own(MODEL[0], [0, (p.facing - 0x400) & 0xFFF, (self.pitch + half) & 0xFFF]);
        specials.turn_from_own(MODEL[1], [0, (self.lag[1] - 0x400) & 0xFFF, (self.lag[0] + bob) & 0xFFF]);
        specials.turn_from_own(MODEL[2], [0, (self.lag[1] - 0x400) & 0xFFF, (half + self.lag[0]) & 0xFFF]);
        let before = self.breath;
        self.breath += if p.health < 2 { 0x40 } else { 0x20 };
        if before < 0x800 && self.breath >= 0x800 {
            self.sounds.push(0x13);
        }
        if before < 0x1000 && self.breath >= 0x1000 {
            self.sounds.push(0x14);
        }
        self.breath &= 0xFFF;
    }
}
