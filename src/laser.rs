//! Buzz's wrist laser: the beams a shot leaves (four slots at 0x882970), as 0x4A5D30 fires
//! them and 0x4A5C40 moves and draws them.
//!
//! A shot is a ray of 4096 level units from Buzz's arm height along his facing. It auto-aims:
//! of the targetable creatures on screen within range, the one nearest his facing; within 8°
//! of it the ray turns onto its centre (and tilts up to ±5.6° to its height); otherwise Buzz
//! turns towards it. The ray stops at the first wall or creature; creatures it hits take the
//! shot (hit kind 2, or 3 charged), unless the laser can't hurt them, when the beam bounces
//! off in a random direction. The beam is drawn from the arm to where the ray stopped; its
//! back end then runs forward at 64 units a tick while it fades over 32 ticks.
//!
//! Units as the game: object units (level units × 32), 12-bit angles, ticks.

use crate::creatures::Creatures;
use crate::game::{Player, Shot, World};
use crate::scene::{self, Batch, Blend, GpuVertex};
use glam::Vec3;

/// One beam (0x2C bytes at 0x882970).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Beam {
    /// The back end (moves forward) and the far end, object units.
    pub start: [i32; 3],
    pub end: [i32; 3],
    /// Per-tick motion of the back end.
    pub step: [i32; 3],
    /// Ticks to live (from 32) and steps until the back end reaches the far end.
    pub life: i32,
    pub steps: i32,
    /// Colour (0x80 = full) and half width in level units.
    pub color: [u8; 3],
    pub width: u8,
}

/// Something a shot did that the caller plays or shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// A sound at a position (object units): 8 for a wall hit, 7 for a bounce.
    Sound(u16, [i32; 3]),
    /// Where a beam of `kind` (0 normal, 1 charged) stopped against something, object units;
    /// `wall` when it was the level.
    Impact { at: [i32; 3], kind: u8, wall: bool, first_person: bool },
}

#[derive(Debug, Clone, Default)]
pub struct Lasers {
    pub beams: [Beam; 4],
    /// Ribbons queued for this frame only (0x44E100: creatures' beams), drawn like beams.
    pub ribbons: Vec<Beam>,
    /// The game's random bytes and read position (0x52ADAC).
    random: Vec<u8>,
    rpos: usize,
}

/// Ray length: 4 × 0x8000 object units.
const RANGE: i32 = 4;

fn sin(a: i32) -> i32 {
    ((a & 0xFFF) as f64 / 4096.0 * std::f64::consts::TAU).sin().mul_add(16384.0, 0.0).round() as i32
}

fn atan(x: i32, z: i32) -> i32 {
    crate::boxes::angle12(x as f32, z as f32)
}

fn wrap(a: i32) -> i32 {
    let a = a & 0xFFF;
    if a > 0x800 { a - 0x1000 } else { a }
}

impl Lasers {
    pub fn new(random: Vec<u8>) -> Lasers {
        Lasers { random, ..Default::default() }
    }

    fn rand(&mut self) -> u8 {
        if self.random.is_empty() {
            return 0x80;
        }
        let b = self.random[self.rpos % self.random.len()];
        self.rpos = (self.rpos + 1) % 0x5DC.min(self.random.len());
        b
    }

    /// The tail of 0x434990 for one shot: the beam, and if it bounced, a second one from where
    /// it stopped (yaw + 0x600 + 4r, pitch 4r − 0x200, no auto-aim; sound 7 there).
    /// `muzzle` is the arm's tip, `visible` says whether a creature slot is on screen.
    pub fn fire(&mut self, shot: Shot, muzzle: [i32; 3], player: &mut Player, world: &World, creatures: &mut Creatures, visible: &dyn Fn(usize) -> bool) -> Vec<Event> {
        let mut events = Vec::new();
        let off = [0, 1, 2].map(|k| muzzle[k] - shot.origin[k]);
        let aim = (!shot.first_person).then_some((player, &*visible));
        if let Some(end) = self.beam(shot.kind - 1, shot.yaw, shot.pitch, shot.origin, off, aim, world, creatures, &mut events) {
            let r = self.rand() as i32;
            self.rand();
            self.beam(shot.kind - 1, shot.yaw + 0x600 + r * 4, r * 4 - 0x200, end, [0; 3], None, world, creatures, &mut events);
            events.push(Event::Sound(7, end));
        }
        events
    }

    /// 0x4A5D30: one beam of `kind` (0 normal, 1 charged). Returns the far end if it bounced.
    #[allow(clippy::too_many_arguments)]
    fn beam(
        &mut self,
        kind: u8,
        mut yaw: i32,
        mut pitch: i32,
        origin: [i32; 3],
        off: [i32; 3],
        aim: Option<(&mut Player, &dyn Fn(usize) -> bool)>,
        world: &World,
        creatures: &mut Creatures,
        events: &mut Vec<Event>,
    ) -> Option<[i32; 3]> {
        let facing = aim.as_ref().map_or(yaw, |(p, _)| p.facing);
        let aim_fp = aim.is_none() && off != [0; 3];
        if let Some((player, visible)) = aim {
            // The candidate nearest the facing.
            let best = creatures
                .aim_candidates(origin)
                .into_iter()
                .filter(|&i| visible(i))
                .map(|i| {
                    let c = &creatures.slots[i];
                    (i, wrap(atan((c.pos[0] - origin[0]) >> 5, (c.pos[2] - origin[2]) >> 5) - yaw))
                })
                .min_by_key(|&(_, d)| d.abs());
            if let Some((i, _)) = best {
                let (centre, _) = creatures.ellipsoid_at(i);
                let (dx, dz) = ((centre[0] - origin[0]) >> 5, (centre[2] - origin[2]) >> 5);
                let a = atan(dx, dz);
                if ((a + 0x80 - yaw) & 0xFFF) < 0x100 {
                    yaw = a;
                    let h = ((dx as f64).hypot(dz as f64)) as i32;
                    pitch = wrap(atan(h, (centre[1] - origin[1]) >> 5) - 0x400).clamp(-0x40, 0x40);
                } else {
                    player.target = a & 0xFFF;
                }
            }
        }
        let slot = (0..4).find(|&k| self.beams[k].life == 0).unwrap_or_else(|| (0..4).min_by_key(|&k| self.beams[k].life).unwrap());
        let (cp, sp) = (sin(pitch + 0x400), sin(pitch - 0x800));
        let (sy, cy) = (sin(yaw), sin(yaw + 0x400));
        let start = [0, 1, 2].map(|k| origin[k] + off[k]);
        let mut delta = [((sy as i64 * cp as i64 * RANGE as i64) >> 13) as i32, sp * RANGE * 2, ((cy as i64 * cp as i64 * RANGE as i64) >> 13) as i32];
        let (color, width) = match kind {
            0 => ([0x80, 0, 0], 0x20),
            1 => ([0x80, 0x80, 0], 0x40),
            _ => ([0, 0x80, 0], 0x20),
        };
        // 0x48C860 against the level, then 0x4A5870 against creatures.
        let o = Vec3::from(origin.map(|v| v as f32 / 32.0));
        let d = Vec3::from(delta.map(|v| v as f32 / 32.0));
        let mut bits = 0;
        if let Some(h) = world.sweep(o, d, 1.0) {
            delta = (d * h.t * 32.0).to_array().map(|v| v as i32);
            bits |= 1;
        }
        bits |= creatures.laser_ray(origin, &mut delta, kind + 1, facing);
        let end = [0, 1, 2].map(|k| origin[k] + delta[k]);
        if bits != 0 {
            events.push(Event::Impact { at: end, kind, wall: bits == 1, first_person: aim_fp });
            if bits == 1 {
                events.push(Event::Sound(8, end));
            }
        }
        // Back end's motion: the beam's direction at 2048 a tick; steps along its longest axis.
        let v = [0, 1, 2].map(|k| ((end[k] - start[k]) >> 2) as f64);
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1.0);
        let step = v.map(|c| ((c / len * 4096.0) as i32) >> 1);
        let axis = (0..3).max_by_key(|&k| step[k].abs()).unwrap();
        let steps = if step[axis] == 0 { 0 } else { (end[axis] - start[axis]) / step[axis] };
        self.beams[slot] = Beam { start, end, step, life: 0x20, steps, color, width };
        (bits & 4 != 0).then_some(end)
    }

    /// 0x4A5C40's movement for one tick.
    pub fn tick(&mut self) {
        for b in &mut self.beams {
            if b.life == 0 {
                continue;
            }
            b.life -= 1;
            b.steps -= 1;
            if b.steps < 1 {
                b.life = 0;
            } else {
                for k in 0..3 {
                    b.start[k] += b.step[k];
                }
            }
        }
    }

    /// The beams as camera-facing ribbons (0x44E1A0) of sprite 9 (texture, its size, the cell
    /// size and cells 0-2): 400-unit segments from the back end towards the far end, the first
    /// in cell 2, the rest in cell 1, each stretching the cell's width along the beam and its
    /// height across it (0x4B9100's corner order); additive, fading with life.
    pub fn beams_batches(&self, camera: Vec3, texture: Option<(usize, [f32; 2], [f32; 2], [[f32; 2]; 3])>) -> Vec<Batch> {
        let mut out = Vec::new();
        for b in self.beams.iter().chain(self.ribbons.iter()).filter(|b| b.life > 0) {
            let end = Vec3::from(b.end.map(|v| v as f32 / 32.0));
            let start = Vec3::from(b.start.map(|v| v as f32 / 32.0));
            let dir = start - end;
            let len = dir.length();
            if len < 1.0 {
                continue;
            }
            let u = dir / len;
            let side = (end - camera).cross(u).normalize_or_zero() * b.width as f32;
            let fade = (b.life * 4) as f32 / 128.0;
            let c = b.color.map(|v| (v as f32 * fade).min(255.0) as u8);
            let color = [0xFF, c[0], c[1], c[2]];
            // `at` is the distance from the far end; walk from the back end (at = len).
            let mut at = len;
            let mut cell = 2;
            while at > 0.0 {
                let next = (at - 400.0).max(0.0);
                // p: the segment's back-end side, q: its far-end side.
                let (p, q) = (end + u * at, end + u * next);
                let uv = |s: f32, t: f32| match texture {
                    Some((_, tex, size, cells)) => {
                        let [cu, cv] = cells[cell];
                        [(cu + s * size[0]) / tex[0], (cv + t * size[1]) / tex[1]]
                    }
                    None => [s, t],
                };
                let v = |pt: Vec3, s: f32, t: f32| GpuVertex { pos: pt.to_array(), uv: uv(s, t), color };
                let (a, bb, cc, d) = (v(q - side, 0.0, 1.0), v(q + side, 0.0, 0.0), v(p + side, 1.0, 0.0), v(p - side, 1.0, 1.0));
                let tex = texture.map(|t| t.0);
                scene::push(&mut out, tex, Blend::Additive, [a, bb, cc]);
                scene::push(&mut out, tex, Blend::Additive, [a, cc, d]);
                at = next;
                cell = 1;
            }
        }
        out
    }
}
