//! Gameplay core (prototype): collision against the level's render geometry and a simple
//! third-person player controller. Game space throughout: game units, +Y down, +Z forward.
//!
//! The original game has its own collision data (probably in `level.dat`); this uses the
//! visible triangles until that is decoded. Movement constants are placeholders to tune.

use crate::scene::Batch;
use glam::Vec3;
use std::collections::HashMap;

pub const PLAYER_HEIGHT: f32 = 460.0;
const RADIUS: f32 = 110.0;
/// Height of the wall-collision sphere centre above the feet.
const CHEST: f32 = 240.0;
const STEP_UP: f32 = 130.0;
const SNAP_DOWN: f32 = 80.0;
// Movement constants from Buzz's controller (0x436220 default ground profile), in object
// units per tick (level units × 32, 60 ticks per second), converted to level units/second.
const TICKS: f32 = 60.0;
const fn per_s(obj_per_tick: f32) -> f32 {
    obj_per_tick / 32.0 * TICKS
}
const fn per_s2(obj_per_tick2: f32) -> f32 {
    obj_per_tick2 / 32.0 * TICKS * TICKS
}
/// Max forward speed 0x380.
const RUN_SPEED: f32 = per_s(0x380 as f32);
/// Acceleration while pushing: accel 0x28 + friction 0x30 per tick (0x4343D0).
const RUN_ACCEL: f32 = per_s2((0x28 + 0x30) as f32);
/// Friction when not pushing: 0x30 per tick.
const RUN_FRICTION: f32 = per_s2(0x30 as f32);
/// Normal jump: vy = -0x600 (passed to 0x4340D0 by the controller).
const JUMP_SPEED: f32 = per_s(0x600 as f32);
/// Airborne gravity 0x40 per tick, capped at 0x800 (0x434D20). Very likely, not certain.
const GRAVITY: f32 = per_s2(0x40 as f32);
const TERMINAL_FALL: f32 = per_s(0x800 as f32);
const CELL: f32 = 1024.0;
/// |normal.y| above this is floor/ceiling, below is wall.
const FLOOR_NY: f32 = 0.6;

pub struct World {
    tris: Vec<[Vec3; 3]>,
    normals: Vec<Vec3>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    pub lo: Vec3,
    pub hi: Vec3,
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl World {
    pub fn from_batches(batches: &[Batch]) -> World {
        let mut tris = Vec::new();
        for b in batches {
            for t in b.vertices.chunks_exact(3) {
                tris.push([0, 1, 2].map(|i| Vec3::from(t[i].pos)));
            }
        }
        let mut normals = Vec::with_capacity(tris.len());
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (i, t) in tris.iter().enumerate() {
            normals.push((t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero());
            let tlo = t[0].min(t[1]).min(t[2]);
            let thi = t[0].max(t[1]).max(t[2]);
            lo = lo.min(tlo);
            hi = hi.max(thi);
            for cx in cell(tlo.x)..=cell(thi.x) {
                for cz in cell(tlo.z)..=cell(thi.z) {
                    grid.entry((cx, cz)).or_default().push(i as u32);
                }
            }
        }
        World { tris, normals, grid, lo, hi }
    }

    fn near(&self, p: Vec3, r: f32) -> impl Iterator<Item = u32> + '_ {
        let (x0, x1, z0, z1) = (cell(p.x - r), cell(p.x + r), cell(p.z - r), cell(p.z + r));
        let mut seen = std::collections::HashSet::new();
        (x0..=x1)
            .flat_map(move |cx| (z0..=z1).map(move |cz| (cx, cz)))
            .filter_map(|k| self.grid.get(&k))
            .flatten()
            .copied()
            .filter(move |i| seen.insert(*i))
    }

    /// Height (y, game space) of the nearest floor at or below `from_y` under (x, z).
    pub fn floor_below(&self, x: f32, z: f32, from_y: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for i in self.near(Vec3::new(x, 0.0, z), 0.0) {
            if self.normals[i as usize].y.abs() < FLOOR_NY {
                continue;
            }
            let [a, b, c] = self.tris[i as usize];
            let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
            if d.abs() < 1e-6 {
                continue;
            }
            let l1 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
            let l2 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
            let l3 = 1.0 - l1 - l2;
            if l1 < -1e-4 || l2 < -1e-4 || l3 < -1e-4 {
                continue;
            }
            let y = l1 * a.y + l2 * b.y + l3 * c.y;
            if y >= from_y && best.is_none_or(|b| y < b) {
                best = Some(y);
            }
        }
        best
    }

    /// Moves a sphere out of wall triangles. Returns the corrected centre.
    pub fn push_out(&self, mut c: Vec3, r: f32) -> Vec3 {
        for _ in 0..3 {
            let mut moved = false;
            for i in self.near(c, r).collect::<Vec<_>>() {
                if self.normals[i as usize].y.abs() >= FLOOR_NY {
                    continue;
                }
                let [a, b, t] = self.tris[i as usize];
                let q = closest_point_on_triangle(c, a, b, t);
                let d = c - q;
                let dist = d.length();
                if dist < r && dist > 1e-4 {
                    // Push horizontally only, so walls never lift or sink the player.
                    let push = Vec3::new(d.x, 0.0, d.z).normalize_or_zero() * (r - dist);
                    c += push;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        c
    }
}

/// Real-Time Collision Detection (Ericson), 5.1.5.
pub fn closest_point_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Input {
    /// Desired move direction in game space (x, z), length ≤ 1.
    pub move_xz: [f32; 2],
    pub jump: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Idle,
    Run,
    /// `rising` while moving upward (game -Y).
    Air { rising: bool },
}

#[derive(Debug, Clone)]
pub struct Player {
    /// Feet position, game space.
    pub pos: Vec3,
    pub vel: Vec3,
    /// Facing angle in radians around the vertical axis (0 = +Z).
    pub yaw: f32,
    pub grounded: bool,
    pub spawn: Vec3,
}

impl Player {
    pub fn new(pos: Vec3) -> Player {
        Player { pos, vel: Vec3::ZERO, yaw: 0.0, grounded: false, spawn: pos }
    }

    /// Horizontal speed in level units per second.
    pub fn horizontal_speed(&self) -> f32 {
        self.vel.x.hypot(self.vel.z)
    }

    pub fn motion(&self) -> Motion {
        if !self.grounded {
            Motion::Air { rising: self.vel.y < 0.0 }
        } else if self.vel.x.hypot(self.vel.z) > 10.0 {
            Motion::Run
        } else {
            Motion::Idle
        }
    }

    pub fn update(&mut self, world: &World, input: Input, dt: f32) {
        let [mx, mz] = input.move_xz;
        // Accelerate towards the pushed direction at full run speed, or brake by friction.
        let horiz = Vec3::new(self.vel.x, 0.0, self.vel.z);
        let target = Vec3::new(mx, 0.0, mz) * RUN_SPEED;
        let pushing = mx != 0.0 || mz != 0.0;
        let rate = if pushing { RUN_ACCEL } else { RUN_FRICTION };
        let delta = target - horiz;
        let step = rate * dt;
        let horiz = if delta.length() <= step { target } else { horiz + delta.normalize() * step };
        self.vel.x = horiz.x;
        self.vel.z = horiz.z;
        if pushing {
            self.yaw = mx.atan2(mz);
        }
        if input.jump && self.grounded {
            self.vel.y = -JUMP_SPEED; // up is -Y
            self.grounded = false;
        }
        self.vel.y = (self.vel.y + GRAVITY * dt).min(TERMINAL_FALL);

        let mut p = self.pos + self.vel * dt;
        let chest = world.push_out(p - Vec3::Y * CHEST, RADIUS);
        p.x = chest.x;
        p.z = chest.z;

        // Land on the nearest floor no more than STEP_UP above the feet.
        let was_grounded = self.grounded;
        self.grounded = false;
        if self.vel.y >= 0.0
            && let Some(f) = world.floor_below(p.x, p.z, p.y - STEP_UP)
            && (p.y >= f || (was_grounded && f - p.y <= SNAP_DOWN))
        {
            p.y = f;
            self.vel.y = 0.0;
            self.grounded = true;
        }
        self.pos = p;

        if self.pos.y > world.hi.y + 4000.0 {
            *self = Player::new(self.spawn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Batch, GpuVertex};

    fn v(x: f32, y: f32, z: f32) -> GpuVertex {
        GpuVertex { pos: [x, y, z], uv: [0.0; 2], color: [0; 4] }
    }

    /// A 4000×4000 floor at y = 0 with a wall at x = 1000.
    fn room() -> World {
        let floor = [v(-2000., 0., -2000.), v(2000., 0., -2000.), v(2000., 0., 2000.), v(-2000., 0., -2000.), v(2000., 0., 2000.), v(-2000., 0., 2000.)];
        let wall = [v(1000., 0., -2000.), v(1000., -1000., -2000.), v(1000., -1000., 2000.), v(1000., 0., -2000.), v(1000., -1000., 2000.), v(1000., 0., 2000.)];
        World::from_batches(&[Batch { vertices: floor.iter().chain(&wall).copied().collect(), ..Default::default() }])
    }

    #[test]
    fn falls_and_lands_on_floor() {
        let w = room();
        let mut p = Player::new(Vec3::new(0.0, -500.0, 0.0));
        for _ in 0..120 {
            p.update(&w, Input::default(), 1.0 / 60.0);
        }
        assert!(p.grounded);
        assert!(p.pos.y.abs() < 1e-3, "{:?}", p.pos);
    }

    #[test]
    fn wall_stops_running() {
        let w = room();
        let mut p = Player::new(Vec3::new(0.0, 0.0, 0.0));
        for _ in 0..240 {
            p.update(&w, Input { move_xz: [1.0, 0.0], jump: false }, 1.0 / 60.0);
        }
        assert!(p.pos.x < 1000.0 - RADIUS + 5.0 && p.pos.x > 800.0, "{:?}", p.pos);
    }

    #[test]
    fn jump_goes_up_then_lands() {
        let w = room();
        let mut p = Player::new(Vec3::ZERO);
        p.update(&w, Input::default(), 1.0 / 60.0);
        p.update(&w, Input { move_xz: [0.0; 2], jump: true }, 1.0 / 60.0);
        let mut peak: f32 = 0.0;
        for _ in 0..120 {
            p.update(&w, Input::default(), 1.0 / 60.0);
            peak = peak.min(p.pos.y);
        }
        assert!(peak < -300.0, "peak {peak}");
        assert!(p.grounded);
    }
}
