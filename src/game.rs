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
use crate::formats::leveldat::Pole;
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
/// The super spin's length, and the charge value where its spinning part ends (0x434EB0).
const SUPER_SPIN: i32 = 300;
const SUPER_SPIN_END: i32 = -0x77;
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
    /// Terrain object it belongs to (`NO_OBJECT` for render-geometry worlds).
    obj: u16,
}

const NO_OBJECT: u16 = u16::MAX;

/// What the world keeps of a terrain object.
#[derive(Debug, Clone)]
struct ObjectInfo {
    flags: u16,
    /// Mover link id (record +0x1A); scripts address movers by link − 1.
    link: i16,
    /// Collides (the gatherer's type check: 0x4878A0 sets type 8, 0x4878D0 type 0). Cells so
    /// scripts can switch and move movers during a tick, while Buzz's collision reads them.
    enabled: std::cell::Cell<bool>,
    /// Moved since load (0x488510), level units.
    offset: std::cell::Cell<Vec3>,
}

/// What a sweep hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// Fraction of the motion travelled before contact.
    pub t: f32,
    /// Contact normal, pointing from the surface towards the sphere.
    pub normal: Vec3,
    /// How far the sphere already overlapped the surface at the start (0 for a clean hit).
    pub depth: f32,
    /// Mover id (link − 1) of what was hit, if it is a mover.
    pub mover: Option<i32>,
    /// Surface type of what was hit: the low byte of its terrain object's flags, unless 0xFF
    /// (0x482A00 / 0x483EF0 copy it for 0x434550).
    pub surface: Option<u8>,
}

/// Overlap with a face that still counts as touching it from the front.
const PENETRATION: f32 = 8.0;

pub struct World {
    tris: Vec<Tri>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    pub lo: Vec3,
    pub hi: Vec3,
    objects: Vec<ObjectInfo>,
    /// Triangles of moving objects (type 8): kept out of the grid, as 0x489C30 keeps them out of
    /// its 5×5 grid.
    always: Vec<Tri>,
    /// The room map (objects flagged 0x400): floors at a quarter of the level's scale, each
    /// object's flags low byte naming its room. Not collision.
    rooms: Vec<Tri>,
    /// Each object's stored position (where its triangles are), for placing movers.
    mover_home: Vec<Vec3>,
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
        let (mut tris, mut always, mut rooms, mut objects) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (i, o) in t.objects.iter().enumerate() {
            // Objects flagged 0x100 start disabled (the gatherer 0x485940 skips them).
            objects.push(ObjectInfo {
                flags: o.flags,
                link: o.linked,
                enabled: std::cell::Cell::new(matches!(o.kind, 6..=8) && o.flags & 0x100 == 0),
                offset: std::cell::Cell::new(Vec3::ZERO),
            });
            let to = if o.flags & 0x400 != 0 {
                &mut rooms
            } else if o.kind == 8 {
                &mut always
            } else {
                &mut tris
            };
            for (p, n) in t.object_triangles(i) {
                let [a, b, c] = p.map(Vec3::from);
                to.push(Tri { a, b, c, n: Vec3::from(n).normalize_or_zero(), two_sided: false, obj: i as u16 });
            }
        }
        let mut w = World::build(tris);
        w.objects = objects;
        w.always = always;
        w.rooms = rooms;
        w.mover_home = t.objects.iter().map(|o| Vec3::from(o.pos.map(|v| v as f32))).collect();
        w
    }

    /// Terrain object of the mover scripts call `id` (link id − 1, as 0x4878A0 indexes the
    /// link table 0x7286EC).
    fn mover(&self, id: i32) -> Option<usize> {
        self.objects.iter().position(|o| o.link as i32 == id + 1 && o.link != 0)
    }

    /// 0x4878A0 (`on`) / 0x4878D0: switch a mover's collision.
    pub fn set_mover(&self, id: i32, on: bool) {
        if let Some(i) = self.mover(id) {
            self.objects[i].enabled.set(on);
        }
    }

    pub fn mover_enabled(&self, id: i32) -> bool {
        self.mover(id).is_some_and(|i| self.objects[i].enabled.get())
    }

    /// 0x488510: put a mover at `pos` (level units); its collision moves with it, from where it
    /// was loaded (the object's stored position).
    pub fn place_mover(&self, id: i32, pos: Vec3) {
        if let Some(i) = self.mover(id) {
            self.objects[i].offset.set(pos - self.mover_home[i]);
        }
    }

    /// The room under `p` (0x4885C0, which works at a quarter scale): the low byte of the
    /// flags of the nearest room-map floor at or below `p` / 4, as the level script reads it
    /// (`0x54DEA0`).
    pub fn room_at(&self, p: Vec3) -> Option<u8> {
        let q = p / 4.0;
        let mut best: Option<(f32, u8)> = None;
        for t in &self.rooms {
            if t.n.y > FLOOR_NY {
                continue;
            }
            if let Some(y) = height_on(t, q.x, q.z)
                && y >= q.y
                && best.is_none_or(|(b, _)| y < b)
            {
                best = Some((y, self.objects[t.obj as usize].flags as u8));
            }
        }
        best.map(|(_, r)| r)
    }

    /// Mover id (link − 1) of a triangle's object, if it is a mover.
    fn mover_of(&self, t: &Tri) -> Option<i32> {
        self.objects.get(t.obj as usize).map(|o| o.link as i32).filter(|&l| l != 0).map(|l| l - 1)
    }

    /// Enabled always-checked triangles at their current positions.
    fn always_tris(&self) -> impl Iterator<Item = Tri> + '_ {
        self.always.iter().filter_map(|t| {
            let o = self.objects.get(t.obj as usize)?;
            let off = o.offset.get();
            o.enabled.get().then(|| Tri { a: t.a + off, b: t.b + off, c: t.c + off, ..*t })
        })
    }

    /// Candidate triangles near the box `lo..hi`: grid cells plus enabled always-checked ones.
    fn candidates(&self, lo: Vec3, hi: Vec3) -> Vec<Tri> {
        let mut out: Vec<Tri> = self
            .near_box(lo, hi)
            .into_iter()
            .map(|i| self.tris[i as usize])
            .filter(|t| t.obj == NO_OBJECT || self.objects.get(t.obj as usize).is_none_or(|o| o.enabled.get()))
            .collect();
        out.extend(self.always_tris().filter(|t| {
            let (tl, th) = (t.a.min(t.b).min(t.c), t.a.max(t.b).max(t.c));
            !(th.cmplt(lo).any() || tl.cmpgt(hi).any())
        }));
        out
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
                Tri { a, b, c, n, two_sided: true, obj: NO_OBJECT }
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
        World { tris, grid, lo, hi, objects: Vec::new(), always: Vec::new(), rooms: Vec::new(), mover_home: Vec::new() }
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
        self.floor_below_obj(x, z, from_y).map(|(y, _)| y)
    }

    /// As `floor_below`, with the mover id (link − 1) of the floor, if it belongs to one.
    pub fn floor_below_obj(&self, x: f32, z: f32, from_y: f32) -> Option<(f32, Option<i32>)> {
        let p = Vec3::new(x, from_y, z);
        let lo = Vec3::new(x, f32::MIN, z);
        let hi = Vec3::new(x, f32::MAX, z);
        let mut best: Option<(f32, u16)> = None;
        for t in self.candidates(lo, hi) {
            if t.n.y > FLOOR_NY {
                continue;
            }
            if let Some(y) = height_on(&t, p.x, p.z)
                && y >= from_y
                && best.is_none_or(|(b, _)| y < b)
            {
                best = Some((y, t.obj));
            }
        }
        best.map(|(y, o)| {
            let link = self.objects.get(o as usize).map(|o| o.link as i32).filter(|&l| l != 0).map(|l| l - 1);
            (y, link)
        })
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
        for t in self.candidates(p0.min(p1) - reach, p0.max(p1) + reach) {
            let mover = self.mover_of(&t);
            let surface = self.objects.get(t.obj as usize).map(|o| o.flags as u8).filter(|&f| f != 0xFF);
            let lo = t.a.min(t.b).min(t.c) - reach;
            let hi = t.a.max(t.b).max(t.c) + reach;
            if p0.max(p1).cmplt(lo).any() || p0.min(p1).cmpgt(hi).any() {
                continue;
            }
            for n in if t.two_sided { [t.n, -t.n].to_vec() } else { vec![t.n] } {
                let d0 = n.dot(p0 - t.a) - r;
                let d1 = n.dot(p1 - t.a) - r;
                // Faces only stop motion into their front side. A start slightly inside the
                // face (up to PENETRATION) still counts, at t = 0, so rounding or a wall push
                // can't let Buzz sink through a floor.
                if d0 >= -PENETRATION && d1 < 0.0 && d1 <= d0 {
                    let s = if d0 > 0.0 { d0 / (d0 - d1) } else { 0.0 };
                    let q = p0 + d * s - n * (r + d0.min(0.0));
                    if inside(q, t.a, t.b, t.c, n) {
                        keep(Some(Hit { t: s, normal: n, depth: (-d0).max(0.0), mover, surface }));
                    }
                }
                // Edges and corners only near the plane, as the original limits them.
                let near = (d0 < 8.0 || d1 < 8.0) && (d0 >= -8.0 - r / 2.0 || d1 >= -8.0 - r / 2.0);
                if near && d.dot(n) < 0.0 {
                    for (e0, e1) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
                        keep(sweep_edge(p0, d, e0, e1, r + 1.0).map(|h| Hit { mover, surface, ..h }));
                    }
                    for v in [t.a, t.b, t.c] {
                        keep(sweep_point(p0, d, v, r + 2.0).map(|h| Hit { mover, surface, ..h }));
                    }
                }
            }
        }
        best
    }
}

/// Height of triangle `t` at (x, z), if (x, z) is inside it seen from above.
fn height_on(t: &Tri, x: f32, z: f32) -> Option<f32> {
    let (a, b, c) = (t.a, t.b, t.c);
    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if d.abs() < 1e-6 {
        return None;
    }
    let l1 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
    let l2 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
    let l3 = 1.0 - l1 - l2;
    if l1 < -1e-4 || l2 < -1e-4 || l3 < -1e-4 {
        return None;
    }
    Some(l1 * a.y + l2 * b.y + l3 * c.y)
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
    Some(Hit { t, normal, depth: 0.0, mover: None, surface: None })
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
    Some(Hit { t, normal, depth: 0.0, mover: None, surface: None })
}

/// Code run inside a tick just before Buzz moves (see `Player::update_with`).
pub type PreMove<'a> = dyn FnMut(&mut Player, &World) + 'a;

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
    /// Stomp pressed since the last update (Circle, pad bit 0x2000): a stomp in the air, a
    /// spin attack on the ground.
    pub stomp: bool,
    /// Circle held (charges the spin).
    pub stomp_held: bool,
    /// Square pressed since the last update (pad bit 0x8000): the wrist laser; and held
    /// (charges it).
    pub fire: bool,
    pub fire_held: bool,
    /// The d-pad itself (pad bits 0x10 up, 0x20 right, 0x40 down, 0x80 left); climbing reads it
    /// directly rather than as a camera-relative direction.
    pub dpad: u8,
}

/// D-pad bits as `Input::dpad` holds them (the PS1 pad layout).
pub mod pad {
    pub const UP: u8 = 0x10;
    pub const RIGHT: u8 = 0x20;
    pub const DOWN: u8 = 0x40;
    pub const LEFT: u8 = 0x80;
}

/// A laser shot Buzz fired this tick (0x434990 calls 0x4A5D30): kind 1 normal, 2 charged;
/// along `yaw` (12-bit) and `pitch` from the arm, 0x2C00 object units above his feet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    pub kind: u8,
    pub yaw: i32,
    pub pitch: i32,
    pub origin: [i32; 3],
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
    pub target: i32,
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
    /// Sounds at other positions (level units), e.g. a pushed box.
    pub sounds_at: Vec<(Sound, Vec3)>,
    /// The mover Buzz's last wall contact belonged to, with the contact normal (link flags
    /// bit 1 and the contact data 0x4885A0 reads), and the mover he stands on (bit 2).
    pub wall_mover: Option<(i32, Vec3)>,
    pub floor_mover: Option<(i32, Vec3)>,
    /// The stick is pushed this tick (pad & 0xF0).
    pub stick_pushed: bool,
    /// 0x53C5DC.
    pub push_lock: i32,
    /// Battery health 0..=14 (0x52F396; below 0 = dying) and lives (0x52F39A).
    pub health: i32,
    pub lives: i32,
    /// 0x52F394: > 0 invulnerable after a hit, < 0 a shield flash.
    pub hurt_timer: i32,
    /// 0x52F398: > 0 the shield pickup's protection.
    pub shield: i32,
    /// 0x52F2DC while dying (0x52B7DC = 2): ticks until the respawn.
    pub dying: i32,
    /// 0x53C650: spin attack ticks left (48 from the start; hits while > 20).
    pub spin: i32,
    /// 0x53C83C: > 0 charging the spin (to 60), < 0 the super spin (from −300; hits below −119).
    pub charge: i32,
    /// Surface type of the floor under Buzz this tick.
    pub surface: Option<u8>,
    circle_held: bool,
    /// 0x53C620: the laser's arm timer: 1–11 raising the arm, 12 the shot, then on to 0x3F
    /// (held, it loops back by 0x28 to keep aiming); 0 = off. 0x53C840: charge (to 0x40).
    pub laser: i32,
    pub laser_charge: i32,
    fire_latch: bool,
    fire_held: bool,
    /// Shots fired since the caller last drained this.
    pub shots: Vec<Shot>,
    /// Ticks until Buzz may say an idle line again (0x50A0BC) and the last line (0x50A0B0).
    voice_timer: i32,
    last_voice: u16,
    /// Poles and ropes of the level (`level.dat` list 61).
    poles: Vec<Pole>,
    /// Climb state 0x53C668: 0 free, < 0 just let go, > 0 on pole `pole` (8 = just grabbed;
    /// bit 2 climbing up, bit 4 sliding down).
    pub climb: i32,
    pole: usize,
    /// Ticks before a jump can take Buzz off a pole he just grabbed (0x53C658).
    grab_lock: i32,
}

/// Idle voice lines alternate between these two sound ids, at most every 0x708 ticks (30 s).
const IDLE_VOICE: [u16; 2] = [0xCE, 0xCF];
const IDLE_VOICE_TICKS: i32 = 0x708;

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
            sounds_at: Vec::new(),
            wall_mover: None,
            floor_mover: None,
            stick_pushed: false,
            push_lock: 0,
            health: 14,
            lives: 5,
            hurt_timer: 0,
            shield: 0,
            dying: 0,
            spin: 0,
            charge: 0,
            surface: None,
            circle_held: false,
            laser: 0,
            laser_charge: 0,
            fire_latch: false,
            fire_held: false,
            shots: Vec::new(),
            voice_timer: 0,
            last_voice: 0,
            poles: Vec::new(),
            climb: 0,
            pole: 0,
            grab_lock: 0,
        }
    }

    /// Gives the player the level's poles and ropes (`leveldat::poles`).
    pub fn set_poles(&mut self, poles: Vec<Pole>) {
        self.poles = poles;
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
            // Starting a fidget may come with a voice line, at most every 30 s.
            if self.voice_timer == 0 {
                self.voice_timer = IDLE_VOICE_TICKS;
                self.last_voice = if self.last_voice == IDLE_VOICE[1] { IDLE_VOICE[0] } else { IDLE_VOICE[1] };
                self.sounds.push(Sound { id: self.last_voice, voice: true });
            }
        }
    }

    /// Sets the facing (12-bit units) and makes it the respawn facing.
    pub fn set_facing(&mut self, angle: i32) {
        self.facing = angle & 0xFFF;
        self.target = self.facing;
        self.spawn_facing = self.facing;
    }

    /// 0x434090 `StartJump(vy, phase)`: launched upwards by something (a spring, a bounce),
    /// leaving the ground with the given jump phase (+0x8C; 2 = released, so a press can still
    /// double jump).
    pub fn start_jump(&mut self, vy: i32, phase: JumpPhase) {
        self.jump = phase;
        self.on_floor = false;
        self.coyote = 0;
        self.state = state::JUMP;
        self.vel[1] = vy;
        self.skid = 0;
        self.jump_start = None;
    }

    /// Feet position in object units (level units × 32), as the game keeps it.
    pub fn pos_obj(&self) -> [i32; 3] {
        self.pos.to_array().map(|v| (v * 32.0).round() as i32)
    }

    /// Doing something that stops pushing boxes, spinning and the like (0x53C828 bits other
    /// than "on the ground"): the super spin, climbing, stomping, a long-fall panic or its
    /// recovery.
    pub fn busy(&self) -> bool {
        self.charge < 0 || self.climb > 0 || self.stomp != 0 || self.fall_timer < 0 || self.fall_timer == FALL_PANIC
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

    /// Advances by `dt` seconds of game time in whole 60 Hz ticks; returns how many ran.
    pub fn update(&mut self, world: &World, input: Input, dt: f32) -> u32 {
        self.update_with(world, input, dt, &mut |_, _| {})
    }

    /// As `update`, running `pre_move` each tick just before Buzz moves, where 0x436220 runs the
    /// box pusher (0x433700), which steers his velocity and moves movers.
    pub fn update_with(&mut self, world: &World, input: Input, dt: f32, pre_move: &mut PreMove<'_>) -> u32 {
        let mut ticks = 0;
        self.jump_latch |= input.jump;
        self.stomp_latch |= input.stomp;
        self.circle_held = input.stomp_held;
        self.fire_latch |= input.fire;
        self.fire_held = input.fire_held;
        self.accum = (self.accum + dt * TICKS).min(MAX_TICKS_PER_UPDATE);
        while self.accum >= 1.0 {
            self.accum -= 1.0;
            let held = input.jump_held || self.jump_latch;
            let pressed = self.jump_latch;
            let stomp = self.stomp_latch;
            self.jump_latch = false;
            self.stomp_latch = false;
            self.tick_pad_with(world, input.move_xz, input.dpad, held, pressed, stomp, pre_move);
            ticks += 1;
        }
        ticks
    }

    /// One game tick (no d-pad, jump press = held).
    pub fn tick(&mut self, world: &World, stick: [f32; 2], jump_held: bool, stomp_pressed: bool) {
        self.tick_pad(world, stick, 0, jump_held, jump_held, stomp_pressed);
    }

    /// One game tick with the raw d-pad and a jump press edge, as climbing needs them.
    pub fn tick_pad(&mut self, world: &World, stick: [f32; 2], dpad: u8, jump_held: bool, jump_pressed: bool, stomp_pressed: bool) {
        self.tick_pad_with(world, stick, dpad, jump_held, jump_pressed, stomp_pressed, &mut |_, _| {});
    }

    #[allow(clippy::too_many_arguments)]
    fn tick_pad_with(&mut self, world: &World, stick: [f32; 2], dpad: u8, jump_held: bool, jump_pressed: bool, stomp_pressed: bool, pre_move: &mut PreMove<'_>) {
        self.stick_pushed = stick != [0.0, 0.0];
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

        self.grab_lock = (self.grab_lock - 1).max(0);
        let fire = std::mem::take(&mut self.fire_latch);
        self.laser_tick(fire, self.fire_held || fire);
        let stomp = self.stomp_tick(stomp_pressed);
        self.spin_tick(stomp_pressed);
        if self.hurt_timer > 0 {
            self.hurt_timer -= 1;
        } else if self.hurt_timer < 0 {
            self.hurt_timer += 1;
        }
        self.shield = (self.shield - 1).max(0);
        if self.dying > 0 {
            self.dying -= 1;
        }
        let climb = self.climb_tick(dpad, stick, jump_pressed);
        match if climb != 0 { 0 } else { stomp } {
            // On a pole: no gravity or ground movement this tick.
            _ if climb == 2 => {}
            // Just let go: steer and accelerate, no airborne step this tick.
            _ if climb == 1 => {
                let max = MAX_SPEED;
                self.turn(None);
                self.accelerate(side_decel, friction, max, false);
            }
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

        // 0x53C5DC: after a box slides away, Buzz stands still for a few ticks.
        if self.push_lock > 0 {
            self.push_lock -= 1;
            self.vel[0] = 0;
            self.vel[2] = 0;
        }
        pre_move(self, world);
        self.move_and_collide(world);

        if self.on_floor {
            self.coyote = COYOTE_TICKS;
        } else if self.coyote > 0 {
            self.coyote -= 1;
        }
        self.fall_bookkeeping();

        self.voice_timer = (self.voice_timer - 1).max(0);
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
        // States that animate the whole body as one (both layers the same animation) put the
        // spin and the laser's arm down (0x4011D0).
        if self.anim.as_ref().and_then(|a| a.moves.get(self.state as usize)).is_some_and(|m| m.anim == m.base_anim) {
            self.spin = 0;
            self.laser = 0;
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

    /// 0x4354E0: grab, climb, slide down and leave poles. Returns 0 when not on a pole, 1 on
    /// the tick Buzz lets go, 2 while climbing.
    fn climb_tick(&mut self, dpad: u8, stick: [f32; 2], jump_pressed: bool) -> i32 {
        if self.poles.is_empty() || self.stomp != 0 || self.fall_timer < 0 || self.fall_timer == FALL_PANIC {
            return 0;
        }
        let was_sliding = self.climb > 0 && self.climb & 4 != 0;
        let (x, y, z) = ((self.pos.x * 32.0) as i32, self.y_obj(), (self.pos.z * 32.0) as i32);
        let mut at = 0; // 2 = at the top
        if self.climb == 0 {
            let near = self.poles.iter().position(|p| {
                let (dx, dz) = ((x - p.pos[0]) >> 8, (z - p.pos[2]) >> 8);
                let dy = p.pos[1] - y;
                p.kind != 3 && dx * dx + dz * dz <= 0x1FF && dy >= -0x1E00 && dy <= p.length - 0x3600
            });
            let Some(i) = near else { return 0 };
            self.pole = i;
            self.climb = 8;
            self.grab_lock = 10;
            self.vel[1] = 0;
        } else if self.climb < 0 {
            // Let go: free again once he's moved away from the pole.
            let p = self.poles[self.pole];
            let (dx, dz) = ((x - p.pos[0]) >> 5, (z - p.pos[2]) >> 5);
            if dx * dx + dz * dz > 0x8400 {
                self.climb = 0;
            }
            return 0;
        } else {
            let p = self.poles[self.pole];
            let dy = p.pos[1] - y;
            if dy < -0x1E00 {
                // Slid off the bottom: fall, no double jump.
                self.climb = -1;
                self.jump = JumpPhase::Double;
                self.state = state::FALL;
                self.jump_start = None;
                return 1;
            }
            if dy >= p.length - 0x3600 {
                self.vel[1] = 0;
                at = 2;
                self.pos.y = (p.pos[1] - p.length + 0x3600) as f32 / 32.0;
            }
        }
        let p = self.poles[self.pole];
        // Hug the pole.
        self.pos.x -= (self.pos.x - p.pos[0] as f32 / 32.0) / 4.0;
        self.pos.z -= (self.pos.z - p.pos[2] as f32 / 32.0) / 4.0;
        self.vel[0] = 0;
        self.vel[2] = 0;
        if p.kind == 2 {
            // Slide pole: down only.
            self.climb = 1;
            if self.coyote != 0 {
                self.climb = -1;
                return 1;
            }
            self.vel[1] = (self.vel[1] + 0x10).min(0x800);
            self.climb |= 4;
        } else if self.climb != 8 || dpad & pad::DOWN == 0 {
            self.climb = 1;
            if dpad & pad::UP == 0 {
                if dpad & pad::DOWN == 0 {
                    if self.vel[1] > 0 {
                        self.vel[1] -= 0x40;
                    }
                    self.vel[1] = self.vel[1].max(0);
                } else {
                    if self.coyote != 0 {
                        self.climb = -1;
                        return 1;
                    }
                    // Slide down, spinning round the pole.
                    self.vel[1] = (self.vel[1] + 0x20).min(0x400);
                    self.facing += self.vel[1] / 16;
                    self.climb |= 4;
                }
            } else {
                self.on_floor = false;
                self.coyote = 0;
                if at != 2 {
                    self.vel[1] = -0x100;
                    self.climb |= 2;
                }
            }
        }
        if dpad & pad::LEFT != 0 {
            self.facing -= 0x20;
        }
        if dpad & pad::RIGHT != 0 {
            self.facing += 0x20;
        }
        if self.climb > 0 && self.climb & 4 != 0 && !was_sliding {
            // 0x4011D0 requests this every sliding tick; once per slide is enough here.
            self.sound(0x23);
        }
        self.facing &= 0xFFF;
        self.target = self.facing;
        let top_exit = at == 2 && p.kind == 1;
        if !(jump_pressed || top_exit) {
            return 2;
        }
        if !top_exit {
            if self.grab_lock != 0 {
                return 2;
            }
            // Jump off, pushed along the stick (or the facing).
            self.vel[1] = -0x400;
            if stick[0] != 0.0 || stick[1] != 0.0 {
                self.facing = ((stick[0].atan2(stick[1]) / std::f32::consts::TAU * 4096.0).round() as i32) & 0xFFF;
                self.target = self.facing;
            }
            self.vel[0] = (sin12(self.facing) * 4) / 2;
            self.vel[2] = (cos12(self.facing) * 4) / 2;
            self.jump = JumpPhase::Holding;
        } else {
            // Over the top: a full jump straight up.
            self.vel = [0, JUMP_VY, 0];
            self.jump = JumpPhase::Released;
        }
        self.jump_start = None;
        self.state = state::JUMP;
        self.sounds.push(Sound { id: 0x19, voice: true });
        self.climb = -1;
        2
    }

    /// 0x434D20. Returns 0 to carry on normally, 5 to stop horizontal motion, 7 to hang.
    fn stomp_tick(&mut self, pressed: bool) -> i32 {
        let blocked = self.coyote != 0 || self.fall_timer < 0 || self.fall_timer == FALL_PANIC || self.stomp != 0 || self.charge != 0 || self.spin != 0;
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
        self.wall_mover = None;
        self.floor_mover = None;
        self.surface = None;
        let start = self.pos;
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
            // Started inside the surface: back out of it first.
            if h.depth > 0.0 {
                c += n * (h.depth + SKIN);
            }
            let rest = d * (1.0 - s);
            d = rest - n * rest.dot(n).min(0.0);
            v -= n * v.dot(n).min(0.0);
            if n.y < FLOOR_NY {
                floor = true;
                self.surface = h.surface.or(self.surface);
                if let Some(m) = h.mover {
                    self.floor_mover = Some((m, n));
                }
            } else {
                if let Some(m) = h.mover {
                    self.wall_mover = Some((m, n));
                }
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
                c += down * (h.t - SKIN / STICK_DOWN).max(0.0) + h.normal * h.depth;
                floor = true;
                self.floor_mover = h.mover.map(|m| (m, h.normal));
                self.surface = h.surface;
            }
        }
        if floor && v.y > 0.0 {
            v.y = 0.0;
        }
        self.pos = c + lift;
        self.on_floor = floor;
        self.vel = [v.x.round() as i32, v.y.round() as i32, v.z.round() as i32];
        // Surfaces (0x434550): types 0-3 hurt, 16-19 kill (Buzz is put back where he was).
        if let Some(s) = self.surface
            && self.dying == 0
        {
            match s >> 2 {
                0 if self.hurt_timer == 0 => self.hurt(0, 2),
                4 => {
                    self.hurt(0, 4);
                    self.pos = start;
                    self.on_floor = false;
                    self.coyote = 0;
                }
                _ => {}
            }
        }
    }

    /// 0x434EB0: Circle on the ground starts a spin attack (48 ticks, sound 0x11). Kept held
    /// with nothing else going on, it charges: from tick 13 the charge whine (sound 0x27) plays
    /// every tick, up to 60. Released fully charged, the super spin: 181 ticks spinning (sound
    /// 0x26 every tick, state 0x13), then 119 dizzy (sound 0x18 once, state 0x14).
    fn spin_tick(&mut self, pressed: bool) {
        if self.spin > 0 {
            self.spin -= 1;
        }
        if pressed && self.coyote != 0 && !self.busy() && self.charge == 0 && self.spin == 0 && self.skid == 0 {
            self.spin = 0x30;
            self.laser = 0;
            self.charge = 1;
            self.sound(0x11);
        }
        if self.circle_held && !self.busy() && self.charge > 0 {
            self.charge = (self.charge + 1).min(0x3C);
            if self.charge >= 0xD {
                self.sound(0x27);
            }
            return;
        }
        if self.charge < 0 {
            let was_spinning = self.charge < SUPER_SPIN_END;
            self.charge += 1;
            if self.charge < SUPER_SPIN_END {
                self.sound(0x26);
            } else if was_spinning {
                self.sound(0x18);
            }
            return;
        }
        // Released: a full charge becomes the super spin.
        self.charge = if self.charge > 0x3B && !self.busy() { -SUPER_SPIN } else { 0 };
    }

    /// The state bits 0x436220 gathers at 0x53C828 each tick, as far as they are ported: 1 on
    /// the ground, 2 super spin, 0x10 climbing, 0x20 long fall or its landing, 0x40 stomping,
    /// 0x80 the laser.
    pub fn state_bits(&self) -> u32 {
        let mut f = (self.coyote != 0) as u32;
        f |= if self.charge < 0 { 2 } else { 0 };
        f |= if self.climb > 0 { 0x10 } else { 0 };
        f |= if self.fall_timer < 0 || self.fall_timer == FALL_PANIC { 0x20 } else { 0 };
        f |= if self.stomp != 0 { 0x40 } else { 0 };
        f |= if self.laser != 0 { 0x80 } else { 0 };
        f
    }

    /// 0x434990: Square raises the arm (11 ticks) and fires (sound 2); pressed again within
    /// the next 41 ticks it fires again. Held, the shot charges (whine 0x27 from tick 13);
    /// released after 64 ticks, a charged shot (sound 6). Climbing, a long fall and the like
    /// put the arm down. Shots go to `shots`, along Buzz's facing.
    fn laser_tick(&mut self, pressed: bool, held: bool) {
        let bits = self.state_bits();
        if bits & 0xF_BE3C != 0 {
            self.laser = 0;
            self.laser_charge = 0;
        } else if self.laser == 0 {
            self.laser_charge = 0;
        }
        let mut fire = 0u8;
        if pressed && bits & 0xF_BEFE == 0 {
            self.laser_charge = 0;
            self.charge = 0;
            self.spin = 0;
            self.laser = 1;
            return;
        }
        if self.laser == 0 {
            return;
        }
        if self.laser < 0xC {
            if self.laser + 1 < 0xC {
                self.laser += 1;
                return;
            }
            fire = 1;
        } else {
            self.laser += 1;
            if !held {
                if self.laser_charge > 0x24 {
                    self.laser = 0x34;
                }
                if self.laser_charge == 0x40 {
                    fire = 2;
                }
                self.laser_charge = 0;
            } else {
                self.laser_charge = (self.laser_charge + 1).min(0x40);
                if pressed && bits & 0xF_BE7E == 0 {
                    if self.laser < 0x35 {
                        fire = 1;
                    } else {
                        self.laser = 0x40 - self.laser;
                    }
                } else if self.laser > 0x33 {
                    self.laser -= 0x28;
                }
                if self.laser_charge > 0xC {
                    self.sound(0x27);
                }
            }
            if self.laser > 0x3F {
                self.laser = 0;
            }
            if fire == 0 {
                return;
            }
        }
        self.laser = 0xC;
        self.sound(if fire == 1 { 2 } else { 6 });
        let p = self.pos_obj();
        self.shots.push(Shot { kind: fire, yaw: self.facing, pitch: 0, origin: [p[0], p[1] - 0x2C00, p[2]] });
    }

    /// The laser's arm pose (0x4011D0): animation 0x1A on the upper body, at frame
    /// (table 0x4DF294[t / 2] · 2 + t % 2) / 2 for timer t; `table` from the exe.
    pub fn laser_pose(&self, table: &[u8]) -> Option<f32> {
        (self.laser > 0).then(|| {
            let t = self.laser as usize;
            ((t & 1) + table.get(t / 2).copied().unwrap_or(0) as usize * 2) as f32 / 2.0
        })
    }

    /// In the spinning part of the super spin (0x53C83C below −0x77).
    pub fn super_spinning(&self) -> bool {
        self.charge < SUPER_SPIN_END
    }

    /// Attacking creatures this tick (0x407440): kind 1 spinning, 5 stomping, 0 not; and the
    /// reach (400 while spinning or just after a stomp lands, else 150 level units).
    pub fn attack(&self) -> (u8, i32) {
        let spinning = self.spin > 0x14 || self.super_spinning();
        let mut reach = if spinning { 400 } else { 150 };
        let mut kind = if spinning { 1 } else { 0 };
        if self.stomp > 0 {
            kind = 5;
        }
        if self.stomp < -0x1E {
            kind = 5;
            reach = 400;
        }
        (kind, reach)
    }

    /// 0x4071E0 `HurtBuzz(angle, mode)`: mode 1 knocks him along `angle`, 2 costs a battery
    /// (then 90 ticks of invulnerability; with none left he dies), 4 kills outright.
    pub fn hurt(&mut self, angle: i32, mode: u32) {
        if self.health < 0 || self.dying > 0 {
            return;
        }
        let mut mode = mode;
        if mode & 1 != 0 {
            self.vel[0] = sin12(angle) >> 2;
            self.vel[2] = cos12(angle) >> 2;
        }
        if mode & 2 != 0 && self.hurt_timer >= 1 {
            mode &= !2;
        } else if mode & 2 != 0 && self.shield != 0 {
            self.hurt_timer = -0x3C;
            mode &= !2;
        }
        if mode & 4 != 0 {
            self.die();
            return;
        }
        if mode & 2 != 0 {
            self.health -= 1;
            self.jump_start = None;
            self.jump = JumpPhase::DoubleReleased;
            self.vel[1] = -0x200;
            self.on_floor = false;
            self.coyote = 0;
            self.stomp = 0;
            self.spin = 0;
            self.charge = 0;
            self.skid = 0;
            if self.health < 0 {
                self.die();
            } else {
                self.hurt_timer = 0x5A;
                self.sounds.push(Sound { id: 0x1A, voice: true });
            }
        }
    }

    fn die(&mut self) {
        self.sounds.push(Sound { id: 0x15, voice: true });
        self.dying = 0x78;
        self.health = -1;
    }

    /// After the death countdown: one life fewer, full health, back at the respawn point.
    pub fn respawn(&mut self, at: Vec3, facing: i32) {
        self.lives -= 1;
        self.health = 14;
        self.dying = 0;
        self.hurt_timer = 0x5A;
        self.pos = at;
        self.vel = [0; 3];
        self.set_facing(facing);
        self.stomp = 0;
        self.spin = 0;
        self.charge = 0;
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
        if self.charge < 0 {
            self.state = if self.super_spinning() { state::SUPER_SPIN } else { state::DIZZY };
        }
        if self.climb > 0 {
            self.state = if self.climb & 4 != 0 {
                state::CLIMB_SLIDE
            } else if self.climb & 2 != 0 {
                state::CLIMB_UP
            } else {
                state::CLIMB_HANG
            };
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

    /// Holding Circle charges the spin (whine from tick 13); released full, the super spin
    /// runs 181 ticks, then 119 dizzy.
    #[test]
    fn held_spin_charges_into_a_super_spin() {
        let w = room();
        let mut p = landed(&w);
        let input = |stomp, held| Input { stomp, stomp_held: held, ..Default::default() };
        p.update(&w, input(true, true), 1.0 / 60.0);
        assert_eq!((p.spin, p.charge), (0x30, 2));
        p.sounds.clear();
        for _ in 0..70 {
            p.update(&w, input(false, true), 1.0 / 60.0);
        }
        assert_eq!(p.charge, 0x3C);
        assert!(p.sounds.iter().filter(|s| s.id == 0x27).count() > 50, "no charge whine");
        p.update(&w, input(false, false), 1.0 / 60.0);
        assert_eq!(p.charge, -SUPER_SPIN);
        assert!(p.super_spinning() && p.attack().0 == 1);
        assert_eq!(p.state, state::SUPER_SPIN);
        for _ in 0..181 {
            p.update(&w, input(false, false), 1.0 / 60.0);
        }
        assert!(!p.super_spinning() && p.charge < 0);
        assert_eq!(p.state, state::DIZZY);
        for _ in 0..119 {
            p.update(&w, input(false, false), 1.0 / 60.0);
        }
        assert_eq!(p.charge, 0);
        // A quick tap is one spin, no charge.
        let mut q = landed(&w);
        q.update(&w, input(true, true), 1.0 / 60.0);
        q.update(&w, input(false, false), 1.0 / 60.0);
        assert_eq!(q.charge, 0);
        assert!(q.spin > 0);
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
        // The first fidget brings an idle voice line.
        assert!(p.sounds.iter().any(|s| s.voice && IDLE_VOICE.contains(&s.id)), "{:?}", p.sounds);
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
    fn grabs_climbs_and_jumps_off_a_pole() {
        let w = room();
        let mut p = landed(&w);
        // A rope hanging from 1200 up to 200 above the floor, 50 units away.
        p.set_poles(vec![Pole { pos: [50 * 32, -200 * 32, 0], kind: 0, length: 1000 * 32 }]);
        p.tick_pad(&w, [0.0; 2], 0, false, false, false);
        assert!(p.climb > 0, "grabbed");
        let y0 = p.pos.y;
        for _ in 0..30 {
            p.tick_pad(&w, [0.0; 2], pad::UP, false, false, false);
        }
        assert!(p.pos.y < y0 - 100.0, "climbed from {y0} to {}", p.pos.y);
        assert_eq!(p.state, state::CLIMB_UP);
        assert!((p.pos.x - 50.0).abs() < 2.0, "hugs the pole: {}", p.pos.x);
        p.tick_pad(&w, [1.0, 0.0], 0, true, true, false);
        assert!(p.climb < 0 && p.vel[1] < 0 && p.vel[0] > 0, "jumped off: {:?} {:?}", p.climb, p.vel);
    }

    #[test]
    fn double_jump_speed_matches_original_clamps() {
        assert_eq!(double_jump_speed_high(0), 0x74C);
        assert_eq!(double_jump_speed_high(0x6A00), 0x22A);
        assert_eq!(double_jump_speed_high(0x7000), 0x74C);
        assert_eq!(double_jump_speed_high(576 * 32), 1063);
    }
}
