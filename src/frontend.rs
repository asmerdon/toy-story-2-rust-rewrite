//! The front end: the title, the main menu and the level select, and the flow between them
//! and the levels (0x49D910).
//!
//! - **Flow** (0x49D910): the title and menus run in scene 0 (`level00\level.ngn`), whose
//!   textures `bgr40`–`bgr44` are their full-screen pictures (0x48F1B0: picture `n` is texture
//!   number 0x28 + n, drawn 320 × 256). "Start game" (0x830C88 = 1) goes to the level select
//!   (0x453CF0), which loads scene 16 (`level06\level1.ngn`), a map of the levels. Picking a
//!   level plays it; finishing one (0x52B7DC = 1 or 5) returns to the level select.
//! - **Title** (0x437FB0): picture 0 (`bgr40`), music `titlescr`, "press jump" pulsing at
//!   y 0xCC. Jump (after 0x1E ticks) plays sound 0 and fades to the menu.
//! - **Main menu** (0x437C40): picture 4 (`bgr44`), music `ygafim`; start game (continue game
//!   once one is under way, 0x830D58), options, load game, movie viewer, exit, at y 0x78 + 0x14k.
//!   Two arrows (sprite 0x3E) slide 2 a tick to the chosen row; up / down move it (sound 1),
//!   jump picks (sound 0, after 0x1F ticks). Exit quits at once (0x4CF359).
//! - **Level select** (0x438A50): see [`Select`].
//! - Text: 0x49B580 draws the menus in sprite 0x32 (letters a–z, `'` as cell 0x2F), 12 pixels
//!   apart, centred on x 0xA0 of 320; 0x49D390 draws the level select's in texture 31 (32-pixel
//!   cells, eight a row), 13 apart, 16 pixels high, centred on x.
//! - Fades (0x4A1BB0 / 0x4A1BE0): the brightness (0–0x80) moves speed / 2 a tick to its target.
//!   Screens start black, fade in at speed 0xC (the level select 6), and leave by fading to
//!   black over 0x17 ticks (0x53CA60).
//!
//! Not ported yet: the attract demo after 900 ticks on the title, options, load game, the
//! movie viewer, the boss victory screen (0x453D90), game over
//! (0x437B20), the credits, FMVs, and the level select's sound at each level (0x4A3B90).

use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::hud::Quad;
use crate::pickups::SpriteDef;

/// Level names by campaign number (0x4F6BE4; entry 0 unused).
const NAMES: u32 = 0x4F6BE4;
/// Tokens needed for the next level, by levels done (i16, 0x503840).
const NEEDED: u32 = 0x503840;
/// Campaign order: internal level ids (0x50268C).
const ORDER: u32 = 0x50268C;

/// The exe tables the front end reads.
#[derive(Debug, Clone, Default)]
pub struct Tables {
    /// Internal level id of each campaign level, in order.
    pub order: Vec<usize>,
    /// Level names by campaign number (index 0 empty).
    pub names: Vec<String>,
    /// Tokens needed, by levels done.
    pub needed: Vec<i32>,
    /// The level-select map's models by level (0x4F6DC4): specials to hide (the level's
    /// blueprint) and to show (its finished model) once the level can be chosen.
    pub models: Vec<(Vec<usize>, Vec<usize>)>,
}

/// Pairs of i16 special lists (ending −1) per level, as 0x438A50 reads them.
const MODELS: u32 = 0x4F6DC4;

impl Tables {
    pub fn read(exe: &Exe) -> Tables {
        let u32_at = |va: u32| exe.at(va, 4).ok().map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        let cstr = |va: u32| -> String { (0..64).map_while(|k| exe.at(va + k, 1).ok().map(|b| b[0])).take_while(|&b| b != 0).map(char::from).collect() };
        let order = (0..15).filter_map(|k| u32_at(ORDER + 4 * k)).map(|v| v as usize).collect();
        let names = (0..16).map(|k| u32_at(NAMES + 4 * k).filter(|&p| p > 0x400000).map(cstr).unwrap_or_default()).collect();
        let needed = (0..16).filter_map(|k| exe.at(NEEDED + 2 * k, 2).ok()).map(|b| i16::from_le_bytes([b[0], b[1]]) as i32).collect();
        let ids = |p: Option<u32>| -> Vec<usize> {
            let Some(p) = p else { return Vec::new() };
            (0..64).map_while(|k| exe.at(p + 2 * k, 2).ok().map(|b| i16::from_le_bytes([b[0], b[1]]))).take_while(|&v| v >= 0).map(|v| v as usize).collect()
        };
        let models = (0..15).map(|k| (ids(u32_at(MODELS + 8 * k)), ids(u32_at(MODELS + 8 * k + 4)))).collect();
        Tables { order, names, needed, models }
    }
}

/// The player's progress through the campaign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// 0x52F0D7[internal id]: collected tokens (bits 0–4); 0x80 once a boss is beaten.
    pub tokens: [u8; 17],
    /// Lives (0x52F39A) and abilities (0x52F2D8), carried from level to level.
    pub lives: i32,
    pub abilities: u8,
}

impl Default for Progress {
    /// A new game (0x4A2C20): five lives, nothing done.
    fn default() -> Progress {
        Progress { tokens: [0; 17], lives: 5, abilities: 0 }
    }
}

/// What 0x49EB50 packs: levels done in a row from the first, tokens in them, and tokens needed
/// for the level after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub levels: usize,
    pub tokens: i32,
    pub needed: i32,
}

impl Progress {
    /// 0x49EB50: counts campaign levels in order while each has any progress.
    pub fn summary(&self, t: &Tables) -> Summary {
        let mut s = Summary { levels: 0, tokens: 0, needed: 0 };
        for &id in &t.order {
            let bits = self.tokens.get(id).copied().unwrap_or(0);
            if bits == 0 {
                break;
            }
            s.tokens += (bits & 0x1F).count_ones() as i32;
            s.levels += 1;
        }
        s.needed = t.needed.get(s.levels).copied().unwrap_or(0);
        s
    }
}

/// The pad as the menus read it (0x88279C), one bit per button.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pad(pub u32);

impl Pad {
    pub const UP: u32 = 0x10;
    pub const RIGHT: u32 = 0x20;
    pub const DOWN: u32 = 0x40;
    pub const LEFT: u32 = 0x80;
    pub const CANCEL: u32 = 0x1000;
    pub const JUMP: u32 = 0x4000;
}

/// Where the front end is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Title,
    Menu,
    Select,
    /// Playing campaign level `n` (1–15).
    Play(usize),
    /// How the level went (0x4398B0), after a normal level.
    Results,
    /// Out of lives (0x437B20): back to the title, and a new game.
    GameOver,
    Quit,
}

/// What the results screen shows of the level just played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelResult {
    /// The level's token bits before it was played (0x830CA8) and now (0x52F0D7[level]).
    pub before: u8,
    pub now: u8,
    /// Coins collected (0x52F39E), and the abilities held (0x52F2D8 bits).
    pub coins: i32,
    pub abilities: u8,
}

impl Screen {
    /// The scene's variant (`campaign::variant_path`): the results screen loads scene 0 with
    /// flags 0x47C, variant 1 (`levelt1.ngn`: picture 40 is `token256`, Slinky's `levcomp`).
    pub fn variant(self) -> u8 {
        u8::from(matches!(self, Screen::Results | Screen::GameOver))
    }

    /// The internal id of the scene this screen runs in.
    pub fn scene(self, t: &Tables) -> usize {
        match self {
            Screen::Title | Screen::Menu | Screen::Results | Screen::GameOver | Screen::Quit => crate::campaign::FRONT,
            Screen::Select => crate::campaign::HUB,
            Screen::Play(n) => t.order.get(n - 1).copied().unwrap_or(1),
        }
    }
}

/// Something for the caller to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A sound with no position (0x4A37E0).
    Sound(u16),
    /// Music track (index into 0x4ECCD8, see `music`); `repeat` as 0x47D880 plays it.
    Music { track: usize, repeat: bool },
}

/// A character of 0x49D390's font: texture 31, a 32-pixel cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// Top-left and size in a `width` × 256 screen.
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub width: f32,
    /// Cell's top-left in the 256 × 256 texture.
    pub cell: [u8; 2],
}

/// One frame of the front end.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    /// Full-screen picture: texture number (`bgrNN`).
    pub picture: Option<u32>,
    pub quads: Vec<Quad>,
    pub glyphs: Vec<Glyph>,
    /// Brightness, 0 (black) to 0x80.
    pub brightness: i32,
    /// The level select's camera: position (object units) and pitch, yaw (12-bit).
    pub camera: Option<([i32; 3], i32, i32)>,
    /// The map's moving pieces: special id, position (level units), yaw (12-bit).
    pub pieces: Vec<(usize, [i32; 3], i32)>,
    /// The map's specials shown (scale 0x1000) or hidden (0), as 0x438A50 sets them up.
    pub shown: Vec<(usize, bool)>,
}

/// The fade (0x4A1BB0 sets the target and speed, 0x4A1BE0 steps it).
#[derive(Debug, Clone, Copy, Default)]
struct Fade {
    level: i32,
    target: i32,
    speed: i32,
}

impl Fade {
    fn to(&mut self, target: i32, speed: i32) {
        self.target = target;
        self.speed = speed;
    }

    fn step(&mut self) {
        let d = self.speed / 2;
        if self.level < self.target {
            self.level = (self.level + d).min(self.target);
        } else {
            self.level = (self.level - d).max(self.target);
        }
    }
}

/// The front end's state.
pub struct FrontEnd {
    pub screen: Screen,
    pub progress: Progress,
    pub tables: Tables,
    /// 0x830D58: a game is under way ("continue game").
    pub started: bool,
    /// 0x52AD8A + 1: the campaign level last chosen.
    pub level: usize,
    /// 0x55A0E4: the level the select's camera starts at (0: the overview).
    pub shown: usize,
    /// Whether the level just played was finished for the first time (moves the select on).
    pub first_clear: bool,
    pub events: Vec<Event>,
    pub select: Option<Select>,
    pub results: Option<Results>,
    /// Random bytes (`rand.dat`) for the results screen's sparkles.
    pub random: Vec<u8>,
    fade: Fade,
    /// Ticks on this screen; 0x53E4A8 (the choice) and 0x53CA60 (ticks left while leaving).
    clock: i32,
    choice: i32,
    leaving: i32,
    /// Menu arrows: the chosen row's y and where they are.
    row: i32,
    arrows: i32,
    prev: Pad,
}

const MENU: [&str; 5] = ["start game", "options", "load game", "movie viewer", "exit"];

impl FrontEnd {
    pub fn new(tables: Tables) -> FrontEnd {
        let mut f = FrontEnd {
            screen: Screen::Title,
            progress: Progress::default(),
            tables,
            started: false,
            level: 1,
            shown: 0,
            first_clear: false,
            events: Vec::new(),
            select: None,
            results: None,
            random: Vec::new(),
            fade: Fade::default(),
            clock: 0,
            choice: 0,
            leaving: 0,
            row: 0x78,
            arrows: 0x78,
            prev: Pad::default(),
        };
        f.enter(Screen::Title);
        f
    }

    fn enter(&mut self, s: Screen) {
        self.screen = s;
        self.clock = 0;
        self.choice = 0;
        self.leaving = 0;
        self.fade = Fade { level: 0, target: 0x80, speed: 0xC };
        match s {
            Screen::Title => self.events.push(Event::Music { track: crate::music::TITLE, repeat: false }),
            Screen::Menu => {
                self.row = 0x78;
                self.arrows = 0x78;
                self.events.push(Event::Music { track: crate::music::MENU, repeat: false });
            }
            Screen::Select => {
                self.fade.speed = 6;
                self.events.push(Event::Music { track: crate::music::MENU, repeat: true });
            }
            Screen::Results => {
                self.fade.speed = 6;
                self.events.push(Event::Music { track: crate::music::LEVEL_DONE, repeat: false });
            }
            Screen::GameOver => {
                self.clock = 0x4B0;
                self.events.push(Event::Music { track: crate::music::GAME_OVER, repeat: false });
            }
            _ => {}
        }
    }

    /// Buzz died with no lives left (0x52B7DC = 2, 0x52F39A = 0): the game-over screen.
    pub fn game_over(&mut self) {
        self.enter(Screen::GameOver);
    }

    /// 0x437B20: picture 1 (`game_over256`) for 0x4B0 ticks, or until a button after the first
    /// 60; it fades over the last 0x18. Then the title, and a new game (0x4A2C20).
    fn game_over_tick(&mut self, pressed: Pad) {
        self.clock = (self.clock - 1).max(0);
        if pressed.0 & (Pad::JUMP | Pad::CANCEL) != 0 && (0x18..0x474).contains(&self.clock) {
            self.clock = 0x18;
        }
        if self.clock < 0x18 && self.fade.target != 0 {
            self.fade.to(0, 0xC);
        }
        if self.clock == 0 {
            self.progress = Progress::default();
            self.started = false;
            self.level = 1;
            self.shown = 0;
            self.enter(Screen::Title);
        }
    }

    /// Starts the level select once its scene's lists are loaded (lists 1–3 and 7–9 of the
    /// map's level.dat) with the random table (`rand.dat`).
    pub fn begin_select(&mut self, lists: &[PointList], random: Vec<u8>) {
        let s = self.progress.summary(&self.tables);
        self.select = Some(Select::new(lists, random, self, s));
    }

    /// The level just played ended (0x52B7DC = 1 or 5): a normal level shows how it went
    /// (`result`), then the level select; a boss level goes straight back to it (its victory
    /// screen, 0x453D90, isn't ported).
    pub fn level_done(&mut self, result: Option<LevelResult>) {
        match result {
            Some(r) => {
                let level = self.level.saturating_sub(1);
                self.results = Some(Results::new(r, level));
                self.enter(Screen::Results);
            }
            None => self.enter(Screen::Select),
        }
    }

    /// The level was quit (0x52B7DC = 4): back to the main menu.
    pub fn level_quit(&mut self) {
        self.enter(Screen::Menu);
    }

    /// One tick (60 Hz) with the buttons held.
    pub fn tick(&mut self, pad: Pad) {
        let pressed = Pad(pad.0 & !self.prev.0);
        self.fade.step();
        match self.screen {
            Screen::Title => self.title(pressed),
            Screen::Menu => self.menu(pressed),
            Screen::GameOver => self.game_over_tick(pressed),
            Screen::Results => {
                if let Some(r) = self.results.as_mut()
                    && r.tick(pressed, &mut self.fade, &mut self.events, &self.random)
                {
                    self.results = None;
                    self.enter(Screen::Select);
                }
            }
            Screen::Select => {
                let s = self.progress.summary(&self.tables);
                let Some(mut sel) = self.select.take() else { return };
                let out = sel.tick(pad, &mut self.prev, s, &mut self.fade, &mut self.events);
                self.select = Some(sel);
                if let Some(cancelled) = out {
                    let sel = self.select.take().unwrap();
                    self.level = sel.level as usize;
                    self.shown = sel.level as usize;
                    if cancelled {
                        self.enter(Screen::Menu);
                    } else {
                        self.started = true;
                        self.first_clear = false;
                        self.screen = Screen::Play(self.level);
                    }
                }
                return;
            }
            _ => {}
        }
        self.prev = pad;
    }

    /// Ticks the leaving countdown; true once it's done.
    fn left(&mut self) -> bool {
        if self.leaving > 0 {
            self.leaving -= 1;
            return self.leaving == 0;
        }
        false
    }

    fn leave(&mut self, choice: i32) {
        self.choice = choice;
        self.leaving = 0x17;
        self.fade.to(0, 0xC);
        self.events.push(Event::Sound(0));
    }

    /// 0x437FB0.
    fn title(&mut self, pressed: Pad) {
        self.clock += 1;
        if self.choice != 0 {
            if self.left() {
                self.enter(Screen::Menu);
            }
            return;
        }
        if pressed.0 & Pad::JUMP != 0 && self.clock > 0x1E {
            self.leave(2);
        }
    }

    /// 0x437C40.
    fn menu(&mut self, pressed: Pad) {
        self.clock += 1;
        if self.choice != 0 {
            if self.left() {
                match self.choice {
                    2 => self.enter(Screen::Select),
                    // Options, load game and the movie viewer aren't ported: back to the menu.
                    _ => self.enter(Screen::Menu),
                }
            }
        } else if self.arrows == self.row {
            if pressed.0 & Pad::JUMP != 0 && self.clock >= 0x1F {
                if self.row == 200 {
                    self.screen = Screen::Quit;
                    return;
                }
                self.leave((self.row - 0x78) / 0x14 + 2);
            } else {
                if pressed.0 & Pad::DOWN != 0 && self.row < 200 {
                    self.row += 0x14;
                    self.events.push(Event::Sound(1));
                }
                if pressed.0 & Pad::UP != 0 && self.row > 0x78 {
                    self.row -= 0x14;
                    self.events.push(Event::Sound(1));
                }
            }
        }
        if self.arrows < self.row {
            self.arrows = (self.arrows + 2).min(self.row);
        } else if self.arrows > self.row {
            self.arrows = (self.arrows - 2).max(self.row);
        }
    }

    /// What to draw this frame.
    pub fn frame(&self, sprites: &[Option<SpriteDef>]) -> Frame {
        let mut f = Frame { brightness: self.fade.level, ..Frame::default() };
        let pulse = |t: i32| {
            let u = t & 0x3F;
            if u > 0x1F { 0x3F - u } else { u }
        };
        match self.screen {
            Screen::Title => {
                f.picture = Some(0x28);
                let a = pulse(self.clock) * 4;
                menu_text(&mut f.quads, sprites, 0xCC, "press jump", a);
            }
            Screen::Menu => {
                f.picture = Some(0x2C);
                let a = (pulse(self.clock) * 4) as u8;
                for (x, cell) in [(0x40, 0), (0xF0, 1)] {
                    put(&mut f.quads, sprites, 320.0, x, self.arrows, 0x3E, cell, [a, a, 0x80], 0xFF, 0x800, 0x800);
                }
                for (k, item) in MENU.iter().enumerate() {
                    let item = if k == 0 && self.started { "continue game" } else { item };
                    menu_text(&mut f.quads, sprites, 0x78 + 0x14 * k as i32, item, 0x80);
                }
            }
            Screen::Select => {
                if let Some(s) = &self.select {
                    s.draw(&mut f, sprites, &self.progress, &self.tables);
                }
            }
            Screen::Results => {
                if let Some(r) = &self.results {
                    r.draw(&mut f, sprites);
                }
            }
            Screen::GameOver => f.picture = Some(0x29),
            _ => {}
        }
        f
    }
}

/// A sprite at (x, y) in a `width`-wide screen, scaled (sx, sy) / 4096 (0x493F40); `mode` as its
/// blend: 0x60 sprites take alpha 0xFF − (mode >> 8).
#[allow(clippy::too_many_arguments)]
fn put(q: &mut Vec<Quad>, sprites: &[Option<SpriteDef>], width: f32, x: i32, y: i32, sprite: usize, cell: usize, color: [u8; 3], mode: i32, sx: i32, sy: i32) {
    let Some(d) = sprites.get(sprite).and_then(|d| d.as_ref()) else { return };
    let alpha = if mode & 0x60 == 0x60 { (0xFF - (mode >> 8)).clamp(0, 0xFF) as u8 } else { 0xFF };
    q.push(Quad {
        sprite,
        cell,
        pos: [x as f32, y as f32],
        size: [(d.size[0] as i32 * sx >> 12) as f32, (d.size[1] as i32 * sy >> 12) as f32],
        width,
        color,
        alpha,
    });
}

/// 0x49B580: a menu line in sprite 0x32, centred; `fade` 0x80 is the plain grey, anything else
/// white with alpha falling as `fade` rises.
fn menu_text(q: &mut Vec<Quad>, sprites: &[Option<SpriteDef>], y: i32, s: &str, fade: i32) {
    let mut x = 0xA0 - 6 * s.len() as i32;
    let (color, mode) = if fade == 0x80 { ([0x80; 3], 0xFF) } else { ([0xFF; 3], fade * 0x200 + 0x60) };
    for c in s.bytes() {
        if c != b' ' {
            let cell = if c == b'\'' { 0x2F } else { c.wrapping_sub(b'a') as usize };
            put(q, sprites, 320.0, x, y, 0x32, cell, color, mode, 0x800, 0x800);
        }
        x += 0xC;
    }
}

/// 0x49D390: a line in texture 31's font, centred on `x`, in a 512-wide screen.
fn font_text(g: &mut Vec<Glyph>, y: i32, x: i32, s: &str) {
    font_text_in(g, 512.0, y, x, s);
}

/// As [`font_text`], in a `width`-wide screen.
fn font_text_in(g: &mut Vec<Glyph>, width: f32, y: i32, x: i32, s: &str) {
    let mut at = x - (s.len() as i32 * 0xD) / 2;
    for c in s.bytes() {
        let cell: Option<(i32, i32)> = match c {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' => {
                let k = match c {
                    b'a'..=b'z' => c - b'a',
                    b'A'..=b'Z' => c - b'A',
                    _ => c - 0x16,
                } as i32;
                Some(((k & 7) << 5, (k >> 3) << 5))
            }
            b'!' => Some((0x60, 0xA0)),
            b'\'' => Some((0xE0, 0xA0)),
            b'(' => Some((0xA0, 0xA0)),
            b')' => Some((0xC0, 0xA0)),
            b',' => Some((0xA0, 0x80)),
            b'.' => Some((0x80, 0x80)),
            b'/' => Some((0x20, 0xA0)),
            b':' => Some((0xE0, 0x80)),
            b';' => Some((0xC0, 0x80)),
            b'?' => Some((0x80, 0xA0)),
            b'\\' => Some((0, 0xA0)),
            _ => None,
        };
        if let Some((u, v)) = cell {
            g.push(Glyph { pos: [at as f32, y as f32], size: [16.0, 16.0], width, cell: [u as u8, v as u8] });
        }
        at += 0xD;
    }
}

/// The level select (0x438A50), in the map scene (16). A camera flies between three views of
/// each level: positions from level.dat list 2, the points they look at from list 1, entry
/// level × 3 + view (entry 1 is the overview the first visit starts at).
///
/// - Far from its view (more than 0x2000 object units), the camera's velocity eases (by 1/32
///   across, 1/16 up) towards half the unit direction × 4096 (0x451FD0); close, it is a
///   sixteenth of the way there plus a sine wobble, and every 256 ticks a random view of the
///   three is picked (`rand.dat`). Close, the view also creeps along the look direction (+0x60
///   a tick to 14000 / 1024 of it); moving away pulls it back (−0x100).
/// - The posts in list 3 (pairs: a centre at the bottom and a point one radius out at the
///   top) push it away (0x438790).
/// - It turns towards the look direction by an eighth of the way, at most 0x2000 / (distance
///   / 128 + 0x100) a tick (0x438650; pitch from the squared components), and the drawn
///   position follows by halves.
/// - Left / right move between level 1 and one past the levels done (auto-repeating after
///   0x20 ticks held); jump picks after 0x3C ticks, unless it's the newest level and there
///   aren't enough tokens ("you need more tokens", the count flashing red for 0xC0 ticks);
///   cancel goes back. Leaving fades for 0x1C ticks.
/// - Drawn: the level's collected tokens (sprite 0x36), the total (sprite 0x36 and digits
///   0x3A), the tokens needed and a lock (0x37) on a locked level, arrows (0x39), the level's
///   name. The "jump to select" / "cancel to go back" lines are placed off screen
///   (x = 13 × length − 0x1DB), so they don't show.
/// - Specials 0x57–0x63 run along lists 7 (0x57, 0x58), 8 (0x59–0x5F) and 9 (0x60–0x63), a
///   quarter of a point a tick, turning a quarter of the way to their heading (0x438910).
pub struct Select {
    /// 1–15.
    pub level: i32,
    /// The highest level that can be chosen.
    pub last: i32,
    looks: Vec<[i32; 3]>,
    spots: Vec<[i32; 3]>,
    posts: Vec<[i32; 3]>,
    rails: [Vec<[i32; 3]>; 3],
    random: Vec<u8>,
    rand_at: usize,
    pos: [i32; 3],
    drawn: [i32; 3],
    vel: [i32; 3],
    /// The eased look direction, and the one the camera turns to.
    eased: [i32; 3],
    dir: [i32; 3],
    zoom: i32,
    close: bool,
    view: usize,
    wobble: u32,
    clock: i32,
    anim: i32,
    refused: i32,
    leaving: i32,
    cancelled: bool,
    held: i32,
    pitch: i32,
    yaw: i32,
    phases: [i32; 13],
    piece_yaw: [i32; 13],
}

impl Select {
    fn new(lists: &[PointList], random: Vec<u8>, f: &FrontEnd, s: Summary) -> Select {
        let list = |k: i16| lists.iter().find(|l| l.kind == k).map(|l| l.points.clone()).unwrap_or_default();
        let (looks, spots) = (list(1), list(2));
        let last = s.levels.min(14) as i32 + 1;
        let mut level = f.level as i32;
        let mut shown = f.shown;
        if last < level {
            shown = 0;
            level = last;
        }
        let at = |v: &Vec<[i32; 3]>, k: usize| v.get(k).copied().unwrap_or_default();
        let start = at(&spots, shown * 3 + 1).map(|v| v << 5);
        let k = level as usize * 3 + 1;
        let eased = [0, 1, 2].map(|i| at(&looks, k)[i] - at(&spots, k)[i]);
        if level < last && f.first_clear {
            level += 1;
        }
        let mut sel = Select {
            level,
            last,
            looks,
            spots,
            posts: list(3),
            rails: [list(7), list(8), list(9)],
            random,
            rand_at: 0,
            pos: start,
            drawn: start,
            vel: [0; 3],
            eased,
            dir: eased,
            zoom: 0,
            close: false,
            view: 1,
            wobble: 3,
            clock: 0,
            anim: 0,
            refused: 0,
            leaving: 0,
            cancelled: false,
            held: 0,
            pitch: 0,
            yaw: 0,
            phases: [0x3200, 0x6400, 0x9000, 0x5D00, 0x4D00, 0x4400, 0x2C00, 0x1000, 0xA00, 0x8B00, 0x6E00, 0x3800, 0x1000],
            piece_yaw: [0; 13],
        };
        sel.aim(0x10);
        sel
    }

    fn at(v: &[[i32; 3]], k: usize) -> [i32; 3] {
        v.get(k).copied().unwrap_or_default()
    }

    /// 0x438650: turns yaw and pitch towards `dir`, at most `rate` a tick.
    fn aim(&mut self, rate: i32) {
        let turn = |angle: i32, target: i32| {
            let d = (target - angle) & 0xFFF;
            if d < 0x800 {
                angle + (d >> 3).min(rate)
            } else {
                angle - ((0x1000 - d) >> 3).min(rate)
            }
        };
        let [x, y, z] = self.dir;
        self.yaw = turn(self.yaw, crate::boxes::angle12(x as f32, z as f32)) & 0xFFF;
        let h = (z as i64 * z as i64 + x as i64 * x as i64) as f32;
        let v = if y < 0 { (y as i64 * y as i64) as f32 } else { -((y as i64 * y as i64) as f32) };
        let a = crate::boxes::angle12(v, h);
        self.pitch = turn(self.pitch, -a) & 0xFFF;
    }

    /// One tick; `Some(cancelled)` once it's left.
    fn tick(&mut self, pad: Pad, prev: &mut Pad, s: Summary, fade: &mut Fade, events: &mut Vec<Event>) -> Option<bool> {
        use crate::script::sin_table as sin;
        self.clock += 1;
        self.anim += 1;
        self.move_pieces();
        let k = self.level as usize * 3 + self.view;
        let spot = Self::at(&self.spots, k);
        let mut d = [0, 1, 2].map(|i| self.zoom * self.eased[i] / 1024 + spot[i] * 32 - self.pos[i]);
        let dist2 = d.iter().map(|&v| (v / 128) as i64 * (v / 128) as i64).sum::<i64>();
        while d.iter().any(|v| v.abs() > 0x4000) {
            d = d.map(|v| v >> 1);
        }
        if d.iter().map(|&v| v * v).sum::<i32>() < 0x4000001 {
            self.wobble += 1;
            self.close = true;
            if self.wobble & 0xFF < 3 {
                self.wobble |= 3;
                self.view = self.random.get(self.rand_at).map_or(1, |&r| r as usize % 3);
                self.rand_at = (self.rand_at + 1) % self.random.len().clamp(1, 0x5DC);
            }
            d[0] += sin(self.clock * 0x13) >> 5;
            d[1] += sin(self.clock * 0x17) >> 4;
            d[2] += sin(self.clock * 0x1D) >> 5;
            self.vel = d.map(|v| v >> 4);
        } else {
            let len = d.iter().map(|&v| (v as f32) * (v as f32)).sum::<f32>().sqrt();
            let n = d.map(|v| (v as f32 * 4096.0 / len) as i32);
            self.wobble = 3;
            self.vel[0] += ((n[0] >> 1) - self.vel[0]) >> 5;
            self.vel[1] += ((n[1] >> 1) - self.vel[1]) >> 4;
            self.vel[2] += ((n[2] >> 1) - self.vel[2]) >> 5;
        }
        self.avoid_posts();
        for i in 0..3 {
            self.pos[i] += self.vel[i];
        }
        let k = self.level as usize * 3 + self.view;
        let (look, spot) = (Self::at(&self.looks, k), Self::at(&self.spots, k));
        self.dir = [0, 1, 2].map(|i| look[i] - spot[i]);
        for i in 0..3 {
            self.eased[i] += (self.dir[i] - self.eased[i]) >> 4;
        }
        self.zoom = if self.close { (self.zoom + 0x60).min(14000) } else { (self.zoom - 0x100).max(0) };
        self.aim(0x2000 / ((dist2 as f64).sqrt() as i32 + 0x100));
        for i in 0..3 {
            self.drawn[i] += (self.pos[i] - self.drawn[i]) >> 1;
        }
        // Holding a direction repeats it (every 16 ticks after 0x20).
        if pad == *prev {
            self.held += 1;
            if self.held > 0x20 && self.held & 0xF < 3 {
                self.held |= 3;
                *prev = Pad(0);
            }
        } else {
            self.held = 0;
        }
        let pressed = pad.0 & !prev.0;
        *prev = pad;
        let locked = self.level == self.last && s.tokens < s.needed;
        if pressed & Pad::JUMP != 0 && self.leaving == 0 && self.clock > 0x3C {
            events.push(Event::Sound(0));
            if locked {
                self.refused = 0xC0;
            } else {
                self.leaving = 1;
                fade.to(0, 6);
            }
        }
        if pad.0 & Pad::CANCEL != 0 && self.leaving == 0 && self.clock > 0x3C {
            fade.to(0, 6);
            events.push(Event::Sound(2));
            self.cancelled = true;
            self.leaving = 2;
        } else if self.leaving != 0 {
            self.leaving += self.leaving.signum();
        } else {
            let before = self.level;
            if pressed & Pad::RIGHT != 0 && self.level < self.last {
                self.level += 1;
            } else if pressed & Pad::LEFT != 0 && self.level >= 2 {
                self.level -= 1;
            }
            if self.level != before {
                self.close = false;
                events.push(Event::Sound(1));
            }
        }
        if self.refused > 0 {
            self.refused -= 1;
        }
        (self.leaving.abs() > 0x1B).then_some(self.cancelled)
    }

    /// 0x438790: list 3's posts push the camera's velocity away across.
    fn avoid_posts(&mut self) {
        let p = self.pos.map(|v| v / 32);
        for pair in self.posts.chunks_exact(2) {
            let (a, b) = (pair[0], pair[1]);
            if !(p[1] < a[1] && b[1] < p[1]) {
                continue;
            }
            let r2 = ((b[2] - a[2]) >> 2).pow(2) + ((b[0] - a[0]) >> 2).pow(2);
            let mut v = [p[0] - a[0], 0, p[2] - a[2]];
            let d2 = ((p[2] - a[2]) >> 2).pow(2) + (v[0] >> 2).pow(2);
            if d2 >= r2 {
                continue;
            }
            while v[0].abs() > 0x4000 || v[2].abs() > 0x4000 {
                v = v.map(|c| c >> 1);
            }
            let len = ((v[0] as f32).powi(2) + (v[2] as f32).powi(2)).sqrt().max(1.0);
            let n = v.map(|c| (c as f32 * 4096.0 / len) as i32);
            let push = (0x800 - d2 * 0x800 / r2).min(0x180);
            self.vel[0] += (n[0] * push) >> 12;
            self.vel[2] += (n[2] * push) >> 12;
        }
    }

    fn rail(&self, k: usize) -> &[[i32; 3]] {
        match k {
            0..=1 => &self.rails[0],
            2..=8 => &self.rails[1],
            _ => &self.rails[2],
        }
    }

    /// 0x438910 for each moving piece of the map: on along its rail, turning a quarter of
    /// the way to face back along its travel (the heading + 0x800).
    fn move_pieces(&mut self) {
        for k in 0..13 {
            let before = self.piece(k);
            let len = self.rail(k).len() as i32;
            if len < 6 {
                continue;
            }
            self.phases[k] += 0x40;
            if len - 5 < self.phases[k] >> 8 {
                self.phases[k] = 0;
            }
            if let (Some(a), Some(b)) = (before, self.piece(k)) {
                let heading = crate::boxes::angle12((b[0] - a[0]) as f32, (b[2] - a[2]) as f32);
                let d = (self.piece_yaw[k] - heading - 0x800) & 0xFFF;
                self.piece_yaw[k] += if d <= 0x800 { -(d >> 2) } else { (0x1000 - d) >> 2 };
                self.piece_yaw[k] &= 0xFFF;
            }
        }
    }

    /// Position of piece `k` (level units): between two points of its rail.
    fn piece(&self, k: usize) -> Option<[i32; 3]> {
        let rail = self.rail(k);
        let (i, f) = ((self.phases[k] >> 8) as usize, self.phases[k] & 0xFF);
        let (a, b) = (*rail.get(i)?, *rail.get(i + 1)?);
        Some([0, 1, 2].map(|c| (a[c] * (0xFF - f) + b[c] * f) >> 8))
    }

    fn draw(&self, f: &mut Frame, sprites: &[Option<SpriteDef>], p: &Progress, t: &Tables) {
        let s = p.summary(t);
        f.camera = Some((self.drawn, self.pitch, self.yaw));
        // Specials 0–99 shown, 100–366 hidden; then for each level up to the last that can be
        // chosen, its blueprint hidden and its finished model shown.
        let mut shown: Vec<bool> = (0..0x16F).map(|k| k < 100).collect();
        for (hide, show) in t.models.iter().take(self.last as usize) {
            for &k in hide {
                if let Some(s) = shown.get_mut(k) {
                    *s = false;
                }
            }
            for &k in show {
                if let Some(s) = shown.get_mut(k) {
                    *s = true;
                }
            }
        }
        f.shown = shown.into_iter().enumerate().collect();
        for k in 0..13 {
            if let Some(pos) = self.piece(k) {
                f.pieces.push((0x57 + k, pos, self.piece_yaw[k]));
            }
        }
        let id = t.order.get(self.level as usize - 1).copied().unwrap_or(0);
        let n = (p.tokens.get(id).copied().unwrap_or(0) & 0x1F).count_ones() as i32;
        let q = &mut f.quads;
        for k in 0..n {
            put(q, sprites, 512.0, (k * 2 - n) * 0x13 + 0x100, 0x32, 0x36, ((k * 4 + 4 + self.anim / 2) & 0x1F) as usize, [0x80; 3], 0x60, 0xCCC, 0x800);
        }
        let red = self.refused > 0 && self.refused & 0x3F >= 0x19;
        let tint = if red { [0x80, 0x20, 0x20] } else { [0x80; 3] };
        put(q, sprites, 320.0, 0xE, 0xC0, 0x36, (self.anim / 2 & 0x1F) as usize, tint, 0xFF, 0x1000, 0x1000);
        put(q, sprites, 512.0, 0x66, 0xD0, 0x3A, (s.tokens / 10 % 10) as usize, tint, 0xFF, 0x1000, 0x1000);
        put(q, sprites, 512.0, 0x73, 0xD0, 0x3A, (s.tokens % 10) as usize, tint, 0xFF, 0x1000, 0x1000);
        let blink = self.clock & 0x3F < 0x30;
        if self.level == self.last && s.tokens < s.needed {
            if blink {
                if s.needed < 10 {
                    put(q, sprites, 512.0, 0xF4, 0x6D, 0x3A, (s.needed % 10) as usize, [0x80; 3], 0x60, 0x181C, 0x181C);
                } else {
                    put(q, sprites, 512.0, 0xE8, 0x6D, 0x3A, (s.needed / 10 % 10) as usize, [0x80; 3], 0x60, 6000, 0x157C);
                    put(q, sprites, 512.0, 0x100, 0x6D, 0x3A, (s.needed % 10) as usize, [0x80; 3], 0x60, 6000, 0x157C);
                }
            }
            put(q, sprites, 320.0, 0x70, 0x58, 0x37, 0, [0x80; 3], 0xFF, 0x1000, 0x1000);
        }
        if self.level > 1 && blink {
            put(q, sprites, 512.0, 0x20, 0x70, 0x39, 0, [0x80; 3], 0xFF, 0x1000, 0x1000);
        }
        if self.level < self.last && blink {
            put(q, sprites, 512.0, 0x1C0, 0x70, 0x39, 1, [0x80; 3], 0xFF, 0x1000, 0x1000);
        }
        if self.refused > 0 && self.refused & 0x3F > 0x18 {
            font_text(&mut f.glyphs, 0x9A, 0x100, "you need more tokens");
        }
        if let Some(name) = t.names.get(self.level as usize) {
            font_text(&mut f.glyphs, 0x20, 0x100, name);
        }
    }
}

/// The results screen (0x4398B0), in scene 0 over picture 0, music `levcomp`.
///
/// - Across the top, the level's title in sprites 0x52 (cells 0xF, the level, 0x10, 0x11) and
///   0x53 (the level), and its five token slots (sprite 0x4C at x 0x24 + 0x34k, y 0x20):
///   tokens held before the level spin, the rest are black.
/// - After 60 ticks, the tokens won this time appear one every 0x1E ticks (sound 1), each in
///   a burst of 15 sparks (sprite 0x50, falling: + 0x18 a tick down, 0x1F ticks). Then the
///   coin count (sprite 0x4E spinning, × 0x4F cell 10, two digits) counts up, one every 3
///   ticks with sound 1.
/// - The five abilities (sprite 0x51, cells 0–4 at x 0x44 + 0x28k, y 0xAD) are lit if held
///   (0x52F2D8 bits 1, 2, 4, 0x10, 8), else black.
/// - A timer from 0x168 counts down and holds between 0x79 and 0xB4; "press jump to exit"
///   blinks while it's between 0x79 and 0xEF. Jump there (sound 0) skips to the end of the
///   counting (timer 0x78), or, once it's done, leaves (timer 0x18). Below 0x18 it fades out;
///   at 0 it's over.
///
/// Approximated: the sparks are drawn blended, not added (mode 0x20).
pub struct Results {
    level: usize,
    before: [bool; 5],
    now: [bool; 5],
    coins: i32,
    abilities: u8,
    timer: i32,
    reveal: i32,
    reveal_mark: i32,
    count: i32,
    count_mark: i32,
    shown: i32,
    blink: i32,
    clock: i32,
    faded: bool,
    rand_at: usize,
    /// Sparks: position and velocity (1/256 pixel), ticks left.
    sparks: Vec<([i32; 2], [i32; 2], i32)>,
}

impl Results {
    fn new(r: LevelResult, level: usize) -> Results {
        let bits = |b: u8| std::array::from_fn(|k| b & (1 << k) != 0);
        let (before, now): ([bool; 5], [bool; 5]) = (bits(r.before), bits(r.now));
        let won = (0..5).filter(|&k| now[k] && !before[k]).count() as i32;
        let mut reveal = won * 0x3C;
        if reveal != 0 {
            reveal += 0x3C;
        }
        let count = r.coins * 3 + 0x10;
        Results {
            level,
            before,
            now,
            coins: r.coins,
            abilities: r.abilities,
            timer: 0x168,
            reveal,
            reveal_mark: reveal - 0x1E,
            count,
            count_mark: count,
            shown: 0,
            blink: 0,
            clock: 0,
            faded: false,
            rand_at: 0,
            sparks: Vec::new(),
        }
    }

    /// One tick; true once it's over.
    fn tick(&mut self, pressed: Pad, fade: &mut Fade, events: &mut Vec<Event>, random: &[u8]) -> bool {
        for (p, v, life) in &mut self.sparks {
            p[0] += v[0];
            p[1] += v[1];
            v[1] += 0x18;
            *life -= 1;
        }
        self.sparks.retain(|s| s.2 > 0);
        if self.timer < 300 {
            if self.reveal < 1 {
                if self.count < 1 {
                    self.shown = (self.shown + 1).min(self.coins);
                } else {
                    if self.count_mark - self.count > 3 {
                        if self.shown < self.coins {
                            events.push(Event::Sound(1));
                        }
                        while self.count_mark - self.count > 3 {
                            self.count_mark -= 3;
                            self.shown = (self.shown + 1).min(self.coins);
                        }
                    }
                    self.count -= 1;
                }
            } else if self.reveal_mark - self.reveal >= 0x1E {
                self.reveal_mark -= 0x1E;
                if let Some(k) = (0..5).find(|&k| !self.before[k] && self.now[k]) {
                    events.push(Event::Sound(1));
                    self.before[k] = true;
                    self.burst(k, random);
                }
                self.reveal -= 1;
            } else {
                self.reveal -= 1;
            }
        }
        if 0x78 < self.timer && self.timer < 0xF0 {
            self.blink = (self.blink + 1) & 0x3F;
        }
        if !(0x78 < self.timer && self.timer < 0xB5) {
            self.timer -= 1;
        }
        if self.timer < 0x18 {
            self.timer = self.timer.max(0);
            if !self.faded {
                fade.to(0, 0xC);
                self.faded = true;
            }
        }
        self.clock += 1;
        if pressed.0 & Pad::JUMP != 0 && 0x78 < self.timer && self.timer < 0xF0 {
            events.push(Event::Sound(0));
            if self.reveal == 0 && self.count == 0 {
                self.timer = 0x18;
            } else {
                self.shown = self.coins;
                self.timer = 0x78;
                self.before = self.now;
            }
        }
        self.timer < 1
    }

    /// 15 sparks from token slot `k`, from four random bytes each.
    fn burst(&mut self, k: usize, random: &[u8]) {
        let r = |i: usize| random.get(i % random.len().max(1)).copied().unwrap_or(0x80) as i32;
        for _ in 0..15 {
            let (a, b, c) = (r(self.rand_at), r(self.rand_at + 1), r(self.rand_at + 2));
            self.rand_at = (self.rand_at + 4) % 0x5DC;
            let dx = (a - 0x80) * 0x10;
            let dy = b * 8 - 0x400;
            let pos = [(k as i32 * 0xD + 0xD) * 0x400 + dx, b * 8 + 0x2400];
            let vel = [dx / 7 + (c >> 4) - 0x10, -0x180 + dy / 7 + (c >> 1)];
            self.sparks.push((pos, vel, 0x1F));
        }
    }

    fn draw(&self, f: &mut Frame, sprites: &[Option<SpriteDef>]) {
        f.picture = Some(0x28);
        let q = &mut f.quads;
        let white = [0xFF; 3];
        let lv = self.level;
        for (x, sprite, cell) in [(0x2C, 0x52, 0xF), (0x60, 0x52, lv), (0x94, 0x52, 0x10), (200, 0x52, 0x11), (0xFC, 0x53, lv)] {
            put(q, sprites, 320.0, x, 0x3C, sprite, cell, white, 0xFF, 0x800, 0x800);
        }
        for k in 0..5 {
            let cell = ((self.clock / 2 + 4 * k as i32) & 0x1F) as usize;
            let color = if self.before[k] { [0x80; 3] } else { [0; 3] };
            put(q, sprites, 320.0, k as i32 * 0x34 + 0x24, 0x20, 0x4C, cell, color, 0, 0x1000, 0x1000);
        }
        for (k, bit) in [1u8, 2, 4, 0x10, 8].into_iter().enumerate() {
            let color = if self.abilities & bit != 0 { white } else { [0; 3] };
            put(q, sprites, 320.0, k as i32 * 0x28 + 0x44, 0xAD, 0x51, k, color, 0xFF, 0x800, 0x800);
        }
        put(q, sprites, 320.0, 0x5C, 0x76, 0x4E, ((10 + self.clock) & 0xF) as usize, [0x80; 3], 0, 0x1000, 0x1000);
        put(q, sprites, 320.0, 0x98, 0x7A, 0x4F, 10, [0x80; 3], 0, 0x1000, 0x1000);
        put(q, sprites, 320.0, 0xBE, 0x7A, 0x4F, (self.shown / 10 % 10) as usize, [0x80; 3], 0, 0x1000, 0x1000);
        put(q, sprites, 320.0, 0xD0, 0x7A, 0x4F, (self.shown % 10) as usize, [0x80; 3], 0, 0x1000, 0x1000);
        for (p, _, life) in &self.sparks {
            put(q, sprites, 320.0, p[0] >> 8, p[1] >> 8, 0x50, (life >> 3) as usize, [0x40, 0x40, 0], 0, 0x1000, 0x1000);
        }
        if 0x78 < self.timer && self.timer < 0xF0 && self.blink < 0x30 {
            font_text_in(&mut f.glyphs, 320.0, 0xDC, 0xA0, "press jump to exit");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> Tables {
        Tables { order: vec![1, 2, 6, 4, 5, 3, 7, 8, 9, 10, 11, 12, 13, 14, 15], names: vec![String::new(); 16], needed: vec![0, 0, 3, 0, 0, 10, 0, 0, 18, 0, 0, 28, 0, 0, 40, 0], models: Vec::new() }
    }

    #[test]
    fn summary_counts_levels_in_campaign_order() {
        let t = tables();
        let mut p = Progress::default();
        assert_eq!(p.summary(&t), Summary { levels: 0, tokens: 0, needed: 0 });
        p.tokens[1] = 0b10011;
        p.tokens[2] = 0b00001;
        // Internal 6 (Bombs Away) is third; internal 4 done without it doesn't count.
        p.tokens[4] = 0b11111;
        assert_eq!(p.summary(&t), Summary { levels: 2, tokens: 4, needed: 3 });
        p.tokens[6] = 0x80;
        assert_eq!(p.summary(&t), Summary { levels: 4, tokens: 9, needed: 0 });
    }

    #[test]
    fn title_and_menu_lead_to_the_level_select() {
        let mut f = FrontEnd::new(tables());
        let run = |f: &mut FrontEnd, pad: u32, n: usize| {
            for _ in 0..n {
                f.tick(Pad(pad));
            }
        };
        // Jump before 0x1E ticks does nothing; after, the title fades to the menu.
        run(&mut f, Pad::JUMP, 1);
        run(&mut f, 0, 0x20);
        assert_eq!(f.screen, Screen::Title);
        run(&mut f, Pad::JUMP, 1);
        run(&mut f, 0, 0x17);
        assert_eq!(f.screen, Screen::Menu);
        // Down twice and back up once: "options" is chosen, which returns to the menu.
        run(&mut f, 0, 0x20);
        for b in [Pad::DOWN, Pad::DOWN, Pad::UP] {
            run(&mut f, b, 1);
            run(&mut f, 0, 0x14);
        }
        assert_eq!(f.row, 0x8C);
        // Back to "start game" and pick it.
        run(&mut f, Pad::UP, 1);
        run(&mut f, 0, 0x14);
        run(&mut f, Pad::JUMP, 1);
        run(&mut f, 0, 0x17);
        assert_eq!(f.screen, Screen::Select);
        assert!(f.events.contains(&Event::Sound(0)) && f.events.contains(&Event::Sound(1)));
    }

    #[test]
    fn results_reveal_count_and_leave_on_jump() {
        let mut f = FrontEnd::new(tables());
        f.level = 1;
        f.level_done(Some(LevelResult { before: 1, now: 0b1011, coins: 12, abilities: 0 }));
        assert_eq!(f.screen, Screen::Results);
        // Nothing happens for 60 ticks; two new tokens then take 3 × 0x3C, coins 12 × 3 + 0x10,
        // while the timer holds at 0xB4 (from tick 180) until jump.
        for _ in 0..320 {
            f.tick(Pad(0));
        }
        let r = f.results.as_ref().unwrap();
        assert_eq!(r.before, [true, true, false, true, false]);
        assert_eq!(r.shown, 12);
        assert_eq!(f.events.iter().filter(|e| **e == Event::Sound(1)).count(), 2 + 12);
        assert_eq!(f.screen, Screen::Results);
        // Jump, once it's all counted, leaves: a fade, then the level select.
        f.tick(Pad(Pad::JUMP));
        for _ in 0..0x18 {
            f.tick(Pad(0));
        }
        assert_eq!(f.screen, Screen::Select);
    }

    #[test]
    fn game_over_returns_to_the_title_with_a_new_game() {
        let mut f = FrontEnd::new(tables());
        f.progress.tokens[1] = 3;
        f.progress.lives = 0;
        f.started = true;
        f.game_over();
        assert_eq!((f.screen, f.screen.variant()), (Screen::GameOver, 1));
        assert_eq!(f.frame(&[]).picture, Some(0x29));
        // A press in the first 60 ticks is ignored; after, it cuts to the fade.
        f.tick(Pad(Pad::JUMP));
        f.tick(Pad(0));
        for _ in 0..100 {
            f.tick(Pad(0));
        }
        assert_eq!(f.screen, Screen::GameOver);
        f.tick(Pad(Pad::JUMP));
        for _ in 0..0x18 {
            f.tick(Pad(0));
        }
        assert_eq!(f.screen, Screen::Title);
        assert_eq!(f.progress, Progress::default());
        assert!(!f.started);
    }

    #[test]
    fn exit_quits() {
        let mut f = FrontEnd::new(tables());
        f.enter(Screen::Menu);
        for _ in 0..4 {
            f.tick(Pad(Pad::DOWN));
            for _ in 0..0x14 {
                f.tick(Pad(0));
            }
        }
        f.tick(Pad(Pad::JUMP));
        assert_eq!(f.screen, Screen::Quit);
    }

    #[test]
    fn font_cells() {
        let mut g = Vec::new();
        font_text(&mut g, 0, 0x100, "a z9!");
        let cells: Vec<_> = g.iter().map(|g| g.cell).collect();
        assert_eq!(cells, vec![[0, 0], [0x20, 0x60], [0x60, 0x80], [0x60, 0xA0]]);
        assert_eq!(g[0].pos[0], (0x100 - 5 * 0xD / 2) as f32);
    }
}
