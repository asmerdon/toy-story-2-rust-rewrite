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
        _ => return None,
    };
    if let Some(r) = random {
        s.set_random(r);
    }
    Some(s)
}
