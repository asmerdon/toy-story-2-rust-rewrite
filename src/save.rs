//! Saved games, in the original's format: `Toy2NN.sav` (slots 1–8; 0x49B830 writes, 0x49B9E0
//! reads) is a u32 length, that many bytes of title ("TOK %d LEV %d"), then the game state
//! block 0x52EF90 (0x188 bytes). Slot 0 (`Toy200.sav`, "default.cfg") holds only the first
//! 0x138 bytes: the default controls and settings.
//!
//! The state block (0x4A2C80 fills it from the globals, 0x4A2CC0 puts them back):
//! - +0x138 lives (0x52F39A), +0x139 the campaign level (0x52AD8A, 0–14), +0x13A abilities
//!   (0x52F2D8), +0x13D / +0x13E music and effects volume, +0x144 health (i16, 0x52F396);
//! - +0x147 + internal id: the level's token bits (0x52F0D7[id]); +0x157 + n movie seen
//!   (0x52F0E7, 0x49EB20); +0x158 + level boss beaten (0x52F0E8).
//!
//! A new game (0x4A2C20): level 0, no abilities, health 0xE, lives 5, the rest from +0x148 on
//! cleared.
//!
//! Deviation: saves go to `TS2_SAVE_DIR`, else `%APPDATA%\ts2-rust` (the original writes beside
//! the game, which is usually read-only under Program Files). Copying the original's
//! `Toy201`–`08.sav` there loads them.

use crate::frontend::{Progress, Tables};
use std::path::{Path, PathBuf};

pub const STATE_LEN: usize = 0x188;
const LIVES: usize = 0x138;
const LEVEL: usize = 0x139;
const ABILITIES: usize = 0x13A;
const HEALTH: usize = 0x144;
const TOKENS: usize = 0x147;
/// Slots shown by the load and save screens.
pub const SLOTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Save {
    pub title: String,
    pub state: Vec<u8>,
}

impl Save {
    /// A new game, its first 0x138 bytes (controls, settings) from `defaults` if given.
    pub fn new_game(defaults: &[u8]) -> Save {
        let mut state = vec![0; STATE_LEN];
        let n = defaults.len().min(LIVES);
        state[..n].copy_from_slice(&defaults[..n]);
        state[LIVES] = 5;
        state[HEALTH..HEALTH + 2].copy_from_slice(&0xEi16.to_le_bytes());
        Save { title: String::new(), state }
    }

    pub fn parse(b: &[u8]) -> Option<Save> {
        let len = u32::from_le_bytes(b.get(0..4)?.try_into().ok()?) as usize;
        let title = String::from_utf8_lossy(b.get(4..4 + len)?).into_owned();
        let mut state = b.get(4 + len..)?.to_vec();
        state.truncate(STATE_LEN);
        Some(Save { title, state })
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut out = (self.title.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(self.title.as_bytes());
        out.extend_from_slice(&self.state);
        out.resize(4 + self.title.len() + STATE_LEN, 0);
        out
    }

    fn byte(&self, at: usize) -> u8 {
        self.state.get(at).copied().unwrap_or(0)
    }

    /// The campaign level last played (0–14).
    pub fn level(&self) -> usize {
        (self.byte(LEVEL) as usize).min(14)
    }

    /// The progress this save holds.
    pub fn progress(&self) -> Progress {
        let mut p = Progress { lives: self.byte(LIVES) as i32, abilities: self.byte(ABILITIES), ..Progress::default() };
        for (id, t) in p.tokens.iter_mut().enumerate() {
            *t = self.byte(TOKENS + id);
        }
        p
    }

    /// Writes `p`, the level (0–14) and the title 0x49B9E0 gives it ("TOK %d LEV %d": tokens,
    /// levels done) into this save, keeping the rest (controls, movies seen).
    pub fn store(&mut self, p: &Progress, level: usize, t: &Tables) {
        self.state.resize(STATE_LEN, 0);
        self.state[LIVES] = p.lives.clamp(0, 255) as u8;
        self.state[LEVEL] = level.min(14) as u8;
        self.state[ABILITIES] = p.abilities;
        for (id, &bits) in p.tokens.iter().enumerate() {
            self.state[TOKENS + id] = bits;
        }
        let s = p.summary(t);
        self.title = format!("TOK {} LEV {}", s.tokens, s.levels);
    }
}

/// Where saves are written.
pub fn dir() -> PathBuf {
    if let Some(d) = std::env::var_os("TS2_SAVE_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("ts2-rust")
}

pub fn path(dir: &Path, slot: usize) -> PathBuf {
    dir.join(format!("Toy2{slot:02}.sav"))
}

pub fn read(dir: &Path, slot: usize) -> Option<Save> {
    Save::parse(&std::fs::read(path(dir, slot)).ok()?)
}

pub fn write(dir: &Path, slot: usize, save: &Save) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(path(dir, slot), save.bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> Tables {
        Tables { order: vec![1, 2, 6, 4, 5, 3, 7, 8, 9, 10, 11, 12, 13, 14, 15], names: Vec::new(), needed: vec![0; 16], models: Vec::new() }
    }

    #[test]
    fn round_trips_in_the_original_size() {
        let mut s = Save::new_game(&[]);
        let mut p = Progress::default();
        p.tokens[1] = 0b10011;
        p.tokens[2] = 0b1;
        p.lives = 3;
        p.abilities = 4;
        s.store(&p, 2, &tables());
        assert_eq!(s.title, "TOK 4 LEV 2");
        let b = s.bytes();
        assert_eq!(b.len(), 407, "as the original's saves");
        let back = Save::parse(&b).unwrap();
        assert_eq!(back.progress(), p);
        assert_eq!(back.level(), 2);
    }

    #[test]
    fn new_game_is_five_lives() {
        let s = Save::new_game(&[7; 0x138]);
        assert_eq!(s.progress(), Progress::default());
        assert_eq!(s.state[0], 7);
        assert_eq!(i16::from_le_bytes([s.state[HEALTH], s.state[HEALTH + 1]]), 0xE);
    }
}
