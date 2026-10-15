//! Reward sparkles (0x49FB40): `level.dat` list 58 marks where pushable boxes belong (group 0)
//! and where challenge rewards wait (group 1), split by a (0, 0, 0) point that 0x414550 drops
//! at load (keeping the split in `0x830E28`). Every 16 ticks a quarter of the points near the
//! camera (within 600 × 256 object units) twinkle: particle 0x71 for box spots, 0x73 for
//! rewards, mode 2. A point is put out for good by 0x49FAB0, which also ends its live
//! sparkle.

use crate::formats::leveldat::PointList;
use crate::particles::Particles;

/// List 58: box spots, then challenge rewards.
pub const REWARDS: i16 = 58;

#[derive(Debug, Clone, Default)]
pub struct Rewards {
    /// Object units; None once put out (y = 0x80000000).
    points: Vec<Option<[i32; 3]>>,
    /// Box spots before this index, rewards from it (0x830E28).
    pub split: usize,
    /// Which quarter twinkles next (0x830C9C).
    next: usize,
    clock: u32,
}

impl Rewards {
    pub fn new(lists: &[PointList]) -> Rewards {
        let Some(l) = lists.iter().find(|l| l.kind == REWARDS) else { return Rewards::default() };
        let split = l.points.iter().position(|p| *p == [0; 3]).unwrap_or(l.points.len());
        let points = l.points.iter().filter(|p| **p != [0; 3]).map(|p| Some(p.map(|v| v * 32))).collect();
        Rewards { points, split, next: 0, clock: 0 }
    }

    /// One tick; `camera` in object units.
    pub fn tick(&mut self, camera: [i32; 3], particles: &mut Particles) {
        self.clock = self.clock.wrapping_add(1);
        if !self.clock.is_multiple_of(16) || self.points.is_empty() {
            return;
        }
        for i in (self.next..self.points.len()).step_by(4) {
            let Some(p) = self.points[i] else { continue };
            let d = [0, 1, 2].map(|k| (camera[k] - p[k]) >> 8);
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < 360_000 {
                particles.spawn(p, if i < self.split { 0x71 } else { 0x73 }, 2);
            }
        }
        self.next += 1;
        if self.next >= self.points.len() || self.next > 3 {
            self.next = 0;
        }
    }

    /// 0x49FAB0(k, group): point k of the group (0 box spots, 1 rewards) goes out, and its
    /// sparkle with it.
    pub fn put_out(&mut self, k: usize, group: usize, particles: &mut Particles) {
        let i = if group != 0 { k + self.split } else { k };
        let Some(Some(p)) = self.points.get(i).copied() else { return };
        let typ = 0x71 + 2 * group as u8;
        if let Some(s) = particles.slots.iter_mut().find(|s| s.life > 0 && s.pos == p && s.typ == typ) {
            s.life = 1;
        }
        self.points[i] = None;
    }
}
