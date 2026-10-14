//! Level 1 (Andy's house): the level's start-up (0x4171D0) and per-frame script (0x417680),
//! the parts that don't need creatures yet. Rooms are the room-map numbers (`0x54DEA0`).
//!
//! - The basement door (special 4) is box 3 of the level's push list: once Buzz pushes it
//!   over its edge, it tips open (up to 0x200 about X, 0x10 per tick) and its collision swaps
//!   from mover 1 (closed) to mover 14 (open).
//! - Stepping onto the basement shelf (a box around x 8988–9822, z −11300 to −10058, y 0–576)
//!   unrolls a plank: special 0x15 grows along x (0x40 per tick) and drags 0x16 with it, and
//!   mover 8 starts colliding.
//! - Special 0xE (a fan) always turns, 0x20 per tick about Y.
//! - Room 2: three spinning tops (specials 9, 0x11, 0x12; 0x417510) and two pop-up platforms
//!   (specials 10, 0x10 with movers 12, 13; 0x417380) that sink and rise, losing their
//!   collision while down.
//! - Room 4: mover 3 is a trampoline: landing on it bounces Buzz (−0x980, −0xC00 from a stomp).
//!
//! - Room 1 (`tick_creatures`): the crib. Its two bars (COTBIT creatures in slots 0 and 2)
//!   can be shot only from the far side (Buzz past x 0x4C274: record state 5 instead of 4).
//!   A hit swings a bar down about Z with bounces (sound 0x32), and it can't be targeted any
//!   more; with both down, the crib's barrier (mover 9) goes and specials 0x13, 0x14 and 0x1A
//!   drop. Until then mover 9 holds, except under Buzz standing on surface 0xE.
//!
//! - Characters (`tick_characters`): Bo Peep and her sheep (room 4), Hamm's coins (room 3),
//!   Mr. Potato Head's ear (room 6), the RC race (room 2), the Tin Man (room 5).
//! - Room 5: a see-saw (mover and special 0xF, 0x49EC00) tipped by where Buzz stands on it.
//!
//! - Room 2: the lift (special 6 sinking with pole 8 once Buzz climbs it). Room 3: the spring
//!   (special 0x17, surface 8: stomp on it to be thrown). Room 4: the fountain, its steam vents
//!   and a hot spot that burns.
//!
//! - The hint Hamm (0x4A1E60) and Bo Peep's calls (0x4A26F0).
//!
//! Not ported yet: the reward sparkles, the camera-height
//! limits on the stairs (Buzz state 0x3C) and the cosmic shield's use.

use crate::boxes::Boxes;
use crate::creatures::Creatures;
use crate::cutscene::Cutscene;
use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::game::{JumpPhase, Player, Sound, World};
use crate::specials::Specials;
use glam::Vec3;

pub struct Level1 {
    /// 0x52F500: door tilt.
    door: i32,
    /// 0x52F5A0: plank growth (0 = not started).
    plank: i32,
    /// 0x52F5CC: fan angle.
    fan: i32,
    /// 0x52F4F4 / 0x52F568: tops' angle and phase; 0x52F5A4 / 0x52F588: pop-ups'.
    tops: (i32, i32),
    pops: (i32, i32),
    /// Rest positions (object units) of the tops and pop-ups, read at start-up.
    home: Vec<(i32, [i32; 3])>,
    /// The crib bars' swing speeds (0x52F574 / 0x52F570), the crib's drop timer (0x52F56C)
    /// and speed (0x52F57C).
    bars: [i32; 2],
    crib: i32,
    crib_v: i32,
    /// Sounds at positions (object units) from the creature parts.
    pub sounds: Vec<(u16, [i32; 3])>,
    /// Voice lines on Buzz's channel (0x49EA60) and sample slots at a position (0x4A3C80:
    /// slot, volume).
    pub voices: Vec<u16>,
    pub slots: Vec<(u16, i32, [i32; 3])>,
    /// A conversation in progress.
    pub cutscene: Option<Cutscene>,
    /// Who stands where at its start: Buzz and the character's slot (positions, facings).
    pub staging: Option<([[i32; 3]; 2], [i32; 2], usize)>,
    /// Tokens handed over or shown (0x830CD0 per token).
    pub given: [bool; 5],
    /// 0x830D48: Mr Potato Head's missing part (8 = his ear; negative once Buzz has it, 0
    /// once returned) and the abilities he gives (0x52F2D8).
    pub item: i32,
    pub abilities: u8,
    /// Hamm's and Mr Potato Head's call timers (0x830C98 / 0x830D34).
    calls: [i32; 2],
    /// The hint-giving Hamm's next hint (0x830D24, −1 until he's first in play) and call timer
    /// (0x830CA4); Bo Peep's call timer (0x830D40).
    hint: i32,
    hint_call: i32,
    bo_call: i32,
    /// The tokens Buzz has collected (bits, `0x52F0D7[level]`), set by the caller.
    pub tokens: u8,
    /// The RC race (room 2): state 0x52F2F8 (1 talked, 2 racing, 3 over), the car's waypoint
    /// and laps (0x52F528 / 0x52F4F8), Buzz's laps (0x52AD64) and his quarter of the room
    /// with the finish-line latch (0x52F584 / 0x52F580).
    pub race: i32,
    waypoint: usize,
    rc_laps: i32,
    pub laps: i32,
    quarter: u32,
    armed: bool,
    /// Tokens to show now (0x4A0DB0 outside a conversation).
    pub reveals: Vec<usize>,
    /// The boss's health bar is up (HUD element 7 kept at 0x5A ticks).
    pub boss_hud: bool,
    /// Room 5's see-saw (mover / special 0xF): its swing speed (0x52F58C) and tilt about Z
    /// (mover table units, 0x4000 a turn).
    tilt: i32,
    tilt_z: i32,
    /// Room 3's spring (0x52F4FC): 0 idle, counting up once stomped (throws Buzz at 7), then
    /// −0x20 counting back up to 0.
    spring: i32,
    /// Room 2's lift (special 6 carrying pole 8): speed (0x52F504: 0 waiting, −1 done), height
    /// and its lowest (0x52F5BC / 0x52F5B4), where it is across (0x52F5B8 / 0x52F5C0), and
    /// the pole's offset from it (0x52F530).
    lift_v: i32,
    lift: [i32; 3],
    lift_max: i32,
    pole_offset: Option<i32>,
    /// Room 4's fountain: its next target point (0x52F52C), the steam timer and vent
    /// (0x52F5B0 / 0x52F598); the tick count behind the every-n-ticks flags (0x4A2960).
    fountain: usize,
    steam: i32,
    vent: usize,
    clock: u32,
    /// Particles the script makes: by type and mode (0x40FDF0) with a spin and a life, and
    /// fully specified (0x40FAE0).
    pub spawns: Vec<([i32; 3], u8, usize, i32, Option<i32>)>,
    pub particles: Vec<crate::creatures::ParticleRequest>,
    /// Random bytes for the effects (the game's `rand.dat` stream).
    random: Vec<u8>,
    rpos: usize,
    story: Story,
}

/// Texts and camera lists of the level's conversations (from toy2.exe and level.dat).
#[derive(Debug, Clone, Default)]
struct Story {
    texts: std::collections::HashMap<u32, String>,
    lists: std::collections::HashMap<i16, Vec<[i32; 3]>>,
    /// The hint texts' addresses, by token.
    hints: Vec<u32>,
}

/// Conversation texts (exe addresses).
const BO_PEEP_THANKS: u32 = 0x4F03E0;
const BO_PEEP_ASK: u32 = 0x4F036C;
const HAMM_THANKS: u32 = 0x502834;
const HAMM_ASK: u32 = 0x5027E0;
const POTATO_ASK: u32 = 0x4F0ADC;
const POTATO_THANKS: u32 = 0x4F0B60;
const POTATO_AFTER: u32 = 0x4F0C10;
const RC_ASK: u32 = 0x4F042C;
/// The hint Hamm's five hints (pointers at 0x4F0F7C, one per token) and his line once Buzz has
/// them all.
const HINTS: u32 = 0x4F0F7C;
const ALL_TOKENS: u32 = 0x502868;
const HINT_HAMM: (usize, i16) = (0x23, 0x27);
const TIN_MAN_TEXT: u32 = 0x4F0330;
/// Characters' slots and their camera lists.
const BO_PEEP: (usize, i16) = (1, 0x1C);
const HAMM: (usize, i16) = (0x1E, 0x1D);
const POTATO: (usize, i16) = (0x1F, 0x19);
const RC: (usize, i16) = (0x1D, 0x1B);
const TIN_MAN: (usize, i16) = (8, 0x1A);
/// The RC car's waypoints (level.dat list 30) and how many it laps.
const RC_TRACK: i16 = 0x1E;
const RC_LAPS: i32 = 3;
/// Room 4's fountain: level.dat list 22, squirting from its first point to the others.
const FOUNTAIN: i16 = 0x16;
/// The steam vents beside it (0x4F0EB8, object units).
const VENTS: [[i32; 3]; 3] = [[0xB81A8, 0x29283, -0x86785], [0xB31E8, 0x29283, -0x86785], [0xB81A8, 0x29283, -0x81685]];

/// The crib bars' creature slots and the health that means "at rest".
const BARS: [usize; 2] = [0, 2];
const BAR_REST: i32 = 0x50;

/// Sine table value (round(16384 · sin)) of a 12-bit angle, as 0x4FE788 holds it.
fn sin_table(a: i32) -> i32 {
    ((a & 0xFFF) as f64 / 4096.0 * std::f64::consts::TAU).sin().mul_add(16384.0, 0.0).round() as i32
}

/// Within `r` (in 256-object-unit steps) of Buzz (0x49F400).
fn near(p: [i32; 3], buzz: [i32; 3], r: i32) -> bool {
    let d = [0, 1, 2].map(|k| (p[k] - buzz[k]) >> 8);
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < r * r
}

impl Level1 {
    /// 0x4171D0's part that sets up objects.
    pub fn new(world: &World, specials: &mut Specials) -> Level1 {
        let home = [9, 0x11, 0x12, 10, 0x10].iter().filter_map(|&id| specials.pos_obj(id).map(|p| (id, p))).collect();
        specials.set_scale(0x15, [0, 0x1000, 0x1000]);
        specials.scale_about(0x16, 0x15);
        world.set_mover(8, false);
        world.set_mover(14, false);
        let lift = specials.pos_obj(6).unwrap_or_default();
        Level1 {
            door: 0,
            plank: 0,
            fan: 0,
            tops: (0, 0),
            pops: (0, 0),
            home,
            bars: [0; 2],
            crib: 0,
            crib_v: 0,
            sounds: Vec::new(),
            voices: Vec::new(),
            slots: Vec::new(),
            cutscene: None,
            staging: None,
            given: [false, false, false, true, false],
            item: 8,
            abilities: 0,
            calls: [0; 2],
            hint: -1,
            hint_call: 0,
            bo_call: 0,
            tokens: 0,
            race: 0,
            waypoint: 0,
            rc_laps: 0,
            laps: 0,
            quarter: 0,
            armed: false,
            reveals: Vec::new(),
            boss_hud: false,
            tilt: 0,
            tilt_z: 0,
            spring: 0,
            lift_v: 0,
            lift,
            lift_max: lift[1] + 0xB800,
            pole_offset: None,
            fountain: 0,
            steam: 0,
            vent: 0,
            clock: 0,
            spawns: Vec::new(),
            particles: Vec::new(),
            random: Vec::new(),
            rpos: 0,
            story: Story::default(),
        }
    }

    /// The conversations' texts (from the exe) and camera lists (from level.dat).
    pub fn set_story(&mut self, exe: &Exe, lists: &[PointList]) {
        let hints: Vec<u32> = (0..5).filter_map(|k| exe.at(HINTS + 4 * k, 4).ok().map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect();
        self.story.hints = hints.clone();
        for va in [BO_PEEP_THANKS, BO_PEEP_ASK, HAMM_THANKS, HAMM_ASK, POTATO_ASK, POTATO_THANKS, POTATO_AFTER, RC_ASK, TIN_MAN_TEXT, ALL_TOKENS].into_iter().chain(hints) {
            let mut s = Vec::new();
            while let Ok(b) = exe.at(va + s.len() as u32, 1) {
                if b[0] == 0 || s.len() > 400 {
                    break;
                }
                s.push(b[0]);
            }
            self.story.texts.insert(va, String::from_utf8_lossy(&s).into_owned());
        }
        for l in lists.iter().filter(|l| matches!(l.kind, 0x19 | 0x1A | 0x1B | 0x1C | 0x1D | RC_TRACK | FOUNTAIN | 0x27)) {
            self.story.lists.insert(l.kind, l.points.clone());
        }
    }

    /// Starts a conversation (0x4027F0) on camera list `list` with `text` (an exe address).
    fn talk(&mut self, slot: usize, list: i16, text: u32, token: Option<usize>, facings: Option<[i32; 2]>) {
        // Only one at a time (0x4027F0 does nothing while 0x50A1F8 is set).
        if self.cutscene.is_some() {
            return;
        }
        let points = self.story.lists.get(&list).cloned().unwrap_or_default();
        let text = self.story.texts.get(&text).cloned().unwrap_or_default();
        let (c, at, f) = Cutscene::new(&points, &text, token);
        self.staging = Some((at, facings.unwrap_or(f), slot));
        self.cutscene = Some(c);
    }

    /// Both crib bars are down and the crib has opened (0x52F56C set).
    pub fn crib_open(&self) -> bool {
        self.crib != 0
    }

    /// The characters of 0x417680 (Bo Peep in room 4, Hamm 0x4A1CE0 in room 3, Mr Potato
    /// Head 0x4A2480 in room 6), after the creatures' update. `coins` is Buzz's count.
    pub fn tick_characters(&mut self, world: &World, player: &Player, cr: &mut Creatures, coins: i32) {
        let talking = self.cutscene.is_some();
        let room = world.room_at(player.pos);
        let touched = |cr: &mut Creatures, slot: usize, id: u8| {
            cr.slots.get_mut(slot).filter(|c| c.id == id && c.flags & 0x200 != 0).map(|c| c.flags &= !0x200).is_some()
        };
        if room == Some(4) && touched(cr, BO_PEEP.0, 7) && cr.sheep >= 0 {
            if cr.sheep == 5 {
                self.voices.push(0xB4);
                self.talk(BO_PEEP.0, BO_PEEP.1, BO_PEEP_THANKS, Some(1), None);
                cr.sheep = -1;
            } else {
                self.voices.push(0xB3);
                self.talk(BO_PEEP.0, BO_PEEP.1, BO_PEEP_ASK, None, None);
            }
        }
        if room == Some(3) && let Some(h) = cr.slots.get(HAMM.0).filter(|c| c.id == 10) {
            let pos = h.pos;
            if !self.given[0] && h.active && !talking {
                self.calls[0] -= 1;
                if self.calls[0] < 0 {
                    self.calls[0] = (pos[0] & 0xFF) * 2 + 0xF0;
                    self.slots.push((72, 128, pos));
                }
            }
            if touched(cr, HAMM.0, 10) && !self.given[0] {
                if coins > 0x31 {
                    self.slots.push((74, 128, pos));
                    self.talk(HAMM.0, HAMM.1, HAMM_THANKS, Some(0), Some([0xE10 & 0xFFF, 0x6E0]));
                } else {
                    self.slots.push((73, 128, pos));
                    self.talk(HAMM.0, HAMM.1, HAMM_ASK, None, Some([0xE10 & 0xFFF, 0x6E0]));
                }
            }
        }
        self.race(room, player, cr);
        self.tin_man(room, player, cr);
        self.hint_hamm(cr, talking);
        // 0x4A26F0(1, 0xB2): Bo Peep calls out while she still wants her sheep.
        if cr.sheep >= 0 && !talking && let Some(b) = cr.slots.get(BO_PEEP.0).filter(|c| c.id == 7 && c.active) {
            let pos = b.pos;
            self.bo_call -= 1;
            if self.bo_call < 0 {
                self.bo_call = self.rand() * 2 + 0xF0;
                self.sounds.push((0xB2, pos));
            }
        }
        if room == Some(6) && let Some(m) = cr.slots.get(POTATO.0).filter(|c| c.id == 9) {
            let pos = m.pos;
            if m.active && !talking {
                self.calls[1] -= 1;
                if self.calls[1] < 0 {
                    self.calls[1] = (pos[2] & 0xFF) * 2 + 0xF0;
                    self.slots.push((75, 128, pos));
                }
            }
            if touched(cr, POTATO.0, 9) {
                let f = Some([0x200, 0xA00]);
                match self.item.signum() {
                    0 => self.talk(POTATO.0, POTATO.1, POTATO_AFTER, None, f),
                    1 => {
                        self.talk(POTATO.0, POTATO.1, POTATO_ASK, None, f);
                        self.slots.push((76, 128, pos));
                    }
                    _ => {
                        self.slots.push((77, 128, pos));
                        self.talk(POTATO.0, POTATO.1, POTATO_THANKS, None, f);
                        self.item = 0;
                        // Level 1's gift (0x503A23): the cosmic shield.
                        self.abilities |= 1;
                    }
                }
            }
        }
    }

    /// Room 2's RC race (0x417680). Touching the car (before its token) starts a challenge
    /// conversation (list 0x1B); when it's over the car races 3 laps round its waypoints. Buzz's
    /// laps count when he crosses the finish line one way (his quarter of the room going from
    /// all four flags to three); three before the car finishes win token 2. Leaving the room
    /// calls it off.
    fn race(&mut self, room: Option<u8>, player: &Player, cr: &mut Creatures) {
        let Some(rc) = cr.slots.get(RC.0).filter(|c| c.id == 8).map(|c| c.pos) else { return };
        if room != Some(2) {
            if self.race != 0 && !self.given[2] {
                self.race = 0;
                cr.racing = false;
                cr.restart(RC.0);
            }
            return;
        }
        let touched = cr.slots[RC.0].flags & 0x200 != 0;
        cr.slots[RC.0].flags &= !0x200;
        if touched && !self.given[2] && self.race == 0 {
            self.talk(RC.0, RC.1, RC_ASK, None, Some([0, 0]));
            self.race = 1;
            self.waypoint = 0;
            self.rc_laps = 0;
            self.laps = 0;
            self.quarter = 0;
            self.armed = true;
            cr.slots[RC.0].health = 0xCA;
        }
        if self.race == 1 && self.cutscene.is_none() {
            cr.slots[RC.0].flags |= 4;
            cr.racing = true;
            self.race = 2;
        }
        if self.race == 2 {
            let [x, _, z] = player.pos_obj();
            let mut q = (z < -0x1A000) as u32;
            q |= if x > 0x2900 { 2 } else { 0 };
            q |= if x < 0x21000 { 4 } else { 0 };
            q |= if z > -0x1E000 { 8 } else { 0 };
            if q == 0xE {
                if self.quarter == 0xF {
                    if !self.armed {
                        self.laps += 1;
                    }
                    self.armed = false;
                }
            } else if q == 0xF && self.quarter == 0xE {
                self.armed = true;
            }
            self.quarter = q;
        }
        if self.race == 0 {
            return;
        }
        let track = self.story.lists.get(&RC_TRACK).cloned().unwrap_or_default();
        if track.is_empty() {
            return;
        }
        let wp = track[self.waypoint.min(track.len() - 1)];
        let (dx, dz) = (wp[0] - (rc[0] >> 5), wp[2] - (rc[2] >> 5));
        let c = &mut cr.slots[RC.0];
        if dx * dx + dz * dz < 360_000 {
            c.home[0] = wp[0] << 5;
            c.home[2] = wp[2] << 5;
            if self.rc_laps < RC_LAPS {
                self.waypoint += 1;
                if self.waypoint > 0x2C || self.waypoint >= track.len() {
                    self.rc_laps += 1;
                    self.waypoint = 0;
                }
            } else if rc[2] > -0x1A000 {
                if self.race == 2 && self.laps < RC_LAPS {
                    self.race = 3;
                }
                c.health = 1;
                cr.racing = false;
            }
        }
        let wp = track[self.waypoint.min(track.len() - 1)];
        let c = &mut cr.slots[RC.0];
        c.target[0] = wp[0] << 5;
        c.target[2] = wp[2] << 5;
        if self.laps == RC_LAPS && self.race == 2 {
            self.reveals.push(2);
            self.given[2] = true;
            self.race = 3;
        }
    }

    /// 0x4A1E60(0x23, 0x27, 0x4F0F7C): the hint Hamm. In play, he calls now and then (slot 70);
    /// touched, he gives the hint for the next token Buzz hasn't got (slot 71), starting from
    /// a random one, or says Buzz has them all.
    fn hint_hamm(&mut self, cr: &mut Creatures, talking: bool) {
        let Some(h) = cr.slots.get_mut(HINT_HAMM.0).filter(|c| c.id != 0 && c.active) else { return };
        let pos = h.pos;
        let touched = h.flags & 0x200 != 0;
        h.flags &= !0x200;
        if self.hint == -1 {
            self.hint = self.rand() & 3;
        }
        if !talking {
            self.hint_call -= 1;
            if self.hint_call < 0 {
                self.hint_call = self.rand() * 2 + 0xF0;
                self.slots.push((70, 128, pos));
            }
        }
        if !touched || talking {
            return;
        }
        let text = if self.tokens & 0x1F != 0x1F {
            self.slots.push((71, 128, pos));
            while self.tokens >> self.hint & 1 != 0 {
                self.hint += 1;
                if self.hint > 4 {
                    self.hint = 0;
                }
            }
            self.hint += 1;
            let t = self.story.hints.get(self.hint as usize - 1).copied().unwrap_or(ALL_TOKENS);
            if self.hint > 4 {
                self.hint = 0;
            }
            t
        } else {
            ALL_TOKENS
        };
        self.talk(HINT_HAMM.0, HINT_HAMM.1, text, None, None);
    }

    /// Gives the script the game's random bytes for its effects.
    pub fn set_random(&mut self, random: Vec<u8>) {
        self.random = random;
    }

    fn rand(&mut self) -> i32 {
        if self.random.is_empty() {
            return 0x80;
        }
        let b = self.random[self.rpos % self.random.len()];
        self.rpos = (self.rpos + 1) % 0x5DC.min(self.random.len());
        b as i32
    }

    /// Every `n` ticks (the flags 0x52F1C2–0x52F1CB of 0x4A2960).
    fn every(&self, n: u32) -> bool {
        self.clock % n == 0
    }

    /// Room 4 (0x417680): near the fountain (within 0x280 × 256 of it), every 64 ticks a
    /// squirt (particle 0xC) arcs from list 22's first point to one of the others, picked 1–16
    /// on; every 120 ticks the next of three vents puffs steam (0x22 mode 2) and every other
    /// tick it smokes (0x10 mode 10). Near the hot spot beside it: within 30 × 256 it burns
    /// Buzz (0x4071E0 mode 2) and it smokes every 8 ticks (0x11 mode 10).
    fn room4_effects(&mut self, player: &mut Player) {
        let buzz = player.pos_obj();
        if near([0xB3E1A, 0x26720, -0x814C1], buzz, 0x280) {
            let pts = self.story.lists.get(&FOUNTAIN).cloned().unwrap_or_default();
            if self.every(64) && pts.len() > 1 {
                let (p0, pk) = (pts[0], pts[self.fountain.min(pts.len() - 1)]);
                let (dx, dz) = ((p0[0] * 32 - pk[0] * 32) >> 8, (p0[2] * 32 - pk[2] * 32) >> 8);
                let u = (crate::boxes::angle12(dx as f32, dz as f32) - 0x800) & 0xFFF;
                let d = ((dx * dx + dz * dz) as f64).sqrt() as i32;
                let reach = (d * 7 / 2).max(1);
                let vy = ((d << 12) / reach) * -0x80 / 64;
                let spin = (self.rand() - 0x80) >> 2;
                self.particles.push(crate::creatures::ParticleRequest {
                    pos: p0.map(|v| v * 32),
                    vel: [sin_table(u) * reach >> 13, vy, sin_table(u + 0x400) * reach >> 13],
                    gravity: 0x80,
                    angle: 0,
                    spin,
                    typ: 0xC,
                });
                self.fountain += 1 + (self.rand() & 0xF) as usize;
                if self.fountain >= pts.len() {
                    self.fountain = self.fountain + 1 - pts.len();
                }
            }
            self.steam += 1;
            if self.steam > 0x78 {
                self.steam -= 0x78;
                self.vent = (self.vent + 1) % 3;
                let v = VENTS[self.vent];
                self.spawns.push(([v[0], v[1] - 0x800, v[2]], 0x22, 2, 0, None));
            }
            if self.every(2) {
                let life = (self.rand() & 0xF) + 0x20;
                self.spawns.push((VENTS[self.vent], 0x10, 10, 0, Some(life)));
            }
        }
        let hot = [0xB74DC, 0x23F3E, -0x6634A];
        if near(hot, buzz, 0x280) {
            if near(hot, buzz, 30) {
                player.hurt(0, 2);
            }
            if self.every(8) {
                let spin = (self.rand() - 0x80) >> 3;
                self.spawns.push((hot, 0x11, 10, spin, None));
            }
        }
    }

    /// Room 2's lift (0x417680): climbing pole 8 sets it going (sound 0x21); special 6 and the
    /// pole sink together, speeding up by 0x20 a tick, down to 0xB800 below where it started.
    fn lift(&mut self, specials: &mut Specials, player: &mut Player) {
        const POLE: usize = 8;
        if self.lift_v == 0 && player.pole_held() == Some(POLE) {
            self.lift_v = 2;
            player.sounds.push(Sound { id: 0x21, voice: false });
        }
        if self.lift_v <= 0 {
            return;
        }
        let Some(pole) = player.pole(POLE) else { return };
        let offset = *self.pole_offset.get_or_insert(pole.pos[1] - self.lift[1]);
        self.lift[1] += self.lift_v;
        if self.lift[1] > self.lift_max {
            self.lift_v = -1;
            self.lift[1] = self.lift_max;
        }
        player.move_pole(POLE, offset + self.lift[1]);
        specials.set_pos(6, self.lift.map(|v| v >> 5));
        if self.lift_v > 0 {
            self.lift_v += 0x20;
        }
    }

    /// Room 3's spring (special 0x17 on surface 8, 0x417680): it shows squashed by its count
    /// (y scale |n| × 0x80, turned |n| × 0x20). Stomping onto it starts the count; at 7 it
    /// throws Buzz (vy −0x1280, heading 0x11E at full speed, sound 0x1C); at 0x21 it rests for
    /// 32 ticks.
    fn spring(&mut self, specials: &mut Specials, player: &mut Player) {
        let n = self.spring.abs();
        specials.set_scale(0x17, [0x1000, n * 0x80, 0x1000]);
        specials.turn_from_own(0x17, [0, 0, n * 0x20]);
        let on = player.on_floor && player.surface == Some(8);
        let mut count = false;
        if on && self.spring == 0 {
            if player.stomp != 0 {
                self.spring = 2;
                count = true;
            }
        } else if self.spring < 0 {
            self.spring = (self.spring + 1).min(0);
        } else if self.spring > 0 {
            count = true;
        }
        if count {
            self.spring += 1;
            if self.spring < 0x21 {
                if self.spring == 7 {
                    player.launch(-0x1280, 0x11E);
                    player.sounds.push(Sound { id: 0x1C, voice: false });
                }
            } else {
                self.spring = -0x20;
            }
        }
    }

    /// 0x49EC00 (mover 0xF, special 0xF, mode 1, limits ±0x1C0, divisor 8): room 5's see-saw.
    /// Buzz standing on it pushes the side he's on down, harder the further out he is and
    /// the harder he lands; off it, it drifts towards its positive tilt. The swing slows by 2
    /// a tick, the tilt changes by an eighth of it, and it bounces back at the limits with
    /// three quarters of its speed. The special follows the mover.
    fn see_saw(&mut self, world: &World, specials: &mut Specials, player: &mut Player) {
        const SAW: i32 = 0xF;
        let mut v = self.tilt;
        let riding = player.on_floor && player.floor_mover.is_some_and(|(m, _)| m == SAW);
        let before = [0, 0, self.tilt_z >> 2];
        if riding {
            let Some(p) = specials.pos_obj(SAW) else { return };
            let b = player.pos_obj();
            // Along the plank (its yaw is 0: x), and the distance from its pivot.
            let along = ((b[0] - p[0]) >> 5) * 0x1000 / 0x1000;
            let dy = (b[1] - p[1]) >> 5;
            let d = (((dy * dy + along * along) as f64).sqrt() as i32) >> 5;
            let push = ((player.fall_speed * d >> 9) + d) / 2;
            v += if along > 0 { push } else { -push };
        } else {
            v += 4;
        }
        if v > 0 {
            v = (v - 2).max(0);
        } else if v < 0 {
            v = (v + 2).min(0);
        }
        self.tilt_z += v / 8;
        if self.tilt_z < -0x700 {
            v = (v * 3 / 4).abs();
            self.tilt_z = -0x700;
        } else if self.tilt_z > 0x700 {
            v = -(v * 3 / 4).abs();
            self.tilt_z = 0x700;
        }
        self.tilt = v;
        let a = [0, 0, self.tilt_z >> 2];
        world.turn_mover(SAW, a);
        specials.set_angles(SAW, a);
        // Standing on it, Buzz turns with it.
        if riding && a != before {
            player.pos = world.ride_turn(SAW, before, a, player.pos);
        }
    }

    /// Room 5's Tin Man (0x417680 with the boss's routine). Coming up to his level in his zone
    /// (standing) starts the challenge (list 0x1A, "ha ha ha ha…"); when it's over the fight
    /// script runs and he chases Buzz. He's kept up at his platform's height. His bar shows
    /// while Buzz is in the arena; his death gives token 4.
    fn tin_man(&mut self, room: Option<u8>, player: &Player, cr: &mut Creatures) {
        if cr.slots.get(TIN_MAN.0).is_none_or(|c| c.id != 5) {
            return;
        }
        let b = &mut cr.boss;
        if std::mem::take(&mut b.challenge) && b.state == 0 && player.on_floor && self.cutscene.is_none() {
            self.talk(TIN_MAN.0, TIN_MAN.1, TIN_MAN_TEXT, None, Some([0xE23, 0x700]));
            cr.boss.state = 1;
        }
        if cr.boss.token && !self.given[4] {
            self.given[4] = true;
            self.reveals.push(4);
        }
        if room == Some(5) {
            if cr.boss.state == 1 && self.cutscene.is_none() {
                cr.boss.state = 2;
                let c = &mut cr.slots[TIN_MAN.0];
                c.set_script(crate::creatures::TIN_MAN_FIGHT);
                c.flags |= 8;
            }
            let c = &mut cr.slots[TIN_MAN.0];
            c.pos[1] = c.pos[1].min(-0x2DE11);
        }
        let [_, y, z] = player.pos_obj();
        self.boss_hud = y > -0x436DC && z > -0x4725B && y < -0x21A16 && cr.boss.state == 2;
    }

    /// The creature parts of 0x417680, after the creatures' update.
    pub fn tick_creatures(&mut self, world: &World, specials: &mut Specials, player: &Player, cr: &mut Creatures) {
        let room = world.room_at(player.pos);
        if room != Some(1) || BARS.iter().any(|&s| cr.slots.get(s).is_none_or(|c| c.id != 11)) {
            return;
        }
        // A bar that was hit swings down and settles.
        for (k, &slot) in BARS.iter().enumerate() {
            let c = &mut cr.slots[slot];
            if c.health == BAR_REST {
                continue;
            }
            c.rec.state = 0;
            self.bars[k] += 4;
            if k == 0 {
                c.angles[2] += self.bars[k];
                if c.angles[2] > 0x700 {
                    self.bars[k] = -(self.bars[k] / 2);
                    c.angles[2] = 0x700;
                    self.sounds.push((0x32, c.pos));
                    if self.bars[k] > -8 {
                        c.health = BAR_REST;
                    }
                }
            } else {
                c.angles[2] = (c.angles[2] - self.bars[k]) & 0xFFF;
                if c.angles[2] < 0x900 {
                    self.bars[k] = -(self.bars[k] / 2);
                    c.angles[2] = 0x900;
                    self.sounds.push((0x32, c.pos));
                    if self.bars[k] > -8 {
                        c.health = BAR_REST;
                    }
                }
            }
        }
        let down = BARS.map(|s| cr.slots[s].rec.state == 0);
        if down[0] && down[1] {
            if self.crib == 0 {
                self.crib = 0x90;
                world.set_mover(9, false);
            }
        } else {
            // Shootable (bit 1) only from the far side.
            let state = if player.pos_obj()[0] < 0x4C274 { 4 } else { 5 };
            for (k, &s) in BARS.iter().enumerate() {
                if !down[k] {
                    cr.slots[s].rec.state = state;
                }
            }
            world.set_mover(9, !(player.on_floor && player.surface == Some(0xE)));
        }
        if self.crib > 0 {
            self.crib -= 1;
            self.crib_v += 0x10;
            if self.crib < 1 {
                self.crib = -1;
            }
            if let Some(p) = specials.pos_obj(0x13) {
                if p[1] > 0x11C00 && self.crib_v > 0 {
                    self.crib_v = -(self.crib_v / 2);
                }
                specials.set_pos(0x13, [p[0] >> 5, (p[1] + self.crib_v) >> 5, p[2] >> 5]);
            }
            if let Some(p) = specials.pos_obj(0x14) {
                specials.set_pos(0x14, [p[0] >> 5, (p[1] + self.crib_v) >> 5, p[2] >> 5]);
            }
            if let Some(p) = specials.pos_obj(0x1A) {
                specials.set_pos(0x1A, [p[0] >> 5, (self.crib_v + p[1] * 4) >> 7, p[2] >> 5]);
            }
        }
    }

    fn home(&self, id: i32) -> Option<[i32; 3]> {
        self.home.iter().find(|(i, _)| *i == id).map(|(_, p)| *p)
    }

    /// One tick of 0x417680.
    pub fn tick(&mut self, world: &World, specials: &mut Specials, player: &mut Player, boxes: &Boxes) {
        let room = world.room_at(player.pos);
        let buzz = player.pos_obj();

        self.clock = self.clock.wrapping_add(1);
        if room == Some(5) {
            self.see_saw(world, specials, player);
        }
        if room == Some(4) {
            self.room4_effects(player);
        }
        if room == Some(2) {
            self.lift(specials, player);
        }
        if room == Some(3) {
            self.spring(specials, player);
        }

        // The basement door: box 3 going over its edge.
        if boxes.going_over(3) {
            if self.door == 0 {
                world.set_mover(14, true);
                world.set_mover(1, false);
            }
            self.door = (self.door + 0x10).min(0x200);
            specials.turn_from_own(4, [self.door, 0, 0]);
        }

        // The basement shelf's plank.
        if self.plank == 0 {
            let [x, y, z] = buzz;
            if (-0x58481..-0x4E942).contains(&z) && (0x46377..0x4CBB6).contains(&x) && (1..0x4800).contains(&y) && player.coyote != 0 {
                world.set_mover(8, true);
                self.plank = 8;
            }
        } else if self.plank < 0x1000 {
            self.plank = (self.plank + 0x40).min(0x1000);
            specials.set_scale(0x15, [self.plank, 0x1000, 0x1000]);
            specials.scale_about(0x16, 0x15);
        }

        // Room 4: the trampoline (mover 3).
        if room == Some(4)
            && player.on_floor
            && let Some((3, n)) = player.floor_mover
            && n.y < -0.75
        {
            let vy = if player.stomp != 0 {
                player.stomp = 0;
                -0xC00
            } else {
                -0x980
            };
            player.start_jump(vy, JumpPhase::Released);
            player.sounds.push(Sound { id: 0x1C, voice: false });
        }

        // Room 2: tops and pop-ups.
        if room == Some(2) {
            self.tops.0 = (self.tops.0 + 0x100) & 0xFFF;
            self.tops.1 = (self.tops.1 + 0x40) & 0x1FFF;
            for (k, id) in [9, 0x11, 0x12].into_iter().enumerate() {
                self.top(specials, player, id, self.tops.1 + [0, 0xAAA, 0x1555][k], self.tops.0 + 0x200 * k as i32);
            }
            self.pops.0 = (self.pops.0 + 0x80) & 0xFFF;
            self.pops.1 = (self.pops.1 + 0x40) & 0x1FFF;
            for (k, (id, mover)) in [(10, 0xC), (0x10, 0xD)].into_iter().enumerate() {
                self.pop(world, specials, player, id, mover, self.pops.1 + 0x1000 * k as i32, self.pops.0 + 0x200 * k as i32);
            }
        }

        // The fan.
        self.fan = (self.fan + 0x20) & 0xFFF;
        specials.turn_from_own(0xE, [0, self.fan, 0]);
    }

    /// 0x417510: a top that spins and squashes in a cycle, near Buzz.
    fn top(&self, specials: &mut Specials, player: &mut Player, id: i32, phase: i32, angle: i32) {
        let Some(p) = self.home(id) else { return };
        if !near(p, player.pos_obj(), 0x280) {
            return;
        }
        let mut v = phase & 0x1FFF;
        if v >= 0x800 {
            if v < 0x1000 {
                v = 0x800;
            } else if v < 0x1800 {
                v = 0x1800 - v;
            } else {
                if v - 0x40 < 0x1800 {
                    player.sounds_at.push((Sound { id: 0x24, voice: false }, Vec3::from(p.map(|c| c as f32 / 32.0))));
                }
                v = 0;
            }
        }
        specials.turn_from_own(id, [0, angle & 0xFFF, 0]);
        specials.set_scale(id, [0x1000, (sin_table(v + 0x400) >> 3) + 0xC00, 0x1000]);
    }

    /// 0x417380: a platform that sinks 512 units and rises again, near Buzz; its collision goes
    /// off on the way down (phase 0x400) and back on on the way up (0x1400, sound 0x25).
    #[allow(clippy::too_many_arguments)]
    fn pop(&self, world: &World, specials: &mut Specials, player: &mut Player, id: i32, mover: i32, phase: i32, angle: i32) {
        let Some(p) = self.home(id) else { return };
        if !near(p, player.pos_obj(), 0x280) {
            return;
        }
        let v = phase & 0x1FFF;
        let h = if v < 0x800 {
            if v - 0x40 < 0x400 && v >= 0x400 {
                world.set_mover(mover, false);
            }
            v
        } else if v < 0x1000 {
            0x800
        } else if v < 0x1800 {
            if v - 0x40 < 0x1400 && v >= 0x1400 {
                world.set_mover(mover, true);
                player.sounds_at.push((Sound { id: 0x25, voice: false }, Vec3::from(p.map(|c| c as f32 / 32.0))));
            }
            0x1800 - v
        } else {
            0
        };
        specials.turn_from_own(id, [0, 0, angle & 0xFFF]);
        let y = (p[1] >> 5) - (sin_table(h + 0x400) >> 6) + 0x100;
        specials.set_pos(id, [p[0] >> 5, y, p[2] >> 5]);
    }
}
