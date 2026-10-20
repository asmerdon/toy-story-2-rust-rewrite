//! Level 11, Al's Penthouse (start-up 0x429D70, frame script 0x42A130).
//!
//! - Water: the level's water height (0x52FDFC, object units above its floor, negative = up)
//!   moves towards a target (0x52FD30): down 0x200 a tick, up 0x40, sound 0x95 while it
//!   moves. Four valve buttons (stomped) pick the target (0, −0x7D00, −0xFA00, −0x1C200);
//!   the plug spot drains it. One of five surface meshes (0x4F3FE8) rides on the water,
//!   chosen by height; special 0x5B grows in only at the third level.
//! - Valve buttons (0x4292C0): the pressed one sinks 0xE10 and blinks (0x4293D0).
//! - Pipes (0x428890): three stomp switches change which of seven pipes flow; each pipe
//!   flickers between two meshes (0x428BA0) and re-links the streams the toy boat follows.
//! - The boat (0x428E70) floats along level.dat lists 1–12, linked by the routes table
//!   (0x4F3DF8), knocking Buzz back, pausing at the gate (special 0x1E). Reaching the end
//!   (list 0) switches mover 0x1B on.
//! - Floats (0x429FB0): four platforms bob on the water once it reaches them; a stomp
//!   bounces one.
//! - Turrets (0x4295B0): six, one per room, turn towards Buzz and lob shots (0x428700).
//!   Stomping a turret's button looks at it for 180 ticks, then wrecks it (0x429800).
//! - Doors: hitting the targets in slots 9 and 10 swings their doors open.
//! - Characters: Hamm, the critter owner (slot 1: find his five critters, token 1), the
//!   horse challenge (slot 2: five collectables against the clock, token 2), the hint
//!   character, info signposts.
//! - The gunslinger (slot 11, 0x4282D0): introduced on entering his arena, then a fight
//!   with a boss bar; he flashes when hit, fires from both hands, and retreats home while
//!   Buzz is off the arena's level. Below 10 health he dies; 120 ticks later, token 4.
//!
//! Not ported: lights and the nearest-light marker (0x44F200 on list 19), the room 5
//! screen overlay (0x49B260), slot 31 kept away from a position the exe caches at 0x53C6A8,
//! and the water surfaces' hiding when the camera is under them (it uses Buzz's head).

use std::collections::HashMap;

use crate::boxes::{angle12, Boxes};
use crate::creatures::{Creatures, ParticleRequest};
use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::game::{JumpPhase, Player, Sound, World};
use crate::script::{near, sin_table, touched, Common, Script};
use crate::specials::Specials;

const BOSS: usize = 11;
const BOSS_TEXT: u32 = 0x4F3828;
const CRITTER_ASK: u32 = 0x4F3874;
const CRITTER_THANKS: u32 = 0x4F3900;
const HORSE_START: u32 = 0x4F3948;
const HORSE_HURRY: u32 = 0x4F39C0;
const HORSE_DONE: u32 = 0x4F39EC;
const HINTS: u32 = 0x4F3F04;
const INFO_TABLE: u32 = 0x4F3F24;
/// 0x4F4024: specials hidden at the start.
const HIDDEN: [i32; 33] = [
    0x2C, 0x2B, 0x14, 0x15, 0x16, 0x17, 0x5B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x42,
    0x43, 0x44, 0x45, 0x46, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4F,
];
/// 0x4F3FE8: water surfaces, the first whose height is at or above the water's: (height,
/// special, surface bit for 0x428C80).
const SURFACES: [(i32, i32, u32); 5] = [(-0x3E80, 0x2B, 0x100), (-0x7D00, 0x14, 1), (-0xFA00, 0x15, 2), (-0x176FF, 0x16, 4), (-0x80000, 0x17, 8)];
/// 0x428C80: surface bits and their specials.
const SURFACE_SPECIALS: [(u32, i32); 5] = [(0x100, 0x2B), (1, 0x14), (2, 0x15), (4, 0x16), (8, 0x17)];
/// 0x4F3FCC: valve buttons: mover, valve bit, water level (× −0x640), reward sparkle.
const VALVES: [(i32, u32, i32, usize); 4] = [(7, 0x10, 0, 3), (8, 0x20, 0x14, 0), (6, 0x40, 0x28, 1), (9, 0x80, 0x48, 2)];
/// 0x4F3FAC: valve bit 0x10 << k: its button's mover and special.
const VALVE_BUTTONS: [(i32, i32); 4] = [(7, 1), (8, 2), (6, 3), (9, 4)];
/// The plug: Buzz within 0x80 of it drains the water (valve bit 0x10).
const PLUG: [i32; 3] = [0x2CC2E, 0x2EE2B, -0x2FC03];
/// 0x4F3FDC: pipe switches: mover, reward sparkle, special, the pipe bits it toggles (0:
/// rotates bits 4 → 8 → 0x10).
const SWITCHES: [(i32, usize, i32, u32); 3] = [(1, 0x0A, 8, 3), (3, 0x0B, 9, 0), (4, 0x0C, 0x0A, 0x60)];
/// 0x4F3F48: pipes: bit, flowing mesh, its second frame, the valve handle, and the handle's
/// angles once the pipe stops.
const PIPES: [(u32, i32, i32, i32, [i32; 3]); 7] = [
    (1, 0x24, 0x11, 0x54, [-0x15E, 0, 0]),
    (2, 0x25, 0x12, 0x54, [0, 0, 0]),
    (4, 0x1F, 0x0C, 0x56, [0xAF, 0, 0]),
    (8, 0x20, 0x0D, 0x56, [0x15E, 0, 0]),
    (0x10, 0x21, 0x0E, 0x56, [0, 0, 0]),
    (0x20, 0x22, 0x0F, 0x55, [0, 0, -0x15E]),
    (0x40, 0x23, 0x10, 0x55, [0, 0, 0]),
];
/// What 0x428890 writes into the routes table when a pipe starts: (bit, list, backwards
/// link, next list, next direction).
const PIPE_LINKS: [(u32, usize, bool, i16, i16); 17] = [
    (1, 3, true, 5, 1),
    (1, 1, false, 2, 0),
    (2, 3, true, 1, 1),
    (2, 1, false, 3, 0),
    (4, 4, false, 9, 0),
    (4, 10, true, 2, 1),
    (4, 8, false, 3, 1),
    (8, 4, false, 10, 1),
    (8, 10, true, 4, 1),
    (8, 8, false, 3, 1),
    (0x10, 4, false, 8, 1),
    (0x10, 10, true, 2, 1),
    (0x10, 8, false, 4, 1),
    (0x20, 11, true, 9, 1),
    (0x20, 8, true, 12, 0),
    (0x40, 11, true, 8, 0),
    (0x40, 8, true, 11, 0),
];
/// 0x4F3DF8: by list, where the boat goes at either end: (list, direction) forwards, then
/// backwards; direction 1 runs the list from its end.
const ROUTES: [[i16; 4]; 13] = [
    [0, 0, 0, 0],
    [2, 0, 7, 0],
    [10, 0, 1, 1],
    [8, 1, 5, 1],
    [9, 1, 6, 0],
    [3, 0, 6, 0],
    [1, 0, 4, 0],
    [0, 0, 1, 0],
    [3, 1, 12, 0],
    [11, 0, 4, 1],
    [11, 1, 2, 1],
    [10, 1, 9, 1],
    [11, 1, 8, 0],
];
/// The floats (0x429FB0's arguments): mover, special, the water height that reaches it.
const FLOATS: [(i32, i32, i32); 4] = [(0xD, 0x27, -0xA28), (0xE, 0x28, -0xA28), (0xF, 0x29, -0x10428), (0x10, 0x2A, -0xA28)];
/// 0x4295B0's calls: the turret's room, its special and base, its wrecked bit.
const TURRETS: [(u8, i32, i32, u32); 6] = [(3, 0x32, 0x3B, 1), (2, 0x33, 0x3C, 2), (5, 0x34, 0x3D, 8), (4, 0x35, 0x3E, 0x10), (1, 0x36, 0x3F, 0x20), (2, 0x3A, 0x41, 4)];
/// 0x4F3E74 by wrecked bit k (1 << k): turret, base, then the wreck's two parts.
const WRECKS: [[i32; 4]; 6] = [[0x32, 0x3B, 0x42, 0x49], [0x33, 0x3C, 0x43, 0x4A], [0x3A, 0x41, 0x48, 0x4F], [0x34, 0x3D, 0x44, 0x4B], [0x35, 0x3E, 0x45, 0x4C], [0x36, 0x3F, 0x46, 0x4D]];
/// 0x4F3FB4 by wrecked bit: the turret button's mover, special, its lit special, reward.
const TURRET_BUTTONS: [(i32, i32, i32, usize); 6] = [(2, 0x0B, 0x30, 9), (0x11, 0x38, 0x30, 7), (0x12, 0x39, 0x53, 8), (5, 5, 0x30, 4), (0x0B, 7, 0x30, 6), (0x0A, 6, 0x30, 5)];
/// 0x4F3ED4: where room 4 drips.
const DRIPS: [[i32; 3]; 4] = [[0x42DD1, 0x1F748, 0x42215], [0x3DFD1, 0x1F748, 0x42215], [0x3DFD1, 0x1F748, 0x3C75B], [0x42DD1, 0x1F748, 0x3C75B]];

/// The toy boat (0x428E70): the list it follows (0x52FD4C, 0 = arrived), the point it
/// heads for (0x52FD84) and which way (0x52FDBC), where it is (0x52FD98), its speed
/// (0x52FDF0), yaw (0x52FD0C), smoke timer (0x52FDB4) and the gate's pause (0x52FD24).
struct Boat {
    list: usize,
    point: i32,
    backwards: bool,
    pos: [i32; 3],
    speed: i32,
    yaw: i32,
    smoke: i32,
    pause: i32,
    paused: bool,
}

pub struct Level11 {
    common: Common,
    paths: HashMap<i16, Vec<[i32; 3]>>,
    routes: [[i16; 4]; 13],
    /// 0x52FDFC / 0x52FD30: the water and where it's going.
    water: i32,
    target: i32,
    /// 0x52FD3C: the surface bits shown, the valve pressed (0x10–0x80) and the blink (0x200).
    shown: u32,
    valves: u32,
    blink: bool,
    blink_phase: i32,
    /// 0x52FDAC: special 0x5B's growth.
    grow: i32,
    /// 0x52FD14: pipes flowing (bits 1–0x40) and the flicker frame (0x800).
    pipes: u32,
    flicker: bool,
    flicker_clock: i32,
    /// 0x52FD44 / 0x52FD74: the pipe switch pressed and how long it shows pressed.
    pressed: (i32, i32),
    boat: Boat,
    /// 0x52FD90… : each float's speed and offset.
    floats: [(i32, i32); 4],
    /// 0x52FD58: per turret, ticks until its wreck (0 = standing, −1 = wrecked).
    wreck_timers: [i32; 6],
    /// 0x52FD48: wrecked turrets, and the turret buttons' blink (0x200).
    wrecked: u32,
    button_blink: bool,
    button_phase: i32,
    /// 0x52FD8C: the turrets' firing clock.
    fire_clock: i32,
    /// 0x52FDC0: the shield's mover swap.
    shielded: bool,
    /// 0x52FD54 / 0x52FD38: the doors' swings.
    doors: [i32; 2],
    /// 0x52FD40: 0 waiting, 1 introduced, 2 fighting, 3.. dying, 200 over.
    phase: i32,
    /// 0x52FD2C: the gunslinger's health seen; 0x52FD10: his flash; 0x52FD70: blink.
    health: i32,
    flash: i32,
    blink_boss: bool,
    /// 0x52FD80 / 0x52FDF4: room 2's spouts; 0x52FDB0 / 0x52FDC4: room 4's drips.
    spout: (i32, i32),
    drip: (i32, usize),
    started: bool,
}

impl std::ops::Deref for Level11 {
    type Target = Common;
    fn deref(&self) -> &Common {
        &self.common
    }
}

impl std::ops::DerefMut for Level11 {
    fn deref_mut(&mut self) -> &mut Common {
        &mut self.common
    }
}

impl Level11 {
    pub fn new(world: &World, specials: &mut Specials, lists: &[PointList], exe: Option<&Exe>) -> Self {
        let paths: HashMap<i16, Vec<[i32; 3]>> = lists.iter().map(|l| (l.kind, l.points.clone())).collect();
        let start = paths.get(&1).and_then(|l| l.get(6)).copied().unwrap_or_default().map(|v| v << 5);
        let mut s = Level11 {
            common: Common { show_collectables: Some(false), ..Default::default() },
            paths,
            routes: ROUTES,
            water: 0,
            target: 0,
            shown: 0,
            valves: 0,
            blink: false,
            blink_phase: 0,
            grow: 0,
            pipes: 0,
            flicker: false,
            flicker_clock: 0,
            pressed: (8, 0),
            boat: Boat { list: 1, point: 6, backwards: false, pos: start, speed: 0x100, yaw: 0, smoke: 0x100, pause: 0, paused: false },
            floats: [(0, 0); 4],
            wreck_timers: [0; 6],
            wrecked: 0,
            button_blink: false,
            button_phase: 0,
            fire_clock: 0,
            shielded: false,
            doors: [0; 2],
            phase: 0,
            health: 0,
            flash: 0,
            blink_boss: false,
            spout: (0, 0),
            drip: (0, 0),
            started: false,
        };
        if let Some(e) = exe {
            s.story.hints = (0..5).filter_map(|k| e.at(HINTS + 4 * k, 4).ok().map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect();
            let texts = [
                crate::script::HAMM_THANKS,
                crate::script::HAMM_ASK,
                crate::script::ALL_TOKENS,
                BOSS_TEXT,
                CRITTER_ASK,
                CRITTER_THANKS,
                HORSE_START,
                HORSE_HURRY,
                HORSE_DONE,
            ];
            s.load_story(e, lists, &texts, Some(INFO_TABLE));
        } else {
            s.story.lists = s.paths.clone();
        }
        // The challenge's five pickups and their specials start hidden (0x429CE0).
        for id in 0x65..0x6A {
            specials.set_scale(id, [0; 3]);
        }
        for id in HIDDEN {
            specials.set_scale(id, [0; 3]);
        }
        s.set_valves(world, specials, 0x10);
        s.set_pipes(specials, 0x25);
        specials.set_scale(0x31, [0; 3]);
        specials.set_scale(0x30, [0; 3]);
        world.set_mover(0x1A, false);
        world.set_mover(0x1B, false);
        s
    }

    /// 0x428C80: shows the surfaces in `mask`, hides the rest; none with Buzz's head under
    /// the water.
    fn surfaces(&mut self, specials: &mut Specials, mut mask: u32, head: i32) {
        if self.water + 0x2EA18 < head {
            mask = 0;
        }
        for (bit, id) in SURFACE_SPECIALS {
            let on = mask & bit != 0;
            if on != (self.shown & bit != 0) {
                specials.set_scale(id, if on { [0x1000; 3] } else { [0; 3] });
                self.shown ^= bit;
            }
        }
    }

    /// 0x4292C0: presses the valve buttons in `bits` (0x10–0x80), lets the others up.
    fn set_valves(&mut self, world: &World, specials: &mut Specials, bits: u32) {
        for (k, (mover, special)) in VALVE_BUTTONS.into_iter().enumerate() {
            let bit = 0x10 << k;
            let down = bits & bit != 0;
            if down == (self.valves & bit != 0) {
                continue;
            }
            if let Some(p) = world.mover_pos(mover) {
                let dy = if down { 0xE10 } else { -0xE10 } as f32 / 32.0;
                world.place_mover(mover, p + glam::Vec3::new(0.0, dy, 0.0));
            }
            specials.set_scale(special, if down { [0x1000, 0x960, 0x1000] } else { [0x1000, 0xFFE, 0x1000] });
            self.valves ^= bit;
        }
    }

    /// 0x4293D0: the pressed valve button blinks (special 0x2C in its place).
    fn blink_valve(&mut self, specials: &mut Specials) {
        let Some(k) = (0..4).find(|k| self.valves & (0x10 << k) != 0) else { return };
        let special = VALVE_BUTTONS[k].1;
        self.blink_phase = (self.blink_phase + 0x20) & 0x3FF;
        let on = if !self.blink && self.blink_phase > 0x300 {
            true
        } else if self.blink && self.blink_phase < 0x300 {
            false
        } else {
            return;
        };
        self.blink = on;
        if let Some(p) = specials.pos_obj(special) {
            specials.set_pos(0x2C, p.map(|v| v >> 5));
        }
        let (lit, unlit) = if on { (0x2C, special) } else { (special, 0x2C) };
        specials.set_scale(unlit, [0; 3]);
        specials.set_scale(lit, [0x1000, 0x800, 0x1000]);
    }

    /// 0x428890: the pipes in `bits` flow. Stopping ones hide and turn their handle;
    /// starting ones show and re-link the boat's routes.
    fn set_pipes(&mut self, specials: &mut Specials, bits: u32) {
        for (bit, a, b, handle, angles) in PIPES {
            if self.pipes & bit != 0 && bits & bit == 0 {
                specials.set_scale(a, [0; 3]);
                specials.set_scale(b, [0; 3]);
                self.pipes &= !bit;
                specials.turn_from_own(handle, angles);
            }
        }
        for (bit, a, ..) in PIPES {
            if self.pipes & bit == 0 && bits & bit != 0 {
                specials.set_scale(a, [0x1000; 3]);
                self.pipes |= bit;
                for &(_, list, back, next, dir) in PIPE_LINKS.iter().filter(|l| l.0 == bit) {
                    let k = if back { 2 } else { 0 };
                    self.routes[list][k] = next;
                    self.routes[list][k + 1] = dir;
                }
            }
        }
    }

    /// 0x428BA0: flowing pipes swap between their two meshes, 24 ticks of 32 on the first.
    fn flicker_pipes(&mut self, specials: &mut Specials) {
        self.flicker_clock = (self.flicker_clock + 1) & 0x1F;
        let first = if self.flicker_clock < 0x18 && !self.flicker {
            true
        } else if self.flicker_clock > 0x18 && self.flicker {
            false
        } else {
            return;
        };
        self.flicker = first;
        for (bit, a, b, ..) in PIPES {
            if self.pipes & bit != 0 {
                let (on, off) = if first { (a, b) } else { (b, a) };
                specials.set_scale(on, [0x1000; 3]);
                specials.set_scale(off, [0; 3]);
            }
        }
    }

    fn stomped(player: &Player, mover: i32) -> bool {
        player.stomp == -40 && player.on_floor && player.floor_mover.is_some_and(|(m, _)| m == mover)
    }

    /// The water moving to its target, and what rides on it.
    fn water(&mut self, specials: &mut Specials, player: &mut Player) {
        let buzz = player.pos_obj();
        if self.water != self.target {
            self.sounds.push((0x95, buzz));
            self.water = if self.water < self.target { (self.water + 0x200).min(self.target) } else { (self.water - 0x40).max(self.target) };
        }
        let head = buzz[1] - 0x3000;
        if self.water == 0 {
            player.water = (0, 0);
            self.surfaces(specials, 0, head);
        } else {
            player.water = (1, self.water + 0x2EA18);
            if let Some(&(_, id, mask)) = SURFACES.iter().find(|s| s.0 <= self.water) {
                if let Some(h) = specials.home_obj(id) {
                    specials.set_pos(id, [h[0] >> 5, (h[1] + self.water) >> 5, h[2] >> 5]);
                }
                self.surfaces(specials, mask, head);
            }
        }
        // Special 0x5B grows in at the third level and shrinks away off it.
        const THIRD: i32 = -64000;
        let grown = |g: i32| sin_table(g + 0x155) >> 2;
        if self.water < THIRD && self.grow != 0 {
            self.grow = 0;
            specials.set_scale(0x5B, [0; 3]);
        } else if self.water == THIRD {
            if self.grow < 0x2AB {
                self.grow = (self.grow + 2).min(0x2AB);
                specials.set_scale(0x5B, [grown(self.grow); 3]);
            }
        } else if self.water > THIRD && self.grow != 0 {
            self.grow -= 2;
            let s = if self.grow < 1 {
                self.grow = 0;
                0
            } else {
                grown(self.grow)
            };
            specials.set_scale(0x5B, [s; 3]);
        }
    }

    /// The valve and pipe switches.
    fn switches(&mut self, world: &World, specials: &mut Specials, player: &Player) {
        for (mover, reward, special, toggle) in SWITCHES {
            if !Self::stomped(player, mover) {
                continue;
            }
            self.put_out.push((reward, 1));
            self.pressed = (special, 0x18);
            let bits = if toggle == 0 {
                let p = self.pipes;
                ((p >> 2) & 4) + (p & 0xC) * 2 + (p & !0x1C)
            } else {
                toggle ^ self.pipes
            };
            self.set_pipes(specials, bits);
        }
        for (mover, bit, level, reward) in VALVES {
            if Self::stomped(player, mover) {
                self.set_valves(world, specials, bit);
                self.target = level * -0x640;
                self.put_out.push((reward, 1));
            }
        }
    }

    /// 0x428E70: the toy boat.
    fn boat(&mut self, world: &World, specials: &mut Specials, player: &mut Player) {
        if self.boat.list == 0 {
            return;
        }
        let buzz = player.pos_obj();
        let b = &mut self.boat;
        if near(buzz, b.pos, 0x30) {
            player.hurt(angle12((buzz[0] - b.pos[0]) as f32, (buzz[2] - b.pos[2]) as f32), 1);
            b.speed = 0x10;
            b.smoke = b.smoke.min(0x28);
        } else if b.speed < 0x100 {
            b.speed += 8;
        }
        if !b.paused {
            let path = self.paths.get(&(b.list as i16)).cloned().unwrap_or_default();
            let p = path.get(b.point as usize).copied().unwrap_or(b.pos.map(|v| v >> 5));
            let d: [i32; 3] = std::array::from_fn(|k| p[k] * 4 - b.pos[k] / 8);
            if d.iter().all(|v| v.abs() < 0x400) {
                let end = if b.backwards {
                    b.point -= 1;
                    -1
                } else {
                    b.point += 1;
                    path.len() as i32
                };
                if b.point == end {
                    let r = self.routes.get(b.list).copied().unwrap_or_default();
                    let (list, dir) = if b.backwards { (r[2], r[3]) } else { (r[0], r[1]) };
                    b.list = list.max(0) as usize;
                    b.backwards = dir != 0;
                    b.point = if b.backwards { self.paths.get(&list).map_or(0, |l| l.len() as i32 - 1) } else { 0 };
                }
            }
            // 0x451FD0: the direction scaled to 4096.
            let len = ((d[0] as f64).powi(2) + (d[1] as f64).powi(2) + (d[2] as f64).powi(2)).sqrt();
            let n = if len > 0.0 { d.map(|v| (v as f64 * 4096.0 / len) as i32) } else { [0; 3] };
            for k in 0..3 {
                b.pos[k] += (b.speed * n[k]) >> 10;
            }
            let t = angle12(-n[2] as f32, -n[0] as f32);
            let u = (b.yaw - t) & 0xFFF;
            b.yaw += if u < 0x801 { -(u >> 2) } else { (0x1000 - u) >> 2 };
            specials.turn_from_own(0x26, [0, b.yaw, 0]);
            specials.turn_from_own(0x50, [0, b.yaw, 0]);
        }
        specials.set_pos(0x26, b.pos.map(|v| v >> 5));
        specials.set_pos(0x50, b.pos.map(|v| v >> 7));
        let (pos, yaw) = (b.pos, b.yaw);
        if self.common.every(4) && near(buzz, pos, 0x300) {
            let spin = self.common.rand() - 0x80;
            let at = [pos[0] - (sin_table(yaw + 0x400) >> 2), pos[1] - 0x4000, pos[2] - (sin_table(yaw) >> 2)];
            self.common.spawns.push((at, 0x11, 0x1B, spin, None));
            self.boat.smoke -= 1;
            if self.boat.smoke < 1 {
                self.boat.smoke = self.common.rand() + 0x1E;
                self.common.sounds.push((0x97, pos));
            }
        }
        let b = &mut self.boat;
        let gate = specials.pos_obj(0x1E).is_some_and(|g| near(g, b.pos, 0x60));
        if b.pause < 1 {
            if gate {
                b.pause = 0xB4;
            }
            b.paused = gate;
        } else {
            b.pause -= 1;
            if gate {
                b.pause = 0xB4;
            }
        }
        if b.list == 0 {
            world.set_mover(0x1B, true);
        }
    }

    /// 0x429FB0: float `k` bobbing on the water.
    fn float(&mut self, world: &World, specials: &mut Specials, player: &Player, k: usize) {
        let (mover, special, reach) = FLOATS[k];
        let Some(m) = world.mover_pos(mover) else { return };
        let m = [m.x, m.y, m.z].map(|v| (v * 32.0) as i32);
        let depth = if reach < self.water { 0 } else { self.water - reach };
        let (mut v, mut y) = self.floats[k];
        if Self::stomped(player, mover) {
            v = 0x500;
        }
        v += if y < 0 { 0x30 } else { -0x30 };
        let was = y;
        y += v;
        if y >= 0 && was < 0 {
            v = if v > 0x200 { v >> 1 } else { 0x200 };
            if depth != 0 {
                self.spawns.push(([m[0], m[1] - y, m[2]], 0x39, 2, 0, None));
            }
        }
        if !(-0x800..=0x2000).contains(&y) {
            v = 0;
        }
        self.floats[k] = (v, y);
        specials.set_pos(special, m.map(|c| c >> 5));
        let home = specials.home_obj(special).map_or(m[1], |h| h[1]);
        let to = if depth > -0x1000 { home + depth - y * depth / 4096 } else { home + y + depth };
        let d = (to - m[1]).clamp(-0x800, 0x800);
        world.move_mover(mover, glam::Vec3::new(0.0, d as f32 / 32.0, 0.0));
    }

    /// 0x428700: a turret's lobbed shot at Buzz from `at`, aimed along `yaw`, at most every
    /// 300 ticks and only from 2000 level units out.
    fn lob(&mut self, at: [i32; 3], yaw: i32, buzz: [i32; 3]) {
        if self.fire_clock <= 1000 {
            return;
        }
        let (r1, r2) = (self.rand(), self.rand());
        let dx = (at[0] + ((r1 - 0x80) << 7) - buzz[0]) >> 5;
        let dz = (at[2] + ((r2 - 0x80) << 7) - buzz[2]) >> 5;
        let d = ((dx as f64 * dx as f64 + dz as f64 * dz as f64).sqrt()) as i32;
        if d < 2000 {
            return;
        }
        let reach = d * 2 / 3;
        let t = (d << 12) / reach;
        let from = [at[0], at[1] - 0x4000, at[2]];
        let vel = [sin_table(yaw + 0x400) * reach / 0x4000, (buzz[1] - from[1]) * 0x80 / t - t / 2, sin_table(yaw - 0x800) * reach / 0x4000];
        self.particles.push(ParticleRequest { pos: from, vel, gravity: 0x80, angle: 0, spin: 0, typ: 0x5C });
        self.sounds.push((0x92, at));
    }

    /// 0x4295B0: a turret in `room` turning to Buzz and firing, knocking him back when he's on
    /// it; wrecked, it smokes.
    fn turret(&mut self, specials: &mut Specials, player: &mut Player, room: Option<u8>, t: (u8, i32, i32, u32)) {
        let (in_room, special, base, bit) = t;
        let Some(at) = specials.pos_obj(special) else { return };
        let buzz = player.pos_obj();
        if room != Some(in_room) || !near(at, buzz, 0x480) {
            return;
        }
        let wrecked = self.wrecked & bit != 0;
        if near(buzz, at, 0x32) {
            player.hurt(angle12((buzz[0] - at[0]) as f32, (buzz[2] - at[2]) as f32), if wrecked { 1 } else { 3 });
        }
        if !wrecked {
            let want = angle12(-(buzz[2] - at[2]) as f32, (buzz[0] - at[0]) as f32);
            let mut yaw = specials.angles(special).unwrap_or_default()[1];
            let u = (want - yaw) & 0xFFF;
            yaw += if u < 0x800 { u.min(8) } else { -(0x1000 - u).min(8) };
            specials.turn_from_own(special, [0, yaw, 0]);
            specials.turn_from_own(base, [0, yaw, 0]);
            self.lob(at, yaw, buzz);
        } else if self.every(7) {
            let spin = self.rand() - 0x80;
            self.spawns.push(([at[0], at[1] - 0x2000, at[2]], 0x11, 0x1A, spin, None));
        }
    }

    /// 0x429910: the turret buttons. Each blinks while Buzz is near; stomped, it looks at its
    /// turret for 180 ticks, then the turret is wrecked (0x429800).
    fn turret_buttons(&mut self, world: &World, specials: &mut Specials, player: &Player) {
        let buzz = player.pos_obj();
        for k in 0..6 {
            if self.wreck_timers[k] != 0 {
                continue;
            }
            let (mover, button, lit, reward) = TURRET_BUTTONS[k];
            let Some(at) = specials.pos_obj(button) else { continue };
            if near(at, buzz, 0x460) {
                if !near(at, buzz, 0x3E0) {
                    if self.button_blink {
                        self.button_blink = false;
                        specials.set_scale(button, [0x1000; 3]);
                        specials.set_scale(lit, [0; 3]);
                    }
                } else if !self.button_blink && self.button_phase > 0x300 {
                    self.button_blink = true;
                    specials.set_pos(lit, at.map(|v| v >> 5));
                    specials.set_scale(button, [0; 3]);
                    specials.set_scale(lit, [0x1000; 3]);
                } else if self.button_blink && self.button_phase < 0x300 {
                    self.button_blink = false;
                    specials.set_scale(button, [0x1000; 3]);
                    specials.set_scale(lit, [0; 3]);
                }
            }
            if Self::stomped(player, mover) {
                specials.set_scale(button, [0x1000, 0x800, 0x1000]);
                if let Some(p) = world.mover_pos(mover) {
                    world.place_mover(mover, p + glam::Vec3::new(0.0, 0xE10 as f32 / 32.0, 0.0));
                }
                self.wreck_timers[k] = 0x3C;
                if let Some(turret) = specials.pos_obj(WRECKS[k][0]) {
                    self.look_at(turret, buzz, 0xB4, 0x20);
                    if let Some(l) = self.look.as_mut() {
                        l.at[1] -= 0x2000;
                        l.from[1] -= 0x4000;
                    }
                }
                specials.set_scale(lit, [0; 3]);
                self.put_out.push((reward, 1));
            }
        }
    }

    /// 0x429800: turret `k` is wrecked: its wreck shows turned as it was, smoke and the jingle.
    fn wreck(&mut self, specials: &mut Specials, k: usize) {
        let [turret, base, part, part2] = WRECKS[k];
        let yaw = specials.angles(turret).unwrap_or_default()[1];
        for id in [part, part2] {
            specials.turn_from_own(id, [0, yaw, 0]);
            specials.set_scale(id, [0x1000; 3]);
        }
        specials.set_scale(turret, [0; 3]);
        specials.set_scale(base, [0; 3]);
        let at = specials.pos_obj(turret).unwrap_or_default();
        for _ in 0..4 {
            let spin = self.rand() - 0x80;
            self.spawns.push(([at[0], at[1] - 0x3000, at[2]], 0x23, 0xE, spin, None));
        }
        self.sounds.push((0xFFFE, at));
    }

    /// The pressed pipe switch shows as special 0x31 for 24 ticks.
    fn pressed_switch(&mut self, specials: &mut Specials) {
        let (special, t) = self.pressed;
        if t == 0 {
            return;
        }
        if t == 0x18 {
            if let Some(p) = specials.pos_obj(special) {
                specials.set_pos(0x31, p.map(|v| v >> 5));
            }
            specials.set_scale(special, [0; 3]);
            specials.set_scale(0x31, [0x1000; 3]);
        }
        let t = (t - 1).max(0);
        self.pressed.1 = t;
        if t == 0 {
            specials.set_scale(special, [0x1000; 3]);
            specials.set_scale(0x31, [0; 3]);
        }
    }

    /// The two doors swinging open once their targets are hit.
    fn doors(&mut self, world: &World, specials: &mut Specials, cr: &Creatures) {
        for (k, (slot, mover, start, ids)) in [(10, 0x17, 0x3AE, [0x19, 0x1A]), (9, 0x16, -0x424, [0x1C, 0x5A])].into_iter().enumerate() {
            if self.doors[k] == 0 && cr.slots.get(slot).is_some_and(|c| c.health != 1) {
                self.doors[k] = 1;
            }
            let t = self.doors[k];
            if t == 0 {
                continue;
            }
            if t == 1 {
                world.turn_mover(mover, [0, start, 0]);
            }
            // Slot 10's door turns sin / 0x12, slot 9's −2 sin / 0x21.
            let a = if k == 0 { sin_table(t) / 0x12 } else { sin_table(t) * -2 / 0x21 };
            for id in ids {
                specials.turn_from_own(id, [0, a, 0]);
            }
            if t < 0x400 {
                self.doors[k] += 0x10;
            }
        }
    }

    /// The horse challenge (slot 2): five collectables before the clock runs out, in rooms
    /// 1, 4 and 8.
    fn horses(&mut self, specials: &mut Specials, cr: &mut Creatures, room: Option<u8>) {
        if touched(cr, 2) {
            let at = cr.slots[2].pos;
            if self.race == 0 {
                self.sounds.push((0x91, at));
                self.talk(2, 0x12, HORSE_START, None, None);
                self.collect_reset = true;
                self.race = 1;
                self.show_collectables = Some(true);
                for id in 0x65..0x6A {
                    specials.set_scale(id, [0x1000; 3]);
                }
            } else if self.collected < 5 {
                self.sounds.push((0x91, at));
                self.talk(2, 0x12, HORSE_HURRY, None, None);
            } else if self.race != 3 {
                self.sounds.push((0x91, at));
                self.talk(2, 0x12, HORSE_DONE, Some(2), None);
                self.race = 3;
                return;
            }
        }
        if self.race == 1 && self.cutscene.is_none() {
            self.race = 2;
            self.laps = 0x7F;
        }
        if self.race == 2 {
            if !matches!(room, Some(1 | 4 | 8)) {
                self.laps = 99;
            }
            if self.every(64) {
                self.laps -= 1;
            }
            if self.laps < 100 {
                self.laps = 100;
                self.race = 0;
                self.show_collectables = Some(false);
                for id in 0x65..0x6A {
                    specials.set_scale(id, [0; 3]);
                }
            }
        }
    }

    /// 0x4282D0 and the frame script's part: the gunslinger.
    fn boss(&mut self, player: &Player, cr: &mut Creatures) {
        if cr.slots.len() <= BOSS {
            return;
        }
        let buzz = player.pos_obj();
        if self.phase == 0 {
            let inside = buzz[0] > -0xB17CE && buzz[0] < -0xA02CE && buzz[2] > -0x3AF7C && buzz[2] < -0x2B52C;
            if inside && buzz[1] < 0x1F448 {
                self.phase = 1;
                self.talk(BOSS, 0x10, BOSS_TEXT, None, None);
            }
        } else if self.phase == 1 {
            if self.cutscene.is_none() {
                self.phase = 2;
                self.boss_hud = true;
                let c = &mut cr.slots[BOSS];
                c.set_script(0x1C / 2);
                c.rec.face = 0;
            }
        } else if self.phase < 0x78 && self.phase > 2 {
            self.phase += 1;
        } else if self.phase >= 0x78 && self.phase != 200 {
            self.reveals.push(4);
            self.given[4] = true;
            self.phase = 200;
        }
        self.blink_boss = !self.blink_boss;
        let c = &mut cr.slots[BOSS];
        if c.health != self.health {
            self.flash = 0x3C;
            self.health = c.health;
            c.rec.state = 4;
            self.common.sounds.push((0xA5, c.pos));
        }
        c.lit = false;
        if self.phase == 2 {
            self.flash -= 1;
            if self.flash < 0 {
                self.flash = 0;
                c.rec.state = 7;
            } else {
                c.lit = self.blink_boss;
            }
        }
        // Off the arena's level he goes home and can't be hurt.
        if buzz[1] < 0x1BCA1 || buzz[1] > 0x22405 {
            c.target[0] = c.home[0];
            c.target[2] = c.home[2];
            c.rec.state = 4;
        } else if self.flash == 0 {
            c.rec.state = 7;
        }
        // His script arms +0x8A: the right hand fires, then ten ticks later the left.
        if c.timer != 0 {
            let mut node = None;
            if c.timer > 0x14 {
                c.timer = 10;
                node = Some((15, 0x93));
            }
            c.timer -= 1;
            if c.timer < 1 {
                c.timer = 0;
                node = Some((16, 0x94));
            }
            if let Some((node, sound)) = node {
                let facing = cr.slots[BOSS].angles[1];
                let flags = cr.slots[BOSS].flags;
                let at = cr.node_point(BOSS, node, [0.0; 3]).unwrap_or(cr.slots[BOSS].pos);
                self.common.sounds.push((sound, at));
                let aim = angle12((buzz[0] - at[0]) as f32, (buzz[2] - at[2]) as f32);
                let a = if ((aim - facing + 0x100) & 0xFFF) > 0x200 { facing } else { aim };
                self.common.particles.push(ParticleRequest { pos: at, vel: [sin_table(a) >> 2, 0x200, sin_table(a + 0x400) >> 2], gravity: 0, angle: 0, spin: 0, typ: 0x61 });
                if flags & 1 != 0 {
                    for _ in 0..5 {
                        let spin = self.common.rand() - 0x80;
                        self.common.spawns.push((at, 100, 0xF, spin, None));
                    }
                }
            }
        }
        let c = &mut cr.slots[BOSS];
        if self.phase == 2 {
            self.common.boss_bar = Some((c.health - 9) * 0x36 / 0x14);
        }
        if c.health < 10 && self.phase == 2 {
            c.set_script(0x68 / 2);
            c.rec.state = 4;
            c.flags &= !0x100;
            self.common.sounds.push((0xFFFE, c.pos));
            self.phase = 3;
            self.common.boss_bar = Some(0);
            c.wait = 0;
        }
        if self.phase > 1 {
            if c.pos[2] < -0x5B4F0 {
                c.home[0] = -0x9D020;
                c.home[2] = -0x62040;
                c.rec.zone = [299, 0x70];
            } else {
                c.home[0] = -0xA85A0;
                c.home[2] = -0x43FC0;
                c.rec.zone = [0x60, 0x17E];
            }
        }
        if self.phase == 200 {
            c.pos[0] = c.home[0];
            c.pos[2] = c.home[2];
        }
    }
}

impl Script for Level11 {
    fn step(&mut self, world: &World, specials: &mut Specials, player: &mut Player, boxes: &Boxes) {
        self.clock = self.clock.wrapping_add(1);
        let room = world.room_at(player.pos);
        // The shield swaps movers 0x19 and 0x1A.
        if player.shield == 0 {
            if self.shielded {
                world.set_mover(0x1A, false);
                world.set_mover(0x19, true);
                self.shielded = false;
            }
        } else if !self.shielded {
            world.set_mover(0x19, false);
            world.set_mover(0x1A, true);
            self.shielded = true;
        }
        self.water(specials, player);
        self.switches(world, specials, player);
        self.flicker_pipes(specials);
        self.turret_buttons(world, specials, player);
        self.blink_valve(specials);
        if near(PLUG, player.pos_obj(), 0x80) {
            self.set_valves(world, specials, 0x10);
            self.target = 0;
        }
        for k in 0..4 {
            self.float(world, specials, player, k);
        }
        self.boat(world, specials, player);
        self.pressed_switch(specials);
        self.fire_clock += 1;
        if self.fire_clock > 300 {
            self.fire_clock += 1000;
        }
        self.button_phase = (self.button_phase + 0x20) & 0x3FF;
        for t in TURRETS {
            self.turret(specials, player, room, t);
        }
        if self.fire_clock > 1000 {
            self.fire_clock -= 0x514;
        }
        // Mover 0xC is a spring: standing on its top (contact normal y below −0x3000 of
        // 0x4000), Buzz is thrown up, higher from a stomp.
        if player.on_floor && player.floor_mover.is_some_and(|(m, n)| m == 0xC && n.y < -0.75) {
            self.put_out.push((0xD, 1));
            let vy = if player.stomp != 0 {
                player.stomp = 0;
                -0xC00
            } else {
                -0x980
            };
            player.start_jump(vy, JumpPhase::Released);
            player.sounds.push(Sound { id: 0x1C, voice: false });
        }
        // The push boxes' children follow them.
        for (child, parent) in [(0x57, 0x51), (0x40, 0x1D)] {
            if let Some(p) = specials.pos_obj(parent) {
                specials.set_pos(child, p.map(|v| v >> 7));
            }
        }
        for k in 0..6 {
            if self.wreck_timers[k] > 0 {
                self.wreck_timers[k] -= 1;
                if self.wreck_timers[k] < 1 {
                    self.wreck_timers[k] = -1;
                    self.wreck(specials, k);
                    self.wrecked |= 1 << k;
                }
            }
        }
        if self.look.is_some() {
            self.fire_clock = 0;
        }
        // Room 2's spouts, alternately every 100 ticks.
        if room == Some(2) {
            self.spout.0 -= 1;
            if self.spout.0 < 0 {
                self.spout.1 = if self.spout.1 == 0 { 1 } else { 0 };
                let (x, z, v) = if self.spout.1 == 1 { (-0x5C7E2, -0x55251, 0x180) } else { (-0x5E7B0, -0x5721F, -0x180) };
                let spin = self.rand() - 0x80;
                self.particles.push(ParticleRequest { pos: [x, 0x187A8, z], vel: [v, -0x800, v], gravity: 0, angle: 0, spin, typ: 0x4C });
                self.spout.0 = 100;
            }
        }
        // Room 1's steam and room 4's drips.
        if room == Some(1) && self.every(2) && near(player.pos_obj(), [0x1187C, 0x2BD92, -0x13B72], 0x300) {
            let (r1, r2) = (self.rand(), self.rand());
            self.spawns.push(([0x1187C + (r1 - 0x80) * 0x20, 0x2BD92, -0x13B72 + (r2 - 0x80) * 0x40], 0x65, 10, 0, None));
        }
        if room == Some(4) {
            self.drip.0 -= 1;
            if self.drip.0 < 1 {
                self.drip.0 = 0x3C;
                self.drip.1 = (self.drip.1 + 1) % 4;
                let at = DRIPS[self.drip.1];
                self.spawns.push((at, 0x74, 2, 0, None));
            }
        }
        if boxes.pushed() == 2 {
            self.put_out.push((1, 0));
        }
    }

    fn step_creatures(&mut self, world: &World, specials: &mut Specials, player: &Player, cr: &mut Creatures, coins: i32) {
        if !std::mem::replace(&mut self.started, true) && cr.slots.len() > BOSS {
            // The gunslinger waits pushed back; his health is remembered.
            cr.slots[BOSS].pos[2] += 0x10000;
            self.health = cr.slots[BOSS].health;
        }
        let room = world.room_at(player.pos);
        self.hamm(cr, 0, 0xF, None, 0, coins);
        self.hint_character(cr, 12, 0x14);
        let open = cr.sheep >= 0;
        self.call(cr, 1, 0xB2, open);
        self.finder(cr, 1, 0x11, CRITTER_ASK, CRITTER_THANKS, None, 1);
        self.horses(specials, cr, room);
        self.doors(world, specials, cr);
        self.boss(player, cr);
    }
}
