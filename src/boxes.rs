//! Pushable objects: boxes, books and the like that Buzz shoves along a `level.dat` path
//! (0x4335D0 setup, 0x4334D0 segment, 0x433700 update; table at 0x53C680, 0x28 bytes each).
//!
//! Each one is a special object (drawn) with a mover (its collision) riding a point list. Buzz
//! pushes while on the ground against its side, with the stick within ±0x180 of the push
//! direction; it then moves 12 units per tick forwards or backwards along the path, a segment
//! at a time. When the next point is straight below the end of a segment, the object slides off
//! by itself past halfway and falls to it. Level scripts watch `ended` (the 0x53C670 −1 state)
//! to open doors and the like.

use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::game::{Player, Sound, World};
use crate::specials::Specials;
use glam::Vec3;
use std::cell::Cell;

/// 12-bit atan2(x, z), as 0x4520D0.
pub fn angle12(x: f32, z: f32) -> i32 {
    ((x.atan2(z) / std::f32::consts::TAU * 4096.0).round() as i32) & 0xFFF
}

#[derive(Debug, Clone, PartialEq)]
pub struct PushBox {
    pub special: i32,
    pub mover: i32,
    /// `level.dat` point list type.
    pub path: i16,
    /// Object units.
    pub pos: [i32; 3],
    /// Segment direction, × 4096 (+0x0C / +0x10).
    dir: [i32; 2],
    /// Falling speed (+0x14); 0 = resting.
    pub fall: i32,
    /// +0x16: 0 normal, > 0 the progress past which it slides off by itself, −1 sliding off.
    pub slide: i32,
    /// Along the segment, level units (+0x18), and its length (+0x1A).
    progress: Cell<i32>,
    length: i32,
    /// Segment heading (+0x1C).
    angle: i32,
    /// Segment start point (+0x1E) and the point it last landed on (+0x20).
    point: i32,
    landed: i32,
}

pub struct Boxes {
    pub boxes: Vec<PushBox>,
    paths: Vec<(i16, Vec<[i32; 3]>)>,
    /// 1-based index of the box being pushed (0x53C648), its offset from Buzz and the push
    /// direction (0x53C81C/0x53C820/0x53C624).
    pushing: Cell<usize>,
    /// Box spots (list 58 group 0) to put out, for the caller.
    pub put_out: Vec<usize>,
    /// Ticks since the box being pushed last moved.
    still: u8,
    offset: [i32; 2],
    push_angle: i32,
    /// Ticks before Buzz says "hnn" again (0x53C670).
    voice_timer: i32,
}

/// Per-level box lists: (special, mover, path) triples until −1, passed to 0x4335D0 by the
/// level's setup (level 1: 0x4171D0).
fn box_list(level: usize) -> Option<u32> {
    match level {
        1 => Some(0x4F0F90),
        2 => Some(0x4F1518),
        // The crane (mover 0) and a crate, pushed along lists 3 and 5.
        4 => Some(0x4F1C88),
        11 => Some(0x4F3E60),
        13 => Some(0x4F46A4),
        _ => None,
    }
}

impl Boxes {
    /// 0x53C648: the 1-based box being pushed, 0 for none.
    pub fn pushed(&self) -> usize {
        self.pushing.get()
    }

    pub fn new(level: usize, lists: &[PointList], exe: Option<&Exe>, world: &World, specials: &mut Specials) -> Boxes {
        let paths = lists.iter().filter(|l| l.kind >= 0 && l.kind < 0x3F).map(|l| (l.kind, l.points.clone())).collect();
        let mut b = Boxes { boxes: Vec::new(), paths, pushing: Cell::new(0), offset: [0; 2], push_angle: 0, voice_timer: 0, put_out: Vec::new(), still: u8::MAX };
        let (Some(exe), Some(mut va)) = (exe, box_list(level)) else { return b };
        while let Ok(t) = exe.at(va, 6) {
            let w = |k: usize| i16::from_le_bytes([t[2 * k], t[2 * k + 1]]);
            if w(0) == -1 || b.boxes.len() >= 10 {
                break;
            }
            let (special, mover, path) = (w(0) as i32, w(1) as i32, w(2));
            va += 6;
            let Some(pts) = b.points(path).map(|p| p.to_vec()) else { continue };
            let Some(&p0) = pts.first() else { continue };
            let mut pb = PushBox { special, mover, path, pos: p0.map(|v| v * 32), dir: [0; 2], fall: 0, slide: 0, progress: Cell::new(0), length: 0, angle: 0, point: 0, landed: 0 };
            b.segment(&mut pb);
            b.place(&pb, world, specials);
            b.boxes.push(pb);
        }
        b
    }

    fn points(&self, path: i16) -> Option<&[[i32; 3]]> {
        self.paths.iter().find(|(k, _)| *k == path).map(|(_, p)| p.as_slice())
    }

    /// 0x4334D0: heading, direction and length of the box's current segment.
    fn segment(&self, b: &mut PushBox) {
        let Some(p) = self.points(b.path) else { return };
        let i = b.point as usize;
        let (Some(a), Some(c)) = (p.get(i), p.get(i + 1)) else { return };
        let (dx, dz) = ((c[0] - a[0]) as f32, (c[2] - a[2]) as f32);
        b.angle = angle12(dx, dz);
        let len = dx.hypot(dz);
        b.length = len as i32;
        b.dir = if len > 0.0 { [(dx * 4096.0 / len) as i32, (dz * 4096.0 / len) as i32] } else { [0; 2] };
        b.slide = 0;
        // The next point straight below this segment's end: slide off from halfway.
        if (b.landed as usize) + 2 < p.len()
            && let Some(d) = p.get(i + 2)
            && c[0] == d[0]
            && c[2] == d[2]
        {
            b.slide = b.length / 2;
        }
    }

    /// Where the box is along its segment (the tail of 0x433700 and its auto-slide).
    fn along(&self, b: &mut PushBox) {
        let Some(&s) = self.points(b.path).and_then(|p| p.get(b.point as usize)) else { return };
        b.pos[0] = ((b.dir[0] * b.progress.get() >> 7) & !31) + s[0] * 32;
        b.pos[2] = ((b.dir[1] * b.progress.get() >> 7) & !31) + s[2] * 32;
    }

    fn place(&self, b: &PushBox, world: &World, specials: &mut Specials) {
        let p = b.pos.map(|v| v >> 5);
        if b.special >= 0 {
            specials.set_pos(b.special, p);
        }
        world.place_mover(b.mover, Vec3::from(p.map(|v| v as f32)));
    }

    /// 0x433700, inside Buzz's tick just before he moves.
    pub fn tick(&mut self, player: &mut Player, world: &World, specials: &mut Specials) {
        self.voice_timer = (self.voice_timer - 1).max(0);
        let before = self.pushing.get().checked_sub(1).and_then(|k| self.boxes.get(k)).map(|b| b.pos);
        self.tick_inner(player, world, specials);
        // The push animation only while the box really moves (or did a moment ago, across a
        // segment change): against a side it can't go, Buzz just leans on it.
        let after = self.pushing.get().checked_sub(1).and_then(|k| self.boxes.get(k)).map(|b| b.pos);
        self.still = if before.is_some() && before != after { 0 } else { self.still.saturating_add(1) };
        player.pushing = self.pushing.get() != 0 && self.still < 6;
    }

    fn tick_inner(&mut self, player: &mut Player, world: &World, specials: &mut Specials) {
        // Pick a box to push: on the floor, pushing the stick, touching a box's side.
        if !player.stick_pushed || player.busy() {
            self.pushing.set(0);
        } else if self.pushing.get() != 0 && ((self.push_angle - player.target) + 0x400) & 0xFFF > 0x800 {
            // The stick turned more than 90° away from the push (end of 0x4346C0): let go.
            self.pushing.set(0);
        } else if self.pushing.get() == 0 && (player.on_floor || player.coyote > 0) {
            // (Or just off it: on the kitchen shelf's slight slope, the tick Buzz meets the spring
            // box finds the wall but not the floor.)
            if let Some((m, n)) = player.wall_mover
                && let Some(k) = self.boxes.iter().position(|b| b.mover == m && b.fall == 0 && b.slide >= 0)
            {
                let dir = (angle12(n.x, n.z) - 0x800) & 0xFFF;
                if ((dir - player.facing) + 0x180) & 0xFFF < 0x300 {
                    let b = &self.boxes[k];
                    self.pushing.set(k + 1);
                    self.offset = [(b.pos[0] - player.pos_obj()[0]) >> 5, (b.pos[2] - player.pos_obj()[2]) >> 5];
                    self.push_angle = dir;
                    if self.voice_timer == 0 {
                        player.sounds.push(Sound { id: 0x2D, voice: true });
                        self.voice_timer = 0x28;
                    }
                }
            }
        }
        // Falling and sliding boxes.
        for k in 0..self.boxes.len() {
            let mut b = self.boxes[k].clone();
            let mut moved = false;
            if b.fall != 0 {
                b.fall = (b.fall + 0x40).min(0x800);
                b.pos[1] += (b.fall >> 5) << 5;
                if let Some(&p) = self.points(b.path).and_then(|p| p.get(b.point as usize))
                    && b.pos[1] >= p[1] * 32
                {
                    b.pos[1] = p[1] * 32;
                    b.fall = 0;
                    b.landed = b.point;
                    self.segment(&mut b);
                    player.sounds_at.push((Sound { id: 0x2F, voice: false }, Vec3::from(b.pos.map(|v| v as f32 / 32.0))));
                }
                moved = true;
            }
            if b.slide < 0 {
                b.progress.set(b.progress.get() + 0x18);
                if b.progress.get() > b.length {
                    b.point += 2;
                    b.fall = 2;
                    b.angle = 0;
                    b.length = 0;
                    b.dir = [0; 2];
                    b.slide = 0;
                    b.progress.set(0);
                }
                self.along(&mut b);
                moved = true;
            }
            if moved {
                self.place(&b, world, specials);
            }
            self.boxes[k] = b;
        }
        if self.pushing.get() == 0 {
            return;
        }
        let k = self.pushing.get() - 1;
        let mut b = self.boxes[k].clone();
        if b.fall != 0 || b.slide < 0 {
            return;
        }
        let count = self.points(b.path).map_or(0, |p| p.len()) as i32;
        let mut moving = false;
        if b.landed < count - 1 {
            if ((self.push_angle - b.angle) + 8) & 0xFFF < 0x10 {
                moving = true;
                // Pushed: its hint sparkle (list 58, group 0) goes for good (0x49FAB0).
                self.put_out.push(k);
                b.progress.set(b.progress.get() + 0xC);
                if b.slide > 0 && b.slide < b.progress.get() {
                    // Over the edge: it goes on by itself.
                    b.slide = -1;
                    player.sounds_at.push((Sound { id: 0x31, voice: false }, Vec3::from(b.pos.map(|v| v as f32 / 32.0))));
                    self.pushing.set(0);
                    player.vel[0] = 0;
                    player.vel[2] = 0;
                    player.push_lock = 10;
                }
            }
            if ((self.push_angle - b.angle) - 0x7F8) & 0xFFF < 0x10 {
                moving = true;
                b.progress.set(b.progress.get() - 0xC);
            }
            // Across segment ends: back to the previous segment at its start (not before the
            // point it last landed on), on to the next at its end (not past the last).
            let back = |s: &Self, b: &mut PushBox| {
                b.point -= 1;
                s.segment(b);
                b.progress.set(b.length);
            };
            if b.progress.get() == 0 && b.landed < b.point {
                back(self, &mut b);
                moving = false;
            } else {
                if b.progress.get() >= b.length {
                    if b.point < count - 2 {
                        b.point += 1;
                        self.segment(&mut b);
                        b.progress.set(0);
                        moving = false;
                    } else {
                        b.progress.set(b.length);
                    }
                }
                if b.progress.get() < 0 {
                    if b.landed < b.point {
                        back(self, &mut b);
                    } else {
                        b.progress.set(0);
                    }
                    moving = false;
                }
            }
        }
        self.along(&mut b);
        self.place(&b, world, specials);
        // Buzz keeps his place behind the box, facing along the push (0x4346C0).
        if self.pushing.get() != 0 {
            player.facing = self.push_angle;
            let p = player.pos_obj();
            player.vel[0] = b.pos[0] - self.offset[0] * 32 - p[0];
            player.vel[2] = b.pos[2] - self.offset[1] * 32 - p[2];
        }
        if moving {
            player.sounds_at.push((Sound { id: 0x2E, voice: false }, Vec3::from(b.pos.map(|v| v as f32 / 32.0))));
        }
        self.boxes[k] = b;
    }

    /// Box `k` is sliding off its edge (+0x16 = −1) or falling (+0x14 ≠ 0), as level scripts
    /// test it (level 1's door: box 3).
    /// For diagnostics: the box being pushed (1-based, 0 none), the push direction and each
    /// box's segment heading.
    pub fn push_state(&self) -> (usize, i32, Vec<i32>) {
        (self.pushing.get(), self.push_angle, self.boxes.iter().map(|b| b.angle).collect())
    }

    /// Level scripts release Buzz when a pushed object starts falling.
    pub fn release(&self, player: &mut Player) {
        self.pushing.set(0);
        player.pushing = false;
    }

    pub fn pushing_index(&self) -> usize { self.pushing.get() }

    pub fn progress(&self, k: usize) -> i32 {
        self.boxes.get(k).map_or(0, |b| b.progress.get())
    }

    /// Level 2 rejects tree movement while Buzz is below its pushing platform.
    pub fn reset_progress(&self, k: usize) {
        if let Some(b) = self.boxes.get(k) { b.progress.set(0); }
    }

    pub fn going_over(&self, k: usize) -> bool {
        self.boxes.get(k).is_some_and(|b| b.slide == -1 || b.fall != 0)
    }
}
