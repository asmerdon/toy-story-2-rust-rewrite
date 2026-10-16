//! Level 8 (start-up 0x423020, frame script 0x423200).
//!
//! - Specials 2–9 spin about Y, each at a speed that swings with its own phase.
//! - Hamm (slot 0, list 0xA), the hint character (0x29, list 5), the finder (slot 2, list
//!   0xB: five aliens, token 1).
//! - A race (slot 1, list 0xC): the racer flies along level.dat list 13, speeding up to 3000
//!   (1/65536 of a segment) a tick; Buzz wins by landing on the far ledge first (token 2),
//!   loses by landing anywhere else or when the racer gets there. Lost, it starts over once
//!   the racer is out of sight.
//! - Water in the pool (mode 3 at −0x8200).
//! - The boss (slot 40, list 0x11, in room 5; token 4) with its routine 0x422660.
//!
//! Not ported: the claw machine (room 4: surface 8 stomps, specials 0xA–0xE and 0x10, the
//! prize pickup), 0x422D20, the cannon in room 2, the swaying specials 0x14–0x17 and lights.

use crate::boxes::Boxes;
use crate::creatures::Creatures;
use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::game::{Player, World};
use crate::script::{sin_table, touched, Common, Script};
use crate::specials::Specials;

const HINTS: u32 = 0x4F2F30;
const FINDER_ASK: u32 = 0x4F2AE4;
const FINDER_THANKS: u32 = 0x4F2B58;
const RACE_ASK: u32 = 0x4F2BA0;
const BOSS_TEXT: u32 = 0x4F2A68;

const HAMM: (usize, i16) = (0, 0xA);
const RACER: (usize, i16) = (1, 0xC);
const FINDER: (usize, i16) = (2, 0xB);
const HINT: (usize, i16) = (0x29, 5);
const BOSS: (usize, i16) = (40, 0x11);
/// The racer's path (level.dat list 13).
const TRACK: i16 = 13;

pub struct Level8 {
    /// The spinners' angle and phase (0x52FB20 pairs).
    spin: [(i32, i32); 8],
    /// The race: position along the track (16.16 segments, 0x52FAD8), speed (0x52FADC) and
    /// the racer's bob (0x52FA84).
    t: i32,
    speed: i32,
    bob: i32,
    common: Common,
}

fn in_box(p: [i32; 3], x: (i32, i32), z: (i32, i32)) -> bool {
    p[0] > x.0 && p[0] < x.1 && p[2] > z.0 && p[2] < z.1
}

impl Level8 {
    pub fn new() -> Level8 {
        Level8 { spin: [(0, 0); 8], t: 0, speed: 0, bob: 0, common: Common { given: [false, false, false, true, false], ..Default::default() } }
    }

    pub fn set_story(&mut self, exe: &Exe, lists: &[PointList]) {
        self.story.hints = (0..5).filter_map(|k| exe.at(HINTS + 4 * k, 4).ok().map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect();
        let texts = [crate::script::HAMM_THANKS, crate::script::HAMM_ASK, crate::script::ALL_TOKENS, FINDER_ASK, FINDER_THANKS, RACE_ASK, BOSS_TEXT];
        self.load_story(exe, lists, &texts, None);
    }

    fn race(&mut self, player: &Player, cr: &mut Creatures) {
        if cr.slots.get(RACER.0).is_none_or(|c| c.id == 0) {
            return;
        }
        if touched(cr, RACER.0) && self.race == 0 {
            self.race = 1;
            self.laps = 2;
            self.talk(RACER.0, RACER.1, RACE_ASK, None, None);
            cr.slots[RACER.0].flags &= !0x80;
            self.t = 0;
            self.speed = 0;
        }
        if self.race == 1 && self.cutscene.is_none() {
            self.race = 2;
            self.t = 1;
        }
        let b = player.pos_obj();
        if self.race == 2 && player.on_floor {
            if b[1] < -0x1419A {
                if in_box(b, (-0x7265C, -0x600DC), (0x129C8, 0x19148)) {
                    self.race = 3;
                    self.laps = 3;
                    if !self.given[2] {
                        self.given[2] = true;
                        self.reveals.push(2);
                    }
                } else if !in_box(b, (-0x924C, -0x204C), (-0x5FD94, -0x3DB94)) {
                    self.race = 4;
                }
            } else {
                self.race = 4;
            }
        }
        let pts = self.story.lists.get(&TRACK).cloned().unwrap_or_default();
        if pts.len() < 2 {
            return;
        }
        if self.t != 0 {
            self.speed = (self.speed + 0x10).min(3000);
            if self.race == 2 {
                let at = cr.slots[RACER.0].pos;
                cr.owned_sounds.push((0x7E, at, RACER.0));
            }
            self.t += self.speed;
            if (self.t >> 16) > pts.len() as i32 - 2 {
                cr.slots[RACER.0].flags |= 0x80;
                self.t = pts.len() as i32 * 0x10000 - 0x10001;
                if self.race != 3 {
                    self.race = 4;
                }
            }
        }
        let (i, f) = ((self.t >> 16).max(0) as usize, self.t & 0xFFFF);
        let (a, c) = (pts[i.min(pts.len() - 1)], pts[(i + 1).min(pts.len() - 1)]);
        let p = [0, 1, 2].map(|k| ((c[k] - a[k]) * f >> 11) + a[k] * 32);
        let bobbed = [p[0], p[1] + (sin_table(self.bob) >> 2), p[2]];
        self.bob = (self.bob + 0x20) & 0xFFF;
        let r = &mut cr.slots[RACER.0];
        r.pos = bobbed;
        r.home = bobbed;
        r.target = bobbed;
        if self.race == 4 && !r.active {
            self.t = 0;
            self.speed = 0;
            self.race = 0;
            r.flags |= 0x80;
        }
    }

    /// The boss (0x52FAE0 = `cr.boss.state`): in room 5, Buzz standing up on the stage
    /// starts it (list 0x11); after it the fight script runs (behaviour + 0x1C, facing freed,
    /// its timer 400) and its routine fights; 120 ticks after it's beaten, token 4.
    fn boss(&mut self, room: Option<u8>, player: &Player, cr: &mut Creatures) {
        let b = &mut cr.boss;
        match b.state {
            0 => {
                if player.on_floor && room == Some(5) && player.pos_obj()[1] >= -0x27A7F && cr.slots.get(BOSS.0).is_some_and(|c| c.id == 0x2F) {
                    b.state = 1;
                    b.last = cr.slots[BOSS.0].health;
                    self.talk(BOSS.0, BOSS.1, BOSS_TEXT, None, None);
                }
            }
            1 => {
                if self.cutscene.is_none() {
                    b.state = 2;
                    b.spin = 600;
                    let c = &mut cr.slots[BOSS.0];
                    c.set_script(0x1C / 2);
                    c.rec.face = 0;
                    c.timer = 400;
                }
            }
            2 => {
                self.boss_bar = Some(((cr.slots[BOSS.0].health - 9) * 0x36 / 0x14).max(0));
                self.boss_hud = room == Some(5);
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

impl Default for Level8 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for Level8 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}

impl std::ops::DerefMut for Level8 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Script for Level8 {
    fn step(&mut self, _world: &World, specials: &mut Specials, player: &mut Player, _boxes: &Boxes) {
        self.clock = self.clock.wrapping_add(1);
        // The spinners: phase + k a tick, angle + sin(phase) >> 10.
        for k in 0..8 {
            let (a, ph) = &mut self.spin[k];
            *ph = (*ph + k as i32 + 2) & 0xFFF;
            *a = (*a + (sin_table(*ph) >> 10)) & 0xFFF;
            specials.turn_from_own(k as i32 + 2, [0, *a, 0]);
        }
        let b = player.pos_obj();
        player.water = if in_box(b, (0x3320E, 0x5960E), (0x269CA, 0x49D8A)) { (3, -0x8200) } else { (0, 0) };
    }

    fn step_creatures(&mut self, world: &World, _specials: &mut Specials, player: &Player, cr: &mut Creatures, coins: i32) {
        self.boss_hud = false;
        self.hamm(cr, HAMM.0, HAMM.1, None, 0, coins);
        self.hint_character(cr, HINT.0, HINT.1);
        if touched(cr, FINDER.0) && cr.sheep >= 0 {
            if cr.sheep == 5 {
                self.talk(FINDER.0, FINDER.1, FINDER_THANKS, Some(1), None);
                cr.sheep = -1;
            } else {
                self.talk(FINDER.0, FINDER.1, FINDER_ASK, None, None);
            }
        }
        self.race(player, cr);
        let room = world.room_at(player.pos);
        cr.boss.hud = room == Some(5);
        self.boss(room, player, cr);
    }
}
