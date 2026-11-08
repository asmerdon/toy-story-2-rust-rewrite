//! Buzz's follow camera, after the original's camera update 0x4045E0 (set up by 0x403450),
//! in the default "auto" mode (option bit 0x40 of 0x52F0CC, set at start-up by 0x49D910).
//! Game space, object units (level units × 32), 12-bit angles.
//!
//! Ported: the two-stage height follow, the yaw that chases Buzz's facing while he moves
//! (with the turn when he runs towards the camera), the swing behind him after he stops, the
//! pitch resting at 0x40, distance 0x4B0 shortened when geometry blocks the view, and the
//! position and aim lagging by 1/(lag/2) per tick, the shake (0x50A510) wobbling the aim's
//! pitch, the wall "whisker" yaw nudges and pitch changes, the distance cut by walls (0x48C860,
//! 0x48D530; both ignore collision objects flagged 0x200) and the pull onto Buzz's height
//! while he rides a mover (0x50A4E0). Not yet: manual mode (L2/R2 orbit, Triangle).

use crate::game::{Hit, Player, World};
use crate::script::sin_table;
use glam::Vec3;

fn sin12(a: i32) -> i32 {
    let t = (a & 0xFFF) as f64 * std::f64::consts::TAU / 4096.0;
    ((16384.0 * t.sin()).round() as i32) >> 2
}

/// 0x4045E0's camera shake: while 0x50A510 is above 0 the aim's pitch target is nudged by
/// sin((−3s & 0x1F) × 0x80) / ((s − 0x32) × 16), a wobble of eleven ticks a cycle that
/// fades from about ±100 (s = 0x28) as `s` counts down a tick at a time.
pub fn shake_wobble(world: &World) -> i32 {
    let s = world.shake.get();
    if s <= 0 {
        return 0;
    }
    world.shake.set((s - 1).max(0));
    (sin12(((-3 * s) & 0x1F) * 0x80) * 4) / ((s - 0x32) * 0x10)
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
    /// 0x50A4E0: ramps up to 0xC0 while riding a mover.
    ride: i32,
    /// +0x32 (1 / 2 turn one way / the other, 4 / 8 pitch down / up) and its turn rate
    /// (0x50A4B4).
    whisker: u16,
    whisker_rate: i32,
}

impl Camera {
    /// 0x403450: behind Buzz, 0x4B0 away, 0x3200 above the floor there.
    pub fn new(buzz: &Player) -> Camera {
        let b = obj_pos(buzz);
        let f = buzz.facing;
        let pos = [b[0] + ((sin12(f - 0x800) * 4) * DISTANCE >> 9), b[1] - 0x3200, b[2] + ((sin12(f - 0x400) * 4) * DISTANCE >> 9)];
        Camera { pos, height_target: b[1], height: b[1], yaw: f, pitch: REST_PITCH, distance: DISTANCE, look_yaw: f, look_pitch: 0, idle: 0, swing: 0, focus: None, ride: 0, whisker: 0, whisker_rate: 0 }
    }

    /// One game tick.
    pub fn tick(&mut self, buzz: &Player, world: &World) {
        let b = obj_pos(buzz);
        let facing = buzz.facing;
        // 0x50A4E0: riding a mover (0x48E1B0: the floor object is type 8), the height follow is
        // pulled onto Buzz, so a lift doesn't drag the camera behind him.
        if buzz.on_floor && buzz.floor_mover.is_some() {
            self.ride = (self.ride + 2).min(0xC0);
        } else {
            self.ride = (self.ride - 1).max(0);
        }
        // Distance eases back to its target at 0x20 per tick.
        self.distance += (DISTANCE - self.distance).clamp(-0x20, 0x20);
        let still = (buzz.vel[0].abs() < 4 && buzz.vel[2].abs() < 4 && buzz.mantle == 0 && buzz.coyote != 0) || (buzz.climb > 0 && buzz.state == crate::buzz::state::CLIMB_HANG);
        if still {
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
            // On a pole (0x53C668) the pitch only comes down from above its rest.
            if self.pitch > REST_PITCH || buzz.climb < 1 {
                self.ease_pitch();
            }
            self.height_target -= (self.height_target - b[1] + 0x1000) >> 5;
            self.height -= (self.height - self.height_target) / 16;
            self.ride_pull(b[1]);
            self.turn_to_focus(b);
            // Where the camera would go, seen from Buzz: a wall in the way shortens the
            // distance at once. The ball swept grows the longer he has stood (0x4045E0).
            let off = self.offset(DISTANCE);
            let delta = [off[0], self.height - b[1] + off[1], off[2]];
            let r = if self.idle < 0x42 { (self.idle / 2) * 0x50 + 200 } else { 0xB18 };
            if let Some((cut, _)) = camera_ray(world, [b[0], b[1] - 0x1F00, b[2]], delta, r) {
                self.distance = length_level(cut).max(10);
            }
        } else {
            self.idle = 0;
            self.height_target = self.height_target.clamp(b[1] - 0x4000, b[1] + 0x4000);
            if (buzz.coyote != 0 && buzz.forward != 0) || b[1] < self.height_target + 0x1000 {
                self.height_target -= (self.height_target - b[1] + 0x1000) >> 5;
            }
            self.height -= (self.height - self.height_target) / 16;
            self.ride_pull(b[1]);
            if buzz.hover != 0 && b[1] - 0x1000 < self.height {
                self.height = b[1] - 0x1000;
            }
            // Chase Buzz's facing; if he runs towards the camera, turn the other way round.
            let mut d = diff12(self.yaw, facing);
            if d.abs() > 0x600 {
                d = ((if d < 1 { -0x800 } else { 0x800 }) - d) * 3;
            }
            let rate = if buzz.skid != 0 { FOLLOW_SKID } else { FOLLOW };
            self.yaw = (self.yaw - d / (rate * 2)) & 0xFFF;
            // Zip line, rocket boots, sliding or the hook's pull: swing in behind him as well.
            if buzz.zip != 0 || buzz.rocket != 0 || buzz.slip != 0 || buzz.grapple == 2 {
                self.yaw = (self.yaw - diff12(self.yaw, facing).clamp(-0x200, 0x200) / 32) & 0xFFF;
            }
            self.whiskers(buzz, b, world);
            self.turn_to_focus(b);
        }
        // Where the camera wants to be, and the lag towards it.
        let off = self.offset(self.distance);
        let target = [b[0] + off[0], self.height - 0x2000 + off[1], b[2] + off[2]];
        let div = (LAG * 2 / 4).max(1);
        for k in 0..3 {
            self.pos[k] -= (self.pos[k] - target[k]) / div;
        }
        // Aim at Buzz's chest, lagging the same way.
        let (dx, dy, dz) = ((b[0] - self.pos[0]) as f64, ((b[1] - 0x1800) - self.pos[1]) as f64, (b[2] - self.pos[2]) as f64);
        let want_yaw = ((dx.atan2(dz) / std::f64::consts::TAU * 4096.0).round() as i32) & 0xFFF;
        // This pitch runs the opposite way to +0x20's, so the shake is taken off.
        let want_pitch = ((-dy).atan2(dx.hypot(dz)) / std::f64::consts::TAU * 4096.0).round() as i32 - shake_wobble(world);
        let half = (LAG / 2).max(1);
        self.look_yaw = (self.look_yaw + diff12(want_yaw, self.look_yaw) / half) & 0xFFF;
        self.look_pitch += diff12(want_pitch & 0xFFF, self.look_pitch & 0xFFF) / half;
        // Moving: a wall between the camera and Buzz takes its length off the distance, and a
        // near one brings the camera straight to it (0x48D530).
        if !still {
            let to_buzz = [b[0] - self.pos[0], b[1] - 0x1800 - self.pos[1], b[2] - self.pos[2]];
            if let Some((cut, _)) = camera_ray(world, self.pos, to_buzz, 32) {
                let d = length_level(cut);
                self.distance = (self.distance - d).max(10);
                if d < 300 {
                    for k in 0..3 {
                        self.pos[k] += cut[k];
                    }
                }
            }
        }
        // Too close to Buzz: rise above him.
        let (hx, hy, hz) = ((b[0] - self.pos[0]) >> 5, (b[1] - self.pos[1] - 0x1000) >> 5, (b[2] - self.pos[2]) >> 5);
        if hx * hx + hy * hy + hz * hz < 160_000 {
            let up = ((160_000 - hx * hx - hz * hz) as f64).sqrt() as i32;
            self.pos[1] = b[1] - up * 32 - 0x1000;
        }
    }

    /// 0x50A4E0's pull on the height follow while riding a mover.
    fn ride_pull(&mut self, y: i32) {
        if self.ride != 0 {
            let e = y - self.height - 0x1000;
            self.height = y - (e / 256) * (0x100 - self.ride) - 0x1000;
        }
    }

    /// 0x50A118: a point (a boss) the orbit turns an eighth of the way towards each tick.
    fn turn_to_focus(&mut self, b: [i32; 3]) {
        if let Some(f) = self.focus.take() {
            let a = crate::boxes::angle12((f[0] - b[0]) as f32, (f[2] - b[2]) as f32);
            self.yaw = (self.yaw - diff12(self.yaw, a) / 8) & 0xFFF;
        }
    }

    /// While Buzz moves (0x4045E0, +0x32): three rays towards where the camera would go, from
    /// his left, his right and low down. A wall on one side turns the orbit away from it; on
    /// both sides or below it picks by how far the camera is off his facing, and lifts or
    /// drops the pitch. Ceilings (a normal more than 12000 of 16384 downwards) count as both.
    fn whiskers(&mut self, buzz: &Player, b: [i32; 3], world: &World) {
        let off = self.offset(self.distance);
        let dy = self.height - b[1] + off[1];
        let (sx, sz) = (sin_table(self.yaw + 0x400) >> 3, sin_table(self.yaw - 0x800) >> 3);
        let side_y = b[1] - 0x1000 - 0xF00;
        let ceiling = |h: &Hit| h.normal.y * 16384.0 > 12000.0;
        let mut hits = 0u32;
        if let Some((_, h)) = camera_ray(world, [b[0] - sx, side_y, b[2] - sz], [off[0] - sx, dy - 0x800, off[2] - sz], 200) {
            hits |= if ceiling(&h) { 9 } else { 1 };
        }
        if let Some((_, h)) = camera_ray(world, [b[0] + sx, side_y, b[2] + sz], [off[0] + sx, dy - 0x800, off[2] + sz], 200) {
            hits |= if ceiling(&h) { 10 } else { 2 };
        }
        if camera_ray(world, [b[0], b[1] - 0x1000 + 0xC00, b[2]], [off[0], dy, off[2]], 200).is_some() {
            hits |= 4;
        }
        let pick = if hits & 8 != 0 { 3 } else { hits & 7 };
        self.whisker = match pick {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 4,
            4 => 8,
            _ => {
                let d = diff12(self.yaw, buzz.facing);
                match d.abs() {
                    a if a >= 0x401 => {
                        if d < 1 {
                            10
                        } else {
                            9
                        }
                    }
                    a if a >= 0x201 => {
                        if d < 1 {
                            2
                        } else {
                            1
                        }
                    }
                    _ => {
                        if buzz.vel[1] < -0x80 {
                            4
                        } else {
                            8
                        }
                    }
                }
            }
        };
        match self.whisker & 3 {
            0 => {
                self.whisker_rate = if self.whisker_rate > 0 { (self.whisker_rate - 0x10).max(0) } else { (self.whisker_rate + 0x10).min(0) };
            }
            k => {
                if k == 1 {
                    self.whisker_rate = (self.whisker_rate.min(0) - 8).max(-0x60);
                }
                if k == 2 {
                    self.whisker_rate = (self.whisker_rate.max(0) + 8).min(0x60);
                }
                self.yaw = (self.yaw + self.whisker_rate / 2) & 0xFFF;
            }
        }
        if self.whisker & 4 != 0 {
            self.pitch = (self.pitch - 8).max(-0x200);
        }
        if self.whisker & 8 != 0 {
            self.pitch = (self.pitch + 8).min(0x300);
        }
        if self.whisker & 0xC == 0 {
            self.ease_pitch();
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
        let flat = sin_table(self.pitch + 0x400) * distance >> 14;
        [-(sin_table(self.yaw) * flat >> 9), sin_table(self.pitch - 0x800) * distance >> 9, -(sin_table(self.yaw + 0x400) * flat >> 9)]
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

/// 0x48C860 / 0x48D530 as the camera calls them: a ball of radius `r` (object units) swept from
/// `from` along `delta` through everything but objects flagged 0x200. The part of `delta` up
/// to the hit, and the hit.
fn camera_ray(world: &World, from: [i32; 3], delta: [i32; 3], r: i32) -> Option<([i32; 3], Hit)> {
    let v = |a: [i32; 3]| Vec3::new(a[0] as f32, a[1] as f32, a[2] as f32) / 32.0;
    let h = world.camera_sweep(v(from), v(delta), r as f32 / 32.0)?;
    Some((delta.map(|d| (d as f32 * h.t) as i32), h))
}

/// A vector's length in level units, as the camera measures it (each axis >> 5 first).
fn length_level(v: [i32; 3]) -> i32 {
    let [x, y, z] = v.map(|k| (k >> 5) as i64);
    ((x * x + y * y + z * z) as f64).sqrt() as i32
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

    /// 0x4045E0: a shake (0x50A510 = 0x28, as a stomp sets it) wobbles the aim's pitch both
    /// ways, counts down one a tick, and leaves the aim where it was once over.
    #[test]
    fn shake_wobbles_the_aim_and_dies_away() {
        let p = Player::new(Vec3::ZERO);
        let w = World::from_triangles(Vec::new());
        let mut c = Camera::new(&p);
        for _ in 0..200 {
            c.tick(&p, &w);
        }
        let rest = c.look_pitch;
        w.shake.set(0x28);
        let (mut lo, mut hi) = (rest, rest);
        for _ in 0..0x28 {
            c.tick(&p, &w);
            lo = lo.min(c.look_pitch);
            hi = hi.max(c.look_pitch);
        }
        assert_eq!(w.shake.get(), 0, "counted down a tick at a time");
        assert!(lo < rest - 4 && hi > rest + 4, "wobbles both ways: {lo} {rest} {hi}");
        for _ in 0..100 {
            c.tick(&p, &w);
        }
        // The lag divides by 8 and truncates: it stops within 7 of its target on either side.
        assert!((c.look_pitch - rest).abs() < 15, "settles back: {} {rest}", c.look_pitch);
        // The first tick's nudge: sin((−3 × 0x28) & 0x1F × 0x80) / ((0x28 − 0x32) × 16).
        w.shake.set(0x28);
        assert_eq!(shake_wobble(&w), (sin12(0x400) * 4) / -160);
    }
}
