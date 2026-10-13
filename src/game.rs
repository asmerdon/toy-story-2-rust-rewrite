//! Gameplay core: Buzz's controller, ported from the original's per-tick routines, plus
//! collision against the level's render geometry (the original collision data is not decoded
//! yet). Game space throughout: level units, +Y down, +Z forward.
//!
//! The controller runs at the original's 60 ticks per second in the original's integer units:
//! velocities in object units per tick (level units × 32) and 12-bit angles (4096 = 360°).
//! One tick follows `BuzzControl` (0x436220): movement profile → stomp (0x434D20) →
//! airborne/jump (0x4340D0) → turn (0x4346C0) → accelerate (0x4343D0) → move and collide →
//! coyote counter and fall timer; then the animation state choice (0x4011D0) and the
//! frame-list step (0x401000). See `docs/re/functions.md`.

use crate::buzz::{state, BuzzAnimator};
use crate::scene::Batch;
use glam::Vec3;
use std::collections::HashMap;

pub const PLAYER_HEIGHT: f32 = 460.0;
/// Buzz collides as one sphere: radius 4000 object units (collider slot field +0x2C, set by
/// 0x489C30), its bottom 0xC0 above his feet (0x484380 lifts the centre by 0xC0 + radius).
const BODY_RADIUS: f32 = 4000.0 / 32.0;
const BODY_LIFT: f32 = 0xC0 as f32 / 32.0;
/// Resolver passes per tick (0x484380 runs 3, or 4 when few polygons are near).
const PASSES: usize = 4;
/// Distance kept from a surface after a hit.
const SKIN: f32 = 0.5;
/// Walls push Buzz out by 3 level units (normal × 3 / 512 in 2.14 units) and cost 1/16 of his
/// horizontal speed (0x482A00, one tick per frame).
const WALL_PUSH: f32 = 3.0;
/// While on the ground, follow it down slopes and steps by up to this much per tick.
const STICK_DOWN: f32 = 64.0;
/// Teeter (0x4011D0): a drop counts when the floor is more than 0x1000 object units below; the
/// probes are 0x1000 object units ahead and behind (sine table >> 2).
const TEETER_DROP: f32 = 128.0;
const TEETER_PROBE: f32 = 128.0;
/// Game logic ticks per second (0x490860).
pub const TICKS: f32 = 60.0;
/// Most ticks run for one rendered frame, so a stall doesn't fast-forward the game.
const MAX_TICKS_PER_UPDATE: f32 = 8.0;

// Default movement profile built by 0x436220 (object units per tick; 1 tick per call).
const SIDE_DECEL: i32 = 0x80;
const FRICTION: i32 = 0x30;
const AIR_FRICTION: i32 = 0x10;
const ACCEL: i32 = 0x28;
const JUMP_VY: i32 = -0x600;
const MAX_SPEED: i32 = 0x380;
const MAX_FORWARD: i32 = 0x400;
const TURN_CAP: i32 = 0x300;
/// Sharp reversals (≥ 0x5DD, about 132°) snap round instead of turning.
const SNAP_TURN: i32 = 0x5DD;
const SKID_TICKS: i32 = 0x1A;
const SKID_FRICTION: i32 = 4;
const SKID_SIDE_DECEL: i32 = 0x10;

// Airborne routine 0x4340D0 with its normal divisor k = 2.
const K: i32 = 2;
const GRAVITY: i32 = 0x100 / (K * K);
const TERMINAL_FALL: i32 = 0x800;
const DOUBLE_JUMP_MAX_RISE: i32 = -0x400;
const DOUBLE_JUMP_APEX: i32 = 0x6A80;
const COYOTE_TICKS: i32 = 6;

// Fall timer 0x53C838.
const FALL_PANIC_AFTER: i32 = 0x3C;
const FALL_PANIC: i32 = 0x50;
const FALL_RECOVERY: i32 = -0x46;

// Stomp timer 0x53C64C.
const STOMP_HANG: i32 = 0xE;
const STOMP_RECOVERY: i32 = -0x28;

/// The original's sine table (0x4FE788 holds round(16384 · sin)), as used: shifted down to 4.12.
fn sin12(a: i32) -> i32 {
    let t = (a & 0xFFF) as f64 * std::f64::consts::TAU / 4096.0;
    ((16384.0 * t.sin()).round() as i32) >> 2
}

fn cos12(a: i32) -> i32 {
    sin12(a + 0x400)
}

/// Object units → level units.
fn obj(v: i32) -> f32 {
    v as f32 / 32.0
}

/// Above the first jump's start (h object units higher), 0x4340D0 launches at the speed that
/// peaks 0x6A80 above that start: v = sqrt(2 · g · (0x6A80 − h)), truncated and clamped to
/// [0x22A, 0x74C]. A negative radicand gives the x87 "indefinite" integer, which the clamp turns
/// into the maximum 0x74C.
fn double_jump_speed_high(h_obj: i32) -> i32 {
    let r = 2 * GRAVITY * (DOUBLE_JUMP_APEX - h_obj);
    if r < 0 { 0x74C } else { ((r as f64).sqrt() as i32).clamp(0x22A, 0x74C) }
}

const CELL: f32 = 1024.0;
/// Floor-like contacts: normal y below this (+Y is down, so floors point to −Y). The resolver
/// treats normals with y < −0x2000 (of 0x4000) as floors: slopes up to 60°.
const FLOOR_NY: f32 = -0.5;

#[derive(Debug, Clone, Copy)]
struct Tri {
    a: Vec3,
    b: Vec3,
    c: Vec3,
    /// Unit normal on the solid side's face.
    n: Vec3,
    /// Render-geometry fallback: winding unknown, so collide from both sides.
    two_sided: bool,
}

/// What a sweep hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// Fraction of the motion travelled before contact.
    pub t: f32,
    /// Contact normal, pointing from the surface towards the sphere.
    pub normal: Vec3,
}

pub struct World {
    tris: Vec<Tri>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    pub lo: Vec3,
    pub hi: Vec3,
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl World {
    /// From visible geometry (when there is no terrain file): two-sided.
    pub fn from_batches(batches: &[Batch]) -> World {
        let mut tris = Vec::new();
        for b in batches {
            for t in b.vertices.chunks_exact(3) {
                tris.push([0, 1, 2].map(|i| Vec3::from(t[i].pos)));
            }
        }
        World::from_triangles(tris)
    }

    /// From the original collision data (`TERRAIN.ALL`, see `formats::terrain`), one-sided
    /// with the stored normals, as the original tests them.
    pub fn from_terrain(t: &crate::formats::terrain::Terrain) -> World {
        let tris = t
            .collision_triangles()
            .into_iter()
            .map(|(p, n)| {
                let [a, b, c] = p.map(Vec3::from);
                Tri { a, b, c, n: Vec3::from(n).normalize_or_zero(), two_sided: false }
            })
            .collect();
        World::build(tris)
    }

    /// Plain triangles, two-sided, normals from the winding.
    pub fn from_triangles(tris: Vec<[Vec3; 3]>) -> World {
        let tris = tris
            .into_iter()
            .map(|[a, b, c]| {
                let mut n = (b - a).cross(c - a).normalize_or_zero();
                // Prefer the upward (−Y) face for floors so floor_below reads them consistently.
                if n.y > 0.0 {
                    n = -n;
                }
                Tri { a, b, c, n, two_sided: true }
            })
            .collect();
        World::build(tris)
    }

    fn build(tris: Vec<Tri>) -> World {
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (i, t) in tris.iter().enumerate() {
            let tlo = t.a.min(t.b).min(t.c);
            let thi = t.a.max(t.b).max(t.c);
            lo = lo.min(tlo);
            hi = hi.max(thi);
            for cx in cell(tlo.x)..=cell(thi.x) {
                for cz in cell(tlo.z)..=cell(thi.z) {
                    grid.entry((cx, cz)).or_default().push(i as u32);
                }
            }
        }
        World { tris, grid, lo, hi }
    }

    /// Triangles whose grid cells overlap the box around `lo..hi` (x/z).
    fn near_box(&self, lo: Vec3, hi: Vec3) -> Vec<u32> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for cx in cell(lo.x)..=cell(hi.x) {
            for cz in cell(lo.z)..=cell(hi.z) {
                if let Some(v) = self.grid.get(&(cx, cz)) {
                    out.extend(v.iter().copied().filter(|i| seen.insert(*i)));
                }
            }
        }
        out
    }

    /// Height (y, game space) of the nearest floor at or below `from_y` under (x, z): the
    /// point floor query (0x486280 / 0x486520).
    pub fn floor_below(&self, x: f32, z: f32, from_y: f32) -> Option<f32> {
        let p = Vec3::new(x, 0.0, z);
        let mut best: Option<f32> = None;
        for i in self.near_box(p, p) {
            let t = &self.tris[i as usize];
            if t.n.y > FLOOR_NY {
                continue;
            }
            let (a, b, c) = (t.a, t.b, t.c);
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

    /// Earliest contact of a sphere of radius `r` moving from `p0` by `d` (the sweep 0x481FB0
    /// runs for each polygon: its face, then its edges as cylinders and corners as spheres).
    pub fn sweep(&self, p0: Vec3, d: Vec3, r: f32) -> Option<Hit> {
        let p1 = p0 + d;
        let reach = Vec3::splat(r + 2.0);
        let mut best: Option<Hit> = None;
        let mut keep = |h: Option<Hit>| {
            if let Some(h) = h
                && best.is_none_or(|b| h.t < b.t)
            {
                best = Some(h);
            }
        };
        for i in self.near_box(p0.min(p1) - reach, p0.max(p1) + reach) {
            let t = &self.tris[i as usize];
            let lo = t.a.min(t.b).min(t.c) - reach;
            let hi = t.a.max(t.b).max(t.c) + reach;
            if p0.max(p1).cmplt(lo).any() || p0.min(p1).cmpgt(hi).any() {
                continue;
            }
            for n in if t.two_sided { [t.n, -t.n].to_vec() } else { vec![t.n] } {
                let d0 = n.dot(p0 - t.a) - r;
                let d1 = n.dot(p1 - t.a) - r;
                // Faces only stop motion into their front side.
                if d0 >= 0.0 && d1 < 0.0 {
                    let s = d0 / (d0 - d1);
                    let q = p0 + d * s - n * r;
                    if inside(q, t.a, t.b, t.c, n) {
                        keep(Some(Hit { t: s, normal: n }));
                    }
                }
                // Edges and corners only near the plane, as the original limits them.
                let near = (d0 < 8.0 || d1 < 8.0) && (d0 >= -8.0 - r / 2.0 || d1 >= -8.0 - r / 2.0);
                if near && d.dot(n) < 0.0 {
                    for (e0, e1) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
                        keep(sweep_edge(p0, d, e0, e1, r + 1.0));
                    }
                    for v in [t.a, t.b, t.c] {
                        keep(sweep_point(p0, d, v, r + 2.0));
                    }
                }
            }
        }
        best
    }
}

/// Is `q` (on the plane) inside triangle abc with normal n?
fn inside(q: Vec3, a: Vec3, b: Vec3, c: Vec3, n: Vec3) -> bool {
    let e = 1e-3;
    let s0 = (b - a).cross(q - a).dot(n);
    let s1 = (c - b).cross(q - b).dot(n);
    let s2 = (a - c).cross(q - c).dot(n);
    (s0 >= -e && s1 >= -e && s2 >= -e) || (s0 <= e && s1 <= e && s2 <= e)
}

/// Sphere centre moving p0 → p0+d against a point: returns the entry time.
fn sweep_point(p0: Vec3, d: Vec3, v: Vec3, r: f32) -> Option<Hit> {
    let m = p0 - v;
    let a = d.dot(d);
    let b = m.dot(d);
    let c = m.dot(m) - r * r;
    if a < 1e-9 || b >= 0.0 {
        return None;
    }
    if c < 0.0 {
        return None; // already inside: leave it to the faces
    }
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let normal = (p0 + d * t - v).normalize_or_zero();
    Some(Hit { t, normal })
}

/// Sphere centre moving p0 → p0+d against segment e0–e1 (a cylinder of radius r).
fn sweep_edge(p0: Vec3, d: Vec3, e0: Vec3, e1: Vec3, r: f32) -> Option<Hit> {
    let ed = e1 - e0;
    let len2 = ed.dot(ed);
    if len2 < 1e-6 {
        return None;
    }
    // Work perpendicular to the edge.
    let perp = |v: Vec3| v - ed * (v.dot(ed) / len2);
    let m = perp(p0 - e0);
    let dp = perp(d);
    let a = dp.dot(dp);
    let b = m.dot(dp);
    let c = m.dot(m) - r * r;
    if a < 1e-9 || b >= 0.0 || c < 0.0 {
        return None;
    }
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let pc = p0 + d * t;
    let s = (pc - e0).dot(ed) / len2;
    if !(0.0..=1.0).contains(&s) {
        return None;
    }
    let normal = (pc - (e0 + ed * s)).normalize_or_zero();
    Some(Hit { t, normal })
}

/// A sound the game asked for this update, at Buzz's position (PlaySound 0x49E660, or the
/// one-per-object voice channel 0x49EA60 when `voice`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sound {
    pub id: u16,
    pub voice: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Input {
    /// Desired move direction in game space (x, z), length ≤ 1 (the stick).
    pub move_xz: [f32; 2],
    /// Jump pressed since the last update (edge); kept until a tick sees it.
    pub jump: bool,
    /// Jump button currently held (Cross, pad bit 0x4000).
    pub jump_held: bool,
    /// Stomp pressed since the last update (Circle, pad bit 0x2000).
    pub stomp: bool,
}

/// Jump phase, as the original keeps it at player +0x8C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JumpPhase {
    /// 0: on the ground, or fell off a ledge without jumping.
    #[default]
    None,
    /// 1: first jump, button still held.
    Holding,
    /// 2: first jump, button released; a new press can double jump.
    Released,
    /// 3: no more jumps until landing (double jump used up or wasted).
    Spent,
    /// 5: double jump, button held.
    Double,
    /// 6: double jump, button released.
    DoubleReleased,
}

#[derive(Debug, Clone)]
pub struct Player {
    /// Feet position, game space, level units.
    pub pos: Vec3,
    /// Velocity in object units per tick (+0x68 / +0x6C / +0x70).
    pub vel: [i32; 3],
    /// Facing, 12-bit (+0x0E); 0 = +Z.
    pub facing: i32,
    /// Facing the stick asks for (+0x48).
    target: i32,
    /// Velocity along / across the facing (+0x74 / +0x78).
    pub forward: i32,
    pub side: i32,
    /// Floor contact from collision (+0x8E).
    pub on_floor: bool,
    /// Ticks of "can still jump" left: 6 while on the floor, counting down in the air (+0x9C).
    pub coyote: i32,
    pub jump: JumpPhase,
    /// Animation state (0x52F390), a row of the move table.
    pub state: u16,
    /// First jump's start height in object units (0x53C674, valid while 0x53C61C is set).
    jump_start: Option<i32>,
    /// 0x53C838: > 0 counts falling ticks, 0x50 = panic, < 0 = landing recovery.
    pub fall_timer: i32,
    /// 0x53C64C: > 0 stomp hang/slam, < 0 recovery after impact.
    pub stomp: i32,
    /// 0x53C5D4: skid ticks after a sharp reversal.
    pub skid: i32,
    /// 0x53C678 / 0x53C5D0: stick-tap length (−1 = off) and the facing it started from.
    tap: i32,
    tap_start: i32,
    /// 0x50A098: standing sub-state chosen when Buzz stops.
    idle_sub: u16,
    jump_latch: bool,
    stomp_latch: bool,
    accum: f32,
    pub spawn: Vec3,
    spawn_facing: i32,
    /// Plays the move table; set by the caller when toy2.exe is available.
    pub anim: Option<BuzzAnimator>,
    /// `data\rand.dat`, the game's table of random bytes, and the read position (0x52ADAC).
    random: Vec<u8>,
    random_pos: usize,
    /// Sounds requested since the caller last drained this.
    pub sounds: Vec<Sound>,
}

/// The read pointer into rand.dat goes back to the start after this many bytes.
const RANDOM_WRAP: usize = 0x5DC;
/// Entry of the standing list where Buzz may start a fidget.
const FIDGET_ENTRY: u32 = 14;

impl Player {
    pub fn new(pos: Vec3) -> Player {
        Player {
            pos,
            vel: [0; 3],
            facing: 0,
            target: 0,
            forward: 0,
            side: 0,
            on_floor: false,
            coyote: 0,
            jump: JumpPhase::None,
            state: state::IDLE,
            jump_start: None,
            fall_timer: 0,
            stomp: 0,
            skid: 0,
            tap: 0,
            tap_start: 0,
            idle_sub: 0,
            jump_latch: false,
            stomp_latch: false,
            accum: 0.0,
            spawn: pos,
            spawn_facing: 0,
            anim: None,
            random: Vec::new(),
            random_pos: 0,
            sounds: Vec::new(),
        }
    }

    /// Gives the player the game's random table (`data\rand.dat`); without it Buzz never fidgets.
    pub fn set_random_table(&mut self, bytes: Vec<u8>) {
        self.random = bytes;
        self.random_pos = 0;
    }

    fn sound(&mut self, id: u16) {
        self.sounds.push(Sound { id, voice: false });
    }

    fn next_random(&mut self) -> Option<u8> {
        let b = *self.random.get(self.random_pos)?;
        self.random_pos += 1;
        if self.random_pos > RANDOM_WRAP {
            self.random_pos = 0;
        }
        Some(b)
    }

    /// On stopping: if the floor under Buzz's centre is a drop (he's standing on an edge),
    /// teeter facing the drop (10) or with it behind him (11). Part of 0x4011D0.
    fn teeter(&self, world: &World) -> Option<u16> {
        let (x, y, z) = (self.pos.x, self.pos.y, self.pos.z);
        let drop = |px: f32, pz: f32, from: f32| world.floor_below(px, pz, from).is_some_and(|f| f - y > TEETER_DROP);
        // 0x4863A0 probes from 0x400 object units above the feet.
        if !drop(x, z, y - 32.0) {
            return None;
        }
        let (s, c) = (sin12(self.facing) as f32 / 4096.0, cos12(self.facing) as f32 / 4096.0);
        if drop(x + s * TEETER_PROBE, z + c * TEETER_PROBE, y) {
            Some(state::TEETER_FRONT)
        } else if drop(x - s * TEETER_PROBE, z - c * TEETER_PROBE, y) {
            Some(state::TEETER_BACK)
        } else {
            None
        }
    }

    /// While standing on the idle list's entry 14, one random byte a tick; 1 in 4 starts a
    /// fidget (0x1C or 0x1D, by a second byte). Part of 0x4011D0.
    fn roll_fidget(&mut self) {
        let at_entry = self.anim.as_ref().and_then(|a| a.entry()) == Some((state::IDLE, FIDGET_ENTRY));
        if !at_entry || self.state != state::IDLE {
            return;
        }
        if let Some(b) = self.next_random()
            && b & 3 == 0
        {
            let which = self.next_random().unwrap_or(0) & 1;
            self.idle_sub = state::FIDGET + which as u16;
        }
    }

    /// Sets the facing (12-bit units) and makes it the respawn facing.
    pub fn set_facing(&mut self, angle: i32) {
        self.facing = angle & 0xFFF;
        self.target = self.facing;
        self.spawn_facing = self.facing;
    }

    /// Facing in radians around the vertical axis (0 = +Z).
    pub fn yaw(&self) -> f32 {
        self.facing as f32 / 4096.0 * std::f32::consts::TAU
    }

    /// Horizontal speed in level units per second.
    pub fn horizontal_speed(&self) -> f32 {
        obj(self.vel[0]).hypot(obj(self.vel[2])) * TICKS
    }

    fn y_obj(&self) -> i32 {
        (self.pos.y * 32.0).round() as i32
    }

    /// Advances by `dt` seconds of game time in whole 60 Hz ticks.
    pub fn update(&mut self, world: &World, input: Input, dt: f32) {
        self.jump_latch |= input.jump;
        self.stomp_latch |= input.stomp;
        self.accum = (self.accum + dt * TICKS).min(MAX_TICKS_PER_UPDATE);
        while self.accum >= 1.0 {
            self.accum -= 1.0;
            let held = input.jump_held || self.jump_latch;
            let stomp = self.stomp_latch;
            self.jump_latch = false;
            self.stomp_latch = false;
            self.tick(world, input.move_xz, held, stomp);
        }
    }

    /// One game tick.
    pub fn tick(&mut self, world: &World, stick: [f32; 2], jump_held: bool, stomp_pressed: bool) {
        let magnitude = stick[0].hypot(stick[1]).min(1.0);
        let pushing = magnitude > 0.0;
        // Movement profile.
        let mut side_decel = SIDE_DECEL;
        let mut friction = FRICTION;
        if self.coyote == 0 {
            friction = AIR_FRICTION;
        }
        if self.skid > 0 {
            self.skid -= 1;
            if pushing {
                friction = SKID_FRICTION;
                side_decel = SKID_SIDE_DECEL;
            }
        }

        match self.stomp_tick(stomp_pressed) {
            0 => {
                if self.fall_timer < 0 {
                    self.fall_timer = (self.fall_timer + 1).min(0);
                }
                self.airborne(jump_held);
                let mut max = MAX_SPEED;
                if pushing {
                    let angle = (stick[0].atan2(stick[1]) / std::f32::consts::TAU * 4096.0).round() as i32;
                    // Stick magnitude, 0x4000 = full (0x433F40).
                    max = (max * (magnitude * 16384.0) as i32) >> 14;
                    self.turn(Some(angle & 0xFFF));
                } else {
                    self.turn(None);
                }
                self.accelerate(side_decel, friction, max, pushing);
            }
            // 7: hang (no motion at all); 5: no horizontal motion.
            code => {
                self.vel[0] = 0;
                self.vel[2] = 0;
                if code & 2 != 0 {
                    self.vel[1] = 0;
                }
            }
        }

        self.move_and_collide(world);

        if self.on_floor {
            self.coyote = COYOTE_TICKS;
        } else if self.coyote > 0 {
            self.coyote -= 1;
        }
        self.fall_bookkeeping();

        // Animation state, then the frame list. A list's 0xFE end code returns Buzz to state 0
        // and the original picks again straight away.
        for _ in 0..3 {
            self.select_state(world, pushing);
            let speed = (self.forward as f64).hypot(self.side as f64) as i32;
            let Player { anim, state, .. } = self;
            if !anim.as_mut().is_some_and(|a| a.tick(state, speed)) {
                break;
            }
            // 0xFE also clears the standing sub-state (0x50A098).
            self.idle_sub = 0;
        }
        if let Some(a) = self.anim.as_mut() {
            let ids: Vec<u16> = a.sounds.drain(..).collect();
            for id in ids {
                self.sound(id);
            }
        }

        if self.pos.y > world.hi.y + 4000.0 {
            let (spawn, facing, anim) = (self.spawn, self.spawn_facing, self.anim.take());
            *self = Player::new(spawn);
            self.set_facing(facing);
            self.anim = anim;
        }
    }

    /// 0x434D20. Returns 0 to carry on normally, 5 to stop horizontal motion, 7 to hang.
    fn stomp_tick(&mut self, pressed: bool) -> i32 {
        let blocked = self.coyote != 0 || self.fall_timer < 0 || self.fall_timer == FALL_PANIC || self.stomp != 0;
        let can = matches!(self.jump, JumpPhase::Holding | JumpPhase::Released) || self.state == state::DOUBLE_JUMP;
        if pressed && !blocked && can {
            self.stomp = 1;
            self.sound(0x17);
        }
        if self.stomp == 0 {
            return 0;
        }
        if self.stomp < 1 {
            self.stomp += 1;
            if self.stomp > 0 {
                self.stomp = 0;
                return 0;
            }
            if self.stomp < -STOMP_HANG {
                if !self.on_floor {
                    self.vel[1] += 0x40;
                }
                self.vel[1] = self.vel[1].min(TERMINAL_FALL);
                return 5;
            }
        } else if !self.on_floor {
            self.stomp += 1;
            if self.stomp > STOMP_HANG {
                self.vel[1] = TERMINAL_FALL;
                self.stomp = STOMP_HANG;
                return 5;
            }
            return 7;
        } else {
            // Impact: dust and a camera shake in the original (not drawn yet).
            self.stomp = STOMP_RECOVERY;
            self.sound(0xF);
        }
        0
    }

    /// 0x4340D0: jump start, variable height, double jump, gravity.
    fn airborne(&mut self, held: bool) {
        let y = self.y_obj();
        let vy = &mut self.vel[1];
        if !held {
            if self.on_floor {
                self.jump = JumpPhase::None;
            }
            if self.jump == JumpPhase::Holding {
                // Letting go while rising cuts the jump short.
                if *vy < 0 {
                    if *vy < -800 / K {
                        *vy += 0x180 / (K * K);
                    }
                    *vy = *vy / 2 - 0x100 / (K * K);
                }
                self.jump = JumpPhase::Released;
            }
            if self.jump == JumpPhase::Double {
                self.jump = JumpPhase::DoubleReleased;
            }
        } else {
            let double = self.jump == JumpPhase::Released && *vy > DOUBLE_JUMP_MAX_RISE && self.fall_timer >= 0 && self.coyote == 0;
            // A press after a second of falling without having jumped also gets one.
            let rescue = self.jump == JumpPhase::None && self.fall_timer == FALL_PANIC;
            if double || rescue {
                self.fall_timer = 0;
                *vy = match self.jump_start.map(|s| s - y) {
                    Some(h) if h > 0 => -double_jump_speed_high(h),
                    _ => -0x900 / K,
                };
                self.jump = JumpPhase::Double;
                self.state = state::DOUBLE_JUMP;
                self.sounds.push(Sound { id: 0x10, voice: false });
            }
            match self.jump {
                JumpPhase::Holding | JumpPhase::Double | JumpPhase::DoubleReleased => {
                    if self.on_floor {
                        self.jump = JumpPhase::Double;
                    } else if self.jump == JumpPhase::DoubleReleased {
                        self.jump = JumpPhase::Spent;
                    }
                }
                _ if self.coyote == 0 || self.stomp != 0 || self.fall_timer == FALL_PANIC => {
                    // A press that can't double jump uses it up (e.g. a third press).
                    if self.jump == JumpPhase::Released {
                        self.jump = JumpPhase::Spent;
                    }
                }
                _ => {
                    self.jump = JumpPhase::Holding;
                    self.on_floor = false;
                    self.coyote = 0;
                    self.state = state::JUMP;
                    *vy = JUMP_VY;
                    self.skid = 0;
                    self.jump_start = Some(y);
                    self.sounds.push(Sound { id: 0x19, voice: true });
                }
            }
        }
        let vy = &mut self.vel[1];
        if !self.on_floor {
            *vy += GRAVITY;
        }
        *vy = (*vy).min(TERMINAL_FALL);
    }

    /// 0x4346C0: turn towards the stick (`stick` = its 12-bit direction, if pushed).
    fn turn(&mut self, stick: Option<i32>) {
        match stick {
            None => {
                // A short tap from standing only turns part of the way.
                if self.tap > 0 && self.tap < 0x10 && self.coyote != 0 {
                    let mut d = (self.tap_start - self.target) & 0xFFF;
                    if d > 0x800 {
                        d -= 0xFFF; // sic: the original subtracts 0xFFF
                    }
                    self.target = (self.tap_start - self.tap * d / 16) & 0xFFF;
                    self.tap = -1;
                }
                if self.forward < 0x200 && self.side.abs() < 0x200 && self.coyote != 0 {
                    self.tap = 0;
                }
            }
            Some(angle) => {
                if self.tap != -1 && self.tap < 0x10 {
                    if self.tap == 0 {
                        self.tap_start = self.facing;
                    }
                    self.tap = (self.tap + 1).min(0x10);
                }
                self.target = angle;
            }
        }
        let diff = (self.facing - self.target) & 0xFFF;
        let (amount, sign) = if diff <= 0x800 { (diff, -1) } else { (0x1000 - diff, 1) };
        if amount < SNAP_TURN {
            self.facing += sign * amount.min(TURN_CAP) / 8;
        } else {
            // Sharp reversal: snap round; on the ground, skid (sound 0x12).
            self.facing = self.target;
            self.tap_start = self.target;
            if self.coyote != 0 {
                self.skid = SKID_TICKS;
                self.sound(0x12);
            }
        }
        self.facing &= 0xFFF;
    }

    /// 0x4343D0: accelerate along the facing and bleed off sideways speed.
    fn accelerate(&mut self, side_decel: i32, friction: i32, max: i32, pushing: bool) {
        let (s, c) = (sin12(self.facing), cos12(self.facing));
        let [vx, _, vz] = self.vel;
        let toward_zero = |v: i32, by: i32| if v < 0 { (v + by).min(0) } else { (v - by).max(0) };
        let side = toward_zero((vx * c - vz * s) / 4096, side_decel).clamp(-max, max);
        let mut fwd = toward_zero((vz * c + vx * s) / 4096, friction);
        if pushing && fwd < max {
            fwd += ACCEL + friction;
        }
        let fwd = fwd.clamp(-MAX_FORWARD, MAX_FORWARD);
        self.forward = fwd;
        self.side = side;
        self.vel[0] = (side * c + fwd * s) / 4096;
        self.vel[2] = (fwd * c - side * s) / 4096;
    }

    /// Move and collide (0x434550 → 0x484380): sweep Buzz's sphere, slide along what it hits;
    /// floor-like contacts (slopes up to 60°) set floor contact.
    fn move_and_collide(&mut self, world: &World) {
        let lift = Vec3::Y * (BODY_LIFT + BODY_RADIUS);
        let mut c = self.pos - lift;
        let mut v = Vec3::new(self.vel[0] as f32, self.vel[1] as f32, self.vel[2] as f32);
        let mut d = v / 32.0;
        let was = self.on_floor;
        let mut floor = false;
        for _ in 0..PASSES {
            if d.length_squared() < 1e-8 {
                break;
            }
            let Some(h) = world.sweep(c, d, BODY_RADIUS) else {
                c += d;
                break;
            };
            let len = d.length();
            let s = (h.t - SKIN / len).max(0.0);
            c += d * s;
            let n = h.normal;
            let rest = d * (1.0 - s);
            d = rest - n * rest.dot(n).min(0.0);
            v -= n * v.dot(n).min(0.0);
            if n.y < FLOOR_NY {
                floor = true;
            } else {
                c += Vec3::new(n.x, 0.0, n.z) * WALL_PUSH;
                v.x *= 15.0 / 16.0;
                v.z *= 15.0 / 16.0;
            }
        }
        // Stay on the ground walking down slopes and steps.
        if !floor && was && v.y >= 0.0 {
            let down = Vec3::Y * STICK_DOWN;
            if let Some(h) = world.sweep(c, down, BODY_RADIUS)
                && h.normal.y < FLOOR_NY
            {
                c += down * (h.t - SKIN / STICK_DOWN).max(0.0);
                floor = true;
            }
        }
        if floor && v.y > 0.0 {
            v.y = 0.0;
        }
        self.pos = c + lift;
        self.on_floor = floor;
        self.vel = [v.x.round() as i32, v.y.round() as i32, v.z.round() as i32];
    }

    /// Fall timer, after the move (end of 0x436220).
    fn fall_bookkeeping(&mut self) {
        if self.fall_timer < 0 {
            return;
        }
        if self.vel[1] <= 0x80 || self.on_floor {
            if self.coyote == 0 {
                if self.fall_timer != FALL_PANIC {
                    self.fall_timer = 0;
                }
            } else {
                if self.fall_timer == FALL_PANIC {
                    // Landing after a long fall: stop dead, recover.
                    self.fall_timer = FALL_RECOVERY;
                    self.vel = [0; 3];
                    self.sound(0x2F);
                }
                if self.fall_timer > 0 {
                    self.fall_timer = 0;
                }
            }
        } else {
            self.fall_timer += 1;
            if self.fall_timer > FALL_PANIC_AFTER {
                // Cancels a stomp. The original requests the scream every tick; once is enough.
                if self.fall_timer != FALL_PANIC + 1 {
                    self.sound(0x16);
                }
                self.fall_timer = FALL_PANIC;
                self.stomp = 0;
            }
        }
    }

    /// The part of 0x4011D0 that picks the animation state for the moves ported so far.
    /// Not yet: states set by other systems.
    fn select_state(&mut self, world: &World, pushing: bool) {
        if self.coyote == 0 {
            self.idle_sub = 0;
            if self.state == state::TEETER_FRONT || self.state == state::TEETER_BACK {
                self.state = state::RUN;
            } else if self.state == state::JUMP {
                if self.vel[1] > 0 {
                    self.state = state::FALL;
                }
            } else if self.state == 5 {
                self.state = state::LAND;
            }
            if self.fall_timer == FALL_PANIC {
                self.state = state::FALL_PANIC;
            }
        } else {
            if self.state == state::FALL {
                self.state = state::LAND;
            }
            if self.forward < 0x200 && self.side.abs() < 0x200 && !pushing {
                if self.state == state::LAND {
                    self.idle_sub = 0;
                } else {
                    if self.idle_sub == 0 {
                        self.idle_sub = self.teeter(world).unwrap_or(state::IDLE);
                    } else if self.idle_sub == state::IDLE {
                        self.roll_fidget();
                    }
                    self.state = self.idle_sub;
                }
            } else {
                self.idle_sub = 0;
                self.state = state::RUN;
            }
            if self.skid != 0 {
                self.state = state::SKID;
            }
        }
        if self.stomp > 0 {
            self.state = state::STOMP;
        }
        if self.stomp < -STOMP_HANG {
            self.state = state::STOMP_LAND;
        }
        if self.fall_timer < 0 {
            self.state = state::FALL_RECOVER;
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

    fn landed(w: &World) -> Player {
        let mut p = Player::new(Vec3::ZERO);
        for _ in 0..10 {
            p.tick(w, [0.0; 2], false, false);
        }
        assert!(p.on_floor && p.coyote == COYOTE_TICKS);
        p
    }

    #[test]
    fn sine_table_matches_original_samples() {
        assert_eq!(sin12(0), 0);
        assert_eq!(sin12(0x400), 4096);
        assert_eq!(sin12(0x200), 11585 >> 2);
        assert_eq!(sin12(0xC00), -4096);
    }

    #[test]
    fn falls_and_lands_on_floor() {
        let w = room();
        let mut p = Player::new(Vec3::new(0.0, -500.0, 0.0));
        for _ in 0..120 {
            p.update(&w, Input::default(), 1.0 / 60.0);
        }
        assert!(p.on_floor);
        // The sphere rests on the floor with its bottom 0xC0 above the feet, as in the original.
        assert!((p.pos.y - BODY_LIFT).abs() < 2.0, "{:?}", p.pos);
    }

    #[test]
    fn wall_stops_running() {
        let w = room();
        let mut p = Player::new(Vec3::ZERO);
        for _ in 0..240 {
            p.update(&w, Input { move_xz: [1.0, 0.0], ..Default::default() }, 1.0 / 60.0);
        }
        assert!(p.pos.x < 1000.0 - BODY_RADIUS + 5.0 && p.pos.x > 800.0, "{:?}", p.pos);
    }

    #[test]
    fn turns_gradually_and_runs_at_top_speed() {
        let w = room();
        let mut p = landed(&w);
        // Stick 90° to the left: at most 0x300 / 8 per tick.
        p.tick(&w, [-1.0, 0.0], false, false);
        assert_eq!(p.facing, 0x1000 - 0x60);
        for _ in 0..60 {
            p.tick(&w, [-1.0, 0.0], false, false);
        }
        // Steps are diff / 8, so the last 7 units are never turned (as in the original).
        assert!((p.facing - 0xC00) < 8 && p.facing >= 0xC00, "{:#x}", p.facing);
        assert!(p.forward >= MAX_SPEED - FRICTION && p.forward <= MAX_SPEED + ACCEL + FRICTION, "{}", p.forward);
    }

    #[test]
    fn sharp_reversal_snaps_and_skids() {
        let w = room();
        let mut p = landed(&w);
        for _ in 0..30 {
            p.tick(&w, [0.0, 1.0], false, false);
        }
        p.tick(&w, [0.0, -1.0], false, false);
        assert_eq!(p.facing, 0x800);
        assert_eq!(p.state, state::SKID);
    }

    /// Jumps from the floor holding the button for `hold` ticks; returns the peak height.
    fn jump_peak(hold: usize) -> (f32, Player) {
        let w = room();
        let mut p = landed(&w);
        let ground = p.pos.y;
        let mut peak: f32 = 0.0;
        for i in 0..180 {
            p.tick(&w, [0.0; 2], i <= hold, false);
            peak = peak.min(p.pos.y - ground);
        }
        (peak, p)
    }

    #[test]
    fn held_jump_goes_higher_than_tap_and_lands() {
        let (full, p) = jump_peak(120);
        let (tap, _) = jump_peak(0);
        // 23 ticks rising from −0x5C0 by 0x40 each: 552 level units.
        assert!((full + 552.0).abs() < 2.0, "full jump peak {full}");
        assert!(tap > full / 2.0, "tap {tap} should be well below full {full}");
        assert!(p.on_floor);
        assert_eq!(p.jump, JumpPhase::None);
    }

    #[test]
    fn double_jump_at_apex_peaks_0x6a80_above_start() {
        let w = room();
        let mut p = landed(&w);
        for _ in 0..23 {
            p.tick(&w, [0.0; 2], true, false);
        }
        p.tick(&w, [0.0; 2], false, false);
        assert_eq!(p.jump, JumpPhase::Released);
        p.tick(&w, [0.0; 2], true, false);
        assert_eq!(p.jump, JumpPhase::Double);
        assert_eq!(p.state, state::DOUBLE_JUMP);
        let mut peak: f32 = 0.0;
        for _ in 0..120 {
            p.tick(&w, [0.0; 2], true, false);
            peak = peak.min(p.pos.y - BODY_LIFT);
        }
        assert!((peak + DOUBLE_JUMP_APEX as f32 / 32.0).abs() < 40.0, "double jump peak {peak}");
    }

    #[test]
    fn only_one_double_jump() {
        let w = room();
        let mut p = landed(&w);
        p.tick(&w, [0.0; 2], true, false);
        p.tick(&w, [0.0; 2], false, false);
        for _ in 0..7 {
            p.tick(&w, [0.0; 2], false, false); // wait out the coyote counter
        }
        p.tick(&w, [0.0; 2], true, false);
        assert_eq!(p.jump, JumpPhase::Double);
        p.tick(&w, [0.0; 2], false, false);
        let vy = p.vel[1];
        p.tick(&w, [0.0; 2], true, false);
        assert_eq!(p.jump, JumpPhase::Spent);
        assert_eq!(p.vel[1], vy + GRAVITY);
    }

    #[test]
    fn stomp_hangs_then_slams() {
        let w = room();
        let mut p = landed(&w);
        for _ in 0..20 {
            p.tick(&w, [0.0; 2], true, false);
        }
        p.tick(&w, [0.0; 2], true, true);
        let y = p.pos.y;
        for _ in 0..10 {
            p.tick(&w, [0.0; 2], true, false);
        }
        assert_eq!(p.pos.y, y, "hangs in the air");
        assert_eq!(p.state, state::STOMP);
        for _ in 0..40 {
            p.tick(&w, [0.0; 2], true, false);
            if p.on_floor {
                break;
            }
        }
        assert!(p.on_floor);
        p.tick(&w, [0.0; 2], false, false);
        assert_eq!(p.state, state::STOMP_LAND);
    }

    #[test]
    fn long_fall_panics_and_recovers() {
        let w = room();
        let mut p = Player::new(Vec3::new(0.0, -8000.0, 0.0));
        let mut panicked = false;
        for _ in 0..300 {
            p.tick(&w, [0.0; 2], false, false);
            panicked |= p.state == state::FALL_PANIC;
            if p.on_floor {
                break;
            }
        }
        assert!(panicked);
        p.tick(&w, [0.0; 2], false, false);
        assert_eq!(p.state, state::FALL_RECOVER);
    }

    #[test]
    fn standing_still_rolls_a_fidget_on_entry_14() {
        use crate::formats::exe::BuzzMove;
        let row = |list: Vec<u8>| BuzzMove { frames: vec![], end: 0, list, anim: 0, base_anim: 0, rate_raw: 0x10000 };
        let mut moves: Vec<BuzzMove> = (0..33).map(|_| row(vec![0, 0xFF, 1])).collect();
        moves[state::IDLE as usize] = row((0..16).chain([0xFF, 16]).collect());
        moves[state::FIDGET as usize] = row(vec![0, 1, 2, 0xFE]);
        let w = room();
        let mut p = landed(&w);
        p.anim = Some(BuzzAnimator::new(moves));
        p.set_random_table(vec![4, 0, 0, 0]);
        let mut seen = Vec::new();
        for _ in 0..40 {
            p.tick(&w, [0.0; 2], false, false);
            seen.push(p.state);
        }
        // Entry 14 reads 4 (no), then 0 → fidget 0x1C (second byte 0); it ends back in idle.
        let first = seen.iter().position(|&s| s == state::FIDGET).expect("no fidget");
        assert!(seen[first..].contains(&state::IDLE), "{seen:?}");
    }

    fn quad(a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) -> Vec<[Vec3; 3]> {
        let [a, b, c, d] = [a, b, c, d].map(Vec3::from);
        vec![[a, b, c], [a, c, d]]
    }

    /// Floor at y = 0 for x < 500, a step of height `h` (y = −h) beyond, 2000 wide in z.
    fn step(h: f32) -> World {
        let mut t = quad([-2000., 0., -1000.], [500., 0., -1000.], [500., 0., 1000.], [-2000., 0., 1000.]);
        t.extend(quad([500., 0., -1000.], [500., -h, -1000.], [500., -h, 1000.], [500., 0., 1000.]));
        t.extend(quad([500., -h, -1000.], [3000., -h, -1000.], [3000., -h, 1000.], [500., -h, 1000.]));
        World::from_triangles(t)
    }

    fn run_at_step(h: f32) -> Player {
        let w = step(h);
        let mut p = landed(&w);
        p.set_facing(0x400);
        for _ in 0..70 {
            p.tick(&w, [1.0, 0.0], false, false);
        }
        p
    }

    #[test]
    fn low_steps_are_climbed_and_high_ones_block() {
        let low = run_at_step(40.0);
        assert!(low.pos.x > 600.0 && low.on_floor, "{:?}", low.pos);
        let high = run_at_step(200.0);
        assert!(high.pos.x < 500.0, "{:?}", high.pos);
    }

    #[test]
    fn standing_over_an_edge_teeters() {
        // A platform ending at x = 0 with a 400-unit drop.
        let mut t = quad([-2000., 0., -1000.], [0., 0., -1000.], [0., 0., 1000.], [-2000., 0., 1000.]);
        t.extend(quad([0., 400., -1000.], [2000., 400., -1000.], [2000., 400., 1000.], [0., 400., 1000.]));
        let w = World::from_triangles(t);
        // Centre 60 units past the edge: the sphere still rests on the corner.
        let mut p = Player::new(Vec3::new(60.0, -20.0, 0.0));
        p.set_facing(0x400); // facing +x, towards the drop
        for _ in 0..20 {
            p.tick(&w, [0.0; 2], false, false);
        }
        assert!(p.on_floor, "{:?}", p.pos);
        assert_eq!(p.state, state::TEETER_FRONT);
    }

    #[test]
    fn double_jump_speed_matches_original_clamps() {
        assert_eq!(double_jump_speed_high(0), 0x74C);
        assert_eq!(double_jump_speed_high(0x6A00), 0x22A);
        assert_eq!(double_jump_speed_high(0x7000), 0x74C);
        assert_eq!(double_jump_speed_high(576 * 32), 1063);
    }
}
