//! Particles: sparks, smoke, rings, sparkles and the like (64 slots of 0x3C bytes at
//! 0x529E58), as the original spawns (0x40FDF0 / 0x40FAE0), moves (0x410F40), ends (0x410B80)
//! and draws them (0x445980).
//!
//! - A type (table 0x4EC160, 16 bytes) gives the sprite, frame period and count, life, size,
//!   flags, behaviour kind, the action on death and the colour; a spawn mode (0x4EC948, 0x18
//!   bytes) gives the starting velocity, each component either fixed or random in a range,
//!   gravity, angle and spin.
//! - Each tick: life −1, gravity, motion, frame steps, spin; flag 1 ends it on reaching the
//!   floor; then its kind's own step (grow, shrink, follow Buzz, bounce, leave a trail…) and a
//!   colour fade picked by the kind.
//! - Flags (+0x32): 1 dies on the floor, 0x10 lies flat (drawn horizontally), 0x60 blend
//!   (0 half-transparent, 0x20 additive, 0x40 subtractive, 0x60 opaque).
//!
//! Kinds that steer at creatures or run creature-specific code (homing shots 0x1D/0x26,
//! 0x34) aren't ported yet; they move as plain particles. Zurg shots 0x2F/0x30
//! include arena bounce/trails and Buzz-targeted homing.
//!
//! Units as the game: object units, 12-bit angles, ticks. Sizes are half extents in level
//! units.

use crate::formats::exe::Exe;
use crate::game::World;
use crate::pickups::SpriteDef;
use crate::scene::{self, Batch, Blend, GpuVertex};
use glam::Vec3;

const SLOTS: usize = 64;
const TYPES: u32 = 0x4EC160;
const TYPE_COUNT: usize = 126;
const MODES: u32 = 0x4EC948;
const MODE_COUNT: usize = 18;

/// A particle type (0x4EC160).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Type {
    pub sprite: u8,
    /// Ticks per frame (negative: random, and the life covers the frames).
    pub period: i8,
    pub life: i16,
    pub frames: u8,
    pub death: u8,
    pub size: [i16; 2],
    pub flags: u16,
    pub kind: u8,
    pub rgb: [u8; 3],
}

/// A spawn mode (0x4EC948): per component of (vx, vy, vz, gravity) a randomisation nibble and
/// a value; the angle and spin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mode {
    pub random: u32,
    pub values: [i32; 4],
    pub angle: i16,
    pub spin: i16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Particle {
    pub pos: [i32; 3],
    pub vel: [i32; 3],
    pub gravity: i32,
    /// Homing pitch (+0x20); gravity holds yaw and vel[1] holds target -1 for Buzz.
    homing_pitch: i32,
    /// +0x1C: the floor under it, when known.
    pub floor: Option<i32>,
    /// +0x24 ticks to live (0 = free); +0x26 / +0x28 size.
    pub life: i32,
    pub size: [i32; 2],
    /// +0x2A frame period, +0x2B countdown, +0x30 frame count, +0x31 frame.
    period: i32,
    countdown: i32,
    frames: u8,
    pub frame: u8,
    pub sprite: u8,
    pub typ: u8,
    death: u8,
    kind: u8,
    pub flags: u16,
    /// +0x34 angle (12-bit), +0x36 spin per tick.
    pub angle: i32,
    pub spin: i32,
    pub rgb: [u8; 3],
    ended: bool,
}

/// Something particles did that the game around them handles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// A sound at a position (object units).
    Sound(u16, [i32; 3]),
    /// A coin particle was picked up here (0x4100F0: the coin count, jingle −1).
    Coin([i32; 3]),
    /// Something hurtful touched Buzz, coming from here: HurtBuzz(angle, 3).
    Hurt([i32; 3]),
}

#[derive(Debug, Clone)]
pub struct Particles {
    pub slots: Vec<Particle>,
    types: Vec<Type>,
    modes: Vec<Mode>,
    /// Round-robin slot (0x52AD90).
    next: usize,
    random: Vec<u8>,
    rpos: usize,
    /// Where the camera is (0x52ADC0): far particles aren't made and die.
    pub camera: [i32; 3],
    pub events: Vec<Event>,
}

/// Velocity randomisation (0x40FDF0) by nibble.
fn randomise(kind: u32, v: i32, r: u8) -> i32 {
    let r = r as i32;
    match kind {
        1 => ((r & v) - (v >> 1)) * 8,
        2 => (r & v) << 3,
        3 => (r & v) * -8,
        4 => ((r & v) + ((v >> 4) & 0xFF0)) * 8,
        5 => ((r & v) - ((v >> 4) & 0xFF0)) * 8,
        6 => ((r & v) - (v >> 1)) * 16,
        _ => v,
    }
}

fn sin14(a: i32) -> i32 {
    ((a & 0xFFF) as f64 / 4096.0 * std::f64::consts::TAU).sin().mul_add(16384.0, 0.0).round() as i32
}

impl Particles {
    pub fn new(exe: Option<&Exe>, random: Vec<u8>) -> Particles {
        let types = (0..TYPE_COUNT as u32)
            .map(|t| {
                let Some(d) = exe.and_then(|e| e.at(TYPES + 16 * t, 16).ok()) else { return Type::default() };
                let s = |k: usize| i16::from_le_bytes([d[k], d[k + 1]]);
                Type {
                    sprite: d[0],
                    period: d[1] as i8,
                    life: s(2),
                    frames: d[4],
                    death: d[5],
                    size: [s(6), s(8)],
                    flags: s(10) as u16,
                    kind: d[12],
                    rgb: [d[13], d[14], d[15]],
                }
            })
            .collect();
        let modes = (0..MODE_COUNT as u32)
            .map(|m| {
                let Some(d) = exe.and_then(|e| e.at(MODES + 0x18 * m, 0x18).ok()) else { return Mode::default() };
                let i = |k: usize| i32::from_le_bytes(d[k..k + 4].try_into().unwrap());
                Mode { random: i(0) as u32, values: [i(4), i(8), i(12), i(16)], angle: i16::from_le_bytes([d[20], d[21]]), spin: i16::from_le_bytes([d[22], d[23]]) }
            })
            .collect();
        Particles { slots: vec![Particle::default(); SLOTS], types, modes, next: 0, random, rpos: 0, camera: [0; 3], events: Vec::new() }
    }

    /// The next byte of the game's random table (0x52ADAC).
    pub fn rand(&mut self) -> u8 {
        if self.random.is_empty() {
            return 0x80;
        }
        let b = self.random[self.rpos % self.random.len()];
        self.rpos = (self.rpos + 1) % 0x5DC.min(self.random.len());
        b
    }

    /// 0x40FDF0: a particle of `typ` with the velocity of spawn `mode`. The slot, if made.
    pub fn spawn(&mut self, pos: [i32; 3], typ: u8, mode: usize) -> Option<usize> {
        let m = self.modes.get(mode).copied().unwrap_or_default();
        let mut v = m.values;
        for (k, shift) in [12, 8, 4, 0].into_iter().enumerate() {
            let n = (m.random >> shift) & 0xF;
            if (1..=6).contains(&n) {
                let r = self.rand();
                v[k] = randomise(n, v[k], r);
            }
        }
        self.create(pos, [v[0], v[1], v[2]], v[3], m.angle as i32, m.spin as i32, typ)
    }

    /// 0x40FAE0: a particle with explicit velocity, gravity, angle and spin.
    pub fn create(&mut self, pos: [i32; 3], vel: [i32; 3], gravity: i32, angle: i32, spin: i32, typ: u8) -> Option<usize> {
        let far: i64 = if matches!(typ, 0x37 | 0x43 | 0x50) { 0x19_0000 } else { 640_000 };
        let d = [0, 1, 2].map(|k| ((self.camera[k] - pos[k]) >> 8) as i64);
        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= far {
            return None;
        }
        // The next slot round, else the one with the least life left.
        let mut k = (self.next + 1) % SLOTS;
        let mut best = (i32::MAX, k);
        for _ in 0..SLOTS {
            let p = &self.slots[k];
            if p.life == 0 || p.flags & 0x80 == 0 {
                best = (0, k);
                break;
            }
            if p.life < best.0 {
                best = (p.life, k);
            }
            k = (k + 1) % SLOTS;
        }
        let slot = best.1;
        self.next = slot;
        let t = self.types.get(typ as usize).copied().unwrap_or_default();
        let mut p = Particle {
            pos,
            vel: vel.map(|v| v / 2),
            gravity: gravity / 4,
            floor: None,
            life: t.life as i32 * 2,
            size: [t.size[0] as i32, t.size[1] as i32],
            frames: t.frames,
            frame: 0,
            sprite: t.sprite,
            typ,
            death: t.death,
            kind: t.kind,
            flags: t.flags | 0x200,
            angle: angle & 0xFFF,
            spin: spin / 2,
            rgb: t.rgb,
            ..Default::default()
        };
        if t.period < 0 {
            let r = self.rand() as i32;
            p.period = ((t.period as i32 * 0x7F + (r & 3)) * 2) as i8 as i32;
            p.life = p.frames as i32 * (p.period & 0xFF) * 2;
        } else {
            p.period = t.period as i32 * 2;
        }
        p.countdown = p.period;
        self.slots[slot] = p;
        Some(slot)
    }

    /// 0x410F40 for one tick. `buzz` is Buzz's feet and facing (kinds that follow him).
    pub fn tick(&mut self, world: &World, buzz: ([i32; 3], i32)) {
        for i in 0..SLOTS {
            if self.slots[i].life < 1 {
                continue;
            }
            let mut p = self.slots[i];
            p.life = (p.life - 1).max(0);
            // Flag 8 repurposes vertical velocity as a target and gravity as yaw
            // (0x410F40). Zurg's 0x6D shots target Buzz, encoded as -1.
            if p.kind == 0x30 && p.flags & 8 != 0 && p.vel[1] == -1 {
                let dx = buzz.0[0] - p.pos[0];
                let dy = buzz.0[1] - p.pos[1] - 0x2000;
                let dz = buzz.0[2] - p.pos[2];
                let yaw = crate::boxes::angle12((dx >> 5) as f32, (dz >> 5) as f32);
                let turn = |to: i32, from: i32| ((to - from + 0x800) & 0xfff) - 0x800;
                p.gravity = (p.gravity + turn(yaw, p.gravity) / 8) & 0xfff;
                let horizontal = ((dx as f64).hypot(dz as f64)) as f32;
                let pitch = crate::boxes::angle12(-dy as f32, horizontal);
                p.homing_pitch = (p.homing_pitch + turn(pitch, p.homing_pitch) / 20) & 0xfff;
                let horizontal_speed = sin14(p.homing_pitch + 0x400);
                p.vel[0] = ((sin14(p.gravity) * horizontal_speed) >> 14) / 16;
                p.vel[2] = ((sin14(p.gravity + 0x400) * horizontal_speed) >> 14) / 16;
                p.pos[1] -= sin14(p.homing_pitch) / 16;
            } else {
                p.pos[1] += p.vel[1];
                p.vel[1] += p.gravity;
            }
            p.pos[0] += p.vel[0];
            p.pos[2] += p.vel[2];
            if p.frames > 1 {
                p.countdown -= 1;
                while p.countdown < 1 {
                    p.countdown += p.period.max(1);
                    p.frame = (p.frame + 1) % p.frames;
                }
            }
            if p.spin != 0 {
                p.angle = (p.angle + p.spin) & 0xFFF;
            }
            if p.flags & 1 != 0 && p.life > 0 {
                if p.vel[0] != 0 || p.vel[2] != 0 || p.floor.is_none() {
                    p.floor = floor_at(world, p.pos);
                }
                if let Some(f) = p.floor
                    && f < p.size[1] * 0x20 + p.pos[1]
                {
                    p.life = 0;
                }
            }
            let d = [0, 1, 2].map(|k| ((self.camera[k] - p.pos[k]) >> 8) as i64);
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 0x19_0000 {
                p.life = 0;
                p.death = 0;
            }
            if p.flags & 0x300 == 0 {
                p.life = 0;
            }
            let fade = self.kind_step(&mut p, world, buzz);
            self.fade(&mut p, fade);
            if p.life == 0 {
                p.ended = true;
            }
            self.slots[i] = p;
        }
        for i in 0..SLOTS {
            if self.slots[i].ended {
                let p = self.slots[i];
                self.slots[i] = Particle::default();
                self.death(&p);
            }
        }
    }

    /// The kind's own step (the switch in 0x410F40); returns the colour fade to use.
    fn kind_step(&mut self, p: &mut Particle, world: &World, buzz: ([i32; 3], i32)) -> u8 {
        let grow = p.period & 0xFF;
        match p.kind {
            2 | 7 => 1,
            3 => 2,
            4 => {
                if p.life > 0x10 {
                    self.spawn(p.pos, 3, 3);
                }
                1
            }
            5 => 3,
            6 => 4,
            8 => {
                let v = p.life & 0x1F;
                p.size = [(sin14(v * 0x80) >> 10) + 0x78, (sin14(v * 0x80 + 0x400) >> 10) + 0x78];
                if p.life > 5 && let Some(k) = self.spawn(p.pos, 0xF, 2) {
                    let r = self.rand() as i32;
                    self.slots[k].spin = (r - 0x80) >> 3;
                }
                0
            }
            9 => {
                p.size = [p.size[0] + 2, p.size[1] + 2];
                1
            }
            0xA => {
                p.size[0] = (p.size[0] - 4).max(0x10);
                p.size[1] = p.size[0];
                5
            }
            0xB => {
                p.size[0] += grow;
                p.size[1] = p.size[0];
                1
            }
            0xC => {
                p.pos = [buzz.0[0], buzz.0[1] - p.life * 0x22A, buzz.0[2]];
                p.size[0] -= 8;
                p.size[1] = p.size[0];
                1
            }
            0xD => {
                p.pos = [buzz.0[0], buzz.0[1] - (p.life + 0x10) * 400, buzz.0[2]];
                p.size[0] -= 5;
                p.size[1] = p.size[0];
                1
            }
            0xE => {
                p.angle = (self.rand() as i32) << 4;
                if p.life < 0x20 { 1 } else { 6 }
            }
            0xF => {
                p.size[0] = (p.size[0] - grow).max(1);
                p.size[1] = p.size[0];
                0
            }
            0x10 => {
                if p.life > 4 {
                    self.spawn(p.pos, 0x24, 0xF);
                }
                0
            }
            0x11 => {
                if p.life > 4 {
                    self.spawn(p.pos, 0x27, 2);
                }
                0
            }
            0x14 => {
                p.size[0] += grow;
                p.size[1] = p.size[0];
                3
            }
            0x16 => {
                if p.size[0] < 0x140 {
                    p.size[0] = (p.size[0] + 0x20).min(0x140);
                }
                p.size[1] = p.size[0];
                0
            }
            0x18 => {
                p.size[0] = (p.size[0] - grow).max(1);
                1
            }
            0x19 | 0x1C => {
                // Bouncing (coins, nuts and bolts): off the floor, slower each time.
                let f = floor_at(world, p.pos);
                if let Some(f) = f
                    && f < p.size[1] * 0x20 + p.pos[1]
                {
                    p.pos[1] = f - p.size[1] * 0x20;
                    if p.vel[1] > 0 {
                        p.vel[1] = if p.kind == 0x19 { -p.vel[1] * 6 / 8 } else { -p.vel[1] * 3 / 4 };
                    }
                    let stop = if p.kind == 0x19 { 0x100 } else { 0x400 };
                    if p.vel[1].abs() < stop {
                        p.life = 0;
                    }
                    self.events.push(Event::Sound(if p.kind == 0x19 { 0x1F } else { 0x68 }, p.pos));
                }
                0
            }
            0x1B => {
                p.size = [(p.size[0] - grow).max(1), (p.size[1] - grow).max(1)];
                0
            }
            0x1F => {
                p.size[0] = (p.size[0] - 4).max(0x10);
                p.size[1] = p.size[0];
                1
            }
            0x21 => {
                p.size[0] = (p.size[0] - grow / 2).max(1);
                p.size[1] = p.size[0];
                1
            }
            0x22 => {
                p.size[0] += grow;
                p.size[1] = p.size[0];
                7
            }
            0x25 => {
                p.size[0] = (p.size[0] - grow).max(1);
                p.size[1] = p.size[0] / 2;
                1
            }
            0x27 => {
                if p.life > 4 && p.flags & 0x200 != 0 && let Some(k) = self.spawn(p.pos, 0x5A, 2) {
                    let r = self.rand() as i32;
                    self.slots[k].spin = r - 0x80;
                }
                3
            }
            0x28 => {
                p.size[0] -= grow;
                p.size[1] = p.size[0];
                3
            }
            0x29 => {
                if p.life > 4 && p.flags & 0x200 != 0 {
                    self.spawn(p.pos, 0x62, 2);
                }
                0
            }
            0x2A => {
                // Twinkling sparkle drifting down.
                if p.vel[1] > 0x180 {
                    p.vel[1] = 0x180;
                    let (a, b) = (self.rand() as i32, self.rand() as i32);
                    p.vel[0] = a * 2 - 0x100;
                    p.vel[2] = b * 2 - 0x100;
                }
                p.size[0] = (sin14((p.life & 0x3F) * 0x40) / grow.max(1)).abs();
                0
            }
            0x2D => {
                p.size[0] -= grow;
                p.size[1] = p.size[0] / 2;
                2
            }
            0x2E => {
                p.size[0] += grow;
                p.size[1] = p.size[0];
                1
            }
            0x2F | 0x30 => {
                // Zurg's arena projectiles, 0x42B090 / 0x42B250.
                if p.life > 4 && p.life % 4 == 0 {
                    self.spawn([p.pos[0] - p.vel[0], p.pos[1], p.pos[2] - p.vel[2]],
                        if p.kind == 0x2F { 0x6e } else { 0x6f }, 2);
                }
                if p.kind == 0x2F {
                    let mut bounced = false;
                    let inside = p.pos[0] > -0x256d6 && p.pos[0] < 0xe0aa
                        && p.pos[2] > -0x1b3e9 && p.pos[2] < 0x1b297;
                    if !inside {
                        p.floor = Some(400000);
                    } else {
                        if p.floor.is_none() { p.floor = Some(-0x12bd3); }
                        if p.pos[1] < -0x12bd2 {
                            p.floor = Some(-0x12bd3);
                        } else if p.floor == Some(-0x12bd3) {
                            p.pos[1] = -0x12bd3;
                            p.vel[1] = -((p.vel[1] * 7) >> 3).abs();
                            bounced = true;
                        } else {
                            p.vel[0] = -p.vel[0];
                            p.vel[2] = -p.vel[2];
                            bounced = true;
                        }
                    }
                    if p.pos[0] < -0x29b56 { p.vel[0] = p.vel[0].abs(); bounced = true; }
                    if p.pos[0] > 0x2a62a { p.vel[0] = -p.vel[0].abs(); bounced = true; }
                    if p.pos[2] < -0x230e9 { p.vel[2] = p.vel[2].abs(); bounced = true; }
                    if p.pos[2] > 0x22b97 { p.vel[2] = -p.vel[2].abs(); bounced = true; }
                    if bounced { self.events.push(Event::Sound(0x43, p.pos)); }
                }
                let dx = ((-0xbd7c - p.pos[0]) >> 5) as i64;
                let dz = ((0x99 - p.pos[2]) >> 5) as i64;
                if dx * dx + dz * dz < 0x100000 { p.life = 0; }
                0
            }
            0x32 => {
                if p.life > 0x3F {
                    let n = 0x52 - p.life;
                    let t = self.types.get(p.typ as usize).copied().unwrap_or_default();
                    p.rgb = t.rgb.map(|c| ((c as i32 * n) / 2 >> 3) as u8);
                }
                2
            }
            0x33 => {
                if p.life > 4 {
                    let t = (p.period & 0xFF) as u8 >> 1;
                    if let Some(k) = self.spawn(p.pos, t, 2) {
                        let r = self.rand() as i32;
                        self.slots[k].spin = r - 0x80;
                    }
                }
                if p.period & 0xFF == 0x76 { 2 } else { 0 }
            }
            0x35 => {
                p.size[0] = (sin14((p.life & 0x3F) * 0x40) / grow.max(1)).abs();
                0
            }
            _ => 0,
        }
    }

    /// The colour fades of 0x410F40 (by the kind's mode), from the type's colour and the life.
    fn fade(&self, p: &mut Particle, mode: u8) {
        let base = self.types.get(p.typ as usize).map_or([0x80; 3], |t| t.rgb).map(|c| c as i32);
        let l = p.life;
        let scale = |c: i32, n: i32, sh: u32| ((c * n) / 2 >> sh) as u8;
        match mode {
            1 if l < 0x20 => p.rgb = base.map(|c| scale(c, l, 4)),
            2 if l < 0x40 => p.rgb = base.map(|c| scale(c, l, 5)),
            3 if l < 0x20 => {
                p.rgb[0] = scale(base[0], l, 4);
                if l > 0x10 {
                    p.rgb[1] = scale(base[1], l - 0x10, 3);
                    p.rgb[2] = scale(base[2], l - 0x10, 3);
                } else {
                    p.rgb[1] = 0;
                    p.rgb[2] = 0;
                }
            }
            4 if l < 0x100 => p.rgb = base.map(|c| scale(c, l, 7)),
            5 => {
                if l < 0x20 {
                    p.rgb[0] = if l < 0x10 { scale(base[0], l, 4) } else { 0 };
                    p.rgb[2] = if l < 0xD { 0 } else { scale(base[2], l - 0xC, 4) };
                    p.rgb[1] = if l < 0x19 { 0 } else { scale(base[1], l - 0x18, 3) };
                } else {
                    p.rgb[0] = 0;
                }
            }
            6 => {
                let r = (p.life & 0x7F) + 0x80;
                p.rgb = base.map(|c| (c * r >> 8) as u8);
            }
            7 => {
                if l < 0x20 {
                    p.rgb = base.map(|c| scale(c, l, 4));
                } else {
                    let n = 0x2A - l;
                    p.rgb = base.map(|c| scale(c, n, 2));
                }
            }
            _ => {}
        }
    }

    /// 0x410B80: what a particle leaves behind when it ends.
    fn death(&mut self, p: &Particle) {
        match p.death {
            3 => {
                // Burst into dust on the floor.
                let y = p.size[1] * 0x20 + p.pos[1];
                for _ in 0..5 {
                    if let Some(k) = self.spawn([p.pos[0], y, p.pos[2]], 0x25, 0x11) {
                        let r = self.rand() as i32;
                        self.slots[k].life = (r & 7) * 2 + 0x20;
                    }
                }
                self.spawn([p.pos[0], y, p.pos[2]], 0x28, 2);
                self.events.push(Event::Sound(0xB, p.pos));
            }
            5 => {
                for _ in 0..5 {
                    let r = self.rand() as i32;
                    self.rand();
                    self.rand();
                    let o = ((r & 0x3F) - 0x20) * 0x3C;
                    if let Some(k) = self.spawn([p.pos[0] + o, p.pos[1] + o, p.pos[2] + o], 0x29, 0xF) {
                        let (a, b) = (self.rand() as i32, self.rand() as i32);
                        self.slots[k].spin = a - 0x80;
                        self.slots[k].life = (b & 0xF) * 2 + 0x18;
                    }
                }
            }
            _ => {}
        }
    }

    /// 0x445980: every live particle as a quad of its sprite cell, camera-facing and turned by
    /// its angle, or lying flat (flag 0x10). `texture(slot)` gives a `texNN` slot's index and
    /// size; `right` / `up` are the camera's axes in game space.
    pub fn batches(&self, sprites: &[Option<SpriteDef>], texture: &dyn Fn(i16) -> Option<(usize, [f32; 2])>, right: Vec3, up: Vec3) -> Vec<Batch> {
        let mut out = Vec::new();
        for p in self.slots.iter().filter(|p| p.life > 0 && p.sprite != 0) {
            let Some(def) = sprites.get(p.sprite as usize).and_then(|d| d.as_ref()) else { continue };
            let Some((tex, [tw, th])) = texture(def.texture) else { continue };
            let [cu, cv] = def.cells.get(p.frame as usize).copied().unwrap_or([0, 0]);
            let (u0, v0) = (cu as f32 / tw, cv as f32 / th);
            let (u1, v1) = ((cu as f32 + def.size[0] as f32) / tw, (cv as f32 + def.size[1] as f32) / th);
            let (alpha, blend) = match p.flags & 0x60 {
                0 => (0x40, Blend::Alpha),
                0x20 => (0xFF, Blend::Additive),
                0x40 => (0xFF, Blend::Darken),
                _ => (0xFF, Blend::Alpha),
            };
            let color = [alpha, p.rgb[0], p.rgb[1], p.rgb[2]];
            let c = Vec3::from(p.pos.map(|v| v as f32 / 32.0));
            let a = p.angle as f32 / 4096.0 * std::f32::consts::TAU;
            let (w, h) = (p.size[0] as f32, p.size[1] as f32);
            let (ax, ay) = if p.flags & 0x10 == 0 {
                // Facing the camera, rolled by the angle.
                let (s, k) = a.sin_cos();
                (right * k + up * s, up * k - right * s)
            } else {
                // Flat, turned about the vertical the other way.
                let (s, k) = (-a).sin_cos();
                (Vec3::new(k, 0.0, -s), Vec3::new(s, 0.0, k))
            };
            let c = if p.flags & 0x10 != 0 { c - Vec3::Y * 10.0 } else { c };
            let v = |sx: f32, sy: f32, u: f32, vv: f32| GpuVertex { pos: (c + ax * (sx * w) + ay * (sy * h)).to_array(), uv: [u, vv], color };
            let (q0, q1, q2, q3) = (v(-1.0, 1.0, u0, v0), v(1.0, 1.0, u1, v0), v(1.0, -1.0, u1, v1), v(-1.0, -1.0, u0, v1));
            scene::push(&mut out, Some(tex), blend, [q0, q1, q2]);
            scene::push(&mut out, Some(tex), blend, [q0, q2, q3]);
        }
        out
    }

    /// 0x4100F0: particles that touch Buzz (flag 2), within their size + 100 of his centre
    /// (0x1CC0 above his feet; heights count half). By sprite: shots and the like hurt him and
    /// end (some with their death action); while he spins (or super spins) the ones marked
    /// deflectable fly back off him instead (sound 7); coins (sprite 16) are collected.
    pub fn touch(&mut self, buzz: [i32; 3], spinning: bool) {
        let centre = [buzz[0], buzz[1] - 0x1CC0, buzz[2]];
        let mut hurt = None;
        for i in 0..SLOTS {
            let p = self.slots[i];
            if p.life < 1 || p.flags & 2 == 0 {
                continue;
            }
            let d = [((centre[0] - p.pos[0]) as i64 * 0x100) >> 13, ((centre[1] - p.pos[1]) as i64 * 0x80) >> 13, ((centre[2] - p.pos[2]) as i64 * 0x100) >> 13];
            let r = (p.size[0] + 100) as i64;
            if r * r <= d[0] * d[0] + d[1] * d[1] + d[2] * d[2] {
                continue;
            }
            // (how it hurts: 0 not, 1 always, 2 unless he spins; death action; sound)
            let (how, death, coin) = match p.sprite {
                0 | 1 | 0x13 => (1, 0i8, false),
                4 | 5 | 0x33 | 0x34 => (2, p.death as i8, false),
                0x10 => (0, 5, true),
                0x15 => (1, 4, false),
                0x35 | 0x36 => (1, p.death as i8, false),
                0x37 => (2, 0, false),
                _ => continue,
            };
            if how == 2 && spinning {
                // Batted away.
                let a = crate::boxes::angle12((buzz[0] - p.pos[0]) as f32, (buzz[2] - p.pos[2]) as f32);
                let q = &mut self.slots[i];
                q.flags &= !0xA;
                q.vel = [sin14(a + 0x800) >> 4, -0x400, sin14(a + 0xC00) >> 4];
                q.gravity = 0x30;
                q.life = 0x32;
                self.events.push(Event::Sound(7, q.pos));
                continue;
            }
            if how != 0 {
                hurt = Some(p.pos);
            }
            let q = &mut self.slots[i];
            if death > 0 {
                q.death = death as u8;
                q.life = 1;
            } else if death < 0 {
                q.life = 1;
            } else if coin || how != 0 {
                q.life = 1;
            }
            if coin {
                self.events.push(Event::Coin(q.pos));
            }
        }
        if let Some(at) = hurt {
            self.events.push(Event::Hurt(at));
        }
    }

    pub fn live(&self) -> usize {
        self.slots.iter().filter(|p| p.life > 0).count()
    }
}

/// 0x486280 at a particle: the nearest floor above it (within 2048) else below, object units.
fn floor_at(world: &World, pos: [i32; 3]) -> Option<i32> {
    world.floor_near(pos[0] as f32 / 32.0, pos[2] as f32 / 32.0, pos[1] as f32 / 32.0).map(|(f, _)| (f * 32.0) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn randomisation_matches_the_spawner() {
        assert_eq!(randomise(1, 0x3F, 0xFF), (0x3F - 0x1F) * 8);
        assert_eq!(randomise(2, 0x1F, 0x0F), 0x0F << 3);
        assert_eq!(randomise(3, 0x7F, 0x10), -0x80);
        assert_eq!(randomise(0, 0x123, 0x55), 0x123);
    }
}
