//! Creatures: everything spawned from the level's creature list (packet type 35), as the
//! original runs them. One slot per spawn record (64 at 0x52C840, 0x9C bytes each).
//!
//! - 0x406CD0 sets a creature up from its record: position, facing, health, respawn time,
//!   behaviour script and flags.
//! - 0x4086F0 picks the creatures near the camera each frame and runs 0x4076F0 on them.
//! - 0x4076F0 is shared by every type: a small script interpreter (the behaviour picked by the
//!   record, table 0x4E02C4) that chooses targets, animations and waits, then movement: turn
//!   towards the target, accelerate, stay inside the creature's zone (a rotated rectangle
//!   around its home), fall and follow the floor, or hover; then the animation clock. Each
//!   type's own routine (+0x94) runs after it; those are not ported here yet.
//!
//! Units as the game: object units (level units × 32), 12-bit angles, ticks.

use crate::formats::all::{self, Ellipsoid, HitShapes};
use crate::formats::exe::Exe;
use crate::formats::packets::Spawn;
use crate::game::{Player, World};

/// Spawn record fields beyond position and id (`Spawn::params` holds record bytes 13–31).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Record {
    pub behaviour: u8,
    /// +0x0E: turn rate; non-zero also makes it face its target while moving.
    pub turn: u8,
    /// +0x0F: fixed facing (× 16), 0 = free.
    pub face: u8,
    pub health: u8,
    pub respawn: u8,
    pub flags: u16,
    /// +0x14 / +0x16: half sizes of the zone (× 256 object units); +0x18 its angle (× 8).
    pub zone: [i16; 2],
    pub zone_angle: i16,
    /// +0x1A: a state byte scripts and level code share.
    pub state: u8,
    /// +0x1C / +0x1D: deceleration along / across (× 2 per tick, ÷ 4).
    pub decel: [u8; 2],
    /// +0x1E: acceleration towards `speed` (0xFF = back off), +0x1F: top speed (× 16).
    pub accel: u8,
    pub speed: u8,
}

impl Record {
    pub fn from_params(p: &[u8; 19]) -> Record {
        let w = |k: usize| i16::from_le_bytes([p[k], p[k + 1]]);
        Record {
            behaviour: p[0],
            turn: p[1],
            face: p[2],
            health: p[3],
            respawn: p[4],
            flags: w(5) as u16,
            zone: [w(7), w(9)],
            zone_angle: w(11),
            state: p[13],
            decel: [p[15], p[16]],
            accel: p[17],
            speed: p[18],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Creature {
    /// creatures.cfg id (+0x14); 0 = empty slot.
    pub id: u8,
    pub rec: Record,
    /// Original record values the scripts change, restored on a respawn.
    rec0: Record,
    spawn: [i32; 3],
    /// Object units.
    pub pos: [i32; 3],
    /// +0x0C / +0x0E / +0x10: tilt, facing, lean.
    pub angles: [i32; 3],
    /// +0x44 / +0x48 / +0x4C.
    pub vel: [i32; 3],
    /// +0x40.
    pub flags: u16,
    pub home: [i32; 3],
    pub target: [i32; 3],
    /// +0x68: the facing it turns towards.
    pub target_facing: i32,
    /// +0x12 animation, +0x74 its frame list (index into the shared table 0x4E0588 and the
    /// position in it), +0x18 the clock (16.16, high word = frame).
    pub anim: u16,
    seq: (usize, usize),
    pub time: u32,
    /// +0x6E / +0x6F: animation rate on the ground / in the air (< 0: fixed).
    rates: [i8; 2],
    /// +0x78: script wait, +0x80: script position.
    pub wait: i32,
    pc: usize,
    /// +0x7A respawn ticks, +0x7E health, +0x70 hurt (> 0) / dying (< 0) timer.
    pub respawn: i32,
    pub health: i32,
    pub hurt: i32,
    /// +0x8A: a timer type routines use.
    pub timer: i32,
    /// +0x7C: knock-back ticks.
    pub knock: i32,
    /// +0x90: last floor height.
    floor: i32,
    /// Jump mode of the current move (opcodes 6 / 7).
    jump: u8,
    /// Updated this frame (near the camera).
    pub active: bool,
    /// Running the shared death script (pointer 0x4E02CC) instead of its own.
    dying_script: bool,
    /// +0x34: drawn lit up (the Tin Man's flash when hurt).
    pub lit: bool,
    /// +0x2C–0x30: the model's scale (0x1000 = as built).
    pub scale: i32,
    /// A script's override of its hit ellipsoid (offset y, radius), as level 3 grows and
    /// shrinks the slime's table.
    pub hit_size: Option<(i32, i32)>,
}

/// The Tin Man fight (level 1, room 5): the boss's routine 0x416AB0 and the room script's
/// part of it.
#[derive(Debug, Clone, Default)]
pub struct Boss {
    /// 0x52F578: 0 waiting, 1 talking, 2 fighting, 3 beaten.
    pub state: i32,
    /// 0x52F5A8: its health when last hurt.
    pub last: i32,
    /// 0x52F590: flash ticks; 0x52F5C8 the blink.
    flash: i32,
    blink: bool,
    /// 0x52B7E0: the HUD bar, 0..54.
    pub bar: i32,
    /// 0x830E3E: the engine's level.
    hum: i32,
    /// Buzz is close enough to be challenged (the routine's call of 0x4027F0).
    pub challenge: bool,
    /// 0x50A118: the camera turns towards here this tick.
    pub focus: Option<[i32; 3]>,
    /// Its death animation is far enough on to give token 4.
    pub token: bool,
    /// Jingles to play (id, where).
    pub jingles: Vec<(i32, [i32; 3])>,
    /// Other bosses' phases: a bob, a wave and a spin angle (level 2's kite: 0x52F6EC,
    /// 0x52F6B8, 0x52F65C).
    pub phase: i32,
    pub wave: i32,
    pub spin: i32,
    /// Its bar shows and the camera may turn to it this tick (set by the routine).
    pub hud: bool,
}

/// The Tin Man's script entry points (word offsets into behaviour 5: the pointer
/// 0x4E02D8 + 0x14 / 0xB4 / 0xCA bytes).
pub const TIN_MAN_FIGHT: usize = 0x0A;
const TIN_MAN_HURT: usize = 0x5A;
const TIN_MAN_DIES: usize = 0x65;

/// Whether `p` (object units) is inside creature `c`'s zone (the rotated rectangle about its
/// home, as script op 0x11 and the chase test).
pub fn in_zone(c: &Creature, p: [i32; 3]) -> bool {
    let r = c.rec;
    let (zx, zz) = (r.zone[0] as i32, r.zone[1] as i32);
    let (zs, zc) = (sin(r.zone_angle as i32 * 8) >> 2, cos(r.zone_angle as i32 * 8) >> 2);
    let (dx, dz) = ((p[0] - c.home[0]) >> 5, (p[2] - c.home[2]) >> 5);
    let u = ((dx * zc - dz * zs) >> 7) + zx * 0x100;
    let v = ((dz * zc + dx * zs) >> 7) + zz * 0x100;
    (u as u32) < (zx << 9) as u32 && (v as u32) < (zz << 9) as u32
}

impl Creature {
    /// Points its script at word `pc` of its own behaviour script, starting there now.
    pub fn set_script(&mut self, pc: usize) {
        self.pc = pc;
        self.wait = 0;
        self.dying_script = false;
    }
}

/// Something creatures did this tick that the game around them handles.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A creature died here (0x405D20 mode 1 drops a coin unless its flags have 0x800).
    /// `centre` is its ellipsoid's centre; `burst` what flies off: n > 0 fireballs (type 0x23),
    /// n < 0 sparkles and smoke (types 99 and 0x11), 0 nothing.
    Died { slot: usize, pos: [i32; 3], coin: bool, centre: [i32; 3], burst: i32 },
}

/// A particle a creature's routine makes (0x40FAE0's arguments), for the caller to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParticleRequest {
    pub pos: [i32; 3],
    pub vel: [i32; 3],
    pub gravity: i32,
    pub angle: i32,
    pub spin: i32,
    pub typ: u8,
}

/// What a creature's update reported, for its type routine (the struct 0x4076F0 passes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Step {
    /// Bit 1: moving towards its target; 2: on the ground; 4: the animation advanced.
    pub flags: u16,
    /// Bit 1: Buzz is inside its zone (chasing).
    pub chase: u16,
    pub side: i32,
    pub forward: i32,
}

pub struct Creatures {
    pub slots: Vec<Creature>,
    scripts: Vec<Option<Vec<i16>>>,
    seqs: Vec<Vec<u8>>,
    /// The game's random bytes (`rand.dat`) and read position (0x52ADAC).
    random: Vec<u8>,
    rpos: usize,
    /// Sounds requested since the caller last drained this: (id, position in object units).
    pub sounds: Vec<(u16, [i32; 3])>,
    pub events: Vec<Event>,
    /// Owned sounds (looping while played each tick) with the slot that owns them.
    pub owned_sounds: Vec<(u16, [i32; 3], usize)>,
    /// Particles the type routines made this tick.
    pub particles: Vec<ParticleRequest>,
    /// Particles to spawn by type and mode (0x40FDF0), with a spin.
    pub spawns: Vec<([i32; 3], u8, usize, i32)>,
    /// Bo Peep's sheep found (0x52B7D8; −1 once she has them back).
    pub sheep: i32,
    /// An RC race is on (0x52F2F8 > 1).
    pub racing: bool,
    /// Every creature is in play wherever the camera is (level 6, 0x4086F0).
    pub always_active: bool,
    /// The Tin Man fight.
    pub boss: Boss,
    /// Hits routines dealt to other slots this tick (slot, angle, kind), applied after.
    pending_hits: Vec<(usize, i32, usize)>,
    /// Buzz this tick (object units), for the type routines.
    buzz: [i32; 3],
    /// Ticks run, for the every-n-ticks flags (0x4A2960).
    clock: u32,
    /// Buzz hurt by a routine this tick (0x4071E0: angle, mode).
    pub hurts: Vec<(i32, u32)>,
    /// Ribbons to draw this frame (0x44E100: start, end, colour, half width), object units.
    pub ribbons: Vec<([i32; 3], [i32; 3], [u8; 3], u8)>,
    /// Hit shapes by creature id, from each character's `.all` file (slot +0x38 sphere and
    /// +0x84 ellipsoid table, set by 0x43B0C0); `default_shape` for the rest.
    pub shapes: std::collections::HashMap<u8, HitShapes>,
    default_shape: HitShapes,
    /// Hit kinds (0x4E066C): (type, knock-back ticks, damage).
    hits: Vec<[i16; 3]>,
    death_script: Option<Vec<i16>>,
}

const HITS: u32 = 0x4E066C;
/// Ellipsoid used by characters without a table of their own (0x43B9B0 points every type at
/// it), with the default sphere (0, 500, 0) radius 500.
const DEFAULT_ELLIPSOID: u32 = 0x4F6EA0;
const DEATH_SCRIPT: u32 = 0x4E02CC;

const BEHAVIOURS: u32 = 0x4E02C4;
const SEQUENCES: u32 = 0x4E0588;
/// Words read from each behaviour pointer: scripts loop back with −1, so a window is enough.
const SCRIPT_WORDS: u32 = 512;

fn sin(a: i32) -> i32 {
    ((a & 0xFFF) as f64 / 4096.0 * std::f64::consts::TAU).sin().mul_add(16384.0, 0.0).round() as i32
}

fn cos(a: i32) -> i32 {
    sin(a + 0x400)
}

fn atan(x: i32, z: i32) -> i32 {
    crate::boxes::angle12(x as f32, z as f32)
}

/// `dir/name`, matching the name without regard to case (the install mixes cases).
fn find_file(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(name)))
}

/// Frame lists of the shared sequence table: frame bytes up to 0xFF and the byte after it.
pub fn sequences(exe: &Exe) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for k in 0..64u32 {
        let Ok(b) = exe.at(SEQUENCES + 4 * k, 4) else { break };
        let p = u32::from_le_bytes(b.try_into().unwrap());
        if !(0x4D0000..0x510000).contains(&p) {
            out.push(vec![0, 0xFF, 1]);
            continue;
        }
        let mut s = Vec::new();
        let mut va = p;
        while let Ok(b) = exe.at(va, 1) {
            s.push(b[0]);
            va += 1;
            if b[0] == 0xFF || s.len() > 255 {
                s.extend(exe.at(va, 1).map(|b| b[0]).ok());
                break;
            }
        }
        out.push(s);
    }
    out
}

impl Creatures {
    pub fn new(spawns: &[Spawn], exe: Option<&Exe>, random: Vec<u8>) -> Creatures {
        let scripts = (0..=255u32)
            .map(|b| {
                let e = exe?;
                let p = u32::from_le_bytes(e.at(BEHAVIOURS + 4 * b, 4).ok()?.try_into().ok()?);
                if !(0x4D0000..0x510000).contains(&p) {
                    return None;
                }
                Some((0..SCRIPT_WORDS).map_while(|k| e.at(p + 2 * k, 2).ok().map(|w| i16::from_le_bytes([w[0], w[1]]))).collect())
            })
            .collect();
        let seqs = exe.map(sequences).unwrap_or_default();
        let hits = exe
            .and_then(|e| e.at(HITS, 36).ok())
            .map(|b| (0..6).map(|k| [0, 1, 2].map(|j| i16::from_le_bytes([b[6 * k + 2 * j], b[6 * k + 2 * j + 1]]))).collect())
            .unwrap_or_else(|| vec![[0, 0, 0], [1, 30, 2], [2, 4, 1], [2, 4, 4], [3, 4, 2], [1, 30, 2]]);
        let death_script = exe.and_then(|e| {
            let p = u32::from_le_bytes(e.at(DEATH_SCRIPT, 4).ok()?.try_into().ok()?);
            Some((0..64u32).map_while(|k| e.at(p + 2 * k, 2).ok().map(|w| i16::from_le_bytes([w[0], w[1]]))).collect())
        });
        let mut c = Creatures {
            slots: Vec::new(),
            scripts,
            seqs,
            random,
            rpos: 0,
            sounds: Vec::new(),
            events: Vec::new(),
            owned_sounds: Vec::new(),
            particles: Vec::new(),
            spawns: Vec::new(),
            sheep: 0,
            racing: false,
            always_active: false,
            boss: Boss::default(),
            pending_hits: Vec::new(),
            buzz: [0; 3],
            clock: 0,
            hurts: Vec::new(),
            ribbons: Vec::new(),
            shapes: Default::default(),
            default_shape: HitShapes {
                sphere: [0, 500, 0, 500],
                ellipsoids: vec![
                    exe.and_then(|e| e.at(DEFAULT_ELLIPSOID, 16).ok())
                        .map(|b| HitShapes::entry(&(0..8).map(|k| i16::from_le_bytes([b[2 * k], b[2 * k + 1]])).collect::<Vec<_>>()))
                        .unwrap_or(Ellipsoid { offset: [0, -250, 0], scale: [512, 256, 512], radius: 250 }),
                ],
            },
            hits,
            death_script,
        };
        for s in spawns.iter().take(64) {
            let rec = Record::from_params(&s.params);
            let mut cr = Creature {
                id: s.creature,
                rec,
                rec0: rec,
                spawn: s.pos.map(|v| v * 32),
                pos: [0; 3],
                angles: [0; 3],
                vel: [0; 3],
                flags: rec.flags,
                home: [0; 3],
                target: [0; 3],
                target_facing: 0,
                anim: 0,
                seq: (0, 0),
                time: 0,
                rates: [0; 2],
                wait: 0,
                pc: 0,
                respawn: 0,
                health: 0,
                hurt: 0,
                timer: 0,
                knock: 0,
                floor: 0,
                jump: 0,
                active: false,
                dying_script: false,
                lit: false,
                scale: 0x1000,
                hit_size: None,
            };
            c.reset(&mut cr, true);
            c.slots.push(cr);
        }
        c
    }

    /// Reads every character's hit shapes: `creatures.cfg` in `data` names each id's file
    /// (`<dir>\\<name>.all`, the directory relative to a level folder).
    pub fn load_shapes(&mut self, data: &std::path::Path) {
        let Ok(cfg) = std::fs::read_to_string(data.join("creatures.cfg")) else { return };
        let ids: std::collections::HashSet<u8> = self.slots.iter().map(|c| c.id).collect();
        for line in cfg.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let [kw, id, name, dir, ..] = f[..] else { continue };
            let Ok(id) = id.parse::<u8>() else { continue };
            if kw != "CREATURE" || !ids.contains(&id) {
                continue;
            }
            let dir = dir.trim_start_matches("..\\").to_ascii_lowercase();
            if let Some(h) = find_file(&data.join(&dir), &format!("{}.all", name.to_ascii_lowercase()))
                .and_then(|p| std::fs::read(p).ok())
                .and_then(|b| all::hit_shapes(&b))
            {
                self.shapes.insert(id, h);
            }
        }
    }

    /// The hit shapes of creature id `id`.
    pub fn shape(&self, id: u8) -> &HitShapes {
        self.shapes.get(&id).unwrap_or(&self.default_shape)
    }

    /// A creature's ellipsoid for its current animation, with any script override.
    fn ellipsoid_of(&self, c: &Creature) -> Ellipsoid {
        let mut e = self.shape(c.id).ellipsoid(c.anim as usize);
        if let Some((oy, r)) = c.hit_size {
            e.offset[1] = oy.clamp(-0x8000, 0x7FFF) as i16;
            e.radius = r.clamp(1, 0x7FFF) as i16;
        }
        e
    }

    /// Centre of slot `i`'s ellipsoid for its current animation, object units, and the ellipsoid.
    pub fn ellipsoid_at(&self, i: usize) -> ([i32; 3], Ellipsoid) {
        let c = &self.slots[i];
        let e = self.ellipsoid_of(c);
        let (s, k) = (sin(c.angles[1] - 0x800) >> 2, sin(c.angles[1] - 0x400) >> 2);
        let (ox, oy, oz) = (e.offset[0] as i32, e.offset[1] as i32, e.offset[2] as i32);
        ([((ox * k + oz * s) >> 12) + c.pos[0], oy + c.pos[1], ((oz * k - ox * s) >> 12) + c.pos[2]], e)
    }

    /// 0x4A5870: a laser ray from `origin` along `delta` (object units) against every creature
    /// in play whose record allows it (+0x1A bit 4), as ellipsoids. The nearest hit shortens
    /// `delta` to the entry point. Returns 0 (nothing), 2 (hit) or 4 (hit something the laser
    /// can't hurt, record bit 1 clear: the beam bounces). With `kind` 1 or 2 the hit lands
    /// (hit kinds 2 and 3 of 0x4E066C) at Buzz's `facing`.
    pub fn laser_ray(&mut self, origin: [i32; 3], delta: &mut [i32; 3], kind: u8, facing: i32) -> u32 {
        let mut found = None;
        for i in 0..self.slots.len() {
            let c = &self.slots[i];
            if c.id == 0 || !c.active || c.health <= 0 || c.rec.state & 4 == 0 || c.knock < 0 || c.hurt < 0 {
                continue;
            }
            let (centre, e) = self.ellipsoid_at(i);
            let (s, k) = (sin(c.angles[1] - 0x800) >> 2, sin(c.angles[1] - 0x400) >> 2);
            let [sx, sy, sz] = e.scale.map(|v| v as i32);
            // Into the ellipsoid's frame, where it is a sphere: level units.
            let local = |v: [i32; 3]| {
                let (x, z) = (v[0] - centre[0], v[2] - centre[2]);
                [((x * k + z * s) >> 12) * sx >> 8, (v[1] - centre[1]) * sy >> 8, ((z * k - x * s) >> 12) * sz >> 8]
            };
            let a = local(origin).map(|v| -(v >> 5));
            let end = [0, 1, 2].map(|j| origin[j] + delta[j]);
            let b = local(end).map(|v| -(v >> 5));
            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]].map(|v| -v);
            let len = ((d[0] as f64).powi(2) + (d[1] as f64).powi(2) + (d[2] as f64).powi(2)).sqrt();
            if len < 1.0 {
                continue;
            }
            let u = d.map(|v| v as f64 / len);
            // Centre relative to the ray start is −a.
            let t = a.iter().zip(&u).map(|(&p, &q)| p as f64 * q).sum::<f64>();
            if t <= 0.0 || t > len {
                continue;
            }
            let perp2 = (0..3).map(|j| (a[j] as f64 - t * u[j]).powi(2)).sum::<f64>();
            let r = e.radius as f64;
            if perp2 >= r * r {
                continue;
            }
            let t_in = t - (r * r - perp2).sqrt();
            // Back to the world: undo the scales and the rotation.
            let lx = t_in * u[0] * 256.0 * 32.0 / sx.max(1) as f64;
            let ly = t_in * u[1] * 256.0 * 32.0 / sy.max(1) as f64;
            let lz = t_in * u[2] * 256.0 * 32.0 / sz.max(1) as f64;
            let (kf, sf) = (k as f64 / 4096.0, s as f64 / 4096.0);
            *delta = [(lx * kf - lz * sf) as i32, ly as i32, (lz * kf + lx * sf) as i32];
            found = Some(i);
        }
        let Some(i) = found else { return 0 };
        if kind != 0 {
            self.hit(i, facing, if kind == 1 { 2 } else { 3 });
        }
        if self.slots[i].rec.state & 1 == 0 { 4 } else { 2 }
    }

    /// Creatures the laser auto-aims at (0x4A5D30): in play, targetable (+0x1A bit 4), not
    /// dying, within 4096 level units of `origin` and 768 of its height. Slot indices.
    pub fn aim_candidates(&self, origin: [i32; 3]) -> Vec<usize> {
        (0..self.slots.len())
            .filter(|&i| {
                let c = &self.slots[i];
                if c.id == 0 || !c.active || c.health <= 0 || c.rec.state & 4 == 0 || c.hurt < 0 {
                    return false;
                }
                let d = [0, 1, 2].map(|j| ((c.pos[j] - origin[j]) >> 5) as i64);
                d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < 0x100_0000 && d[1].abs() < 0x300
            })
            .collect()
    }

    fn rand(&mut self) -> u8 {
        if self.random.is_empty() {
            return 0x80;
        }
        let b = self.random[self.rpos % self.random.len()];
        self.rpos = (self.rpos + 1) % 0x5DC.min(self.random.len());
        b
    }

    /// 0x406CD0: (re)start a creature from its record.
    fn reset(&self, c: &mut Creature, first: bool) {
        let r = c.rec0;
        c.rec = r;
        c.pos = c.spawn;
        c.health = r.health as i32;
        c.flags = if first || c.flags & 0x800 == 0 { r.flags } else { r.flags.wrapping_add(0x800) };
        c.respawn = if r.respawn == 100 { 0x708 } else { r.respawn as i32 };
        c.home = c.pos;
        c.target = c.pos;
        c.angles = [0, (r.face as i32) << 4, 0];
        c.target_facing = c.angles[1];
        c.vel = [0; 3];
        c.time = 0;
        c.anim = 0;
        c.seq = (0, 0);
        c.wait = 0;
        c.pc = 0;
        c.hurt = 0;
        c.timer = 0;
        c.knock = 0;
        c.floor = c.pos[1];
        c.jump = 0;
        c.dying_script = false;
        if c.id == 0x18 {
            c.health = 0;
            c.respawn = 10000;
        }
    }

    fn set_anim(&self, c: &mut Creature, anim: u16, seq: usize) {
        c.anim = anim;
        c.seq = (seq, 0);
        c.time = (self.seqs.get(seq).and_then(|s| s.first()).copied().unwrap_or(0) as u32) << 16;
    }

    /// 0x405C80: one entry along the frame list (0xFF 1 holds, 0xFF n goes back n).
    fn step_anim(&self, c: &mut Creature) {
        let Some(s) = self.seqs.get(c.seq.0) else { return };
        let i = c.seq.1;
        let at = |k: usize| s.get(k).copied().unwrap_or(0xFF);
        if at(i + 2) == 0xFF && at(i + 3) == 0 {
            c.time = (at(i) as u32) << 16 | 0xFFFF;
            return;
        }
        let mut j = i + 1;
        if at(j) == 0xFF {
            let n = at(i + 2) as usize;
            if n == 1 {
                j = i;
                c.time |= 0xFFFF;
            } else {
                j = (j + 1).saturating_sub(n);
            }
        }
        c.seq.1 = j;
        c.time = (at(j) as u32) << 16 | (c.time & 0xFFFF);
    }

    /// 0x4086F0 + 0x4076F0 for one tick. `camera` and `buzz` in object units.
    pub fn tick(&mut self, world: &World, camera: [i32; 3], buzz: [i32; 3]) -> Vec<(usize, Step)> {
        let mut steps = Vec::new();
        self.buzz = buzz;
        self.clock = self.clock.wrapping_add(1);
        for i in 0..self.slots.len() {
            let mut c = self.slots[i].clone();
            c.active = false;
            if c.id == 0 {
                continue;
            }
            if c.health <= 0 {
                // Dead: wait out the respawn time (≥ 5000: never), then start again.
                if c.respawn == 0 {
                    self.reset(&mut c, false);
                } else if c.respawn < 5000 {
                    c.respawn = (c.respawn - 1).max(0);
                }
                self.slots[i] = c;
                continue;
            }
            let d = [0, 1, 2].map(|k| (camera[k] - c.pos[k]) >> 8);
            let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            // In reach of the camera: 400 plus an eighth of its bounding radius (0x4086F0).
            let near = (self.shape(c.id).sphere[3] as i32 >> 3) + 400;
            // Health 0xCA (racing) keeps it in play wherever the camera is, and in level 6
            // (the boss arena) everything is always in play.
            let d2 = if self.always_active || c.health == 0xCA { 10 } else { d2 };
            if (c.flags & 1 == 0 && d2 >= near * near) || c.flags & 0x2000 != 0 {
                self.slots[i] = c;
                continue;
            }
            c.active = true;
            let s = self.think(&mut c, world, buzz);
            self.routine(i, &mut c, &s, world);
            // 0x4086F0: > 0 a delayed death, < 0 dying (then removed).
            if c.hurt > 0 {
                c.hurt -= 1;
                if c.hurt == 0 {
                    self.kill(i, &mut c);
                }
            } else if c.hurt < 0 {
                c.hurt += 1;
                if c.hurt == 0 {
                    self.remove(&mut c);
                }
            }
            self.slots[i] = c;
            steps.push((i, s));
        }
        for (slot, angle, kind) in std::mem::take(&mut self.pending_hits) {
            self.hit(slot, angle, kind);
        }
        steps
    }

    /// The creature's own routine (+0x94, set by 0x406CD0 from its type), after the shared update.
    fn routine(&mut self, slot: usize, c: &mut Creature, s: &Step, world: &World) {
        match c.id {
            4 => self.zurg3(slot, c, s),
            0xC => self.lawnmower(slot, c),
            0xD => self.army_man(c),
            0xE => self.zurg_car(slot, c, s),
            0xF => self.kite(slot, c, s),
            0x13 => self.friend(c, 0x6B, 0x6C, 0x5A, false),
            0x14 => self.zurg_pod(slot, c, s, world),
            0x16 => self.drill(slot, c),
            0x20 => self.clown(c),
            0x1A => self.dino(c),
            0x17 => {
                self.friend(c, 0x8E, 0x8E, 0x5A, true);
                // Upside down in its record's pose (0x4259B0).
                if c.rec.decel[0] < 0x80 {
                    c.angles[0] = 0xC00;
                    c.angles[2] = 0x800;
                }
            }
            0x18 => self.plane(slot, c),
            0x19 => self.plane_box(slot, c),
            0x1B => self.zurg_boat(c),
            0x1C => self.friend(c, 0x77, 0x77, 0x3C, false),
            0x26 => self.friend(c, 0x80, 0xB8, 300, false),
            0x2B => self.friend(c, 0xA2, 0xA2, 0x3C, true),
            5 => self.tin_man(slot, c, s),
            6 => self.sheep(c),
            8 => self.rc(slot, c, s),
            _ => {}
        }
    }

    /// 0x416F30, the RC car in level 1: the engine (sound 0x37, owned) while it goes; racing,
    /// the animations for speeding up (1) and skidding round (2 / 3), with dust from the back
    /// wheels and sound 0x36 then.
    fn rc(&mut self, slot: usize, c: &mut Creature, s: &Step) {
        if s.forward > 0 {
            self.owned_sounds.push((0x37, c.pos, slot));
        }
        if !self.racing {
            return;
        }
        let mut anim = 0u16;
        let mut seq = 2usize;
        if s.forward < c.rec.speed as i32 * 8 {
            anim = 1;
            seq = 0xB;
        }
        if s.forward < s.side.abs() {
            anim = if s.side < 0 { 2 } else { 3 };
            seq = 0xC;
        }
        if anim != c.anim && (anim != 0 || c.anim != 1 || c.time >> 16 > 0xD) {
            self.set_anim(c, anim, seq);
        }
        if anim == 1 || s.side.abs() > 0x200 {
            let f = c.angles[1];
            for (a, b) in [(0x680, -0x580), (-0x680, -0x280)] {
                let at = [c.pos[0] + (sin(f + a) >> 1), c.pos[1] - 0x800, c.pos[2] + (sin(f + b) >> 1)];
                let spin = self.rand() as i32 - 0x80;
                self.spawns.push((at, 0x2A, 10, spin));
            }
            self.sounds.push((0x36, c.pos));
        }
    }

    /// 0x416AB0, the Tin Man. It flashes for 60 ticks when hurt; its acceleration grows as
    /// its health falls ((health + 14) × 12, as a byte). In its open animations (3, 5) the
    /// laser can hurt it (record state 7, else 4); a hit sends its script to the flinch, or
    /// below 10 health to its death (harmless, fireballs, jingle −2, fight over). Its death
    /// animation (7) smokes and, from frame 13, gives token 4. Buzz in its zone and on the
    /// arena's level: challenged (fight not started) or the camera turns to it (fighting);
    /// out of it during the charge (animation 2), the fight script starts again. The HUD bar is
    /// (health − 9) × 54 / 11; the engine (sound 0x35) hums while it moves.
    fn tin_man(&mut self, slot: usize, c: &mut Creature, s: &Step) {
        let b = &mut self.boss;
        b.blink = !b.blink;
        b.flash -= 1;
        if b.flash < 0 {
            b.flash = 0;
            c.lit = false;
        } else {
            c.lit = b.blink;
        }
        c.rec.accel = ((c.health + 14) * 12) as u8;
        if c.anim == 3 || c.anim == 5 {
            c.rec.state = 7;
            if c.health != self.boss.last {
                self.boss.flash = 0x3C;
                if c.health < 10 {
                    c.pc = TIN_MAN_DIES;
                    c.rec.state = 4;
                    c.flags &= !0x100;
                    let e = self.shape(c.id).ellipsoid(0);
                    let at = [0, 1, 2].map(|k| c.pos[k] + e.offset[k] as i32);
                    for _ in 0..5 {
                        let spin = self.rand() as i32 - 0x80;
                        self.spawns.push((at, 0x23, 0xE, spin));
                    }
                    self.boss.jingles.push((-2, c.pos));
                    self.boss.state = 3;
                } else {
                    c.pc = TIN_MAN_HURT;
                }
                self.boss.last = c.health;
                c.wait = 0;
            }
        } else {
            c.rec.state = 4;
        }
        let buzz = self.buzz;
        let inside = s.chase & 1 != 0 || in_zone(c, buzz);
        if c.anim == 7 {
            if c.time >> 16 > 0xC {
                self.boss.token = true;
            }
            let f = c.angles[1];
            let at = [c.pos[0] + sin(f) * 2 / 3, c.pos[1] - 0x5000, c.pos[2] + cos(f) * 2 / 3];
            // Every 7 ticks (0x52F1C7).
            if self.clock % 7 == 0 {
                let spin = self.rand() as i32 - 0x80;
                self.spawns.push((at, 0x11, 10, spin));
            }
            if self.rand() & 3 != 0 && c.time >> 16 == 0xE {
                self.spawns.push((at, 4, 4, 0));
            }
        }
        if inside && c.anim != 7 {
            // On the arena's level (below the shelf above it).
            if buzz[1] > -0x3213A && buzz[1] < 0 {
                match self.boss.state {
                    0 => self.boss.challenge = buzz[1] < -0x2BD6E,
                    2 => self.boss.focus = Some(c.pos),
                    _ => {}
                }
            }
        } else if c.anim == 2 {
            c.wait = 0;
            c.pc = TIN_MAN_FIGHT;
        }
        self.boss.bar = (c.health - 9) * 0x36 / 0xB;
        if s.forward < 1 {
            if self.boss.hum > 0x200 {
                self.boss.hum -= 0x20;
            }
        } else {
            self.boss.hum = s.forward * 2 + 0x800;
        }
        if c.health > 9 && self.boss.hum > 0x200 {
            self.owned_sounds.push((0x35, c.pos, slot));
        }
    }

    /// A point `local` (object units, the model's frame: −z ahead, −y up) on creature `c`, as
    /// 0x43C1C0 turns it by the facing.
    fn local(c: &Creature, local: [i32; 3]) -> [i32; 3] {
        let f = c.angles[1] + 0x800;
        let (s, k) = (sin(f), cos(f));
        [c.pos[0] + ((local[0] * k + local[2] * s) >> 14), c.pos[1] + local[1], c.pos[2] + ((local[2] * k - local[0] * s) >> 14)]
    }

    /// 0x410410 (n = 0x32): five sparkles bursting from `at`.
    fn sparkle_burst(&mut self, at: [i32; 3]) {
        for _ in 0..5 {
            let r = self.rand() as i32;
            let o = ((r & 0x3F) - 0x20) * 0x32 * 2;
            let spin = self.rand() as i32 - 0x80;
            self.spawns.push(([at[0] + o, at[1] + o, at[2] + o], 0x29, 0xF, spin));
        }
    }

    /// The "find them all" characters (health 0x66): a call (`call`) every (r & 0x7F) + `every`
    /// ticks (`only_friendly`: while health is 0x66); touched, the level's count (0x52B7D8)
    /// goes up with sparkles and `found`, and it's gone. 0x41BB80 (0x13), 0x4259B0 (0x17),
    /// 0x420ED0 (0x1C), 0x422C70 (0x26), 0x41DEC0 (0x2B).
    fn friend(&mut self, c: &mut Creature, call: u16, found: u16, every: i32, only_friendly: bool) {
        let friendly = c.health == 0x66;
        if !only_friendly || friendly {
            c.timer -= 1;
            if c.timer < 1 {
                c.timer = (self.rand() as i32 & 0x7F) + every;
                self.sounds.push((call, c.pos));
            }
        }
        if c.flags & 0x200 != 0 && friendly {
            self.sheep += 1;
            self.sparkle_burst([c.pos[0], c.pos[1] - 0x2000, c.pos[2]]);
            self.sounds.push((found, c.pos));
            self.remove(c);
        }
    }

    /// 0x418CE0, the lawnmower (0xC): its engine (0x4B, owned); in play, a puff (0x31 mode 2,
    /// turned at random) every 16 ticks and grass clippings (0x32) thrown out behind it every
    /// other tick.
    fn lawnmower(&mut self, slot: usize, c: &mut Creature) {
        self.owned_sounds.push((0x4B, c.pos, slot));
        if !c.active {
            return;
        }
        if self.clock.is_multiple_of(16) {
            self.rand();
            self.spawns.push((c.pos, 0x31, 2, 0));
        }
        if self.clock.is_multiple_of(2) {
            let f = c.angles[1];
            let at = [c.pos[0] + sin(f - 0x800), c.pos[1], c.pos[2] + sin(f - 0x400)];
            let r0 = self.rand() as i32;
            let r1 = self.rand() as i32;
            self.rand();
            self.rand();
            let u = (f + 0x500 + r0 * 6) & 0xFFF;
            self.particles.push(ParticleRequest { pos: at, vel: [sin(u) >> 4, r1 - 0xC00, cos(u) >> 4], gravity: 0x100, angle: r1 << 4, spin: r1 * 2 - 0x100, typ: 0x32 });
            self.rand();
        }
    }

    /// 0x418610, an army man (0xD, health 0x66): now and then (every 32 ticks, 1 in 4) a call
    /// (0x48); touched, the count goes up (sparkles, 0x49) and he's gone. Every (r & 0x7F) +
    /// 0x80 ticks in play he throws something (particle 0x79, sound 0x60).
    fn army_man(&mut self, c: &mut Creature) {
        if c.health != 0x66 {
            return;
        }
        if self.clock.is_multiple_of(32) && self.rand() & 3 == 0 {
            self.sounds.push((0x48, c.pos));
        }
        if c.flags & 0x200 != 0 {
            self.sheep += 1;
            self.sparkle_burst([c.pos[0], c.pos[1] - 0x2000, c.pos[2]]);
            self.remove(c);
            self.sounds.push((0x49, c.pos));
            return;
        }
        c.timer -= 1;
        if c.timer < 1 {
            c.timer = (self.rand() as i32 & 0x7F) + 0x80;
            if c.active {
                let r = self.rand() as i32 - 0x80;
                self.particles.push(ParticleRequest { pos: [c.pos[0], c.pos[1] - 0x1000, c.pos[2]], vel: [r * 4, -0xC00, r], gravity: 0x60, angle: 0, spin: r * 4, typ: 0x79 });
                self.sounds.push((0x60, c.pos));
            }
        }
    }

    /// 0x4064A0, Zurg's car (0xE): hit (health below 0x65 changed), its script goes to its
    /// hit reaction (behaviour entry + 0x7C) unless it's in animation 1; a horn (0x42) every 8
    /// ticks when its record's acceleration is 0xFF; its engine (0x41) while it drives, with
    /// dust (0x27 mode 2) from both back wheels every 4 ticks at speed.
    fn zurg_car(&mut self, slot: usize, c: &mut Creature, s: &Step) {
        if c.timer == 0 {
            c.timer = c.health;
        }
        if c.timer != c.health && c.health < 0x65 {
            c.timer = c.health;
            if c.anim != 1 {
                c.set_script(0x7C / 2);
            }
        }
        if self.clock.is_multiple_of(8) && c.rec.accel == 0xFF {
            self.sounds.push((0x42, c.pos));
        }
        if s.forward > 0 {
            self.owned_sounds.push((0x41, c.pos, slot));
        }
        if c.active && c.hurt >= 0 && s.forward > 0x100 && self.clock.is_multiple_of(4) {
            let f = c.angles[1];
            for (a, b) in [(0x7A0, -0x460), (-0x7A0, -0x3A0)] {
                let at = [c.pos[0] + (sin(f + a) * 3 >> 2), c.pos[1] - 0x1000, c.pos[2] + (sin(f + b) * 3 >> 2)];
                self.spawns.push((at, 0x27, 2, 0));
            }
        }
    }

    /// 0x406620, a Zurg pod (0x14): leans into its sideways speed, hums (0x5B); while Buzz is
    /// in its zone, every 360 ticks it dodges sideways, and from 280 down to 100 it sweeps a
    /// laser at the ground ahead and below (green ribbon, sound 0x5C and sparks where it
    /// lands); Buzz within 200 units of that spot is hurt.
    fn zurg_pod(&mut self, slot: usize, c: &mut Creature, s: &Step, world: &World) {
        let side = s.side.clamp(-0x200, 0x200);
        c.angles[2] -= (c.angles[2] + side) / 16;
        if c.hurt >= 0 {
            self.owned_sounds.push((0x5B, c.pos, slot));
        }
        if s.chase & 1 == 0 {
            c.timer = 0x168;
            return;
        }
        c.timer -= 1;
        if c.timer < 0 {
            c.timer = 0x168;
            let a = c.angles[1] + if self.rand() < 0x80 { -0x200 } else { 0x200 };
            c.vel[0] = sin(a) >> 4;
            c.vel[2] = cos(a) >> 4;
        }
        if c.timer < 0x119 && c.timer > 99 {
            let from = Self::local(c, [0, 0, -100]);
            let f = c.angles[1];
            let delta = [sin(f) << 2, 96000, cos(f) << 2];
            let o = glam::Vec3::from(from.map(|v| v as f32 / 32.0));
            let d = glam::Vec3::from(delta.map(|v| v as f32 / 32.0));
            let t = world.sweep(o, d, 1.0).map_or(1.0, |h| h.t);
            let hit = [0, 1, 2].map(|k| from[k] + (delta[k] as f32 * t) as i32);
            self.ribbons.push((from, hit, [0, 0x80, 0], 0x20));
            self.sounds.push((0x5C, hit));
            if self.clock.is_multiple_of(2) {
                self.spawns.push((hit, 4, 4, 0));
            }
            if self.clock.is_multiple_of(4) {
                self.spawns.push((hit, 0x46, 2, 0));
            }
            let b = self.buzz;
            let d = [0, 1, 2].map(|k| (b[k] - hit[k]) >> 8);
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < 25 * 25 {
                self.hurts.push((atan(b[0] - hit[0], b[2] - hit[2]), 3));
            }
        }
    }

    /// 0x406960, a toy plane (0x18): its engine (0x5D); touched while at health 1 it takes a
    /// spin hit; its script's timer sends it back to its box (the slot before it, or two
    /// before) to take off again; exhaust (0x58 mode 3) behind it every 8 ticks.
    fn plane(&mut self, slot: usize, c: &mut Creature) {
        if c.hurt >= 0 {
            self.owned_sounds.push((0x5D, c.pos, slot));
        }
        if c.flags & 0x200 != 0 && c.health == 1 {
            self.pending_hits.push((slot, (c.angles[1] - 0x800) & 0xFFF, 1));
        }
        if c.timer != 0 {
            c.timer = 0;
            let b = if self.slots.get(slot.wrapping_sub(1)).is_some_and(|b| b.id == 0x19) { slot - 1 } else { slot.wrapping_sub(2) };
            if let Some(bx) = self.slots.get(b) {
                let p = bx.pos;
                c.pos = p;
                c.home[1] = p[1] - 0x5000;
                c.target = [p[0], p[1] - 0x5000, p[2]];
            }
        }
        if self.clock.is_multiple_of(8) && c.active {
            let at = Self::local(c, [0, -200, 400]);
            self.spawns.push((at, 0x58, 3, 0));
        }
    }

    /// 0x4068E0, a plane's box (0x19): when its script's timer fires, a dead plane in the next
    /// slot (or the one after) comes back in 30 ticks and the box's script launches it
    /// (behaviour entry + 0x6A); with both up it goes to + 0x76.
    fn plane_box(&mut self, slot: usize, c: &mut Creature) {
        if c.timer == 0 {
            return;
        }
        c.timer = 0;
        for k in [slot + 1, slot + 2] {
            if let Some(p) = self.slots.get_mut(k)
                && p.health == 0
            {
                p.respawn = 0x1E;
                c.set_script(0x6A / 2);
                return;
            }
        }
        c.set_script(0x76 / 2);
    }

    /// 0x4189C0, the Zurg kite (0xF), level 2's boss; `boss.state` is the level script's
    /// 0x52F664 (0 waiting, 1 talking, 2 fighting). It bobs (lean from a 0x24-a-tick phase)
    /// and always faces Buzz, and flashes for 60 ticks when hurt. Before the fight it hangs
    /// high (y −0x7C000 with a wave). Fighting, while Buzz is up on the roof (y < −0x72000)
    /// the camera turns to it and the boss bar shows; Buzz out of its zone up there, it swoops
    /// at him spinning (faster as it weakens, engine 0x3C); in its zone it hovers 0xA000
    /// above him and dodges every 120 ticks, and a hit (sound 0x98) restarts its script's
    /// wait. Buzz off the roof, it goes home and can't be hurt. It never comes below
    /// −0x741AD.
    fn kite(&mut self, slot: usize, c: &mut Creature, s: &Step) {
        let b = &mut self.boss;
        b.blink = !b.blink;
        b.flash -= 1;
        if b.flash < 0 {
            b.flash = 0;
            c.lit = false;
        } else {
            c.lit = b.blink;
        }
        b.phase = (b.phase + 0x24) & 0xFFF;
        c.angles[2] = sin(b.phase) >> 6;
        b.wave = (b.wave + 0x40) & 0xFFF;
        let buzz = self.buzz;
        c.target_facing = (atan(c.pos[0] - buzz[0], c.pos[2] - buzz[2]) - 0x800) & 0xFFF;
        if self.boss.state == 2 {
            let high = buzz[1] < -0x71FFF;
            if high {
                self.boss.focus = Some(c.pos);
                self.boss.hud = true;
            } else {
                c.flags = (c.flags & !8) | 4;
                c.target[0] = c.home[0];
                c.target[2] = c.home[2];
                c.rec.state = 4;
                c.wait = 0x32;
            }
            let by = buzz[1].min(-0x721AD);
            let wave = sin(self.boss.wave);
            if s.chase & 1 == 0 && high {
                c.timer = 2;
                c.target = [buzz[0], (wave - 0x3000 + by).min(-0x741AD), buzz[2]];
                let spin = c.health * -0x14 + 0x108;
                self.owned_sounds.push((0x3C, c.pos, slot));
                self.boss.spin = (self.boss.spin + spin) & 0xFFF;
                c.angles[1] = self.boss.spin;
                c.vel[0] = sin(c.target_facing) >> 5;
                c.vel[2] = cos(c.target_facing) >> 5;
            } else {
                if self.boss.last != c.health {
                    self.boss.flash = 0x3C;
                    self.boss.last = c.health;
                    c.wait = 0;
                    if c.health > 0 {
                        self.sounds.push((0x98, c.pos));
                    }
                }
                self.boss.spin = c.angles[1];
                c.target[1] = wave - 0xA000 + by;
                c.timer -= 1;
                if c.timer < 0 {
                    c.timer = 0x78;
                    let a = c.angles[1] + if self.rand() < 0x80 { -0x200 } else { 0x200 };
                    c.vel[0] = sin(a) >> 3;
                    c.vel[2] = cos(a) >> 3;
                }
            }
        } else {
            c.pos[1] = (sin(self.boss.wave) >> 3) - 0x7C000;
        }
        c.pos[1] = c.pos[1].min(-0x741AD);
    }

    /// 0x41B780, the drill (0x16), level 4's boss; `boss.state` is the script's 0x52F804. It
    /// rumbles (0x99, owned; 0x9A when touched) and shakes the camera near Buzz (not ported).
    /// Fighting, it flashes for 60 ticks after a hit (then hurtable again: record state 7,
    /// else 4); the laser hurts it (state 5) unless Buzz has the disk launcher out. It kicks
    /// up dust (0x04 mode 9) every 4 ticks before it's beaten; while Buzz is up on the rim
    /// (y < −0x7DE25) it throws rocks at him (0x54, every 32 ticks, one in four aimed), and
    /// its bar is health × 54 / 30.
    fn drill(&mut self, slot: usize, c: &mut Creature) {
        self.owned_sounds.push((0x99, c.pos, slot));
        if c.flags & 0x200 != 0 {
            c.flags &= !0x200;
            self.sounds.push((0x9A, c.pos));
        }
        let b = &mut self.boss;
        b.blink = !b.blink;
        if b.state > 1 {
            b.flash -= 1;
            if b.flash < 0 {
                b.flash = 0;
                c.rec.state = 7;
                c.lit = false;
            } else {
                c.lit = b.blink;
            }
        }
        if c.health != b.last {
            b.flash = 0x3C;
            b.last = c.health;
            c.rec.state = 4;
        }
        c.rec.state = if b.state < 2 { 4 } else { 5 };
        if b.state < 3 && self.clock.is_multiple_of(4) {
            self.spawns.push((c.pos, 4, 9, 0));
        }
        if b.state == 2 && self.buzz[1] < -0x7DE25 {
            if self.clock.is_multiple_of(32) {
                if self.rand() & 3 == 0 {
                    let buzz = self.buzz;
                    let (dx, dz) = ((c.pos[0] - buzz[0]) >> 5, (c.pos[2] - buzz[2]) >> 5);
                    let u = atan(dx, dz) & 0xFFF;
                    let d = (((dx * dx + dz * dz) as f64).sqrt() as i32).max(4);
                    let reach = (d * 3 / 4).max(1);
                    let t = ((d << 12) / reach).max(1);
                    let vy = ((t * -0x80) >> 8) - ((c.pos[1] - buzz[1]) * 0x80) / t;
                    let spin = self.rand() as i32 - 0x80;
                    self.particles.push(ParticleRequest { pos: c.pos, vel: [sin(u - 0x800) * reach >> 14, vy, sin(u - 0x400) * reach >> 14], gravity: 0x80, angle: 0, spin, typ: 0x54 });
                } else {
                    let spin = self.rand() as i32 - 0x80;
                    self.spawns.push((c.pos, 0x54, 0xE, spin));
                }
            }
            self.boss.bar = c.health * 0x36 / 0x1E;
        }
    }

    /// 0x41DDB0, the clown (0x20), level 5's boss; `boss.state` is the script's 0x52F8E8. A
    /// hit (health changed) flashes it for 60 ticks during which it can't be hurt (record
    /// state 4, then 7); touched, sound 0xA1.
    fn clown(&mut self, c: &mut Creature) {
        let b = &mut self.boss;
        b.blink = !b.blink;
        if c.health != b.last {
            b.flash = 0x3C;
            b.last = c.health;
            c.rec.state = 4;
        }
        if c.flags & 0x200 != 0 {
            c.flags &= !0x200;
            self.sounds.push((0xA1, c.pos));
        }
        c.lit = false;
        if b.state >= 2 {
            b.flash -= 1;
            if b.flash < 0 {
                b.flash = 0;
                c.rec.state = 7;
            } else {
                c.lit = b.blink;
            }
        }
    }

    /// 0x420AF0, the dino (0x1A), level 7's boss; `boss.state` is the script's 0x52FA14. A
    /// hit flashes it for 60 ticks (record state 4, then 7 while fighting; sound 0x79); its
    /// script's timer makes it breathe fire (particles 0x55 from its mouth, sound 0x78, the
    /// camera shakes at 30); below 10 health it dies (script + 0x74, harmless, five fireballs,
    /// jingle −2, the fight over); then it smokes.
    fn dino(&mut self, c: &mut Creature) {
        let b = &mut self.boss;
        b.blink = !b.blink;
        if c.health != b.last {
            b.flash = 0x3C;
            b.last = c.health;
            c.rec.state = 4;
            self.sounds.push((0x79, c.pos));
        }
        if c.timer != 0 {
            let at = Self::local(c, [0x14, -0x258, 0]);
            let u = (c.angles[1] - 0x80 + self.rand() as i32) & 0xFFF;
            self.sounds.push((0x78, c.pos));
            let r = self.rand() as i32;
            self.rand();
            self.particles.push(ParticleRequest { pos: at, vel: [sin(u) >> 3, r + 0x200, cos(u) >> 3], gravity: 0, angle: 0, spin: r - 0x80, typ: 0x55 });
            c.timer = (c.timer - 1).max(0);
        }
        c.lit = false;
        if self.boss.state >= 2 {
            self.boss.flash -= 1;
            if self.boss.flash < 0 {
                self.boss.flash = 0;
                if self.boss.state == 2 {
                    c.rec.state = 7;
                }
            } else {
                c.lit = self.boss.blink;
            }
        }
        if c.health < 10 && self.boss.state == 2 {
            c.set_script(0x74 / 2);
            c.rec.state = 4;
            c.flags &= !0x100;
            let e = self.shape(c.id).ellipsoid(0);
            let at = [0, 1, 2].map(|k| c.pos[k] + e.offset[k] as i32);
            for _ in 0..5 {
                let spin = self.rand() as i32 - 0x80;
                self.spawns.push((at, 0x23, 0xE, spin));
            }
            self.boss.jingles.push((-2, c.pos));
            self.boss.state = 3;
        }
        if self.boss.state >= 3 && self.clock.is_multiple_of(7) {
            let f = c.angles[1];
            let at = [c.pos[0] - 3 * (sin(f - 0x400) >> 2), c.pos[1] - 0x400, c.pos[2] - 3 * (sin(f) >> 2)];
            let spin = self.rand() as i32 - 0x80;
            self.spawns.push((at, 0x11, 10, spin));
        }
    }

    /// 0x41DF70, Zurg's boat (0x1B): every 200 ticks it lobs a shot (0x5C) ahead from its
    /// bow.
    fn zurg_boat(&mut self, c: &mut Creature) {
        c.timer -= 1;
        if c.timer < 0 {
            c.timer = 200;
            let at = Self::local(c, [0, -500, -300]);
            let f = c.angles[1];
            self.particles.push(ParticleRequest { pos: at, vel: [sin(f) >> 2, -0xC00, cos(f) >> 2], gravity: 0x80, angle: 0, spin: 0, typ: 0x5C });
        }
    }

    /// 0x405D20 mode 1 on slot `i`: its death (animation, coin, burst).
    pub fn kill_slot(&mut self, i: usize) {
        let Some(mut c) = self.slots.get(i).cloned() else { return };
        self.kill(i, &mut c);
        self.slots[i] = c;
    }

    /// Starts slot `i` again from its record (0x406CD0 with 0: keeps a dead one dead).
    pub fn restart(&mut self, i: usize) {
        let mut c = self.slots[i].clone();
        self.reset(&mut c, false);
        self.slots[i] = c;
    }

    /// 0x416A60, a sheep: touched, it's found (the count 0x52B7D8 +1, sparks 0x410410, sound
    /// 0x20) and goes.
    fn sheep(&mut self, c: &mut Creature) {
        if c.flags & 0x200 == 0 {
            return;
        }
        self.sheep += 1;
        let at = [c.pos[0], c.pos[1] - 0x2000, c.pos[2]];
        for _ in 0..5 {
            let r = self.rand() as i32;
            let o = ((r & 0x3F) - 0x20) * 0x32 * 2;
            let spin = self.rand() as i32 - 0x80;
            self.spawns.push(([at[0] + o, at[1] + o, at[2] + o], 0x29, 0xF, spin));
        }
        self.sounds.push((0x20, c.pos));
        self.remove(c);
    }

    /// 0x406220, ZURG3 (the flying robot): leans into its sideways speed and hums (sound
    /// 0x2C); while Buzz is in its zone it fires every 360 ticks: the attack animation at 200,
    /// a shot from its right side at 182 and its left at 166 (particle 0x26, sound 0xD), back to
    /// hovering at 136, and a dodge to one side when the count runs out.
    fn zurg3(&mut self, slot: usize, c: &mut Creature, s: &Step) {
        let side = s.side.clamp(-0x100, 0x100);
        c.angles[2] -= (c.angles[2] + side) / 16;
        if c.hurt >= 0 {
            self.owned_sounds.push((0x2C, c.pos, slot));
        }
        if s.chase & 1 == 0 {
            if c.anim == 1 {
                self.set_anim(c, 0, 7);
            }
            c.timer = 0x104;
            return;
        }
        let before = c.timer;
        c.timer -= 1;
        if c.timer < 0 {
            c.timer = 0x168;
            let a = c.angles[1] + if self.rand() < 0x80 { -0x200 } else { 0x200 };
            c.vel[0] = sin(a) >> 4;
            c.vel[2] = sin(a + 0x400) >> 4;
            return;
        }
        let f = c.angles[1];
        if before > 200 && c.timer < 0xC9 {
            self.set_anim(c, 1, 8);
        }
        let shot = |dx: i32, dz: i32| ParticleRequest {
            pos: [c.pos[0] + (dx >> 2), c.pos[1], c.pos[2] + (dz >> 2)],
            vel: [sin(f) >> 4, -0x200, sin(f + 0x400) >> 4],
            gravity: 0x40,
            angle: 0,
            spin: 0x80,
            typ: 0x26,
        };
        if before > 0xB6 && c.timer < 0xB7 {
            self.particles.push(shot(sin(f + 0x400), sin(f - 0x800)));
            self.sounds.push((0xD, c.pos));
        }
        if before > 0xA6 && c.timer < 0xA7 {
            self.particles.push(shot(sin(f - 0x400), sin(f)));
            self.sounds.push((0xD, c.pos));
        }
        if before > 0x88 && c.timer < 0x89 {
            self.set_anim(c, 0, 7);
        }
    }

    /// 0x4076F0.
    fn think(&mut self, c: &mut Creature, world: &World, buzz: [i32; 3]) -> Step {
        let r = c.rec;
        let (zx, zz) = (r.zone[0] as i32, r.zone[1] as i32);
        let (zs, zc) = (sin(r.zone_angle as i32 * 8) >> 2, cos(r.zone_angle as i32 * 8) >> 2);
        let mut dec = [(r.decel[0] as i32) << 1, (r.decel[1] as i32) << 1];
        let mut step = Step::default();
        let mut walk = c.flags & 4 != 0;
        let mut jump_mode = 0u8;
        let in_zone = |p: [i32; 3], home: [i32; 3]| {
            let (dx, dz) = ((p[0] - home[0]) >> 5, (p[2] - home[2]) >> 5);
            let u = ((dx * zc - dz * zs) >> 7) + zx * 0x100;
            let v = ((dz * zc + dx * zs) >> 7) + zz * 0x100;
            (u as u32) < (zx << 9) as u32 && (v as u32) < (zz << 9) as u32
        };
        // +0x7C: knocked back (by a hit): stops quickly.
        if c.knock > 0 {
            c.knock -= 1;
            dec = [0x20, 0x20];
        }
        c.wait -= 1;
        if c.wait < 0 {
            self.run_script(c, buzz, in_zone(buzz, c.home), &mut jump_mode);
        }
        if let Some(s) = self.script(c) {
            match s.get(c.pc).copied().unwrap_or(0) {
                6 => {
                    jump_mode = 1;
                    c.wait = 0x14;
                }
                7 => {
                    jump_mode = 2;
                    c.wait = 0x14;
                }
                0x20 => c.target_facing = (atan(c.pos[0] - buzz[0], c.pos[2] - buzz[2]) - 0x800) & 0xFFF,
                _ => {}
            }
        }
        c.jump = jump_mode;
        let mut goal = c.target;
        if r.face != 0 {
            c.target_facing = (r.face as i32) << 4;
        }
        // Chasing: Buzz inside the zone.
        if c.flags & 8 != 0 && in_zone(buzz, c.home) {
            step.chase |= 1;
            goal = [buzz[0], goal[1], buzz[2]];
            let (above, keep, approach) = match c.id {
                4 | 0x14 => (0x5000, 0x4B0, true),
                5 => (0x800, 0, false),
                0xF => (-1, 0x5DC, false),
                _ => (0x800, 0, true),
            };
            if (c.pos[1] - buzz[1]).abs() < 0x10000 {
                if above != -1 {
                    goal[1] = buzz[1] - above;
                }
            } else {
                goal[1] = c.home[1];
            }
            if approach {
                c.wait += 1;
            }
            walk = true;
            if keep != 0 {
                let (dx, dz) = ((c.pos[0] - buzz[0]) >> 5, (c.pos[2] - buzz[2]) >> 5);
                if dx * dx + dz * dz < keep * keep {
                    let a = atan(dx, dz);
                    c.pos[0] -= (c.pos[0] - (sin(a) * keep >> 9) - buzz[0]) >> 4;
                    c.pos[2] -= (c.pos[2] - (cos(a) * keep >> 9) - buzz[2]) >> 4;
                }
            }
        }
        // Velocity in the creature's frame.
        let f = c.angles[1];
        let (fs, fc) = (sin(f) >> 2, cos(f) >> 2);
        let mut side = (c.vel[0] * (sin(0x400 - f) >> 2) + c.vel[2] * (sin(-f) >> 2)) >> 12;
        let mut fwd = (c.vel[2] * (sin(0x400 - f) >> 2) - c.vel[0] * (sin(-f) >> 2)) >> 12;
        if c.flags & 0x400 == 0 {
            let toward = |v: i32, by: i32| if v < 0 { (v + by / 4).min(0) } else { (v - by / 4).max(0) };
            side = toward(side, dec[0]);
            fwd = toward(fwd, dec[1]);
        }
        if walk {
            if r.turn != 0 {
                c.target_facing = (atan(c.pos[0] - goal[0], c.pos[2] - goal[2]) - 0x800) & 0xFFF;
            }
            if r.accel == 0xFF {
                if fwd > -(r.speed as i32) * 8 {
                    fwd -= (dec[1] + 0x20) / 4;
                }
            } else if fwd < (r.speed as i32) * 16 {
                fwd += (r.accel as i32 + dec[1]) / 4;
            }
            step.flags |= 1;
        }
        let mut d = (c.target_facing - c.angles[1]) & 0xFFF;
        if d > 0x7FF {
            d -= 0x1000;
        }
        c.angles[1] = (c.angles[1] + ((r.turn as i32 * d) >> 7)) & 0xFFF;
        c.vel[0] = (fwd * fs + side * fc) >> 12;
        c.vel[2] = (fwd * fc - side * fs) >> 12;
        if r.speed == 0 {
            side = 0;
            fwd = 0;
            c.vel[0] = 0;
            c.vel[2] = 0;
        } else {
            c.pos[0] += c.vel[0];
            c.pos[2] += c.vel[2];
        }
        // Keep inside the zone.
        let (dx, dz) = (c.pos[0] - c.home[0], c.pos[2] - c.home[2]);
        let u = (dx * zc - dz * zs) >> 12;
        let v = (dz * zc + dx * zs) >> 12;
        let (cu, cv) = (u.clamp(-zx * 0x100, zx * 0x100), v.clamp(-zz * 0x100, zz * 0x100));
        if (cu, cv) != (u, v) {
            c.pos[0] = ((cu * zc + cv * zs) >> 12) + c.home[0];
            c.pos[2] = ((cv * zc - cu * zs) >> 12) + c.home[2];
        }
        // Height: fall onto the floor, or hover.
        let y0 = c.pos[1];
        let floor_at = |p: [i32; 3]| world.floor_below(p[0] as f32 / 32.0, p[2] as f32 / 32.0, (p[1] - 0x1900) as f32 / 32.0).map(|f| (f * 32.0) as i32);
        if c.flags & 0x10 == 0 {
            c.vel[1] = (c.vel[1] + 0x40).min(0x800);
            let ny = y0 + c.vel[1];
            if c.flags & 0x20 == 0 {
                match floor_at([c.pos[0], ny, c.pos[2]]) {
                    Some(f) => c.floor = f,
                    None => {}
                }
            } else {
                c.floor = c.home[1];
            }
            if c.floor - 0x200 < ny {
                if jump_mode == 0 {
                    c.pos[1] = ny + ((c.floor - ny) >> 2).max(-0x400);
                } else {
                    // Landed from a jump: on with the script.
                    c.wait = 0;
                    c.pc += 2;
                    if c.flags & 0x400 != 0 {
                        c.flags &= !0x400;
                        if jump_mode == 2 {
                            c.vel = [0; 3];
                            step.flags |= 2;
                            c.pos[1] = c.floor;
                            return self.finish(c, step, side, fwd);
                        }
                    }
                    c.pos[1] = c.floor;
                }
                c.vel[1] = 0;
                step.flags |= 2;
            } else {
                c.pos[1] = ny;
            }
        } else if c.flags & 0x20 == 0 || c.flags & 0x40 != 0 {
            let ny = y0 + ((goal[1] - y0) >> 6);
            c.pos[1] = match floor_at([c.pos[0], ny, c.pos[2]]) {
                Some(f) if ny >= f => ny + ((f - ny) >> 2).max(-0x400),
                _ => ny,
            };
        } else {
            c.pos[1] = c.home[1];
        }
        self.finish(c, step, side, fwd)
    }

    /// The animation clock, then the report for the type routine.
    fn finish(&self, c: &mut Creature, mut step: Step, side: i32, fwd: i32) -> Step {
        let rate = c.rates[if step.flags & 2 != 0 { 0 } else { 1 }] as i32;
        let adv = if rate < 0 { -rate * 0x200 } else { fwd * rate / 2 };
        if adv > 0 {
            let mut adv = adv as u32;
            while adv > 0x10000 {
                step.flags |= 4;
                self.step_anim(c);
                adv -= 0x10000;
            }
            let t0 = c.time;
            c.time = c.time.wrapping_add(adv);
            if (c.time ^ t0) & 0xFFFF_0000 != 0 {
                step.flags |= 4;
                self.step_anim(c);
            }
        }
        step.side = side;
        step.forward = fwd;
        step
    }

    fn script(&self, c: &Creature) -> Option<&Vec<i16>> {
        if c.dying_script {
            return self.death_script.as_ref();
        }
        self.scripts.get(c.rec0.behaviour as usize).and_then(|s| s.as_ref())
    }

    /// 0x405D20 mode 1: the death animation (types 3 / 0xE / 0x18: animation 2 for 94 ticks,
    /// type 4: 62 ticks; others go at once), a coin dropped.
    fn kill(&mut self, slot: usize, c: &mut Creature) {
        let e = self.ellipsoid_of(c);
        let (s, k) = (sin(c.angles[1] - 0x800) >> 2, sin(c.angles[1] - 0x400) >> 2);
        let (ox, oz) = (e.offset[0] as i32, e.offset[2] as i32);
        let centre = [((ox * k + oz * s) >> 12) + c.pos[0], e.offset[1] as i32 + c.pos[1], ((oz * k - ox * s) >> 12) + c.pos[2]];
        let burst = match c.id {
            3 | 4 | 0xE | 0x14 | 0x18 => 3,
            0x1B => 4,
            0x10 | 0x19 | 0x36 => 6,
            0x21 | 0x29 | 0x2E => -5,
            _ => 0,
        };
        self.events.push(Event::Died { slot, pos: c.pos, coin: c.flags & 0x800 == 0, centre, burst });
        c.flags |= 0x800;
        c.rates = [-32, -32];
        c.vel = [0, -0x400, 0];
        let (anim, seq, t) = match c.id {
            3 | 0xE => (Some(2), 1, -0x5E),
            0x18 => {
                c.vel[1] = -0xB00;
                c.respawn = 10000;
                (Some(2), 1, -0x5E)
            }
            4 => (Some(2), 7, -0x3E),
            0x14 => (Some(1), 20, -0x46),
            _ => (None, 0, -1),
        };
        if let Some(a) = anim {
            self.set_anim(c, a, seq);
        }
        c.hurt = t;
        self.sounds.push((10, c.pos));
    }

    /// 0x405D20 mode 2: gone; back after its respawn time, or never if it has none.
    fn remove(&mut self, c: &mut Creature) {
        if c.respawn == 0 {
            c.id = 0;
        }
        c.flags &= !3;
        c.health = 0;
    }

    /// 0x408A60 `HitCreature(c, angle, kind)`: knock-back, then damage by the hit table; at 0
    /// health it runs the death script and dies.
    pub fn hit(&mut self, slot: usize, angle: i32, kind: usize) {
        let mut c = self.slots[slot].clone();
        let [typ, knock, damage] = self.hits.get(kind).copied().unwrap_or([0; 3]);
        if typ == 2 && c.rec.state & 1 == 0 {
            return;
        }
        if c.rec.decel[1] != 0xFF {
            c.vel[0] = sin(angle) >> 5;
            c.vel[2] = cos(angle) >> 5;
            c.flags &= !4;
        }
        if c.knock == 0 {
            c.knock = knock as i32;
            if typ != 0 {
                if c.health < 100 {
                    c.health -= damage as i32;
                    self.sounds.push((9, c.pos));
                }
                if c.health < 1 {
                    c.flags &= 0xFE63;
                    c.health = 999;
                    c.wait = 0;
                    c.pc = 0;
                    c.dying_script = true;
                    self.kill(slot, &mut c);
                }
            }
        }
        self.slots[slot] = c;
    }

    /// 0x407440: Buzz touching creatures. His centre (0x1CC0 above his feet) against first the
    /// creature's sphere, then its ellipsoid, each grown by his reach (150 level units, 400
    /// spinning or just after a stomp). His attack (spin, stomp) hits them; harmful ones (flag
    /// 0x100) hurt him when he isn't attacking; others just push him away.
    pub fn contact(&mut self, player: &mut Player) {
        let (kind, reach) = player.attack();
        let b = player.pos_obj();
        let chest = [b[0], b[1] - 0x1CC0, b[2]];
        for i in 0..self.slots.len() {
            let c = &self.slots[i];
            if c.id == 0 || !c.active || c.health <= 0 || c.hurt < 0 || c.knock < 0 {
                continue;
            }
            let sh = self.shape(c.id);
            let sp = [0, 1, 2].map(|j| (sh.sphere[j] as i32 + c.pos[j] - chest[j]) >> 5);
            let rr = sh.sphere[3] as i32 + reach;
            if c.hit_size.is_none() && sp[0] * sp[0] + sp[1] * sp[1] + sp[2] * sp[2] >= rr * rr {
                continue;
            }
            let (centre, e) = self.ellipsoid_at(i);
            let (s, k) = (sin(c.angles[1] - 0x800) >> 2, sin(c.angles[1] - 0x400) >> 2);
            let (x, z) = (centre[0] - chest[0], centre[2] - chest[2]);
            let lz = ((k * z - s * x) >> 12) * e.scale[2] as i32 >> 13;
            let lx = ((s * z + k * x) >> 12) * e.scale[0] as i32 >> 13;
            let ly = (centre[1] - chest[1]) * e.scale[1] as i32 >> 13;
            let r = e.radius as i32 + reach;
            if lx * lx + ly * ly + lz * lz >= r * r || c.flags & 0x80 == 0 {
                continue;
            }
            let angle = atan(b[0] - c.pos[0], b[2] - c.pos[2]);
            // A stomp only lands from above.
            let k = if kind != 0 && c.rec.state & 2 != 0 && !(kind == 5 && ly < 0) { kind as usize } else { 0 };
            let mode = if c.flags & 0x100 != 0 && kind == 0 {
                3
            } else if c.health == 0x66 || k != 0 {
                0
            } else {
                1
            };
            self.slots[i].flags |= 0x200;
            self.hit(i, (angle + 0x800) & 0xFFF, k);
            player.hurt(angle, mode);
        }
    }

    /// The interpreter part of 0x4076F0: runs until an opcode it doesn't handle (a yield).
    fn run_script(&mut self, c: &mut Creature, buzz: [i32; 3], buzz_in_zone: bool, _jump: &mut u8) {
        let Some(s) = self.script(c).cloned() else { return };
        let start = c.pc;
        let mut p = c.pc;
        let mut cond = false;
        let w = |k: usize| s.get(k).copied().unwrap_or(0) as i32;
        let r = c.rec;
        let (zx, zz) = (r.zone[0] as i32, r.zone[1] as i32);
        let (zs, zc) = (sin(r.zone_angle as i32 * 8) >> 2, cos(r.zone_angle as i32 * 8) >> 2);
        for _ in 0..256 {
            let op = w(p);
            let next = match op {
                1 => {
                    c.wait = w(p + 1);
                    p + 2
                }
                2 => {
                    c.wait = (w(p + 1) & self.rand() as i32) + w(p + 2);
                    p + 3
                }
                3 => {
                    let mut rr = w(p + 1);
                    if rr < 0 {
                        rr = (zx.min(zz) * -0x40) / rr;
                    }
                    let a = ((self.rand() as i32) << 8 | self.rand() as i32) & 0xFFF;
                    c.target = [(sin(a - 0x800) * rr >> 9) + c.home[0], c.home[1], (sin(a - 0x400) * rr >> 9) + c.home[2]];
                    p + 2
                }
                5 => {
                    let m = w(p + 1) * 0x20;
                    let pick = |s: &mut Self, half: i32| {
                        if half * 0x100 < m {
                            0
                        } else {
                            let v = (s.rand() as i32 * half * 2).clamp(m, half * 0x200 - m);
                            v - half * 0x100
                        }
                    };
                    let u = pick(self, zx);
                    let v = pick(self, zz);
                    c.target = [((u * zc + v * zs) >> 12) + c.home[0], c.home[1], ((v * zc - u * zs) >> 12) + c.home[2]];
                    p + 2
                }
                8 => {
                    if cond {
                        (p as i32 + w(p + 1) + 1) as usize
                    } else {
                        p + 2
                    }
                }
                0xB => p + 2,
                0xC => {
                    c.flags = c.flags & w(p + 1) as u16 | w(p + 2) as u16;
                    p + 3
                }
                0xD => {
                    self.set_anim(c, w(p + 1) as u16, w(p + 2).max(0) as usize);
                    p + 3
                }
                0xE => {
                    cond = (w(p + 1) as u8 & self.rand()) == 0;
                    p + 2
                }
                0xF => {
                    c.rec.speed = ((w(p + 1) as u16) >> 4) as u8;
                    p + 2
                }
                0x10 => {
                    let (dx, dz) = ((c.pos[0] - buzz[0]) >> 5, (c.pos[2] - buzz[2]) >> 5);
                    cond = dx * dx + dz * dz < w(p + 1) * w(p + 1);
                    p + 2
                }
                0x11 => {
                    cond = buzz_in_zone;
                    p + 1
                }
                0x12 => {
                    let (dx, dz) = ((c.pos[0] - c.target[0]) >> 5, (c.pos[2] - c.target[2]) >> 5);
                    cond = dx * dx + dz * dz < w(p + 1) * w(p + 1);
                    p + 2
                }
                0x13 => p + 2,
                0x14 => {
                    c.rec.accel = w(p + 1) as u8;
                    p + 2
                }
                0x15 => p + 1,
                0x16 => {
                    c.rates = [w(p + 1) as i8, w(p + 2) as i8];
                    p + 3
                }
                0x17 => {
                    self.sounds.push((w(p + 1) as u16, c.pos));
                    p + 2
                }
                0x18 => {
                    let h = w(p + 1);
                    let t = (-h >> 5).max(1);
                    c.vel = [(c.target[0] - c.pos[0]) / t, h, (c.target[2] - c.pos[2]) / t];
                    c.flags |= 0x400;
                    p + 2
                }
                0x19 => {
                    match w(p + 1) {
                        1 => c.target_facing = (atan(c.pos[0] - c.target[0], c.pos[2] - c.target[2]) - 0x800) & 0xFFF,
                        2 => c.target_facing = (atan(c.pos[0] - buzz[0], c.pos[2] - buzz[2]) - 0x800) & 0xFFF,
                        _ => {}
                    }
                    p + 2
                }
                0x1A => {
                    c.vel[0] = w(p + 1);
                    c.vel[2] = w(p + 2);
                    p + 3
                }
                0x1B => {
                    c.target[0] = buzz[0];
                    if c.flags & 0x20 == 0 {
                        c.target[1] = buzz[1];
                    }
                    c.target[2] = buzz[2];
                    p + 1
                }
                0x1C => {
                    c.vel[1] = w(p + 1);
                    p + 2
                }
                0x1D => {
                    c.rec.turn = w(p + 1) as u8;
                    p + 2
                }
                0x1E => {
                    c.rec.state = w(p + 1) as u8;
                    p + 2
                }
                0x1F => {
                    let mut a = atan(buzz[0] - c.pos[0], buzz[2] - c.pos[2]);
                    let mut rr = w(p + 1);
                    if rr < 1 {
                        a = (a + 0x80 + rr) & 0xFFF;
                        rr = 1;
                    }
                    c.target[0] = sin(a) * rr + c.pos[0];
                    c.target[2] = cos(a) * rr + c.pos[2];
                    p + 2
                }
                0x21 => {
                    c.timer = w(p + 1);
                    p + 2
                }
                0x22 => p + 1,
                0x23 => {
                    let a = atan(buzz[0] - c.pos[0], buzz[2] - c.pos[2]);
                    c.target[0] = c.pos[0] - sin(a) * w(p + 1);
                    c.target[2] = c.pos[2] - cos(a) * w(p + 1);
                    p + 2
                }
                -1 => (p as i32 - w(p + 1)).max(0) as usize,
                _ => {
                    // A yield (4, 6, 7, …): stop here; when the script resumes from it, step
                    // past it.
                    if p == start {
                        p += 1;
                    }
                    break;
                }
            };
            if next == p {
                if p == start {
                    p += 1;
                }
                break;
            }
            p = next;
        }
        c.pc = p;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_decode() {
        // Level 1, slot 9: a ZURG1 hopping robot.
        let p = [1, 10, 0, 4, 100, 0x80, 0x01, 25, 0, 155, 0, 0, 0, 7, 0, 128, 64, 128, 64];
        let r = Record::from_params(&p);
        assert_eq!((r.behaviour, r.turn, r.health, r.respawn, r.flags, r.zone, r.speed), (1, 10, 4, 100, 0x180, [25, 155], 64));
    }
}
