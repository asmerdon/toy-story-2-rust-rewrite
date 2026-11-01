//! Rain (levels 5 and 14): the drops of 0x44ED90 / 0x44F010 and the splashes the level
//! scripts (0x41E880, 0x42E790) put on the floor ahead of the camera.
//!
//! - 64 drops. One is made a tick (0x100 of 0x100 per drop) around a point ahead of the
//!   camera: up to 0x200 × 0x20 either side, out to (0..0x11F) × 0x200 at its yaw ± 90°,
//!   0x14000 above it. Each falls 0x1000 a tick (more while Buzz falls faster than 0x400:
//!   twice his speed on top) and goes 0x24000 below where it started.
//! - Each is drawn as a 30 × 200 unit quad of the level's sprite (0x33 on level 14, 0x34 on
//!   level 5) in dim grey (0x404040), facing the camera.
//! - Every 8 ticks a splash (particle 0x1B, mode 2) lands on the floor at a random spot up to
//!   0x80 × 0x100 either side of a point 3 sines (0xC000) ahead of the camera, under a probe
//!   from 0x8000 above the camera, if that floor is below the camera; on the water instead
//!   when the water is higher (level 5), and none with the camera under water.
//!
//! Units: object units, 12-bit angles.

use crate::particles::Particles;
use crate::pickups::SpriteDef;
use crate::scene::{self, Batch, Blend, GpuVertex};
use glam::Vec3;

const DROPS: usize = 64;

#[derive(Debug, Clone)]
pub struct Rain {
    /// x, y, z and the height it stops at; 0 = free.
    drops: [[i32; 4]; DROPS],
    /// 0x557708: drops owed, 0x100 each.
    owed: i32,
    /// The sprite the drops are drawn with (0x557AA4).
    pub sprite: u8,
    clock: u32,
}

fn sin14(a: i32) -> i32 {
    crate::script::sin_table(a)
}

impl Rain {
    pub fn new(sprite: u8) -> Rain {
        Rain {
            drops: [[0; 4]; DROPS],
            owed: 0,
            sprite,
            clock: 0,
        }
    }

    /// One tick. `cam` is the camera (object units), `yaw` its heading (12-bit, sin = x),
    /// `buzz_vy` Buzz's vertical speed, `water` the level's water surface when the camera
    /// can be under it (level 5).
    pub fn tick(
        &mut self,
        world: &crate::game::World,
        particles: &mut Particles,
        cam: [i32; 3],
        yaw: i32,
        buzz_vy: i32,
        water: Option<i32>,
    ) {
        self.clock = self.clock.wrapping_add(1);
        // The drops fall.
        let speed = 0x1000 + if buzz_vy > 0x400 { buzz_vy * 2 } else { 0 };
        let mut free = Vec::new();
        for (i, d) in self.drops.iter_mut().enumerate() {
            if d[3] == 0 {
                free.push(i);
                continue;
            }
            d[1] += speed;
            if d[3] < d[1] {
                d[3] = 0;
            }
        }
        // New ones ahead of the camera.
        self.owed += 0x100;
        while self.owed > 0xFF {
            let Some(i) = free.pop() else { break };
            let turn = particles.rand() as i32 * 8 - 0x400;
            let out = particles.rand() as i32;
            let (jx, jz) = (
                particles.rand() as i32 - 0x80,
                particles.rand() as i32 - 0x80,
            );
            let a = yaw + turn;
            let x = cam[0] + jx * 0x20 + (sin14(a) * out >> 5);
            let z = cam[2] + jz * 0x20 + (sin14(a + 0x400) * out >> 5);
            let y = cam[1] - 0x14000;
            self.drops[i] = [x, y, z, y + 0x24000];
            self.owed -= 0x100;
        }
        // Splashes (every 8 ticks), not with the camera under water.
        if self.clock.is_multiple_of(8) && water.is_none_or(|w| w - cam[1] >= 0) {
            let (jx, jz) = (
                particles.rand() as i32 - 0x80,
                particles.rand() as i32 - 0x80,
            );
            let x = jx * 0x100 + cam[0] + sin14(yaw) * 3;
            let z = jz * 0x100 + cam[2] + sin14(yaw + 0x400) * 3;
            let from = (cam[1] - 0x8000) as f32 / 32.0;
            if let Some(f) = world.floor_below(x as f32 / 32.0, z as f32 / 32.0, from) {
                let mut f = (f * 32.0) as i32;
                if let Some(w) = water {
                    f = f.min(w);
                }
                if cam[1] < f {
                    particles.spawn([x, f, z], 0x1B, 2);
                }
            }
        }
    }

    /// 0x44F010: the drops as camera-facing quads. `right` / `up` are the camera's axes in
    /// game space; `texture(slot)` gives a `texNN` slot's index and size.
    pub fn batches(
        &self,
        sprites: &[Option<SpriteDef>],
        texture: &dyn Fn(i16) -> Option<(usize, [f32; 2])>,
        right: Vec3,
        up: Vec3,
    ) -> Vec<Batch> {
        let mut out = Vec::new();
        let Some(def) = sprites.get(self.sprite as usize).and_then(|d| d.as_ref()) else {
            return out;
        };
        let Some((tex, [tw, th])) = texture(def.texture) else {
            return out;
        };
        let [cu, cv] = def.cells.first().copied().unwrap_or([0, 0]);
        let (u0, v0) = (cu as f32 / tw, cv as f32 / th);
        let (u1, v1) = (
            (cu as f32 + def.size[0] as f32) / tw,
            (cv as f32 + def.size[1] as f32) / th,
        );
        let color = [0xFF, 0x40, 0x40, 0x40];
        for d in self.drops.iter().filter(|d| d[3] != 0) {
            let c = Vec3::new(d[0] as f32, d[1] as f32, d[2] as f32) / 32.0;
            let v = |sx: f32, sy: f32, u: f32, vv: f32| GpuVertex {
                pos: (c + right * (sx * 30.0) + up * (sy * 200.0)).to_array(),
                uv: [u, vv],
                color,
            };
            let (q0, q1, q2, q3) = (
                v(-1.0, 1.0, u0, v0),
                v(1.0, 1.0, u1, v0),
                v(1.0, -1.0, u1, v1),
                v(-1.0, -1.0, u0, v1),
            );
            scene::push(&mut out, Some(tex), Blend::Additive, [q0, q1, q2]);
            scene::push(&mut out, Some(tex), Blend::Additive, [q0, q2, q3]);
        }
        out
    }

    /// Drops falling now.
    pub fn falling(&self) -> usize {
        self.drops.iter().filter(|d| d[3] != 0).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_fall_and_are_replaced() {
        let world = crate::game::World::from_triangles(vec![]);
        let mut p = Particles::new(None, vec![0x40, 0x90, 0x10, 0xF0]);
        let mut r = Rain::new(0x33);
        for _ in 0..40 {
            r.tick(&world, &mut p, [0, 0, 0], 0, 0, None);
        }
        // One a tick, each living 0x24000 / 0x1000 = 36 ticks.
        assert!((34..=40).contains(&r.falling()), "{}", r.falling());
        assert!(
            r.drops
                .iter()
                .filter(|d| d[3] != 0)
                .all(|d| d[1] >= -0x14000 && d[1] <= -0x14000 + 0x24000)
        );
    }
}
