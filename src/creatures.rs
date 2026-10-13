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

use crate::formats::exe::Exe;
use crate::formats::packets::Spawn;
use crate::game::World;

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
    wait: i32,
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
}

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
        let mut c = Creatures { slots: Vec::new(), scripts, seqs, random, rpos: 0, sounds: Vec::new() };
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
            };
            c.reset(&mut cr, true);
            c.slots.push(cr);
        }
        c
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
            let near = 400;
            if c.flags & 1 == 0 && d2 >= near * near || c.flags & 0x2000 != 0 {
                self.slots[i] = c;
                continue;
            }
            c.active = true;
            let s = self.think(&mut c, world, buzz);
            if c.hurt > 0 {
                c.hurt -= 1;
            }
            self.slots[i] = c;
            steps.push((i, s));
        }
        steps
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
        self.scripts.get(c.rec0.behaviour as usize).and_then(|s| s.as_ref())
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
