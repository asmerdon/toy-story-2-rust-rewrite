//! Level 2 (Andy's neighbourhood): the start-up (0x418E50) and frame script (0x4190C0).
//!
//! - Two see-saws (movers / specials 0 and 1, turned 0x961 and 0xB4A, mode 0).
//! - Water: the pool past x −0x246FF (mode 1, surface 0x4400) and the pond (mode 2, 0x1800).
//! - Hamm (slot 0xF, token 0), the hint Hamm (slot 0x1B), the sarge (slot 6) who wants his
//!   five troops found (token 1) and the Zurg kite boss on the roof (slot 0x1A, token 4).
//!   (The script also has level 1's RC race for slot 0x1D, but level 2 has only 28 slots.)
//! - Two stomp switches (surfaces 8 and 9): each sinks its button (specials 2 / 3), shows its
//!   marker (0x20 / 0x21) and puts out its reward sparkle; with one down the planks
//!   (specials 4, 8, 9, 10) sway, with both they settle, the gate (mover 5) closes the old way
//!   (mover 6) and the spring (special 0x15, surface 10) appears.
//! - A rope on pole 0 that lowers special 0x19 once climbed, stretching special 0x1B.
//!
//! Pushable drawbridge (5 / 0xB) and tree (0x1E / 0x1F) swap collision as they fall.
//! - The fifth troop (slot 4) is in the lawn's holes (list 2, seven points): it pops up at one,
//!   ducks when Buzz comes near and the hole's dirt shows for a while; stomping that hole while
//!   it shows fills it in (a mound stays). With six filled the troop comes out and can be
//!   collected (0x4190C0's mole game, see `Level2::troop_holes`).
//! Not ported: the kite's string and sky effects, the dirt model over a hole (0x49E910).
//! The pool ball (special 6, mover 7) inflates from the squash pad (surface 11).

use crate::boxes::Boxes;
use crate::creatures::Creatures;
use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::game::{Player, Sound, World};
use crate::script::{sin_table, Common, Script, Tilt};
use crate::specials::Specials;

const INFO_TABLE: u32 = 0x4F153C;
const HINTS: u32 = 0x4F1528;
const SARGE_ASK: u32 = 0x4F0FC0;
const SARGE_THANKS: u32 = 0x4F1078;
const KITE_TEXT: u32 = 0x4F10BC;

const HAMM: (usize, i16) = (0xF, 8);
const HINT: (usize, i16) = (0x1B, 0xB);
const SARGE: (usize, i16) = (6, 7);
const KITE: (usize, i16) = (0x1A, 9);

pub struct Level2 {
    saws: [Tilt; 2],
    /// The switches' buttons (0x52F6C8 / 0x52F6CC: 0x1000 up, then sinking to 0), the planks'
    /// sway (0x52F6D8 phase, 0x52F614 / 0x52F618 tilt) and the door state (0x52F610).
    buttons: [i32; 2],
    sway: (i32, i32, i32),
    door: u8,
    /// The spring (0x52F66C).
    spring: i32,
    /// The rope (pole 0, special 0x19): speed (0x52F6A8), height (0x52F684) and where.
    rope_v: i32,
    rope: [i32; 3],
    /// Pole 0 has been put where the rope hangs (0x418E50 does it at the start).
    pole_set: bool,
    /// Pool ball and pump, 0x52F648 / 0x52F6B4 / 0x52F674.
    ball: [i32; 3],
    ball_scale: i32,
    ball_velocity: [i32; 2],
    pad: i32,
    pad_target: i32,
    pad_cooldown: i32,
    inflating: bool,
    bridge_angle: i32,
    bridge_speed: i32,
    tree_angle: i32,
    tree_speed: i32,
    holes: Holes,
    common: Common,
}

/// The troop's holes (list 2, level units): 0x52F5DC where the troop is, 0x52F5E8 where it
/// ducked from, 0x52F61C the ticks its dirt shows, 0x52F6D0 the holes filled (the original
/// marks one with y 0x80000000) and 0x52F6D4 the scan for the mounds.
#[derive(Default)]
struct Holes {
    points: Vec<[i32; 3]>,
    filled: Vec<bool>,
    at: usize,
    last: usize,
    timer: i32,
    count: i32,
    scan: usize,
}

impl Level2 {
    /// 0x418E50's part that sets up objects.
    pub fn new(world: &World, specials: &mut Specials) -> Level2 {
        for id in [5, 4, 0xC] {
            world.set_mover(id, false);
        }
        for id in [0x15, 0x20, 0x21] {
            specials.set_scale(id, [0; 3]);
        }
        specials.set_scale(0x1B, [0x1000, 0x118, 0x1000]);
        specials.turn_from_own(5, [0, 0, 0x2EE]);
        let rope = specials.pos_obj(0x19).unwrap_or_default();
        let l = Level2 {
            saws: Default::default(),
            buttons: [0x1000; 2],
            sway: (0xC00, 0, 0),
            door: 0,
            spring: 0,
            rope_v: 0,
            rope,
            pole_set: false,
            ball: specials.pos_obj(6).unwrap_or_default(),
            ball_scale: 0xFFF,
            ball_velocity: [0; 2],
            pad: 0,
            pad_target: 0,
            pad_cooldown: 0,
            inflating: false,
            bridge_angle: 0x2EE,
            bridge_speed: 0,
            tree_angle: 0,
            tree_speed: 0,
            holes: Holes::default(),
            common: Common { given: [false, false, false, true, false], ..Default::default() },
        };
        // 0x488AA0: the see-saws' turns.
        world.turn_mover(0, [0, 0x961, 0]);
        world.turn_mover(1, [0, 0xB4A, 0]);
        l
    }

    pub fn set_story(&mut self, exe: &Exe, lists: &[PointList]) {
        if let Some(l) = lists.iter().find(|l| l.kind == 2) {
            self.holes.points = l.points.clone();
            self.holes.filled = vec![false; l.points.len()];
        }
        self.story.hints = (0..5).filter_map(|k| exe.at(HINTS + 4 * k, 4).ok().map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect();
        let texts = [crate::script::HAMM_THANKS, crate::script::HAMM_ASK, crate::script::ALL_TOKENS, SARGE_ASK, SARGE_THANKS, KITE_TEXT];
        self.load_story(exe, lists, &texts, Some(INFO_TABLE));
    }


    /// 0x4190C0's troop in the holes (slot 4, pinned to its target height while it has flag
    /// 0x20). It ducks (target y 0x5000) when Buzz comes within 768 and then comes up at the
    /// next unfilled hole; its dirt shows for 100 ticks and a stomp (recovery below -0x14)
    /// within 48 of that hole fills it in. Six filled frees the troop (flags |= 0x84,
    /// &= ~0x20). Mounds (0x3A) show at the filled holes near the camera (here: Buzz).
    fn troop_holes(&mut self, player: &Player, cr: &mut Creatures) {
        let Some(c) = cr.slots.get_mut(4) else { return };
        let h = &mut self.holes;
        let n = h.points.len();
        if n == 0 || c.id == 0 {
            return;
        }
        let buzz = player.pos_obj();
        let near = |a: [i32; 3], b: [i32; 3]| (0..3).map(|k| { let d = ((a[k] - b[k]) >> 5) as i64; d * d }).sum::<i64>() < 0x90000;
        if c.active && c.flags & 0x20 != 0 && near(buzz, c.pos) && c.target[1] != 0x5000 {
            c.target[1] = 0x5000;
            h.timer = 100;
            h.last = h.at;
            self.common.sounds.push((0x31, c.pos));
        }
        if c.pos[1] > 0x4800 {
            let usable = |h: &Holes, k: usize| !h.filled[k] && k != h.last;
            h.at += 1;
            if h.at >= n {
                h.at -= n;
            }
            if !usable(h, h.at) {
                let mut tries = 0;
                loop {
                    h.at += 1;
                    if h.at >= n {
                        h.at = 0;
                    }
                    tries += 1;
                    if usable(h, h.at) || tries >= n {
                        break;
                    }
                }
            }
            let p = h.points[h.at].map(|v| v << 5);
            c.pos = [p[0], 0x3800, p[2]];
            c.home = p;
            c.target = p;
        }
        // The mounds at filled holes (one hole scanned a tick).
        let from_hole = |buzz: [i32; 3], p: [i32; 3]| [buzz[1] + 0xBB0, buzz[2] - p[2], buzz[0] - p[0]].map(|v| (v >> 8) as i64).iter().map(|v| v * v).sum::<i64>();
        if h.filled[h.scan] {
            let p = h.points[h.scan].map(|v| v << 5);
            if from_hole(buzz, p) < 0x40000 {
                self.common.spawns.push(([p[0], -0xBB0, p[2]], 0x3A, 3, 0, None));
            }
        }
        h.scan = (h.scan + 1) % n;
        if h.timer > 0 {
            h.timer -= 1;
            if h.timer == 79 {
                let p = h.points[h.last].map(|v| v << 5);
                self.common.spawns.push(([p[0], -0x23B0, p[2]], 0x3B, 2, 0, None));
            }
        }
        if player.on_floor && player.stomp < -0x14 && h.at != h.last && h.timer != 0 {
            let p = h.points[h.last].map(|v| v << 5);
            if from_hole(buzz, p) < 0x900 {
                h.filled[h.last] = true;
                h.count += 1;
                h.timer = 0;
                if h.count > 5 {
                    c.flags = (c.flags & !0x20) | 0x84;
                }
                self.common.pop.push(0x3B);
            }
        }
    }

    /// The two pushed obstacles, after the pump/switch/rope updates in 0x4190C0.
    fn obstacles(&mut self, world: &World, specials: &mut Specials, player: &mut Player, boxes: &Boxes) {
        if self.bridge_speed != i32::MIN {
            self.bridge_speed = if self.bridge_angle < 0x299 { self.bridge_speed + 1 }
                else if boxes.pushing_index() == 1 { 8 } else { 0 };
            let before = self.bridge_angle;
            self.bridge_angle -= self.bridge_speed / 2;
            if before >= 0x299 && self.bridge_angle < 0x299 {
                world.set_mover(2, false);
                world.set_mover(3, false);
                world.set_mover(4, true);
                boxes.release(player);
            }
            if self.bridge_angle < 1 {
                self.bridge_speed = -(self.bridge_speed >> 2);
                self.bridge_angle = 0;
                if self.bridge_speed.abs() < 4 { self.bridge_speed = i32::MIN; }
                else if let Some(pos) = specials.pos_obj(5) { self.sounds.push((0x34, pos)); }
            }
            specials.turn_from_own(5, [0, 0, self.bridge_angle]);
            specials.turn_from_own(0xB, [0, 0, self.bridge_angle]);
        }
        if self.tree_angle == 0 {
            if player.pos_obj()[1] < -0x102F9 {
                if boxes.progress(1) != 0 {
                    self.tree_angle = 1;
                    world.set_mover(0xB, false);
                    world.set_mover(0xC, true);
                    player.vel[0] = 0;
                    player.vel[2] = 0;
                    boxes.release(player);
                }
            } else { boxes.reset_progress(1); }
        } else {
            if boxes.pushing_index() == 2 { boxes.release(player); }
            if self.tree_angle < 0x245 {
                self.tree_angle += self.tree_speed;
                if self.tree_angle >= 0x245 {
                    self.tree_angle = 0x245;
                    if let Some(pos) = specials.pos_obj(0x1E) { self.sounds.push((0x34, pos)); }
                }
                specials.turn_from_own(0x1E, [0, 0, 0xFFF - self.tree_angle]);
                specials.turn_from_own(0x1F, [0, 0, 0xFFF - self.tree_angle]);
                self.tree_speed += 1;
            }
        }
    }

    /// Pump and buoyant ball (0x4190C0). Velocities and positions are object units.
    fn pool_ball(&mut self, world: &World, specials: &mut Specials, player: &Player) {
        self.pad_cooldown = (self.pad_cooldown - 1).max(0);
        if player.surface == Some(11) && player.coyote != 0 {
            if self.pad_cooldown == 0 && self.pad_target == 0 {
                self.put_out.push((3, 1));
                self.sounds.push((0x38, player.pos_obj()));
                self.pad_cooldown = 30;
                self.pad_target = 0xC00;
            }
        } else {
            self.pad_target = 0;
        }
        if self.ball_scale == 0x1000 {
            let on_ball = player.floor_mover.is_some_and(|(id, _)| id == 7)
                && !player.wall_mover.is_some_and(|(id, _)| id == 7);
            if player.stomp == -40 && on_ball && player.pos_obj()[1] < 0x4400 {
                self.ball_velocity[1] = 0x5A8;
            }
            specials.set_pos(6, self.ball.map(|v| v >> 5));
            self.ball_velocity[0] = (self.ball_velocity[0] - 4).max(0);
            self.ball_velocity[1] = if self.ball[1] < 0x4400 {
                (self.ball_velocity[1] + 0x30).min(0x600)
            } else { self.ball_velocity[1] - 0x30 };
            self.ball[0] += self.ball_velocity[0];
            let y = self.ball[1] + self.ball_velocity[1];
            if y >= 0x4400 && self.ball[1] < 0x4400 {
                let splash = [self.ball[0], 0x4400, self.ball[2]];
                self.spawns.push((splash, 0x39, 2, 0, None));
                if player.stomp == 0 {
                    self.ball_velocity[1] = if self.ball_velocity[1] > 0x21F { self.ball_velocity[1] >> 1 } else { 0x21F };
                }
                let sound_pos = [self.ball[0], y, self.ball[2]];
                self.sounds.push((0x39, sound_pos));
            }
            self.ball[1] = y;
            let pos = glam::Vec3::from([self.ball[0], y - 0x1000, self.ball[2]].map(|v| v as f32 / 32.0));
            if let Some(old) = world.mover_pos(7) { world.move_mover(7, pos - old); }
        } else {
            if self.inflating {
                self.ball_scale += if player.stomp < 0 { 0x30 } else { 0x20 };
                if self.ball_scale >= 0x1000 {
                    self.ball_scale = 0x1000;
                    self.ball_velocity = [0x352, -0x712];
                    let pos = self.ball;
                    self.sounds.push((0xB, pos));
                }
            } else { self.ball_scale = (self.ball_scale - 10).max(0x400); }
            specials.set_scale(6, [0x1000, self.ball_scale, 0x1000]);
        }
        if self.pad != 0 { specials.set_scale(7, [0x1000, 0x1000 - self.pad, 0x1000]); }
        self.inflating = self.pad - self.pad_target < -0x80;
        self.pad -= (self.pad - self.pad_target) / 8;
    }

    /// The switches, planks, door and spring (see the module notes).
    fn switches(&mut self, world: &World, specials: &mut Specials, player: &mut Player) {
        let surface = if player.on_floor && player.stomp != 0 { player.surface.filter(|s| (8..=0xB).contains(s)) } else { None };
        let mut sound = None;
        for (k, (surf, shown, hidden)) in [(8u8, 0x20, 0x22), (9, 0x21, 0x23)].into_iter().enumerate() {
            if self.buttons[k] == 0x1000 && surface == Some(surf) {
                self.buttons[k] = 0xFFF;
                sound = Some(0x38);
                self.put_out.push((k, 1));
                specials.set_scale(shown, [0x1000; 3]);
                specials.set_scale(hidden, [0; 3]);
            }
        }
        let mut down = 0;
        for (k, id) in [2, 3].into_iter().enumerate() {
            if self.buttons[k] != 0x1000 {
                down |= 1 << k;
                if self.buttons[k] > 0 {
                    self.buttons[k] = (self.buttons[k] - 0x40).max(0);
                    specials.set_scale(id, [0x1000, self.buttons[k], 0x1000]);
                }
            }
        }
        let planks = [4, 8, 9, 10];
        if down == 1 || down == 2 {
            let (phase, _, _) = self.sway;
            let a = (sin_table(phase) >> 9) + 0x20;
            self.sway.0 = (phase + 0x20) & 0xFFF;
            let x = if down == 1 { -a } else { a };
            self.sway.1 = a;
            self.sway.2 = x;
            for id in planks {
                specials.turn_from_own(id, [x, 0, a]);
            }
        } else if down == 3 {
            if self.door == 0 {
                self.door = 1;
                world.set_mover(5, true);
                world.set_mover(6, false);
            }
            let mut moving = false;
            if self.sway.1 < 0x80 {
                self.sway.1 = (self.sway.1 + 2).min(0x80);
                moving = true;
            }
            if self.sway.2 > 0 {
                self.sway.2 = (self.sway.2 - 2).max(0);
                moving = true;
            } else if self.sway.2 < 0 {
                self.sway.2 = (self.sway.2 + 2).min(0);
                moving = true;
            }
            if moving {
                for id in planks {
                    specials.turn_from_own(id, [self.sway.2, 0, self.sway.1]);
                }
            } else if self.door == 1 {
                self.door = 2;
                specials.set_scale(0x15, [0x1000; 3]);
                specials.set_scale(4, [0; 3]);
                sound = Some(0x3B);
            }
        }
        if let Some(id) = sound {
            self.sounds.push((id, [0x59280, 0x1000, -0x5C127]));
        }
        // The spring on surface 10, once the door is done (0x52F66C): as level 1's.
        let on = surface == Some(10);
        let mut count = false;
        if on && self.spring == 0 {
            if self.door != 0 {
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
                    self.put_out.push((2, 1));
                    player.sounds.push(Sound { id: 0x1C, voice: false });
                    player.launch(-0x1080, 0);
                }
            } else {
                self.spring = -0x20;
            }
        }
        if on || self.spring != 0 {
            specials.turn_from_own(0x15, [(-self.spring.abs() & 0x7F) << 5, 0, 0]);
        }
    }

    /// The rope (pole 0): climbing it lowers special 0x19 (speeding up by 0x10 a tick to
    /// 0x800, sound 0x21) until −0x2500; the rope (special 0x1B) stretches with it, and the
    /// pole slides only while it moves.
    fn rope(&mut self, specials: &mut Specials, player: &mut Player) {
        if self.rope_v == 0 {
            if player.pole_held() != Some(0) {
                return;
            }
            self.rope_v = 2;
            player.sounds.push(Sound { id: 0x21, voice: false });
            player.set_pole_kind(0, 2);
        }
        if self.rope_v <= 0 {
            return;
        }
        let before = self.rope[1];
        self.rope[1] += self.rope_v;
        self.rope_v = (self.rope_v + 0x10).min(0x800);
        if self.rope[1] > -0x2500 {
            self.rope[1] = -0x2500;
            player.set_pole_kind(0, 0);
            self.rope_v = -1;
        }
        player.stretch_pole(0, self.rope[1] - before);
        specials.set_pos(0x19, self.rope.map(|v| v >> 5));
        let s = (((self.rope[1] + 0x5826C) * 0x100) / 0x55D2).clamp(0, 0x1000);
        specials.set_scale(0x1B, [0x1000, s, 0x1000]);
    }

    /// The Zurg kite (0x52F664 is `cr.boss.state`): Buzz standing up on the roof's middle
    /// starts the challenge (list 9); after it the kite's fight script runs (behaviour entry
    /// + 0x1C); its bar is health × 54 / 10; once it's gone, 120 ticks later, token 4.
    fn kite(&mut self, player: &Player, cr: &mut Creatures) {
        let [x, y, z] = player.pos_obj();
        let b = &mut cr.boss;
        self.boss_hud = std::mem::take(&mut b.hud);
        match b.state {
            0 => {
                if player.on_floor && y < -0x72000 && x > 0x20CC6 && x < 0x35FC6 && z > 0x2D174 && z < 0x4C7F4 && cr.slots.get(KITE.0).is_some_and(|c| c.id == 0xF) {
                    cr.boss.state = 1;
                    cr.boss.last = cr.slots[KITE.0].health;
                    self.talk(KITE.0, KITE.1, KITE_TEXT, None, None);
                }
            }
            1 => {
                if self.cutscene.is_none() {
                    b.state = 2;
                    cr.slots[KITE.0].set_script(0x1C / 2);
                }
            }
            2 => {
                let k = &cr.slots[KITE.0];
                if k.id != 0 && k.health > 0 {
                    self.boss_bar = Some(k.health * 0x36 / 10);
                } else {
                    b.state = 3;
                    self.boss_bar = Some(0);
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
}

impl std::ops::Deref for Level2 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}

impl std::ops::DerefMut for Level2 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Script for Level2 {
    fn step(&mut self, world: &World, specials: &mut Specials, player: &mut Player, boxes: &Boxes) {
        self.clock = self.clock.wrapping_add(1);
        if !std::mem::replace(&mut self.pole_set, true) {
            player.move_pole(0, self.rope[1]);
        }
        self.saws[0].step(world, specials, player, 0, 0, 0x961, 0, -0x1C0, 0x1C0, 8);
        self.saws[1].step(world, specials, player, 1, 1, 0xB4A, 0, -0x1C0, 0xE0, 8);
        player.water = if player.pos_obj()[0] < -0x246FF { (1, 0x4400) } else { (2, 0x1800) };
        self.pool_ball(world, specials, player);
        self.switches(world, specials, player);
        self.rope(specials, player);
        self.obstacles(world, specials, player, boxes);
    }

    fn step_creatures(&mut self, _world: &World, _specials: &mut Specials, player: &Player, cr: &mut Creatures, coins: i32) {
        self.hamm(cr, HAMM.0, HAMM.1, Some([0xC00, 0x400]), 0, coins);
        self.hint_character(cr, HINT.0, HINT.1);
        self.call(cr, SARGE.0, 0xB2, cr.sheep >= 0);
        self.finder(cr, SARGE.0, SARGE.1, SARGE_ASK, SARGE_THANKS, Some([0x700, 0x200]), 1);
        self.kite(player, cr);
        self.troop_holes(player, cr);
    }
}
