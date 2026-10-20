//! Level 4 (start-up 0x41C190, frame script 0x41C640).
//!
//! - Four hanging platforms (movers / specials 1–4, turned 0xC00, 0xC00, 0, 0) follow path
//!   programs (0x4F1BA4, 0x4F1BC8, 0x4F1BF4, 0x4F1C1C) and sway (0x49EC00 mode 2, ±0xE0, /16);
//!   they stop while Buzz stands on one or on the platform hanging under it (movers 7, 8, 9,
//!   6), which 0x41BEE0 keeps under its special 0x14 + k with the rope stretched between.
//! - Movers 0xE–0x11 (specials 0x29–0x2C) run path programs 0x4F1C64 / 0x4F1C40.
//! - Three presses (specials 0x24–0x26, movers 10 / 12 / 11; 0x41BC20) rise, hold and slam
//!   down while Buzz is near.
//! - Stomping surface 0x24 lowers the drawbridge (mover 0x1A, specials 0x1C / 0x1D) for 240
//!   ticks. Stomping on the buttons (movers 0x16–0x18) presses them (sparkles 4–6) and lifts
//!   the lift (mover 0x15, special 0x44) to the button's height.
//! - Water in the gully (mode 2 at −0x7A78).
//! - Characters: a timed challenge (slot 0x1A, list 0x1F: find five things in time, token 2),
//!   Mr Potato Head (slot 0x15: the disk launcher), Hamm (0x14), the hint character (0x11),
//!   the finder (slot 0x12, list 0x20: five little tykes, token 1).
//! - The drill boss (slot 0x18, list 0x22, token 4).
//!
//! - The paint puzzle (token 3): see [`Paint`]. The crane and a crate are push boxes
//!   (0x4F1C88).
//!
//! Not ported: the dripping and lights.

use crate::boxes::Boxes;
use crate::creatures::Creatures;
use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::game::{Player, World};
use crate::script::{near, touched, Common, PathRunner, Script, Tilt};
use crate::specials::Specials;
use glam::Vec3;

const INFO_TABLE: u32 = 0x4F1CA4;
const HINTS: u32 = 0x4F1CB8;
const CHALLENGE_ASK: u32 = 0x4F1710;
const CHALLENGE_HURRY: u32 = 0x4F178C;
const CHALLENGE_DONE: u32 = 0x4F17C0;
const POTATO_TEXTS: [u32; 3] = [0x4F1818, 0x4F18A0, 0x4F192C];
const FINDER_ASK: u32 = 0x4F15D4;
const FINDER_THANKS: u32 = 0x4F1650;
const DRILL_TEXT: u32 = 0x4F169C;

const CHALLENGE: (usize, i16) = (0x1A, 0x1F);
const POTATO: (usize, i16) = (0x15, 0x1E);
const HAMM: (usize, i16) = (0x14, 0x1D);
const HINT: (usize, i16) = (0x11, 0x23);
const FINDER: (usize, i16) = (0x12, 0x20);
const DRILL: (usize, i16) = (0x18, 0x22);

/// The hanging platforms: mover = special + 1, its yaw, the platform under it, which axis its
/// low rig follows (0x41BEE0's third argument) and the rope's length.
const RIGS: [(i32, i32, i32, bool, i32); 4] = [(1, 0xC00, 7, false, 0x21980), (2, 0xC00, 8, true, 48000), (3, 0, 9, false, 72000), (4, 0, 6, true, 0xCE40)];
const RIG_PATHS: [u32; 4] = [0x4F1BA4, 0x4F1BC8, 0x4F1BF4, 0x4F1C1C];
/// Movers 0xE–0x11 with their specials and programs.
const SHUTTLES: [(i32, i32, u32); 4] = [(0xE, 0x29, 0x4F1C64), (0xF, 0x2A, 0x4F1C64), (0x10, 0x2B, 0x4F1C40), (0x11, 0x2C, 0x4F1C40)];
/// The presses: special, its mover, cycle length.
const PRESSES: [(i32, i32, i32); 3] = [(0x24, 10, 300), (0x25, 0xC, 0x147), (0x26, 0xB, 0x15D)];
/// The drill's lanes (0x4F1CCC…): x and z thresholds, then the pairs it snaps between.
const LANES_X: [i32; 4] = [-0x52354, -0x26A54, -0x1F994, 0xC02C];
const LANES_Z: [i32; 4] = [-0x791DE, -0x4D95E, -0x46B9E, -0x1B29E];
const SNAP_X: [i32; 4] = [-0x52354, -0x26A53, -0x1F994, 0xC02D];
const SNAP_Z: [i32; 4] = [-0x791DE, -0x4D95D, -0x46B9E, -0x1B29D];

pub struct Level4 {
    tilts: [Tilt; 4],
    rigs: [PathRunner; 4],
    shuttles: [PathRunner; 4],
    path_flags: u16,
    /// Presses: their cycle count, fall speed and drop (0x52F7A4… / 0x52F7B8… / 0x52F7AC…).
    presses: [(i32, i32, i32); 3],
    /// The drawbridge (0x52F7E0: 0 up, 1 lowering, then counting).
    bridge: i32,
    /// The buttons pressed (0x52F784 bits 1–4) and the lift's speed (0x52F7DC).
    buttons: u32,
    lift_v: i32,
    /// The drill fight (0x52F804) and the drill's lane state (0x52F7CC).
    lanes: u32,
    /// The timed challenge's clock tick (every 64 ticks).
    in_water: bool,
    paint: Paint,
    common: Common,
}

/// The paint puzzle (in 0x41C640). The crane (mover 0, pushed along list 3 like a box)
/// carries the paint (slot 5, creature PAINT): its colour is the slot's light, its fill the
/// slot's height. Stops 5–7 of list 3 are under the red, blue and yellow taps; stomping the
/// matching button (surfaces 0x21–0x23) there pours that colour. Two different colours mix;
/// the mix goes to its target at stops 1–3 (purple, orange, green). The same colour twice
/// drains away. All three targets done: token 3.
#[derive(Default)]
struct Paint {
    /// 0x52F7C0: the button stomped (surface − 0x1F); 0x52F7D8: the pour's countdown (60).
    colour: i32,
    timer: i32,
    /// 0x52F7C4: the crane's stop (1-based, 0 between stops).
    stop: i32,
    /// 0x52F80C / 0x52F764: the first and second colours in (1 red, 2 blue, 3 yellow).
    first: i32,
    second: i32,
    /// 0x52F868: a third pour was refused.
    full: bool,
    /// 0x52F86C: the fill height; 0x52F760: draining (32 down to 0).
    fill: i32,
    drain: i32,
    /// 0x52F770: targets done (bits 1, 2, 4).
    done: u32,
}

/// 0x4F1B64: every 8 ticks one entry: done targets show their lit sign (special) or swap
/// back (negative: hide −special, show −special + 3).
const SIGNS: [(u32, i32); 8] = [(1, 0x30), (2, 0x31), (4, 0x32), (0, 0), (1, -0x30), (2, -0x31), (4, -0x32), (0, 0)];
const PAINT: usize = 5;

impl Level4 {
    pub fn new(world: &World, specials: &mut Specials, exe: Option<&Exe>) -> Level4 {
        for (m, yaw, ..) in RIGS {
            world.turn_mover(m, [0, yaw, 0]);
        }
        for id in [0x33, 0x34, 0x35, 0x21, 0x22, 0x23] {
            specials.set_scale(id, [0; 3]);
        }
        for m in [0xA, 0xC, 0xB] {
            world.set_mover(m, false);
        }
        specials.turn_from_own(0x1C, [0, 0, 0x266]);
        specials.turn_from_own(0x1D, [0, 0, 0x266]);
        let load = |va: u32| exe.map(|e| PathRunner::load(e, va)).unwrap_or_default();
        Level4 {
            tilts: Default::default(),
            rigs: RIG_PATHS.map(load),
            shuttles: SHUTTLES.map(|(.., va)| load(va)),
            path_flags: 0,
            presses: [(0, 0, 0), (0x21, 0, 0), (0x42, 0, 0)],
            bridge: 0,
            buttons: 0,
            lift_v: 0,
            lanes: 0,
            in_water: false,
            paint: Paint::default(),
            // 0x503A2A: level 4's missing item (9) and gift (bit 4, the disk launcher).
            // Token 3 is the paint puzzle's (0x41C640 reveals it once 0x52F770 = 7).
            common: Common { given: [false; 5], item: 9, show_collectables: Some(false), ..Default::default() },
        }
    }

    pub fn set_story(&mut self, exe: &Exe, lists: &[PointList]) {
        self.story.hints = (0..5).filter_map(|k| exe.at(HINTS + 4 * k, 4).ok().map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect();
        let mut texts = vec![crate::script::HAMM_THANKS, crate::script::HAMM_ASK, crate::script::ALL_TOKENS, CHALLENGE_ASK, CHALLENGE_HURRY, CHALLENGE_DONE, FINDER_ASK, FINDER_THANKS, DRILL_TEXT];
        texts.extend(POTATO_TEXTS);
        self.load_story(exe, lists, &texts, Some(INFO_TABLE));
    }

    /// The paint puzzle: see [`Paint`].
    fn paint(&mut self, world: &World, specials: &mut Specials, player: &Player, cr: &mut Creatures) {
        let Some(crane) = world.mover_pos(0).map(|p| [p.x, p.y, p.z].map(|v| (v * 32.0) as i32)) else { return };
        if cr.slots.len() <= PAINT {
            return;
        }
        let buzz = player.pos_obj();
        cr.slots[PAINT].pos = crane;
        cr.slots[PAINT].lit = true;
        let p = &mut self.paint;
        // The same colour twice: it drains.
        if p.timer < 1 && matches!(p.first + p.second * 3, 4 | 8 | 12) {
            p.drain = 0x20;
            p.first = 0;
            p.second = 0;
            self.common.sounds.push((0xFFFA, buzz));
        }
        let p = &mut self.paint;
        p.stop = 0;
        let mut poured = false;
        if p.drain > 0 {
            p.full = false;
            cr.slots[PAINT].scale_y = Some(p.fill * p.drain / 32);
            p.drain -= 1;
        } else {
            let surface = player.surface.unwrap_or(0) as i32;
            if player.on_floor && player.stomp != 0 && (0x20..0x24).contains(&surface) && p.timer < 1 {
                p.colour = surface - 0x1F;
                p.timer = 0x3C;
                for k in 0..3 {
                    self.common.put_out.push((k, 1));
                }
            }
            let p = &mut self.paint;
            if let Some(stops) = self.common.story.lists.get(&3) {
                for (k, s) in stops.iter().enumerate() {
                    if ((crane[0] >> 5) - s[0]).abs() < 200 {
                        p.stop = k as i32 + 1;
                    }
                }
            }
            if p.timer == 0x3C {
                poured = true;
                if p.stop > 4 && p.stop - 3 == p.colour {
                    if p.first == 0 {
                        p.first = p.stop - 4;
                    } else if p.second == 0 {
                        p.second = p.stop - 4;
                        p.full = false;
                    } else {
                        p.full = true;
                    }
                }
            }
        }
        let p = &mut self.paint;
        if p.timer > 0 {
            // The poured drop (special colour + 0x1F): squashed in, whole, then gone.
            let drop = p.colour + 0x1F;
            if p.colour > 1 {
                let s = if poured || p.timer > 0x34 { [0x1000, (0x3C - p.timer) * 0x200, 0x1000] } else if p.timer > 0xB { [0x1000; 3] } else { [0; 3] };
                specials.set_scale(drop, s);
            }
            if !p.full && p.stop - 3 == p.colour && (5..=7).contains(&p.stop) {
                let t = p.timer;
                let c = &mut cr.slots[PAINT];
                let mixed = match p.second * 3 - 1 + p.first {
                    0 => Some((0x3C, Some([0x680, 0, 0]))),
                    1 => Some((0x3C, Some([0, 0, 0x600]))),
                    2 => Some((0x3C, Some([0x600, 0x580, 0]))),
                    3 | 7 | 0xB => Some((0x7C, None)),
                    4 => Some((0x7C, Some([(0x3C - t) * 0xE, 0, t * 8 + 0x420]))),
                    5 => Some((0x7C, Some([0x600, t * 10 + 0x328, 0]))),
                    6 => Some((0x7C, Some([t * 0xC + 0x3B0, 0, (0x3C - t) * 0x12]))),
                    8 => Some((0x7C, Some([(t * 3 + 0xC) * 8, t * 6 + 0x418, 0]))),
                    9 => Some((0x7C, Some([t * 2 + 0x608, (0x3C - t) * 0xC, 0]))),
                    10 => Some((0x7C, Some([0, (0x3C - t) * 0x10, (t * 3 + 0xC) * 8]))),
                    _ => None,
                };
                if let Some((top, light)) = mixed {
                    if let Some(l) = light {
                        c.light = l;
                    }
                    p.fill = (top - t) * 0x20;
                }
                c.scale_y = Some(p.fill);
            }
            p.timer -= 1;
        }
        // At a target with its mix: done.
        let mix = p.second * 3 - 1 + p.first;
        let hit = match p.stop {
            1 => matches!(mix, 4 | 6),
            2 => matches!(mix, 5 | 9),
            3 => matches!(mix, 8 | 10),
            _ => false,
        };
        if hit {
            p.done |= 1 << (p.stop - 1);
            p.drain = 0x20;
            p.first = 0;
            p.second = 0;
            self.common.sounds.push((0xFFFB, buzz));
        }
        let p = &self.paint;
        let c = &mut cr.slots[PAINT];
        if c.scale_y.unwrap_or(c.scale) < 0x180 {
            c.flags &= !1;
        }
        if self.common.every(8) {
            let (mask, s) = SIGNS[(self.common.clock as usize % 64) >> 3];
            if mask & p.done != 0 {
                let (on, off) = if s < 0 { (-s + 3, -s) } else { (s, s + 3) };
                specials.set_scale(off, [0; 3]);
                specials.set_scale(on, [0x1000; 3]);
            }
        }
        if p.done == 7 && !self.common.given[3] {
            self.common.given[3] = true;
            self.common.reveals.push(3);
        }
    }

    /// 0x41BEE0: the rig under hanging platform `k`: the rope (special 0x39 + k) stretched
    /// from the platform's home height, the hanger (0x10 + k) and the low platform's special
    /// (0x14 + k, following along one axis) with its mover.
    fn rig(world: &World, specials: &mut Specials, k: usize) {
        let (m, _, low, along_x, len) = RIGS[k];
        let s = m - 1;
        let (Some(p), Some(h)) = (specials.pos_obj(s), specials.home_obj(s)) else { return };
        let dy = p[1] - h[1];
        specials.set_scale(s + 0x39, [0x1000, ((dy + len) * 0x1000) / len, 0x1000]);
        specials.set_pos(s + 8, p.map(|v| v >> 5));
        let hanger = specials.pos_obj(s + 0x10).unwrap_or(p);
        specials.set_pos(s + 0x10, [p[0] >> 5, hanger[1] >> 5, p[2] >> 5]);
        specials.set_pos(s + 0x39, [p[0] >> 5, hanger[1] >> 5, p[2] >> 5]);
        for id in [s + 0x14, s + 0x18] {
            let q = specials.pos_obj(id).unwrap_or(p);
            let to = if along_x { [p[0] >> 5, q[1] >> 5, q[2] >> 5] } else { [q[0] >> 5, q[1] >> 5, p[2] >> 5] };
            specials.set_pos(id, to);
            if id == s + 0x14 && let Some(at) = world.mover_pos(low) {
                world.move_mover(low, Vec3::from(to.map(|v| v as f32)) - at);
            }
        }
    }

    /// 0x41BC20: a press near Buzz (0x480 × 256): rising over 50 ticks (its collision off for
    /// the first 5), held up to 150, then falling with 0x180 a tick², slamming down (dust 0x19,
    /// eight chips 0x42, sound 0x69) and bouncing; its collision on near the bottom; at the end
    /// of its cycle, sound 0x6A and again.
    fn press(&mut self, world: &World, specials: &mut Specials, buzz: [i32; 3], k: usize) {
        let (id, mover, cycle) = PRESSES[k];
        let Some(at) = specials.pos_obj(id) else { return };
        if !near(at, buzz, 0x480) {
            return;
        }
        let home = specials.home_obj(id).unwrap_or(at);
        let (n, v, drop) = &mut self.presses[k];
        *n += 1;
        let y = if *n < 0x32 {
            world.set_mover(mover, *n < 5);
            home[1] - *n * 0x400
        } else if *n < 0x96 {
            *v = 0;
            *drop = 0;
            home[1] - 0xC800
        } else if *n < 0xFA {
            *v += 0x180;
            *drop += *v;
            if *drop > 0xC800 {
                *drop = 0xC800;
                if *v > 3000 {
                    self.common.spawns.push(([home[0], home[1] - 0xC00, home[2]], 0x19, 2, 0, None));
                    self.common.sounds.push((0x69, home));
                    for _ in 0..8 {
                        let spin = self.common.rand() - 0x80;
                        self.common.rand();
                        self.common.spawns.push((home, 0x42, 0xE, spin, None));
                    }
                }
                *v = -(*v / 4);
            }
            world.set_mover(mover, *drop >= 0xB401);
            home[1] - 0xC800 + *drop
        } else {
            if *n > cycle {
                *n = 0;
                self.common.sounds.push((0x6A, home));
            }
            return;
        };
        specials.set_pos(id, [home[0] >> 5, y >> 5, home[2] >> 5]);
        if let Some(m) = world.mover_pos(mover) {
            world.move_mover(mover, Vec3::new(m.x, (y >> 5) as f32, m.z) - m);
        }
    }

    /// The drawbridge (mover 0x1A): stomping surface 0x24 lowers it (sound 0x6E while it moves,
    /// 0x6D at the ends) to −0x998, it stays for 240 ticks, then rises back.
    fn drawbridge(&mut self, world: &World, specials: &mut Specials, player: &Player) {
        let at = player.pos_obj();
        if self.bridge == 0 {
            if player.on_floor && player.stomp != 0 && player.surface == Some(0x24) {
                self.bridge = 1;
                self.put_out.push((3, 1));
                self.sounds.push((0x6D, at));
            }
            return;
        }
        let z = world.mover_turn(0x1A)[2];
        let mut dz = 0;
        if self.bridge == 1 {
            if z < -0x998 / 4 {
                self.bridge = 2;
                self.sounds.push((0x6D, at));
            } else {
                dz = -2;
                self.sounds.push((0x6E, at));
            }
        } else {
            let before = self.bridge;
            self.bridge += 1;
            if before < 0xF0 && self.bridge >= 0xF0 {
                self.sounds.push((0x6D, at));
            }
            if self.bridge >= 0xF0 {
                if z < -8 {
                    dz = 2;
                    self.sounds.push((0x6E, at));
                } else if z < 0 {
                    dz = -z;
                } else {
                    self.bridge = 0;
                    self.sounds.push((0x6D, at));
                }
            }
        }
        let a = [0, 0, z + dz];
        world.turn_mover(0x1A, a);
        let tilt = (z + dz) * 4 / 4 + 0x266;
        specials.turn_from_own(0x1C, [0, 0, tilt]);
        specials.turn_from_own(0x1D, [0, 0, tilt]);
    }

    /// The buttons (movers 0x16–0x18): stomped on, each goes down (and its special 0x41 + k
    /// turns) and lifts the lift (mover 0x15, special 0x44) to its height, speeding up by 8 a
    /// tick to 0x400 and easing in at the top.
    fn lift(&mut self, world: &World, specials: &mut Specials, player: &Player) {
        if player.stomp != 0 {
            for k in 0..3 {
                let m = 0x16 + k as i32;
                if self.buttons & (1 << k) == 0 && player.floor_mover.is_some_and(|(f, _)| f == m) {
                    world.turn_mover(m, [-0x180 >> 2, 0, 0]);
                    specials.turn_from_own(0x41 + k as i32, [-0x180, 0, 0]);
                    self.buttons |= 1 << k;
                    self.put_out.push((4 + k, 1));
                }
            }
        }
        let goal = if self.buttons & 4 != 0 {
            -0x63380
        } else if self.buttons & 2 != 0 {
            -0x3B600
        } else if self.buttons & 1 != 0 {
            -0x15E00
        } else {
            return;
        };
        let (Some(home), Some(at)) = (specials.home_obj(0x44), world.mover_pos(0x15)) else { return };
        let y = (at.y * 32.0) as i32;
        specials.set_pos(0x44, [(at.x) as i32, (at.y) as i32, (at.z) as i32]);
        let dy = if self.buttons & 8 == 0 {
            if home[1] + goal < y {
                self.lift_v = (self.lift_v + 8).min(0x400);
            } else {
                self.lift_v -= 8;
                if self.lift_v < 0 {
                    self.buttons |= 8;
                    self.lift_v = 0;
                }
            }
            -self.lift_v
        } else if y + 0xE100 < home[1] + goal {
            self.lift_v = (self.lift_v + 8).min(0x400);
            self.lift_v
        } else {
            self.lift_v -= 8;
            if self.lift_v < 0 {
                self.buttons &= !8;
                self.lift_v = 0;
            }
            self.lift_v
        };
        world.move_mover(0x15, Vec3::new(0.0, dy as f32 / 32.0, 0.0));
    }

    /// The timed challenge (0x52F2F8 = `race`, 0x52AD64 = `laps` as its clock): touched, the
    /// character asks Buzz to find five things (the challenge's collectables appear, the count
    /// resets); the clock starts at 150 and drops every 64 ticks; at 100 it's over and they go
    /// again. Touched while it runs: "hurry up", or with five found "well done" and token 2.
    fn challenge(&mut self, cr: &mut Creatures, collected: i32) {
        if touched(cr, CHALLENGE.0) {
            if self.race == 0 {
                self.voices.push(0xB6);
                self.talk(CHALLENGE.0, CHALLENGE.1, CHALLENGE_ASK, None, None);
                self.race = 1;
                self.collect_reset = true;
                self.show_collectables = Some(true);
            } else if collected < 5 {
                self.talk(CHALLENGE.0, CHALLENGE.1, CHALLENGE_HURRY, None, None);
            } else if self.race != 3 {
                self.voices.push(0xB7);
                self.talk(CHALLENGE.0, CHALLENGE.1, CHALLENGE_DONE, Some(2), None);
                self.race = 3;
            }
        }
        if self.race == 1 && self.cutscene.is_none() {
            self.race = 2;
            self.laps = 0x96;
        }
        if self.race == 2 {
            if self.every(64) {
                self.laps -= 1;
            }
            if self.laps < 100 {
                self.laps = 100;
                self.race = 0;
                self.show_collectables = Some(false);
            }
        }
    }

    /// The drill (0x52F804 = `cr.boss.state`): Buzz standing near it down in the pit
    /// (y < −0x9BFC5, within 300 × 256) starts the challenge; after it the drill's fight script
    /// runs (behaviour + 0x18) and the camera's bar shows from y −0x7DE25 up; once it's gone,
    /// token 4. It keeps to the pit's lanes and never rises above −0x9C3E0.
    fn drill(&mut self, player: &Player, cr: &mut Creatures) {
        let Some(d) = cr.slots.get(DRILL.0).map(|c| (c.id, c.pos)) else { return };
        let buzz = player.pos_obj();
        let b = &mut cr.boss;
        match b.state {
            0 => {
                if d.0 == 0x16 && player.on_floor && buzz[1] < -0x9BFC5 && near(d.1, buzz, 300) {
                    b.state = 1;
                    self.talk(DRILL.0, DRILL.1, DRILL_TEXT, None, Some([0x580, 0xDC0]));
                }
            }
            1 => {
                if self.cutscene.is_none() {
                    b.state = 2;
                    cr.slots[DRILL.0].set_script(0x18 / 2);
                }
            }
            2 => {
                if d.0 == 0 || cr.slots[DRILL.0].health <= 0 {
                    b.state = 3;
                    self.boss_bar = Some(0);
                } else if buzz[1] < -0x7DE25 {
                    self.boss_hud = true;
                }
            }
            3..0x78 => b.state += 1,
            0x78 => {
                if !self.given[4] {
                    self.given[4] = true;
                    self.reveals.push(4);
                }
                b.state = 200;
            }
            _ => {}
        }
        if d.0 != 0x16 {
            return;
        }
        // Its lanes.
        let c = &mut cr.slots[DRILL.0];
        let count = |v: i32, t: &[i32; 4]| t.iter().take_while(|&&x| v >= x).count() as u32;
        let mut u = count(c.pos[0], &LANES_X) + (count(c.pos[2], &LANES_Z) << 4);
        if u & 0x11 == 0x11 {
            if self.lanes & 1 == 0 {
                let i = ((u & 3) - 1) as usize;
                let (a, b) = (SNAP_X[i], SNAP_X[(i + 1).min(3)]);
                c.pos[0] = if c.pos[0] < b - (b - a) / 2 { a } else { b };
                u &= !1;
            }
            if self.lanes & 0x10 == 0 {
                let i = ((u >> 4 & 3) - 1) as usize;
                let (a, b) = (SNAP_Z[i], SNAP_Z[(i + 1).min(3)]);
                c.pos[2] = if c.pos[2] < b - (b - a) / 2 { a } else { b };
                u &= !0x10;
            }
        }
        self.lanes = u;
        c.pos[1] = c.pos[1].min(-0x9C3E0);
    }
}

impl std::ops::Deref for Level4 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}

impl std::ops::DerefMut for Level4 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Script for Level4 {
    fn step(&mut self, world: &World, specials: &mut Specials, player: &mut Player, _boxes: &Boxes) {
        self.clock = self.clock.wrapping_add(1);
        let lists = std::mem::take(&mut self.common.story.lists);
        let on = player.floor_mover.filter(|_| player.on_floor).map(|(m, _)| m);
        for k in 0..4 {
            let (m, yaw, low, ..) = RIGS[k];
            self.tilts[k].step(world, specials, player, m, m - 1, yaw, 2, -0xE0, 0xE0, 0x10);
            if on != Some(m) && on != Some(low) {
                let mut rand = || self.common.rand();
                let mut flags = self.path_flags;
                self.rigs[k].step(world, specials, m, m - 1, &mut flags, &lists, &mut rand);
                self.path_flags = flags;
            }
            Self::rig(world, specials, k);
        }
        for (k, (m, s, _)) in SHUTTLES.into_iter().enumerate() {
            let mut flags = self.path_flags;
            let mut rand = || self.common.rand();
            self.shuttles[k].step(world, specials, m, s, &mut flags, &lists, &mut rand);
            self.path_flags = flags;
        }
        self.common.story.lists = lists;
        let buzz = player.pos_obj();
        for k in 0..3 {
            self.press(world, specials, buzz, k);
        }
        self.drawbridge(world, specials, player);
        self.lift(world, specials, player);
        // The gully's water.
        let [x, y, z] = buzz;
        let wet = x > -0x5C801 && x < 0x15F9F && z > -0x83718 && z < -0x10E28 && y >= -0x8D8A;
        player.water = if wet { (2, -0x7A78) } else { (0, 0) };
        self.in_water = wet;
    }

    fn step_creatures(&mut self, world: &World, specials: &mut Specials, player: &Player, cr: &mut Creatures, coins: i32) {
        self.paint(world, specials, player, cr);
        self.boss_hud = false;
        let open = self.race == 0;
        self.call(cr, CHALLENGE.0, 0xB5, open);
        let collected = self.collected;
        self.challenge(cr, collected);
        // Level 4's gift (0x503A2B): the disk launcher.
        self.potato(cr, POTATO.0, POTATO.1, POTATO_TEXTS, [0x440, 0xC40], 4);
        self.hamm(cr, HAMM.0, HAMM.1, Some([0xE10, 0x6E0]), 0, coins);
        self.hint_character(cr, HINT.0, HINT.1);
        if touched(cr, FINDER.0) && cr.sheep >= 0 {
            if cr.sheep == 5 {
                self.talk(FINDER.0, FINDER.1, FINDER_THANKS, Some(1), None);
                cr.sheep = -1;
            } else {
                self.talk(FINDER.0, FINDER.1, FINDER_ASK, None, None);
            }
        }
        self.drill(player, cr);
    }
}
