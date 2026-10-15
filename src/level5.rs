//! Level 5 (start-up 0x41E390, frame script 0x41E880).
//!
//! - Eight gliding platforms (0x41E150) in pairs along level.dat lists 0, 5, 2 and 3: each
//!   heads for the point a parameter t (0x1000 a segment) gives along its list, moving at
//!   speed × 4 object units a tick, t stepping on as it gets there and wrapping to the start;
//!   a pair stops while Buzz stands on either.
//! - Two see-saws (movers 2 / 1, specials 4 / 6, mode 2, ±0x1C0, /1); specials 5 / 7 copy them.
//! - A trampoline (mover 3).
//! - Water (mode 1) with its surface at 0x70000 past z 0xF329F, else 0x10000.
//! - The clown boss (slot 3, list 0xD, in room 2; token 4) with its routine 0x41DDB0.
//! - Hamm (0x14, list 0xF), the hint character (0x22, list 0x14), the finder (0x13, list 0x10:
//!   five baby ducks, token 1) and a timed challenge (0x12, list 0xE: five bones, token 2).
//!
//! Not ported: the stomp-started machine and the bubbles it blows (specials 0xC / 0xD and
//! 0x12 / 0x13 carrying poles 0 / 1), the gate that falls (0x53C6E8; movers 0x12 / 0x13,
//! specials 2 / 3), the drips along list 62, underwater fog and sounds.

use crate::boxes::Boxes;
use crate::creatures::Creatures;
use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::game::{JumpPhase, Player, Sound, World};
use crate::script::{touched, Common, Script, Tilt};
use crate::specials::Specials;
use glam::Vec3;

const HINTS: u32 = 0x4F2184;
const CLOWN_TEXT: u32 = 0x4F1D10;
const FINDER_ASK: u32 = 0x4F1D5C;
const FINDER_THANKS: u32 = 0x4F1DEC;
const CHALLENGE_ASK: u32 = 0x4F1E3C;
const CHALLENGE_HURRY: u32 = 0x4F1EB0;
const CHALLENGE_DONE: u32 = 0x4F1ED4;

const CLOWN: (usize, i16) = (3, 0xD);
const HAMM: (usize, i16) = (0x14, 0xF);
const HINT: (usize, i16) = (0x22, 0x14);
const FINDER: (usize, i16) = (0x13, 0x10);
const CHALLENGE: (usize, i16) = (0x12, 0xE);

/// The gliding platforms: mover, list, specials, speed, starting t; pairs share a stop.
const GLIDES: [(i32, i16, i32, i32, i32, i32); 8] = [
    (8, 0, 0x1D, 0x21, 0x60, 0),
    (5, 0, 0x1E, 0x22, 0x60, 0x3C000),
    (6, 5, 0x1F, 0x23, 0x60, 0),
    (7, 5, 0x20, 0x24, 0x60, 0x3C000),
    (9, 2, 0x15, 0, 0xC0, 0),
    (10, 2, 0x16, 0, 0xC0, 0x3C000),
    (11, 3, 0x17, 0, 0xC0, 0),
    (12, 3, 0x18, 0, 0xC0, 0x3C000),
];

pub struct Level5 {
    glide: [i32; 8],
    placed: bool,
    saws: [Tilt; 2],
    common: Common,
}

/// 0x41E020: the point `t` (0x1000 a segment) along a list, level units.
fn along(list: &[[i32; 3]], t: i32) -> [i32; 3] {
    let i = (t >> 12).max(0) as usize;
    let f = t & 0xFFF;
    let a = list.get(i).copied().unwrap_or_default();
    let b = list.get(i + 1).copied().unwrap_or(a);
    [0, 1, 2].map(|k| ((b[k] - a[k]) * f >> 12) + a[k])
}

impl Level5 {
    pub fn new(specials: &mut Specials, world: &World) -> Level5 {
        world.set_mover(0x13, false);
        let _ = specials;
        Level5 {
            glide: GLIDES.map(|g| g.5),
            placed: false,
            saws: Default::default(),
            // 0x41E300: the challenge's five things are hidden until it starts.
            common: Common { given: [false, false, false, true, false], show_collectables: Some(false), ..Default::default() },
        }
    }

    pub fn set_story(&mut self, exe: &Exe, lists: &[PointList]) {
        self.story.hints = (0..5).filter_map(|k| exe.at(HINTS + 4 * k, 4).ok().map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect();
        let texts = [crate::script::HAMM_THANKS, crate::script::HAMM_ASK, crate::script::ALL_TOKENS, CLOWN_TEXT, FINDER_ASK, FINDER_THANKS, CHALLENGE_ASK, CHALLENGE_HURRY, CHALLENGE_DONE];
        self.load_story(exe, lists, &texts, None);
    }

    /// 0x41E150 for platform `k`.
    fn glide(&mut self, world: &World, specials: &mut Specials, k: usize) {
        let (m, list, s1, s2, speed, _) = GLIDES[k];
        let Some(pts) = self.story.lists.get(&list).cloned() else { return };
        let Some(at) = world.mover_pos(m) else { return };
        let t = &mut self.glide[k];
        let cur = if *t > (pts.len() as i32 - 2) * 0x1000 {
            *t = 0;
            let p = Vec3::from(along(&pts, 0).map(|v| v as f32));
            world.move_mover(m, p - at);
            p
        } else {
            let p = along(&pts, *t);
            let d = Vec3::new(p[0] as f32 - at.x, p[1] as f32 - at.y, p[2] as f32 - at.z);
            if d.x * 32.0 < 4096.0 {
                *t += 0x1000;
            }
            // Speed × 4 object units a tick along the way.
            world.move_mover(m, d.normalize_or_zero() * (speed * 4) as f32 / 32.0);
            at
        };
        let c = [cur.x as i32, cur.y as i32, cur.z as i32];
        if s2 != 0 {
            specials.set_pos(s2, c);
        }
        specials.set_pos(s1, c);
    }

    /// The clown (0x52F8E8 = `cr.boss.state`): in room 2, Buzz on the ground starts it (list
    /// 0xD); after it the clown's fight script runs (behaviour + 0x1C, facing freed); its bar
    /// shows in room 2; once it's gone, token 4.
    fn clown(&mut self, room: Option<u8>, player: &Player, cr: &mut Creatures) {
        let alive = cr.slots.get(CLOWN.0).is_some_and(|c| c.id != 0 && c.health > 0);
        let b = &mut cr.boss;
        match b.state {
            0 => {
                if player.on_floor && room == Some(2) && alive {
                    b.state = 1;
                    b.last = cr.slots[CLOWN.0].health;
                    self.talk(CLOWN.0, CLOWN.1, CLOWN_TEXT, None, None);
                }
            }
            1 => {
                if self.cutscene.is_none() {
                    b.state = 2;
                    let c = &mut cr.slots[CLOWN.0];
                    c.set_script(0x1C / 2);
                    c.rec.face = 0;
                }
            }
            2 => {
                if !alive {
                    b.state = 3;
                    self.boss_bar = Some(0);
                } else {
                    self.boss_bar = Some(cr.slots[CLOWN.0].health * 0x36 / 0x14);
                    self.boss_hud = room == Some(2);
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
    }

    /// The timed challenge (as level 4's: clock from 0x82).
    fn challenge(&mut self, cr: &mut Creatures) {
        if touched(cr, CHALLENGE.0) {
            if self.race == 0 {
                self.voices.push(0xB6);
                self.talk(CHALLENGE.0, CHALLENGE.1, CHALLENGE_ASK, None, None);
                self.race = 1;
                self.collect_reset = true;
                self.show_collectables = Some(true);
            } else if self.collected < 5 {
                self.talk(CHALLENGE.0, CHALLENGE.1, CHALLENGE_HURRY, None, None);
            } else if self.race != 3 {
                self.voices.push(0xB7);
                self.talk(CHALLENGE.0, CHALLENGE.1, CHALLENGE_DONE, Some(2), None);
                self.race = 3;
            }
        }
        if self.race == 1 && self.cutscene.is_none() {
            self.race = 2;
            self.laps = 0x82;
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
}

impl std::ops::Deref for Level5 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}

impl std::ops::DerefMut for Level5 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Script for Level5 {
    fn step(&mut self, world: &World, specials: &mut Specials, player: &mut Player, _boxes: &Boxes) {
        self.clock = self.clock.wrapping_add(1);
        if !std::mem::replace(&mut self.placed, true) {
            for (k, g) in GLIDES.iter().enumerate() {
                if let (Some(pts), Some(at)) = (self.story.lists.get(&g.1), world.mover_pos(g.0)) {
                    world.move_mover(g.0, Vec3::from(along(pts, g.5).map(|v| v as f32)) - at);
                }
                let _ = k;
            }
            world.clear_motion();
        }
        let on = player.floor_mover.filter(|_| player.on_floor).map(|(m, _)| m);
        for pair in [[0, 1], [2, 3], [4, 5], [6, 7]] {
            if pair.iter().any(|&k| on == Some(GLIDES[k].0)) {
                continue;
            }
            for k in pair {
                self.glide(world, specials, k);
            }
        }
        self.saws[0].step(world, specials, player, 2, 4, 0, 2, -0x1C0, 0x1C0, 1);
        self.saws[1].step(world, specials, player, 1, 6, 0, 2, -0x1C0, 0x1C0, 1);
        if let Some(a) = specials.angles(4) {
            specials.turn_from_own(5, a);
        }
        if let Some(a) = specials.angles(6) {
            specials.turn_from_own(7, a);
        }
        // The trampoline (mover 3), as level 1's.
        if player.on_floor
            && let Some((3, n)) = player.floor_mover
            && n.y < -0.75
        {
            self.put_out.push((1, 1));
            let vy = if player.stomp != 0 {
                player.stomp = 0;
                -0xC00
            } else {
                -0x980
            };
            player.start_jump(vy, JumpPhase::Released);
            player.sounds.push(Sound { id: 0x1C, voice: false });
        }
        let z = player.pos_obj()[2];
        player.water = (1, if z > 0xF329F { 0x70000 } else { 0x10000 });
    }

    fn step_creatures(&mut self, world: &World, _specials: &mut Specials, player: &Player, cr: &mut Creatures, coins: i32) {
        self.boss_hud = false;
        let room = world.room_at(player.pos);
        self.clown(room, player, cr);
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
        let open = self.race == 0;
        self.call(cr, CHALLENGE.0, 0xB5, open);
        self.challenge(cr);
    }
}
