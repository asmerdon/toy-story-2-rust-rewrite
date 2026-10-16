//! Pickups: coins, batteries, Pizza Planet tokens, extra lives and power-ups, as the original
//! sets them up, draws and collects them.
//!
//! - 0x447DB0 builds one list at level start: the `level.dat` list 63 entries (x, y, z, id),
//!   then every special object from a per-level first id (0x30 in level 1) up, taking position
//!   and size from the `level.dat` object records (0x54F628).
//! - Ids below 0x30 are sprites (16 is the spinning coin); the rest are `.ngn` special objects,
//!   whose kind 0x44E520 picks by the polygon count of their PS1 model.
//! - 0x440F70 (each frame) draws the sprites with a floor shadow, tumbles the special objects,
//!   and marks everything within the draw distance of the camera as collectable.
//! - 0x4A0F80 (each frame) collects marked entries near Buzz's chest and applies them.
//!
//! Positions are level units (+Y down), as `level.dat` stores them.

use crate::formats::exe::Exe;
use crate::formats::leveldat::{self, Object, PickupPoint};
use crate::formats::ngn::Ngn;
use crate::game::World;
use crate::scene::{self, Batch};

/// What a pickup does, from the PS1 model's polygon count (0x44E520) or, for sprites, coin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Sprites (ids below 0x30): one coin, up to 99 (`0x52F39E`); 50 plays sound 0x4F.
    Coin,
    /// 32 polygons: health + 4, up to 14 (`0x52F396`).
    Battery,
    /// 60 polygons: 900 ticks without damage (0x4A50D0, `0x52F398`); the object grows to
    /// three times its size and stays.
    Shield,
    /// 36 or 72 polygons: a Pizza Planet token.
    Token,
    /// 18 polygons: an extra life, up to 9 (`0x52F39A`).
    Life,
    /// 6 polygons: starts a cutscene (0x402610) and stays. Not ported.
    Cutscene,
    /// 39 polygons: 0x4A4D60 (250 ticks, sound 0x4C BOOTLNCH). Not ported.
    Launch,
    /// 100 polygons: `0x882938` + 5 (up to 10); comes back after 400 ticks.
    AmmoA,
    /// 30 polygons: `0x882964` + 10 (up to 30); comes back after 400 ticks.
    AmmoB,
    /// 20 polygons: hover boots (0x4A4B70); comes back after 400 ticks. Hovering is not ported.
    HoverBoots,
    /// 31, 50 or 80 polygons: a challenge item ("find five ..."), counted in `0x830D4C`.
    Collectable,
    /// 19 polygons: sets `0x53C824` to 1200 ticks. Not identified.
    Timer,
    /// Any other model: flips the sign of `0x830D48` (a per-level value from 0x503A22).
    Other,
}

impl Kind {
    /// 0x44E520.
    pub fn from_polys(polys: u16) -> Kind {
        match polys {
            0x06 => Kind::Cutscene,
            0x12 => Kind::Life,
            0x13 => Kind::Timer,
            0x14 => Kind::HoverBoots,
            0x1E => Kind::AmmoB,
            0x1F | 0x32 | 0x50 => Kind::Collectable,
            0x20 => Kind::Battery,
            0x24 | 0x48 => Kind::Token,
            0x27 => Kind::Launch,
            0x3C => Kind::Shield,
            100 => Kind::AmmoA,
            _ => Kind::Other,
        }
    }

    fn respawns(self) -> bool {
        matches!(self, Kind::AmmoA | Kind::AmmoB | Kind::HoverBoots)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pickup {
    /// Below 0x30: sprite index; otherwise the special-object id.
    pub id: u8,
    pub kind: Kind,
    pub pos: [i32; 3],
    /// Collection radius in steps of 8 units, less 14 (byte +0xD & 0x7F); 0 = not collectable.
    pub radius: u8,
    /// Floor height under a sprite, for its shadow (none if more than 0x1800 below).
    pub floor: Option<i32>,
    /// Taken out of the list (the game sets y to 0x80000000).
    pub removed: bool,
    /// Scale applied to a special object (0x4CCB20); 0 hides it.
    pub scale: f32,
    /// 12-bit angles added to a special object's own (0x440F70 tumbles them).
    pub spin: [i32; 3],
    /// Within the draw distance this frame (byte +0xD bit 0x80); only these can be collected.
    pub visible: bool,
    /// A coin dropped by a beaten enemy, still falling: its speed (level units per tick).
    pub falling: Option<f32>,
}

impl Pickup {
    pub fn is_sprite(&self) -> bool {
        self.id < 0x30
    }
}

/// A sprite from the table at 0x4EC0A4 (copied to 0x557500 by 0x447D40): a texture slot
/// (`texNN` in the level's `.ngn`), a cell size in pixels and the cells' top-left corners.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteDef {
    pub texture: i16,
    pub size: [u8; 2],
    pub cells: Vec<[u8; 2]>,
}

const SPRITE_TABLE: u32 = 0x4EC0A4;
/// The coin animation uses ten cells.
const COIN_FRAMES: usize = 10;

pub fn sprite_def(exe: &Exe, index: u8) -> Option<SpriteDef> {
    let p = u32::from_le_bytes(exe.at(SPRITE_TABLE + 4 * index as u32, 4).ok()?.try_into().ok()?);
    if p == 0 {
        return None;
    }
    let d = exe.at(p, 8 + 2 * COIN_FRAMES).ok()?;
    Some(SpriteDef {
        texture: i16::from_le_bytes([d[0], d[1]]),
        size: [d[2], d[3]],
        cells: (0..COIN_FRAMES).map(|f| [d[8 + 2 * f], d[9 + 2 * f]]).collect(),
    })
}

/// Something that happened this tick, for sound and display.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Collected { index: usize, kind: Kind, pos: [i32; 3] },
    /// The 50th coin (sound 0x4F, non-positional).
    FiftyCoins,
    /// A token appearing bursts (0x4A0F80: sound 0x33 and 16 sparkles 0x2B from the table
    /// 0x5039C4), level units.
    TokenBurst([i32; 3]),
}

/// Draw distance (`0x54BEF0`, set to 0x680 by 0x43E6E0); sprites fade over the last part.
const DRAW_DISTANCE: i32 = 0x680;
/// Ticks before a collected power-up comes back (`0x830C90`).
const RESPAWN_TICKS: i32 = 400;

/// Per-level setup the level's start-up code does (0x4A1A50 â†’ level 1: 0x4171D0).
struct LevelSetup {
    /// i16 special ids of the level's five tokens (0x4A0C80 hides them all at the start).
    token_list: u32,
    /// First of five ids where challenge-reward tokens appear (also hidden at the start).
    reward_start: u8,
    /// Token indices the setup reveals at once (0x4A0DB0 with `instant`).
    revealed: &'static [usize],
}

fn level_setup(level: usize) -> Option<LevelSetup> {
    match level {
        1 => Some(LevelSetup { token_list: 0x4F0EDC, reward_start: 0x48, revealed: &[3] }),
        // The other levels' start-ups (0x4A1A50's table): the same call, token 3 shown.
        2 => Some(LevelSetup { token_list: 0x4F154C, reward_start: 0x41, revealed: &[3] }),
        4 => Some(LevelSetup { token_list: 0x4F1C98, reward_start: 0x75, revealed: &[] }),
        5 => Some(LevelSetup { token_list: 0x4F2178, reward_start: 0x41, revealed: &[3] }),
        7 => Some(LevelSetup { token_list: 0x4F2A48, reward_start: 0x41, revealed: &[3] }),
        8 => Some(LevelSetup { token_list: 0x4F2F0C, reward_start: 0x41, revealed: &[3] }),
        10 => Some(LevelSetup { token_list: 0x4F3798, reward_start: 0x73, revealed: &[3] }),
        11 => Some(LevelSetup { token_list: 0x4F3F18, reward_start: 0x71, revealed: &[3] }),
        13 => Some(LevelSetup { token_list: 0x4F46C0, reward_start: 0x40, revealed: &[3] }),
        14 => Some(LevelSetup { token_list: 0x4F4C68, reward_start: 0x78, revealed: &[3] }),
        _ => None,
    }
}

/// Items whose availability depends on moves unlocked so far (0x4A2080): tables of
/// {level, id to hide when unlocked, pickup id, mover} until 0xFF, one per progress bit
/// (`0x52F2D8`). Locked (a new game): the pickup is shown but cannot be collected.
const PROGRESS_TABLES: [u32; 5] = [0x503A44, 0x503A54, 0x503AA4, 0x503AB8, 0x503AC8];

pub struct Pickups {
    pub items: Vec<Pickup>,
    pub sprites: Vec<Option<SpriteDef>>,
    pub coins: i32,
    pub health: i32,
    /// Five at a new game (confirmed by the user against the original).
    pub lives: i32,
    /// Collected token indices (bits), as `0x52F0D7[level]` stores them.
    pub tokens: u8,
    /// The level's five token ids, if its setup is known.
    pub token_ids: Vec<u8>,
    pub collectables: i32,
    pub shield: i32,
    pub ammo_a: i32,
    pub ammo_b: i32,
    pub timer: i32,
    /// Coin animation clock, 0..20 (`0x54E04E`).
    frame: i32,
    /// (index, ticks left) of a collected power-up waiting to come back.
    respawn: Option<(usize, i32)>,
    /// The last shield picked up (it shrinks back to twice its size when another is taken).
    shield_item: Option<usize>,
    /// This level's ability gates (0x503A44…): ability bit, cage special, pickup id, cage
    /// mover, the pickup's scale once open.
    gates: Vec<(u8, i32, u8, i32, f32)>,
    /// Tokens appearing (0x830CD8): item and ticks left from 0x84.
    appearing: Vec<(usize, i32)>,
    /// The sparkles' velocities (0x5039C4: 16 × x, y, z), object units a tick × 4.
    pub burst: Vec<[i32; 3]>,
}

impl Pickups {
    /// 0x447DB0, then the level's own setup (tokens, progress gates).
    pub fn new(level: usize, dat: &[u8], exe: Option<&Exe>) -> Result<Pickups, leveldat::Error> {
        let lists = leveldat::parse(dat)?;
        let objects = leveldat::special_objects(dat)?;
        let points = leveldat::pickup_points(&lists);
        Ok(Pickups::build(level, &points, &objects, exe))
    }

    /// The floor under each pickup, for the sprites' shadows. 0x447DB0 probes from 10 units
    /// above the point; a floor more than 0x1800 below counts as none.
    pub fn probe_floors(&mut self, world: &World) {
        for p in &mut self.items {
            p.floor = world.floor_below(p.pos[0] as f32, p.pos[2] as f32, (p.pos[1] - 10) as f32).map(|f| f as i32).filter(|f| f - p.pos[1] <= 0x1800);
        }
    }

    pub fn build(level: usize, points: &[PickupPoint], objects: &[Option<Object>], exe: Option<&Exe>) -> Pickups {
        let kind_of = |id: u8| match objects.get(id as usize).copied().flatten() {
            Some(o) => Kind::from_polys(o.polys),
            None => Kind::Other,
        };
        let item = |id: u8, pos: [i32; 3], radius: u8| {
            Pickup {
                id,
                kind: if id < 0x30 { Kind::Coin } else { kind_of(id) },
                pos,
                radius,
                floor: None,
                removed: false,
                scale: 1.0,
                spin: [0; 3],
                visible: false,
                falling: None,
            }
        };
        let mut items: Vec<Pickup> = points.iter().map(|p| item(p.id, p.pos, 0x11)).collect();
        let first = match level {
            4 | 10 | 11 | 14 => 0x60,
            5 => 0x50,
            16 => 400,
            _ => 0x30,
        };
        for (id, o) in objects.iter().enumerate().skip(first) {
            if let Some(o) = o
                && id < 0x100
            {
                items.push(item(id as u8, o.pos, (o.size >> 3) as u8));
            }
        }
        let sprites = (0..0x30u8).map(|i| exe.and_then(|e| sprite_def(e, i))).collect();
        let mut p = Pickups {
            items,
            sprites,
            coins: 0,
            health: 14,
            lives: 5,
            tokens: 0,
            token_ids: Vec::new(),
            collectables: 0,
            shield: 0,
            ammo_a: 0,
            ammo_b: 0,
            timer: 0,
            frame: 0,
            respawn: None,
            shield_item: None,
            gates: Vec::new(),
            appearing: Vec::new(),
            burst: Vec::new(),
        };
        if let Some(exe) = exe {
            p.level_setup(level, exe);
        }
        p
    }

    fn by_id(&mut self, id: u8) -> Option<&mut Pickup> {
        self.items.iter_mut().find(|p| p.id == id)
    }

    fn level_setup(&mut self, level: usize, exe: &Exe) {
        if let Some(setup) = level_setup(level)
            && let Ok(b) = exe.at(setup.token_list, 10)
        {
            // 0x4A0C80: hide every token and every reward spot; 0x4A0DB0 then brings back the
            // ones the level starts with.
            self.token_ids = (0..5).map(|k| i16::from_le_bytes([b[2 * k], b[2 * k + 1]]) as u8).collect();
            for k in 0..5 {
                for id in [self.token_ids[k], setup.reward_start + k as u8] {
                    if let Some(p) = self.by_id(id) {
                        p.scale = 0.0;
                        p.removed = true;
                    }
                }
            }
            for &k in setup.revealed {
                self.reveal_token(k);
            }
        }
        if let Ok(b) = exe.at(0x5039C4, 96) {
            self.burst = b.chunks_exact(6).map(|c| [0, 2, 4].map(|k| i16::from_le_bytes([c[k], c[k + 1]]) as i32)).collect();
        }
        // 0x4A2080 with no moves unlocked yet.
        for (bit, table) in PROGRESS_TABLES.into_iter().enumerate() {
            let mut va = table;
            for _ in 0..64 {
                let Ok(e) = exe.at(va, 4) else { break };
                if e[0] == 0xFF {
                    break;
                }
                if e[0] as usize == level {
                    // 0x4A27A0's scale argument: 8 for the first table, 4 for the rest (<< 10).
                    let open = if bit == 0 { 2.0 } else { 1.0 };
                    self.gates.push((1 << bit, e[1] as i32, e[2], e[3] as i32, open));
                }
                if e[0] as usize == level
                    && let Some(p) = self.by_id(e[2])
                {
                    p.scale = if table == 0x503AC8 { 0.75 } else { 1.0 };
                    p.radius = 0;
                }
                va += 4;
            }
        }
    }

    /// Shows or hides every challenge collectable (the level scripts' 0x52F814 table).
    pub fn show_collectables(&mut self, shown: bool) {
        for i in 0..self.items.len() {
            if self.items[i].kind == Kind::Collectable {
                self.show_item(i, shown);
            }
        }
    }

    /// Hides pickup `i` (y = 0x80000000, as scripts do) or brings it back.
    pub fn show_item(&mut self, i: usize, shown: bool) {
        if let Some(p) = self.items.get_mut(i) {
            p.removed = !shown;
            p.scale = if shown { 1.0 } else { 0.0 };
        }
    }

    /// 0x4A27A0 for the abilities Buzz has: their gated pickups can be taken (radius 0x1F,
    /// scaled); returns the cages (special, mover) to take away.
    pub fn open_gates(&mut self, abilities: u8) -> Vec<(i32, i32)> {
        let mut cages = Vec::new();
        for (bit, cage, id, mover, scale) in self.gates.clone() {
            if abilities & bit == 0 {
                continue;
            }
            if let Some(p) = self.by_id(id) {
                p.radius = 0x1F;
                p.scale = scale;
            }
            cages.push((cage, mover));
        }
        cages
    }

    /// 0x4A0DB0 (full form): token `k` appears with its animation (hidden for 35 ticks, then
    /// popping in and wobbling down to size over 0x84 ticks, bursting at 100). Returns where
    /// it is (level units), for the look-at scene, if it wasn't already out.
    pub fn reveal_token_animated(&mut self, k: usize) -> Option<[i32; 3]> {
        let &id = self.token_ids.get(k)?;
        if self.tokens & (1 << k) != 0 {
            return None;
        }
        let i = self.items.iter().position(|p| p.id == id)?;
        if self.appearing.iter().any(|a| a.0 == i) || (!self.items[i].removed && self.items[i].scale > 0.0) {
            return None;
        }
        let p = &mut self.items[i];
        p.removed = false;
        p.scale = 0.0;
        self.appearing.push((i, 0x84));
        Some(p.pos)
    }

    /// One tick of the appearing tokens' animation (0x4A0F80).
    fn tick_appearing(&mut self, events: &mut Vec<Event>) {
        let sin = |a: i32| ((a & 0xFFF) as f64 / 4096.0 * std::f64::consts::TAU).sin().mul_add(16384.0, 0.0).round() as i32;
        let mut done = Vec::new();
        for (n, (i, t)) in self.appearing.iter_mut().enumerate() {
            let prev = *t;
            *t = (*t - 1).max(0);
            if prev > 100 && *t <= 100 {
                events.push(Event::TokenBurst(self.items[*i].pos));
            }
            let s = if prev < 0x58 {
                (((sin(((prev - 4) & 0xF) * 0x100) >> 3) + 0x200) >> (5 - prev / 16).clamp(0, 5)) + 0x1000
            } else if prev < 0x60 {
                sin((0x5F - prev) * 0x80) >> 2
            } else {
                0
            };
            self.items[*i].scale = if *t == 0 { 1.0 } else { s as f32 / 4096.0 };
            if *t == 0 {
                done.push(n);
            }
        }
        for n in done.into_iter().rev() {
            self.appearing.remove(n);
        }
    }

    /// 0x4A0DB0 (instant form): token `k` of the level's five becomes visible and collectable.
    pub fn reveal_token(&mut self, k: usize) {
        if let Some(&id) = self.token_ids.get(k)
            && self.tokens & (1 << k) == 0
            && let Some(p) = self.by_id(id)
        {
            p.scale = 1.0;
            p.removed = false;
        }
    }

    /// Token `k`, if it's still to collect, moves to `pos` (level units; level 10's runaway
    /// token).
    pub fn move_token(&mut self, k: usize, pos: [i32; 3]) {
        if let Some(&id) = self.token_ids.get(k)
            && self.tokens & (1 << k) == 0
            && let Some(p) = self.by_id(id)
        {
            p.pos = pos;
        }
    }

    /// 0x4A0E60: token `k` goes out of sight again (not collected).
    pub fn hide_token(&mut self, k: usize) {
        if let Some(&id) = self.token_ids.get(k)
            && self.tokens & (1 << k) == 0
            && let Some(p) = self.by_id(id)
        {
            p.scale = 0.0;
            p.removed = true;
        }
    }

    /// A coin a beaten enemy drops (0x405D20 throws one up; it falls to the floor below).
    pub fn drop_coin(&mut self, pos: [i32; 3], floor: Option<i32>) {
        self.items.push(Pickup {
            id: 16,
            kind: Kind::Coin,
            pos: [pos[0], pos[1] - 128, pos[2]],
            radius: 0x11,
            floor,
            removed: false,
            scale: 1.0,
            spin: [0; 3],
            visible: false,
            falling: Some(-16.0),
        });
    }

    /// One 60 Hz tick. `buzz` is Buzz's feet and `camera` the camera position, level units.
    pub fn tick(&mut self, buzz: [f32; 3], camera: [f32; 3]) -> Vec<Event> {
        self.frame = (self.frame + 1) % 20;
        for p in self.items.iter_mut() {
            if let Some(v) = p.falling {
                let rest = p.floor.map_or(i32::MAX, |f| f - 120);
                p.pos[1] += v as i32;
                p.falling = Some(v + 2.0);
                if p.pos[1] >= rest {
                    p.pos[1] = rest;
                    p.falling = None;
                }
            }
        }
        self.shield = (self.shield - 1).max(0);
        self.timer = (self.timer - 1).max(0);
        // 0x440F70: visibility by horizontal distance from the camera, and the tumble.
        let limit = (DRAW_DISTANCE / 4) * (DRAW_DISTANCE / 4);
        let (cx, cz) = (camera[0] as i32, camera[2] as i32);
        for (i, p) in self.items.iter_mut().enumerate() {
            p.visible = false;
            if p.removed {
                continue;
            }
            let (dx, dz) = ((cx - p.pos[0]) >> 4, (cz - p.pos[2]) >> 4);
            if dx * dx + dz * dz - 100 >= limit {
                continue;
            }
            p.visible = true;
            if !p.is_sprite() {
                let i = i as i32;
                let step = [10, ((i >> 2 & 3) * 3 + 7) * 2, ((i & 7) + 4) * 2];
                for (s, d) in p.spin.iter_mut().zip(step) {
                    *s = (*s + d) & 0xFFF;
                }
            }
        }
        // 0x4A0F80: a collected power-up returns after 400 ticks.
        if let Some((i, t)) = self.respawn {
            if t <= 1 {
                self.restore(i);
                self.respawn = None;
            } else {
                self.respawn = Some((i, t - 1));
            }
        }
        // Collection: around a point 230 units above Buzz's feet, in steps of 8 units.
        let b = buzz.map(|v| v.floor() as i32);
        let mut events = Vec::new();
        self.tick_appearing(&mut events);
        for i in 0..self.items.len() {
            let p = &self.items[i];
            if p.removed || !p.visible || p.radius & 0x7F == 0 {
                continue;
            }
            let d = [(b[0] - p.pos[0]) >> 3, (b[1] - 0xE6 - p.pos[1]) >> 3, (b[2] - p.pos[2]) >> 3];
            let r = (p.radius & 0x7F) as i32 + 14;
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= r * r {
                continue;
            }
            events.extend(self.collect(i));
        }
        events
    }

    fn restore(&mut self, i: usize) {
        if let Some(p) = self.items.get_mut(i) {
            p.removed = false;
            p.scale = 1.0;
        }
    }

    /// Applies pickup `i` (0x4A0F80's switch) and takes it out of the level.
    fn collect(&mut self, i: usize) -> Vec<Event> {
        let (kind, pos, sprite) = (self.items[i].kind, self.items[i].pos, self.items[i].is_sprite());
        let mut events = vec![Event::Collected { index: i, kind, pos }];
        // 1: hide and remove; 0: remove only (stays drawn); 2: stays.
        let mut fate = if sprite { 0 } else { 1 };
        match kind {
            Kind::Coin => {
                self.coins = (self.coins + 1).min(99);
                if self.coins == 50 {
                    events.push(Event::FiftyCoins);
                }
            }
            Kind::Battery => self.health = (self.health + 4).min(14),
            Kind::Shield => {
                if let Some(prev) = self.shield_item.replace(i) {
                    let p = &mut self.items[prev];
                    p.removed = false;
                    p.scale = 2.0;
                }
                self.shield = 900;
                self.items[i].scale = 3.0;
                fate = 0;
            }
            Kind::Token => {
                let id = self.items[i].id;
                if let Some(k) = self.token_ids.iter().position(|&t| t == id) {
                    self.tokens |= 1 << k;
                } else {
                    // Levels whose token table isn't set up here: count in order.
                    self.tokens |= 1 << self.tokens.count_ones().min(7);
                }
            }
            Kind::Life => self.lives = (self.lives + 1).min(9),
            Kind::Cutscene => fate = 2,
            Kind::AmmoA | Kind::AmmoB | Kind::HoverBoots => {
                match kind {
                    Kind::AmmoA => {
                        self.ammo_a = (self.ammo_a + 5).min(10);
                        self.ammo_b = 0;
                    }
                    Kind::AmmoB => {
                        self.ammo_b = (self.ammo_b + 10).min(30);
                        self.ammo_a = 0;
                    }
                    _ => {}
                }
                if let Some((prev, _)) = self.respawn.take() {
                    self.restore(prev);
                }
                self.respawn = Some((i, RESPAWN_TICKS));
            }
            Kind::Collectable => self.collectables += 1,
            Kind::Timer => self.timer = 1200,
            Kind::Launch | Kind::Other => {}
        }
        let p = &mut self.items[i];
        match fate {
            1 => {
                p.scale = 0.0;
                p.removed = true;
            }
            0 => p.removed = true,
            _ => {}
        }
        debug_assert!(!kind.respawns() || fate == 1);
        events
    }

    /// Coins and other sprites with their floor shadows (0x440F70): camera-facing, 200 units
    /// across, fading out near the draw distance; shadows lie flat 10 units above the floor.
    /// `right` and `up` are the camera's axes in game space (+Y down).
    pub fn sprite_batches(&self, ngn: &Ngn, camera: [f32; 3], right: [f32; 3], up: [f32; 3]) -> Vec<Batch> {
        let mut out = Vec::new();
        let limit = (DRAW_DISTANCE / 4) * (DRAW_DISTANCE / 4);
        let (cx, cz) = (camera[0] as i32, camera[2] as i32);
        let texture = |slot: i16| -> Option<(usize, f32, f32)> {
            let name = format!("tex{slot:02}");
            let i = ngn.textures.iter().position(|t| t.name.eq_ignore_ascii_case(&name))?;
            let (w, h, _) = ngn.textures[i].dimensions()?;
            Some((i, (w as f32).abs(), (h as f32).abs()))
        };
        for (i, p) in self.items.iter().enumerate() {
            if !p.is_sprite() || p.removed {
                continue;
            }
            let (dx, dz) = ((cx - p.pos[0]) >> 4, (cz - p.pos[2]) >> 4);
            let d2 = dx * dx + dz * dz - 100;
            if d2 >= limit {
                continue;
            }
            let alpha = ((limit - d2) >> 6).min(0xFF) as u8;
            let Some(def) = self.sprites.get(p.id as usize).and_then(|d| d.as_ref()) else { continue };
            let Some((tex, tw, th)) = texture(def.texture) else { continue };
            // Each entry is two cells further on than the one before it.
            let frame = ((self.frame >> 1) as usize + 2 * (i + 1)) % COIN_FRAMES;
            let [u, v] = def.cells[frame];
            let uv = [[u as f32 / tw, v as f32 / th], [(u as f32 + def.size[0] as f32) / tw, (v as f32 + def.size[1] as f32) / th]];
            let pos = p.pos.map(|v| v as f32);
            scene::sprite(&mut out, Some(tex), pos, 100.0, right, up, uv, [alpha, 0x80, 0x80, 0x80]);
            // Shadow: cell (96, 96)â€“(127, 127) of texture 31, 80 half-size, black at 0x88.
            if let (Some(floor), Some((stex, sw, sh))) = (p.floor, texture(0x1F)) {
                let uv = [[96.0 / sw, 96.0 / sh], [127.0 / sw, 127.0 / sh]];
                scene::sprite(&mut out, Some(stex), [pos[0], floor as f32 - 10.0, pos[2]], 80.0, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], uv, [0x88, 0, 0, 0]);
            }
        }
        out
    }

    /// Writes the special objects' tumble and scale (0x4CCCE0 / 0x4CCB20) into `specials`.
    pub fn apply(&self, specials: &mut crate::specials::Specials) {
        for p in self.items.iter().filter(|p| !p.is_sprite()) {
            let id = p.id as i32;
            specials.turn_from_own(id, p.spin);
            let s = (p.scale * 4096.0) as i32;
            specials.set_scale(id, [s; 3]);
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "coins {}  health {}/14  lives {}  tokens {}/5",
            self.coins,
            self.health,
            self.lives,
            self.tokens.count_ones()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(pos: [i32; 3], size: i16, polys: u16) -> Option<Object> {
        Some(Object { pos, size, flags: 0x49, polys })
    }

    #[test]
    fn coins_are_collected_near_buzz_chest() {
        let points = [PickupPoint { pos: [0, -230, 0], id: 16 }, PickupPoint { pos: [5000, -230, 0], id: 16 }];
        let mut p = Pickups::build(1, &points, &[], None);
        // The visibility pass runs first in the same tick, so a coin in view is collectable.
        let ev = p.tick([100.0, 0.0, 0.0], [0.0, -1000.0, 1000.0]);
        assert_eq!(ev, vec![Event::Collected { index: 0, kind: Kind::Coin, pos: [0, -230, 0] }]);
        assert_eq!(p.coins, 1);
        assert!(p.items[0].removed);
        // Coins reach (0x11 + 14) Ã— 8 = 248 units: 240 is inside, 256 is not.
        assert!(p.tick([5000.0 - 256.0, 0.0, 0.0], [5000.0, -1000.0, 1000.0]).is_empty());
        assert_eq!(p.tick([5000.0 - 240.0, 0.0, 0.0], [5000.0, -1000.0, 1000.0]).len(), 1);
        assert_eq!(p.coins, 2);
    }

    #[test]
    fn out_of_draw_distance_is_not_collected() {
        let points = [PickupPoint { pos: [0, -230, 0], id: 16 }];
        let mut p = Pickups::build(1, &points, &[], None);
        assert!(p.tick([0.0, 0.0, 0.0], [20000.0, 0.0, 0.0]).is_empty());
    }

    #[test]
    fn kinds_and_power_up_respawn() {
        let mut objects = vec![None; 0x33];
        objects[0x30] = obj([0, -230, 0], 216, 0x20);
        objects[0x31] = obj([3000, -230, 0], 160, 0x14);
        objects[0x32] = obj([6000, -230, 0], 160, 6);
        let mut p = Pickups::build(1, &[], &objects, None);
        assert_eq!(p.items.iter().map(|i| i.kind).collect::<Vec<_>>(), [Kind::Battery, Kind::HoverBoots, Kind::Cutscene]);
        p.health = 5;
        p.tick([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        assert_eq!(p.health, 9);
        assert_eq!(p.items[0].scale, 0.0);
        p.tick([3000.0, 0.0, 0.0], [3000.0, 0.0, 0.0]);
        assert!(p.items[1].removed);
        for _ in 0..RESPAWN_TICKS {
            p.tick([9000.0, 0.0, 9000.0], [3000.0, 0.0, 0.0]);
        }
        assert!(!p.items[1].removed && p.items[1].scale == 1.0);
        // Cutscene triggers stay where they are.
        p.tick([6000.0, 0.0, 0.0], [6000.0, 0.0, 0.0]);
        assert!(!p.items[2].removed);
    }

    #[test]
    fn specials_tumble_at_their_own_rates() {
        let mut objects = vec![None; 0x32];
        objects[0x30] = obj([0, 0, 0], 216, 0x48);
        objects[0x31] = obj([0, 0, 0], 216, 0x48);
        let mut p = Pickups::build(1, &[], &objects, None);
        p.tick([9000.0, 0.0, 9000.0], [0.0, 0.0, 0.0]);
        assert_eq!(p.items[0].spin, [10, 14, 8]);
        assert_eq!(p.items[1].spin, [10, 14, 10]);
    }
}
