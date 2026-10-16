//! What every level's script shares (0x4A1A50 starts a level's script, 0x4A1B00 runs it each
//! frame): conversations and info scenes, the HUD counters scripts drive, token reveals,
//! sounds and particles they ask for, and the game's random stream. Each level's own script
//! (`level1`, …) implements [`Script`] and derefs to its [`Common`].

use crate::boxes::Boxes;
use crate::creatures::{Creatures, ParticleRequest};
use crate::cutscene::Cutscene;
use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::game::{Player, World};
use crate::specials::Specials;

/// Texts and camera lists of the level's conversations (from toy2.exe and level.dat).
#[derive(Debug, Clone, Default)]
pub struct Story {
    pub texts: std::collections::HashMap<u32, String>,
    pub lists: std::collections::HashMap<i16, Vec<[i32; 3]>>,
    /// Hint texts' addresses, by token.
    pub hints: Vec<u32>,
    /// The info pickups' scenes (0x4025C0's table): pickup id, camera list, text address,
    /// Buzz's facing.
    pub infos: Vec<(u8, i16, u32, i32)>,
}

/// State and requests every level script has.
#[derive(Default)]
pub struct Common {
    /// A conversation or info scene in progress.
    pub cutscene: Option<Cutscene>,
    /// Who stands where at its start: Buzz and the character's slot (positions, facings).
    pub staging: Option<([[i32; 3]; 2], [i32; 2], usize)>,
    pub story: Story,
    /// Sounds at positions (object units).
    pub sounds: Vec<(u16, [i32; 3])>,
    /// Voice lines on Buzz's channel (0x49EA60) and sample slots at a position (0x4A3C80:
    /// slot, volume).
    pub voices: Vec<u16>,
    pub slots: Vec<(u16, i32, [i32; 3])>,
    /// Tokens handed over or shown (0x830CD0 per token).
    pub given: [bool; 5],
    /// Tokens to show now (0x4A0DB0 outside a conversation).
    pub reveals: Vec<usize>,
    /// Tokens to move (level units) or hide again this tick.
    pub token_moves: Vec<(usize, [i32; 3])>,
    pub hides: Vec<usize>,
    /// The tokens Buzz has collected (bits, `0x52F0D7[level]`), set by the caller.
    pub tokens: u8,
    /// 0x830D48: the level's missing item (from 0x503A22; negative once Buzz has it, 0 once
    /// returned), and the abilities (0x52F2D8) Buzz has.
    pub item: i32,
    pub abilities: u8,
    /// HUD counters: the race state and laps (element 6), whether a boss bar shows
    /// (element 7) and its value (0x52B7E0, −1 = the creatures' own).
    pub race: i32,
    pub laps: i32,
    pub boss_hud: bool,
    pub boss_bar: Option<i32>,
    /// Particles the script makes: by type and mode (0x40FDF0) with a spin and a life, and
    /// fully specified (0x40FAE0).
    pub spawns: Vec<([i32; 3], u8, usize, i32, Option<i32>)>,
    pub particles: Vec<ParticleRequest>,
    /// Reward sparkles to put out (0x49FAB0: index, group).
    pub put_out: Vec<(usize, usize)>,
    /// Ticks run, behind the every-n-ticks flags (0x4A2960).
    pub clock: u32,
    /// A look-at scene (0x4020F0): the camera at `from` looking at `at` (object units) for
    /// `ticks` (0x50A1F4) while play stands still.
    pub look: Option<Look>,
    /// The script counted the look-at down itself this tick (else the caller does).
    pub look_ticked: bool,
    /// The level is over (0x830CC4 = 1: the token screen).
    pub done: bool,
    /// Pickups (by index in list 63) to hide (false) or bring back (true).
    pub pickups: Vec<(usize, bool)>,
    /// Particle types whose live particles end now (the game sets their life to 1).
    pub pop: Vec<u8>,
    /// The challenge collectables Buzz has (0x830D4C), set by the caller; a request to zero
    /// it, and to show or hide the challenge's collectables.
    pub collected: i32,
    pub collect_reset: bool,
    pub show_collectables: Option<bool>,
    /// The characters' call timers, as 0x4A2080 sets them at the start: Hamm (0x830C98),
    /// the hint character (0x830CA4) and its next hint (0x830D24, −1 until first in play),
    /// the finder (0x830D40), Mr Potato Head (0x830D34).
    pub timers: Timers,
    /// The game's random bytes (`rand.dat`) and read position.
    pub(crate) random: Vec<u8>,
    pub(crate) rpos: usize,
}

impl Common {
    /// Gives the script the game's random bytes for its effects.
    pub fn set_random(&mut self, random: Vec<u8>) {
        self.random = random;
    }

    pub fn rand(&mut self) -> i32 {
        if self.random.is_empty() {
            return 0x80;
        }
        let b = self.random[self.rpos % self.random.len()];
        self.rpos = (self.rpos + 1) % 0x5DC.min(self.random.len());
        b as i32
    }

    /// Every `n` ticks (the flags 0x52F1C2–0x52F1CB of 0x4A2960).
    pub fn every(&self, n: u32) -> bool {
        self.clock.is_multiple_of(n)
    }

    /// Loads conversation texts (exe addresses), the info-scene table (0x4025C0's argument)
    /// and every camera list.
    pub fn load_story(&mut self, exe: &Exe, lists: &[PointList], texts: &[u32], info_table: Option<u32>) {
        if let Some(table) = info_table {
            for k in 0..10 {
                let Ok(b) = exe.at(table + 16 * k, 16) else { break };
                let w = |i: usize| i32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
                if w(0) < 0 {
                    break;
                }
                self.story.infos.push((w(0) as u8, w(1) as i16, w(2) as u32, w(3)));
            }
        }
        let infos: Vec<u32> = self.story.infos.iter().map(|i| i.2).collect();
        let hints = self.story.hints.clone();
        for va in texts.iter().copied().chain(hints).chain(infos) {
            let mut s = Vec::new();
            while let Ok(b) = exe.at(va + s.len() as u32, 1) {
                if b[0] == 0 || s.len() > 400 {
                    break;
                }
                s.push(b[0]);
            }
            self.story.texts.insert(va, String::from_utf8_lossy(&s).into_owned());
        }
        for l in lists.iter().filter(|l| !l.points.is_empty()) {
            self.story.lists.insert(l.kind, l.points.clone());
        }
    }

    /// Starts a conversation (0x4027F0) on camera list `list` with `text` (an exe address).
    pub fn talk(&mut self, slot: usize, list: i16, text: u32, token: Option<usize>, facings: Option<[i32; 2]>) {
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

    /// 0x402610: touching an info pickup (id `id`) plays its scene: Buzz stands at the first
    /// point of its camera list facing the table's way while the text is shown. Nothing if
    /// the pickup has no scene or one is already running.
    pub fn info(&mut self, id: u8) -> bool {
        let Some(&(_, list, text, facing)) = self.story.infos.iter().find(|i| i.0 == id) else { return false };
        if self.cutscene.is_some() {
            return false;
        }
        self.talk(usize::MAX, list, text, None, Some([facing & 0xFFF, 0]));
        true
    }
}

/// See [`Common::look`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub from: [i32; 3],
    pub at: [i32; 3],
    pub ticks: i32,
}

impl Common {
    /// 0x4020F0(target, ticks, distance): leaves first person and holds the camera
    /// `distance` × 0x400 out from `target` on Buzz's side, looking at it.
    pub fn look_at(&mut self, target: [i32; 3], buzz: [i32; 3], ticks: i32, distance: i32) {
        let a = crate::boxes::angle12((target[0] - buzz[0]) as f32, (target[2] - buzz[2]) as f32);
        let from = [target[0] + (sin_table(a - 0x800) >> 4) * distance, target[1], target[2] + (sin_table(a - 0x400) >> 4) * distance];
        self.look = Some(Look { from, at: target, ticks });
    }

    /// The scene's remaining ticks (0x50A1F4), 0 without one.
    pub fn look_ticks(&self) -> i32 {
        self.look.map_or(0, |l| l.ticks)
    }

    /// 0x402A10's countdown: one tick of the scene, ending it at 0.
    pub fn tick_look(&mut self) {
        self.look_ticked = true;
        if let Some(l) = self.look.as_mut() {
            l.ticks -= 1;
            if l.ticks < 1 {
                self.look = None;
            }
        }
    }
}

/// A mover following a path program (0x48ACC0): the program (i16 words from the exe), where
/// it is in it, a wait count and the speed ramp (0 still, ramping up to 0x40, negative while
/// slowing for the next point).
#[derive(Debug, Clone, Default)]
pub struct PathRunner {
    pub program: Vec<i16>,
    pub pc: usize,
    wait: i32,
    ramp: i32,
}

impl PathRunner {
    /// Reads a program from the exe (it loops with command 0, so 256 words are plenty).
    pub fn load(exe: &Exe, va: u32) -> PathRunner {
        let program = (0..256).map_while(|k| exe.at(va + 2 * k, 2).ok().map(|b| i16::from_le_bytes([b[0], b[1]]))).collect();
        PathRunner { program, ..Default::default() }
    }

    /// One tick for mover `mover` carrying special `special`. Commands: 0 n — back n words;
    /// 1 dx dy dz speed — to the special's home plus (dx, dy, dz), easing in and out; 2 n — wait
    /// n ticks; 3 mask base — wait (r & mask) + base; 4 list point speed — to a level.dat
    /// point, easing; 6 / 7 mask — wait until the shared flags have / lack it; 8 / 9 mask —
    /// set / clear flags; 10 list point speed — to a point at a steady speed. Speeds are in
    /// level units a tick. The special is put where the mover is.
    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, world: &World, specials: &mut Specials, mover: i32, special: i32, flags: &mut u16, lists: &std::collections::HashMap<i16, Vec<[i32; 3]>>, rand: &mut dyn FnMut() -> i32) {
        let w = |k: usize| self.program.get(k).copied().unwrap_or(0) as i32;
        let at = self.pc;
        let here = || world.mover_pos(mover).map(|p| [p.x as i32, p.y as i32, p.z as i32]);
        match w(at) {
            0 => {
                self.pc = at.saturating_sub(w(at + 1) as usize);
                self.wait = 0;
            }
            1 | 4 | 10 => {
                let Some(cur) = here() else { return };
                specials.set_pos(special, cur);
                let (target, speed, len) = match w(at) {
                    1 => {
                        let h = specials.home_obj(special).unwrap_or_default();
                        ([(h[0] >> 5) + w(at + 1), (h[1] >> 5) + w(at + 2), (h[2] >> 5) + w(at + 3)], w(at + 4), 5)
                    }
                    k => {
                        let p = lists.get(&(w(at + 1) as i16)).and_then(|l| l.get(w(at + 2) as usize)).copied().unwrap_or(cur);
                        (p, w(at + 3), if k == 4 { 4 } else { 4 })
                    }
                };
                let d = [0, 1, 2].map(|k| (cur[k] - target[k]).abs());
                let near = |r: i32| d.iter().all(|&v| v < r);
                let v = if w(at) == 10 {
                    if near(speed + 8) {
                        None
                    } else {
                        Some(speed)
                    }
                } else {
                    if near(speed * 4 + 0x30) && self.ramp > 0 {
                        self.ramp = -0x40;
                    }
                    if self.ramp < 0 {
                        self.ramp = (self.ramp + 2).min(0);
                    } else if self.ramp < 0x40 {
                        self.ramp += 2;
                    }
                    if self.ramp != 0 && !near(speed + 8) {
                        let s = if self.ramp < 0 {
                            (-self.ramp * speed) >> 6
                        } else if self.ramp < 0x40 {
                            (self.ramp * speed) >> 6
                        } else {
                            speed
                        };
                        Some(s.max(1))
                    } else {
                        None
                    }
                };
                match v {
                    Some(s) => {
                        let dir = glam::Vec3::new((target[0] - cur[0]) as f32, (target[1] - cur[1]) as f32, (target[2] - cur[2]) as f32).normalize_or_zero();
                        let step = dir * s as f32;
                        world.move_mover(mover, step);
                    }
                    None => {
                        self.ramp = 0;
                        self.wait = 0;
                        self.pc = at + len;
                    }
                }
            }
            2 => {
                if self.wait == 0 {
                    self.wait = w(at + 1);
                } else {
                    self.wait -= 1;
                    if self.wait < 1 {
                        self.wait = 0;
                        self.pc = at + 2;
                    }
                }
            }
            3 => {
                if self.wait == 0 {
                    self.wait = (w(at + 1) & rand()) + w(at + 2);
                } else {
                    self.wait -= 1;
                    if self.wait < 1 {
                        self.wait = 0;
                        self.pc = at + 3;
                    }
                }
            }
            6 if *flags & w(at + 1) as u16 != 0 => self.pc = at + 2,
            7 if *flags & w(at + 1) as u16 == 0 => self.pc = at + 2,
            8 => {
                *flags |= w(at + 1) as u16;
                self.pc = at + 2;
            }
            9 => {
                *flags &= !(w(at + 1) as u16);
                self.pc = at + 2;
            }
            _ => {}
        }
    }
}

/// See [`Common::timers`].
#[derive(Debug, Clone, Copy)]
pub struct Timers {
    pub hamm: i32,
    pub hint_call: i32,
    pub hint: i32,
    pub finder: i32,
    pub potato: i32,
}

impl Default for Timers {
    fn default() -> Timers {
        Timers { hamm: 200, hint_call: 100, hint: -1, finder: 0xFA, potato: 0x96 }
    }
}

/// Hamm's two lines (0x4A1CE0), the hint character's line once Buzz has every token.
pub const HAMM_THANKS: u32 = 0x502834;
pub const HAMM_ASK: u32 = 0x5027E0;
pub const ALL_TOKENS: u32 = 0x502868;

/// A slot's touch flag (0x200), taken.
pub fn touched(cr: &mut Creatures, slot: usize) -> bool {
    cr.slots.get_mut(slot).filter(|c| c.id != 0 && c.flags & 0x200 != 0).map(|c| c.flags &= !0x200).is_some()
}

impl Common {
    fn in_view(cr: &Creatures, slot: usize) -> Option<[i32; 3]> {
        cr.slots.get(slot).filter(|c| c.id != 0 && c.active && c.in_reach).map(|c| c.pos)
    }

    /// 0x4A1CE0(slot, list, Buzz's facing, Hamm's, token): until his token is given, Hamm
    /// calls (sample 72) every 2r + 0xF0 ticks in view; touched, with 50 coins or more he
    /// hands it over (sample 74, "well done buzz…"), else he asks for them (sample 73).
    pub fn hamm(&mut self, cr: &mut Creatures, slot: usize, list: i16, facings: Option<[i32; 2]>, token: usize, coins: i32) {
        let given = self.given.get(token).copied().unwrap_or(true);
        if !given && self.cutscene.is_none() && let Some(pos) = Self::in_view(cr, slot) {
            self.timers.hamm -= 1;
            if self.timers.hamm < 0 {
                self.timers.hamm = self.rand() * 2 + 0xF0;
                self.slots.push((72, 128, pos));
            }
        }
        let pos = cr.slots.get(slot).map_or([0; 3], |c| c.pos);
        if touched(cr, slot) && !given {
            if coins > 0x31 {
                self.slots.push((74, 128, pos));
                self.talk(slot, list, HAMM_THANKS, Some(token), facings.map(|f| f.map(|a| a & 0xFFF)));
            } else {
                self.slots.push((73, 128, pos));
                self.talk(slot, list, HAMM_ASK, None, facings.map(|f| f.map(|a| a & 0xFFF)));
            }
        }
    }

    /// 0x4A1E60(slot, list, hints): the hint character. In view, he calls now and then (slot
    /// 70); touched, he gives the hint for the next token Buzz hasn't got (slot 71), starting
    /// from a random one, or says Buzz has them all. Hints come from `story.hints`.
    pub fn hint_character(&mut self, cr: &mut Creatures, slot: usize, list: i16) {
        let talking = self.cutscene.is_some();
        let Some(pos) = Self::in_view(cr, slot) else { return };
        let touched = touched(cr, slot);
        if self.timers.hint == -1 {
            self.timers.hint = self.rand() & 3;
        }
        if !talking {
            self.timers.hint_call -= 1;
            if self.timers.hint_call < 0 {
                self.timers.hint_call = self.rand() * 2 + 0xF0;
                self.slots.push((70, 128, pos));
            }
        }
        if !touched || talking {
            return;
        }
        let text = if self.tokens & 0x1F != 0x1F {
            self.slots.push((71, 128, pos));
            let t = &mut self.timers.hint;
            while self.tokens >> *t & 1 != 0 {
                *t += 1;
                if *t > 4 {
                    *t = 0;
                }
            }
            *t += 1;
            let k = *t as usize - 1;
            if *t > 4 {
                *t = 0;
            }
            self.story.hints.get(k).copied().unwrap_or(ALL_TOKENS)
        } else {
            ALL_TOKENS
        };
        self.talk(slot, list, text, None, None);
    }

    /// 0x4A26F0(slot, sound): a character calling out every 2r + 0xF0 ticks in view, while
    /// its challenge is open (`open`).
    pub fn call(&mut self, cr: &Creatures, slot: usize, sound: u16, open: bool) {
        if !open || self.cutscene.is_some() {
            return;
        }
        let Some(pos) = Self::in_view(cr, slot) else { return };
        self.timers.finder -= 1;
        if self.timers.finder < 0 {
            self.timers.finder = self.rand() * 2 + 0xF0;
            self.sounds.push((sound, pos));
        }
    }

    /// A "find my five …" character (inline in the level scripts): touched while the count
    /// (0x52B7D8) is open, with all five found he thanks Buzz (voice 0xB4, `thanks`, the
    /// token) and the count closes (−1); otherwise he asks (voice 0xB3, `ask`).
    pub fn finder(&mut self, cr: &mut Creatures, slot: usize, list: i16, ask: u32, thanks: u32, facings: Option<[i32; 2]>, token: usize) {
        if !touched(cr, slot) || cr.sheep < 0 {
            return;
        }
        if cr.sheep == 5 {
            self.voices.push(0xB4);
            self.talk(slot, list, thanks, Some(token), facings);
            cr.sheep = -1;
        } else {
            self.voices.push(0xB3);
            self.talk(slot, list, ask, None, facings);
        }
    }

    /// 0x4A2480(slot, list, ask, thanks, after, facings): Mr Potato Head. He calls (slot 75)
    /// in view; touched: the item still lost → he asks (slot 76); found (`item` < 0) → he
    /// takes it (slot 77), thanks Buzz and gives the level's ability (`gift`, 0x503A23);
    /// after that, his last line.
    pub fn potato(&mut self, cr: &mut Creatures, slot: usize, list: i16, texts: [u32; 3], facings: [i32; 2], gift: u8) {
        if self.cutscene.is_none() && let Some(pos) = Self::in_view(cr, slot) {
            self.timers.potato -= 1;
            if self.timers.potato < 0 {
                self.timers.potato = self.rand() * 2 + 0xF0;
                self.slots.push((75, 128, pos));
            }
        }
        let pos = cr.slots.get(slot).map_or([0; 3], |c| c.pos);
        if !touched(cr, slot) {
            return;
        }
        let f = Some(facings);
        match self.item.signum() {
            0 => self.talk(slot, list, texts[2], None, f),
            1 => {
                self.talk(slot, list, texts[0], None, f);
                self.slots.push((76, 128, pos));
            }
            _ => {
                self.slots.push((77, 128, pos));
                self.talk(slot, list, texts[1], None, f);
                self.item = 0;
                self.abilities |= gift;
            }
        }
    }
}

/// 0x49EC00's state for one tilting mover: swing speed and tilt about Z (mover-table units,
/// 0x4000 a turn).
#[derive(Debug, Clone, Copy, Default)]
pub struct Tilt {
    pub v: i32,
    pub z: i32,
}

impl Tilt {
    /// 0x49EC00(mover, special, v, mode, lo, hi, div) for a mover turned `yaw` (12-bit, set
    /// at the start by 0x488AA0). Buzz standing on it pushes the side he's on down, harder
    /// the further out he is and the harder he lands; off it, mode 1 drifts towards the
    /// positive tilt and mode 2 back towards level; mode 0 just slows. The swing slows by 2 a
    /// tick, the tilt changes by speed / `div`, and it bounces back from `lo` / `hi` (× 4)
    /// with three quarters of its speed. The special follows; Buzz on it turns with it.
    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, world: &World, specials: &mut Specials, player: &mut Player, mover: i32, special: i32, yaw: i32, mode: u8, lo: i32, hi: i32, div: i32) {
        let mut v = self.v;
        let riding = player.on_floor && player.floor_mover.is_some_and(|(m, _)| m == mover);
        let before = [0, yaw, self.z >> 2];
        if riding {
            let Some(p) = specials.pos_obj(special) else { return };
            let b = player.pos_obj();
            // Along the plank (the mover's yaw), and the distance from its pivot.
            let (s, c) = (sin_table(yaw - 0x800) >> 2, sin_table(yaw + 0x400) >> 2);
            let along = (((b[2] - p[2]) >> 5) * s + ((b[0] - p[0]) >> 5) * c) / 0x1000;
            let dy = (b[1] - p[1]) >> 5;
            let d = (((dy * dy + along * along) as f64).sqrt() as i32) >> 5;
            let push = ((player.fall_speed * d >> 9) + d) / 2;
            v += if along > 0 { push } else { -push };
        } else if mode == 1 {
            v += 4;
        } else if mode == 2 && self.z.abs() > 8 {
            v += if self.z < 0 { 3 } else { -3 };
        }
        if v > 0 {
            v = (v - 2).max(0);
        } else if v < 0 {
            v = (v + 2).min(0);
        }
        self.z += v / div;
        if self.z < lo * 4 {
            v = (v * 3 / 4).abs();
            self.z = lo * 4;
        } else if self.z > hi * 4 {
            v = -(v * 3 / 4).abs();
            self.z = hi * 4;
        }
        self.v = v;
        let a = [0, yaw, self.z >> 2];
        world.turn_mover(mover, a);
        specials.set_angles(special, a);
        if riding && a != before {
            player.pos = world.ride_turn(mover, before, a, player.pos);
        }
    }
}

/// A level's own script.
pub trait Script: std::ops::DerefMut<Target = Common> {
    /// The part of the frame script that runs just before Buzz moves (with the boxes).
    fn step(&mut self, world: &World, specials: &mut Specials, player: &mut Player, boxes: &Boxes);
    /// The part that runs after the creatures' update. `coins` is Buzz's count.
    fn step_creatures(&mut self, world: &World, specials: &mut Specials, player: &Player, cr: &mut Creatures, coins: i32);
}

/// Sine table value (round(16384 · sin)) of a 12-bit angle, as 0x4FE788 holds it.
pub fn sin_table(a: i32) -> i32 {
    ((a & 0xFFF) as f64 / 4096.0 * std::f64::consts::TAU).sin().mul_add(16384.0, 0.0).round() as i32
}

/// Within `r` (in 256-object-unit steps) of `q` (0x49F400).
pub fn near(p: [i32; 3], q: [i32; 3], r: i32) -> bool {
    let d = [0, 1, 2].map(|k| (p[k] - q[k]) >> 8);
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < r * r
}

/// The script for level `level` (1-based), set up as its start-up function does.
pub fn for_level(level: usize, world: &World, specials: &mut Specials, exe: Option<&Exe>, lists: &[PointList], random: Option<Vec<u8>>) -> Option<Box<dyn Script>> {
    let mut s: Box<dyn Script> = match level {
        1 => {
            let mut l = crate::level1::Level1::new(world, specials);
            if let Some(e) = exe {
                l.set_story(e, lists);
            }
            Box::new(l)
        }
        3 => Box::new(crate::level3::Level3::new(specials)),
        6 => Box::new(crate::level6::Level6::new()),
        9 => Box::new(crate::level9::Level9::new()),
        10 => {
            let mut l = crate::level10::Level10::new(exe);
            if let Some(e) = exe {
                l.set_story(e, lists);
            }
            Box::new(l)
        }
        8 => {
            let mut l = crate::level8::Level8::new();
            if let Some(e) = exe {
                l.set_story(e, lists);
            }
            Box::new(l)
        }
        7 => {
            let mut l = crate::level7::Level7::new(world, specials, exe);
            if let Some(e) = exe {
                l.set_story(e, lists);
            }
            Box::new(l)
        }
        5 => {
            let mut l = crate::level5::Level5::new(specials, world);
            if let Some(e) = exe {
                l.set_story(e, lists);
            }
            Box::new(l)
        }
        4 => {
            let mut l = crate::level4::Level4::new(world, specials, exe);
            if let Some(e) = exe {
                l.set_story(e, lists);
            }
            Box::new(l)
        }
        2 => {
            let mut l = crate::level2::Level2::new(world, specials);
            if let Some(e) = exe {
                l.set_story(e, lists);
            }
            Box::new(l)
        }
        _ => return None,
    };
    if let Some(r) = random {
        s.set_random(r);
    }
    Some(s)
}
