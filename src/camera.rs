//! Buzz's follow camera, after the original's camera update 0x4045E0 (set up by 0x403450),
//! in the default "auto" mode (option bit 0x40 of 0x52F0CC, set at start-up by 0x49D910).
//! Game space, object units (level units × 32), 12-bit angles.
//!
//! Ported: the two-stage height follow, the yaw that chases Buzz's facing while he moves
//! (with the turn when he runs towards the camera), the swing behind him after he stops, the
//! pitch resting at 0x40, distance 0x4B0 shortened when geometry blocks the view, and the
//! position and aim lagging by 1/(lag/2) per tick. Not yet: the wall "whisker" yaw nudges and
//! pitch changes (0x48C860 probes), the stomp shake, manual mode (L2/R2 orbit, Triangle).

use crate::game::{Player, World};
use glam::Vec3;

fn sin12(a: i32) -> i32 {
    let t = (a & 0xFFF) as f64 * std::f64::consts::TAU / 4096.0;
    ((16384.0 * t.sin()).round() as i32) >> 2
}

/// Signed difference a − b of 12-bit angles, in −0x800..0x800.
fn diff12(a: i32, b: i32) -> i32 {
    let d = (a - b) & 0xFFF;
    if d > 0x7FF { d - 0x1000 } else { d }
}

/// Camera distance target (0x50A128) and lag (0x52AD98) as 0x403450 sets them.
const DISTANCE: i32 = 0x4B0;
const LAG: i32 = 0x10;
const REST_PITCH: i32 = 0x40;
/// Auto-follow rate divisor while running (0x2A; 0x60 while skidding).
const FOLLOW: i32 = 0x2A;
const FOLLOW_SKID: i32 = 0x60;
/// Radius kept clear around the camera (level units).
const CLEARANCE: f32 = 24.0;

#[derive(Debug, Clone)]
pub struct Camera {
    /// Position, object units (param +0..+8).
    pub pos: [i32; 3],
    /// Height followers (+0x18, +0x1C).
    height_target: i32,
    height: i32,
    /// Orbit yaw (+0x28), pitch (+0x2E), distance (+0x26).
    pub yaw: i32,
    pub pitch: i32,
    pub distance: i32,
    /// Aim: yaw (+0x22) and pitch (+0x20) the camera looks along.
    pub look_yaw: i32,
    pub look_pitch: i32,
    /// Ticks Buzz has stood still (0x50A534) and the swing rate it set (0x50A4E4).
    idle: i32,
    swing: i32,
    /// 0x50A118: a point (object units) to turn towards this tick (a boss), then cleared.
    pub focus: Option<[i32; 3]>,
}

impl Camera {
    /// 0x403450: behind Buzz, 0x4B0 away, 0x3200 above the floor there.
    pub fn new(buzz: &Player) -> Camera {
        let b = obj_pos(buzz);
        let f = buzz.facing;
        let pos = [b[0] + ((sin12(f - 0x800) * 4) * DISTANCE >> 9), b[1] - 0x3200, b[2] + ((sin12(f - 0x400) * 4) * DISTANCE >> 9)];
        Camera { pos, height_target: b[1], height: b[1], yaw: f, pitch: REST_PITCH, distance: DISTANCE, look_yaw: f, look_pitch: 0, idle: 0, swing: 0, focus: None }
    }

    /// One game tick.
    pub fn tick(&mut self, buzz: &Player, world: &World) {
        let b = obj_pos(buzz);
        let facing = buzz.facing;
        // Distance eases back to its target at 0x20 per tick.
        self.distance += (DISTANCE - self.distance).clamp(-0x20, 0x20);
        let still = buzz.vel[0].abs() < 4 && buzz.vel[2].abs() < 4 && buzz.coyote != 0;
        if still || (buzz.climb > 0 && buzz.state == crate::buzz::state::CLIMB_HANG) {
            // Standing: after a moment, swing round behind Buzz.
            let before = self.idle;
            self.idle += 1;
            if before < 0x42 && self.idle > 0x41 {
                self.idle += 0x42;
            }
            self.idle = self.idle.min(0xF0);
            let min_rate = 0x140;
            if self.idle < 100 {
                self.idle = 100;
                self.swing = diff12(facing, self.yaw);
            }
            if self.swing.abs() < min_rate || self.idle > 0xA4 {
                self.swing = min_rate;
            }
            let rate = (self.swing * (self.idle - 100).min(8)).abs() / 0x180;
            let d = diff12(self.yaw, facing);
            if d < 0 {
                self.yaw = if d + rate >= 0 { facing } else { self.yaw + rate };
            } else if d > 0 {
                self.yaw = if d - rate <= 0 { facing } else { self.yaw - rate };
            }
            self.yaw &= 0xFFF;
            self.ease_pitch();
            self.height_target -= (self.height_target - b[1] + 0x1000) >> 5;
            self.height -= (self.height - self.height_target) / 16;
        } else {
            self.idle = 0;
            self.height_target = self.height_target.clamp(b[1] - 0x4000, b[1] + 0x4000);
            if (buzz.coyote != 0 && buzz.forward != 0) || b[1] < self.height_target + 0x1000 {
                self.height_target -= (self.height_target - b[1] + 0x1000) >> 5;
            }
            self.height -= (self.height - self.height_target) / 16;
            // Chase Buzz's facing; if he runs towards the camera, turn the other way round.
            let mut d = diff12(self.yaw, facing);
            if d.abs() > 0x600 {
                d = ((if d < 1 { -0x800 } else { 0x800 }) - d) * 3;
            }
            let rate = if buzz.skid != 0 { FOLLOW_SKID } else { FOLLOW };
            self.yaw = (self.yaw - d / (rate * 2)) & 0xFFF;
            self.ease_pitch();
        }
        // A boss fight: the orbit turns an eighth of the way towards facing it each tick.
        if let Some(f) = self.focus.take() {
            let a = crate::boxes::angle12((f[0] - b[0]) as f32, (f[2] - b[2]) as f32);
            self.yaw = (self.yaw - diff12(self.yaw, a) / 8) & 0xFFF;
        }
        // Shorten the distance when something is between Buzz and where the camera wants to be.
        let head = Vec3::new(b[0] as f32, (b[1] - 0x1000) as f32, b[2] as f32) / 32.0;
        let offset = self.offset(self.distance);
        let to = Vec3::new(offset[0] as f32, offset[1] as f32, offset[2] as f32) / 32.0;
        if let Some(h) = world.sweep(head, to, CLEARANCE) {
            self.distance = ((self.distance as f32 * h.t) as i32).max(10);
        }
        // Where the camera wants to be, and the lag towards it.
        let off = self.offset(self.distance);
        let target = [b[0] + off[0], self.height - 0x2000 + off[1], b[2] + off[2]];
        let div = (LAG * 2 / 4).max(1);
        for k in 0..3 {
            self.pos[k] -= (self.pos[k] - target[k]) / div;
        }
        // Keep the camera on Buzz's side of any wall: the lag and the smoothed height can carry it
        // through geometry the distance check above didn't see (the original's 0x48D530 pull-in).
        let cam = self.position();
        if let Some(h) = world.sweep(head, cam - head, CLEARANCE) {
            let safe = head + (cam - head) * (h.t - 0.05).max(0.0);
            self.pos = [safe.x, safe.y, safe.z].map(|v| (v * 32.0) as i32);
        }
        // Aim at Buzz's chest, lagging the same way.
        let (dx, dy, dz) = ((b[0] - self.pos[0]) as f64, ((b[1] - 0x1800) - self.pos[1]) as f64, (b[2] - self.pos[2]) as f64);
        let want_yaw = ((dx.atan2(dz) / std::f64::consts::TAU * 4096.0).round() as i32) & 0xFFF;
        let want_pitch = ((-dy).atan2(dx.hypot(dz)) / std::f64::consts::TAU * 4096.0).round() as i32;
        let half = (LAG / 2).max(1);
        self.look_yaw = (self.look_yaw + diff12(want_yaw, self.look_yaw) / half) & 0xFFF;
        self.look_pitch += diff12(want_pitch & 0xFFF, self.look_pitch & 0xFFF) / half;
        // Too close to Buzz: rise above him.
        let (hx, hy, hz) = ((b[0] - self.pos[0]) >> 5, (b[1] - self.pos[1] - 0x1000) >> 5, (b[2] - self.pos[2]) >> 5);
        if hx * hx + hy * hy + hz * hz < 160_000 {
            let flat = ((hx * hx + hz * hz) as f64).sqrt();
            let up = (160_000.0 - flat * flat).max(0.0).sqrt() as i32;
            self.pos[1] = b[1] - up * 32 - 0x1000;
        }
    }

    /// Pitch rests at 0x40, easing by 8 per tick.
    fn ease_pitch(&mut self) {
        if self.pitch < REST_PITCH {
            self.pitch = (self.pitch + 8).min(REST_PITCH);
        } else if self.pitch > REST_PITCH {
            self.pitch = (self.pitch - 8).max(REST_PITCH);
        }
    }

    /// Offset from Buzz to the camera for the current yaw and pitch at `distance`.
    fn offset(&self, distance: i32) -> [i32; 3] {
        let flat = (sin12(self.pitch + 0x400) * 4) * distance >> 14;
        [
            -((sin12(self.yaw) * 4) * flat >> 9),
            (sin12(self.pitch - 0x800) * 4) * distance >> 9,
            -((sin12(self.yaw + 0x400) * 4) * flat >> 9),
        ]
    }

    /// Position in level units, game space.
    pub fn position(&self) -> Vec3 {
        Vec3::new(self.pos[0] as f32, self.pos[1] as f32, self.pos[2] as f32) / 32.0
    }

    /// Unit view direction in game space (+Y down).
    pub fn forward(&self) -> Vec3 {
        let (y, p) = (self.look_yaw as f32 / 4096.0 * std::f32::consts::TAU, self.look_pitch as f32 / 4096.0 * std::f32::consts::TAU);
        Vec3::new(y.sin() * p.cos(), -p.sin(), y.cos() * p.cos())
    }
}

fn obj_pos(p: &Player) -> [i32; 3] {
    [(p.pos.x * 32.0) as i32, (p.pos.y * 32.0) as i32, (p.pos.z * 32.0) as i32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_behind_buzz_and_looks_at_him() {
        let mut p = Player::new(Vec3::ZERO);
        p.set_facing(0);
        let w = World::from_triangles(Vec::new());
        let mut c = Camera::new(&p);
        for _ in 0..120 {
            c.tick(&p, &w);
        }
        let pos = c.position();
        // Behind (−Z), above (−Y), about 1200 away horizontally.
        assert!(pos.z < -1000.0 && pos.z > -1300.0, "{pos:?}");
        assert!(pos.y < -200.0, "{pos:?}");
        let f = c.forward();
        assert!(f.z > 0.8 && f.y > 0.0, "looks forward and down at Buzz: {f:?}");
    }
}
