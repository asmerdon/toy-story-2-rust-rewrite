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
const RADIUS: f32 = 110.0;
/// Height of the wall-collision sphere centre above the feet.
const CHEST: f32 = 240.0;
const STEP_UP: f32 = 130.0;
const SNAP_DOWN: f32 = 80.0;
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
}

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
            self.select_state(pushing);
            let speed = (self.forward as f64).hypot(self.side as f64) as i32;
            let Player { anim, state, .. } = self;
            if !anim.as_mut().is_some_and(|a| a.tick(state, speed)) {
                break;
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
            // Impact: dust, sound 0xF and a camera shake in the original (not drawn yet).
            self.stomp = STOMP_RECOVERY;
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

    fn move_and_collide(&mut self, world: &World) {
        let mut p = self.pos + Vec3::new(obj(self.vel[0]), obj(self.vel[1]), obj(self.vel[2]));
        let chest = world.push_out(p - Vec3::Y * CHEST, RADIUS);
        p.x = chest.x;
        p.z = chest.z;
        // Land on the nearest floor no more than STEP_UP above the feet.
        let was = self.on_floor;
        self.on_floor = false;
        if self.vel[1] >= 0
            && let Some(f) = world.floor_below(p.x, p.z, p.y - STEP_UP)
            && (p.y >= f || (was && f - p.y <= SNAP_DOWN))
        {
            p.y = f;
            self.vel[1] = 0;
            self.on_floor = true;
        }
        self.pos = p;
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
                    // Landing after a long fall: sound 0x2F, stop dead, recover.
                    self.fall_timer = FALL_RECOVERY;
                    self.vel = [0; 3];
                }
                if self.fall_timer > 0 {
                    self.fall_timer = 0;
                }
            }
        } else {
            self.fall_timer += 1;
            if self.fall_timer > FALL_PANIC_AFTER {
                // Sound 0x16; cancels a stomp.
                self.fall_timer = FALL_PANIC;
                self.stomp = 0;
            }
        }
    }

    /// The part of 0x4011D0 that picks the animation state for the moves ported so far.
    /// Not yet: teeter (10/11), idle fidgets (0x1C/0x1D) and states set by other systems.
    fn select_state(&mut self, pushing: bool) {
        let s = &mut self.state;
        if self.coyote == 0 {
            self.idle_sub = 0;
            if *s == state::JUMP {
                if self.vel[1] > 0 {
                    *s = state::FALL;
                }
            } else if *s == 5 {
                *s = state::LAND;
            }
            if self.fall_timer == FALL_PANIC {
                *s = state::FALL_PANIC;
            }
        } else {
            if *s == state::FALL {
                *s = state::LAND;
            }
            if self.forward < 0x200 && self.side.abs() < 0x200 && !pushing {
                if *s == state::LAND {
                    self.idle_sub = 0;
                } else {
                    if self.idle_sub == 0 {
                        self.idle_sub = state::IDLE;
                    }
                    *s = self.idle_sub;
                }
            } else {
                self.idle_sub = 0;
                *s = state::RUN;
            }
            if self.skid != 0 {
                *s = state::SKID;
            }
        }
        if self.stomp > 0 {
            *s = state::STOMP;
        }
        if self.stomp < -STOMP_HANG {
            *s = state::STOMP_LAND;
        }
        if self.fall_timer < 0 {
            *s = state::FALL_RECOVER;
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
        assert!(p.pos.y.abs() < 1e-3, "{:?}", p.pos);
    }

    #[test]
    fn wall_stops_running() {
        let w = room();
        let mut p = Player::new(Vec3::ZERO);
        for _ in 0..240 {
            p.update(&w, Input { move_xz: [1.0, 0.0], ..Default::default() }, 1.0 / 60.0);
        }
        assert!(p.pos.x < 1000.0 - RADIUS + 5.0 && p.pos.x > 800.0, "{:?}", p.pos);
    }

    #[test]
    fn turns_gradually_and_runs_at_top_speed() {
        let w = room();
        let mut p = landed(&w);
        // Stick 90° to the right: at most 0x300 / 8 per tick.
        p.tick(&w, [1.0, 0.0], false, false);
        assert_eq!(p.facing, 0x60);
        for _ in 0..120 {
            p.tick(&w, [1.0, 0.0], false, false);
        }
        // Steps are diff / 8, so the last 7 units are never turned (as in the original).
        assert!((0x400 - p.facing) < 8, "{:#x}", p.facing);
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
        let mut peak: f32 = 0.0;
        for i in 0..180 {
            p.tick(&w, [0.0; 2], i <= hold, false);
            peak = peak.min(p.pos.y);
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
            peak = peak.min(p.pos.y);
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
    fn double_jump_speed_matches_original_clamps() {
        assert_eq!(double_jump_speed_high(0), 0x74C);
        assert_eq!(double_jump_speed_high(0x6A00), 0x22A);
        assert_eq!(double_jump_speed_high(0x7000), 0x74C);
        assert_eq!(double_jump_speed_high(576 * 32), 1063);
    }
}
