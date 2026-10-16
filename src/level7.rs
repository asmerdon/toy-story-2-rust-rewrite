//! Level 7 (start-up 0x421090, frame script 0x421340).
//!
//! - Path movers 7–9 (specials 0xF–0x11, programs 0x4F294C / 0x4F2970 / 0x4F2994) and 0xD /
//!   0xC / 0xB (specials 0xC–0xE, 0x4F29B8 / 0x4F29DC / 0x4F2A00); the second three only run
//!   once the lock holding each (slots 7–9, laser targets) is shot.
//! - A spring on surface 8 (thrown along 0x81E) and a trampoline on surface 9 (special 3
//!   squashes).
//! - The finder (slot 0xC, list 4: five chicks, token 1), the hatching egg challenge (slot
//!   0xD, list 8: reach the egg, slot 6, before it hatches; the second time quicker, token 2),
//!   Mr Potato Head (slot 0xE: rocket boots), Hamm (1, list 3), the hint character (0x1F,
//!   list 0xB).
//! - The dino boss (slot 0, list 5, in room 4; token 4) with its routine 0x420AF0.
//!
//! Not ported: the bobbing platform and button race (mover 0 / 0xF, 0x52FA24), the rockets
//! (movers 10 / 0xE), the chicks pushed by boxes (0x420F70), the drips and sparks.

use crate::boxes::Boxes;
use crate::creatures::Creatures;
use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::game::{JumpPhase, Player, Sound, World};
use crate::script::{touched, Common, PathRunner, Script};
use crate::specials::Specials;

const INFO_TABLE: u32 = 0x4F2A54;
const HINTS: u32 = 0x4F2A34;
const FINDER_ASK: u32 = 0x4F23A4;
const FINDER_THANKS: u32 = 0x4F2418;
const EGG_FIRST: u32 = 0x4F2460;
const EGG_HURRY: u32 = 0x4F24F0;
const EGG_AGAIN: u32 = 0x4F2520;
const POTATO_TEXTS: [u32; 3] = [0x4F21B8, 0x4F223C, 0x4F22EC];
const DINO_TEXT: u32 = 0x4F2358;

const DINO: (usize, i16) = (0, 5);
const HAMM: (usize, i16) = (1, 3);
const FINDER: (usize, i16) = (0xC, 4);
const EGG_HEN: (usize, i16) = (0xD, 8);
const EGG: usize = 6;
const POTATO: usize = 0xE;
const HINT: (usize, i16) = (0x1F, 0xB);

/// Path movers: mover, special, program, and the lock slot that must be gone first.
const PATHS: [(i32, i32, u32, Option<usize>); 6] = [
    (7, 0xF, 0x4F294C, None),
    (8, 0x10, 0x4F2970, None),
    (9, 0x11, 0x4F2994, None),
    (0xD, 0xC, 0x4F29B8, Some(7)),
    (0xC, 0xD, 0x4F29DC, Some(8)),
    (0xB, 0xE, 0x4F2A00, Some(9)),
];

pub struct Level7 {
    paths: [PathRunner; 6],
    path_flags: u16,
    /// The spring (0x52F9C0) and the trampoline's squash (0x52F9EC).
    spring: i32,
    squash: i32,
    /// The egg: 0 first try, 1 second, 2 done (0x52FA1C); the gate lift's goal and height
    /// (0x52FA10 / 0x52FA08).
    egg: u8,
    gate_goal: i32,
    gate: i32,
    common: Common,
}

impl Level7 {
    pub fn new(world: &World, specials: &mut Specials, exe: Option<&Exe>) -> Level7 {
        for id in [0x1E, 3] {
            specials.set_scale(id, [0x1000, 0, 0x1000]);
        }
        let _ = world;
        let load = |va: u32| exe.map(|e| PathRunner::load(e, va)).unwrap_or_default();
        Level7 {
            paths: PATHS.map(|p| load(p.2)),
            path_flags: 0,
            spring: 0,
            squash: 0,
            egg: 0,
            gate_goal: 0,
            gate: 0,
            // 0x503A30: level 7's missing item (1) and gift (bit 2, the rocket boots).
            common: Common { given: [false, false, false, true, false], item: 1, ..Default::default() },
        }
    }

    pub fn set_story(&mut self, exe: &Exe, lists: &[PointList]) {
        self.story.hints = (0..5).filter_map(|k| exe.at(HINTS + 4 * k, 4).ok().map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect();
        let mut texts = vec![crate::script::HAMM_THANKS, crate::script::HAMM_ASK, crate::script::ALL_TOKENS, FINDER_ASK, FINDER_THANKS, EGG_FIRST, EGG_HURRY, EGG_AGAIN, DINO_TEXT];
        texts.extend(POTATO_TEXTS);
        self.load_story(exe, lists, &texts, Some(INFO_TABLE));
    }

    /// The spring (surface 8) and the trampoline (surface 9), as level 1's.
    fn springs(&mut self, specials: &mut Specials, player: &mut Player) {
        let n = self.spring.abs();
        specials.set_scale(0x1E, [0x1000, n * 0x80, 0x1000]);
        specials.turn_from_own(2, [0, 0, n * 0x20]);
        let on = player.on_floor && player.surface == Some(8);
        let mut count = false;
        if on && self.spring == 0 {
            if player.stomp != 0 {
                self.put_out.push((1, 1));
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
                    player.sounds.push(Sound { id: 0x1C, voice: false });
                    player.launch_at(-0xC00, 0x81E, 3);
                }
            } else {
                self.spring = -0x20;
            }
        }
        if player.on_floor && player.surface == Some(9) {
            player.launched = false;
            self.squash = 1;
            let vy = if player.stomp != 0 {
                player.stomp = 0;
                -0xC00
            } else {
                -0x980
            };
            player.start_jump(vy, JumpPhase::Released);
            player.sounds.push(Sound { id: 0x1C, voice: false });
        }
        if self.squash != 0 {
            if self.squash < 0x3001 {
                let s = crate::script::sin_table(self.squash) >> (((self.squash >> 0xB) & 0xE) + 3);
                specials.set_scale(3, [0x1000, s, 0x1000]);
                self.squash += 0x100;
            } else {
                self.squash = 0;
                specials.set_scale(3, [0x1000, 0, 0x1000]);
            }
        }
    }

    /// The hatching egg (0x52F2F8 = `race`, its clock `laps`).
    fn egg_challenge(&mut self, world: &World, player: &Player, cr: &mut Creatures) {
        let pos = cr.slots.get(EGG_HEN.0).map_or([0; 3], |c| c.pos);
        if touched(cr, EGG_HEN.0) {
            if self.race != 0 {
                self.sounds.push((0x72, pos));
                self.talk(EGG_HEN.0, EGG_HEN.1, EGG_HURRY, None, None);
            } else if self.egg != 2 {
                self.sounds.push((0x72, pos));
                self.race = 1;
                self.gate_goal = 0x4000;
                if self.egg == 0 {
                    self.laps = 0x96;
                    self.talk(EGG_HEN.0, EGG_HEN.1, EGG_FIRST, None, None);
                } else {
                    self.laps = 0x7E;
                    self.talk(EGG_HEN.0, EGG_HEN.1, EGG_AGAIN, Some(2), None);
                }
            }
        }
        if self.race == 1 && self.cutscene.is_none() {
            self.race = 2;
        }
        if self.race == 2 {
            if self.egg == 0 && cr.slots.get(EGG).is_none_or(|c| c.id == 0 || c.health <= 0) {
                // Reached in time: the egg's done; again, quicker, for the token.
                self.egg = 1;
                self.race = 0;
                self.gate_goal = 0;
            } else if self.egg == 1 && self.tokens & 4 != 0 {
                self.egg = 2;
                self.race = 0;
            } else {
                if world.room_at(player.pos) == Some(4) {
                    self.laps = 99;
                }
                if self.every(64) {
                    self.laps -= 1;
                }
                if self.laps < 100 {
                    self.laps = 100;
                    self.race = 0;
                    self.gate_goal = 0;
                }
            }
        }
        if let Some(e) = cr.slots.get_mut(EGG) {
            if self.race != 0 {
                e.flags |= 0x81;
            } else {
                e.flags &= 0x7E | 0xFF00;
            }
        }
    }

    /// The gate lift (special 0x1F, mover 0x12) the egg challenge raises.
    fn gate(&mut self, world: &World, specials: &mut Specials) {
        if self.gate < self.gate_goal {
            self.gate = (self.gate + 0x200).min(self.gate_goal);
        } else if self.gate > self.gate_goal {
            self.gate = (self.gate - 0x200).max(self.gate_goal);
        }
        world.set_mover(0x12, self.gate == 0);
        if let Some(h) = specials.home_obj(0x1F) {
            specials.set_pos(0x1F, [h[0] >> 5, (h[1] - self.gate) >> 5, h[2] >> 5]);
        }
    }

    /// The dino (0x52FA14 = `cr.boss.state`): in room 4, Buzz standing low enough starts it
    /// (list 5); after it the fight script runs (behaviour + 0x1C, facing freed); its routine
    /// ends the fight; 120 ticks later, token 4.
    fn dino(&mut self, room: Option<u8>, player: &Player, cr: &mut Creatures) {
        let b = &mut cr.boss;
        match b.state {
            0 => {
                if player.on_floor && player.pos_obj()[1] >= -0x63B && room == Some(4) && cr.slots.get(DINO.0).is_some_and(|c| c.id == 0x1A) {
                    b.state = 1;
                    b.last = cr.slots[DINO.0].health;
                    self.talk(DINO.0, DINO.1, DINO_TEXT, None, None);
                }
            }
            1 => {
                if self.cutscene.is_none() {
                    b.state = 2;
                    let c = &mut cr.slots[DINO.0];
                    c.set_script(0x1C / 2);
                    c.rec.face = 0;
                }
            }
            2 => {
                self.boss_bar = Some(((cr.slots[DINO.0].health - 9) * 0x36 / 0x14).max(0));
                self.boss_hud = room == Some(4);
            }
            3..0x78 => {
                b.state += 1;
                self.boss_bar = Some(0);
            }
            0x78 => {
                if !self.given[4] {
                    self.given[4] = true;
                    self.reveals.push(4);
                }
                b.state = 200;
            }
            _ => {}
        }
    }
}

impl std::ops::Deref for Level7 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}

impl std::ops::DerefMut for Level7 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Script for Level7 {
    fn step(&mut self, world: &World, specials: &mut Specials, player: &mut Player, _boxes: &Boxes) {
        self.clock = self.clock.wrapping_add(1);
        self.springs(specials, player);
        self.gate(world, specials);
    }

    fn step_creatures(&mut self, world: &World, specials: &mut Specials, player: &Player, cr: &mut Creatures, coins: i32) {
        self.boss_hud = false;
        // The paths, the second three held by their locks.
        let lists = std::mem::take(&mut self.common.story.lists);
        for (k, &(m, s, _, lock)) in PATHS.iter().enumerate() {
            if lock.is_some_and(|l| cr.slots.get(l).is_some_and(|c| c.health > 0)) {
                continue;
            }
            let mut flags = self.path_flags;
            let mut rand = || self.common.rand();
            self.paths[k].step(world, specials, m, s, &mut flags, &lists, &mut rand);
            self.path_flags = flags;
        }
        self.common.story.lists = lists;
        // The locks can be shot (record state 5).
        for l in 7..=9 {
            if let Some(c) = cr.slots.get_mut(l).filter(|c| c.id == 30) {
                c.rec.state = 5;
            }
        }
        let room = world.room_at(player.pos);
        self.dino(room, player, cr);
        if touched(cr, FINDER.0) && cr.sheep >= 0 {
            let at = cr.slots[FINDER.0].pos;
            self.sounds.push((0x76, at));
            if cr.sheep == 5 {
                self.talk(FINDER.0, FINDER.1, FINDER_THANKS, Some(1), None);
                cr.sheep = -1;
            } else {
                self.talk(FINDER.0, FINDER.1, FINDER_ASK, None, None);
            }
        }
        self.egg_challenge(world, player, cr);
        let list = if self.item < 1 { 2 } else { 10 };
        // Level 7's gift (0x503A31): the rocket boots.
        self.potato(cr, POTATO, list, POTATO_TEXTS, [0x600, 0xE00], 2);
        self.hamm(cr, HAMM.0, HAMM.1, Some([0xE10, 0x6E0]), 0, coins);
        self.hint_character(cr, HINT.0, HINT.1);
    }
}
