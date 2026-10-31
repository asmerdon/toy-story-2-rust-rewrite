//! Disc music: track names from 0x413150's table and campaign order from 0x4986D0.
//! Decode PCM without downmixing the CD's stereo channels. No media is shipped here.
use crate::formats::exe::Exe;
use std::path::{Path, PathBuf};

pub fn level_track(exe: &Exe, internal_id: usize) -> Result<String, String> {
    let u32_at = |va| exe.at(va, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).map_err(|e| e.to_string());
    let mut index = None;
    for i in 0..15 {
        if u32_at(0x50268C + i * 4)? as usize == internal_id { index = Some(i); break; }
    }
    let index = index.ok_or_else(|| format!("no music for internal level {internal_id}"))?;
    track(exe, index as usize)
}

/// Front-end and event tracks in 0x4ECCD8 (as 0x47D7F0 / 0x47D880 play them).
pub const MINIBOSS: usize = 0x0F;
pub const RACE: usize = 0x10;
pub const GAME_OVER: usize = 0x11;
pub const COMPLETE: usize = 0x12;
/// The main menu and the level select.
pub const MENU: usize = 0x13;
pub const TITLE: usize = 0x14;
pub const LEVEL_DONE: usize = 0x15;

/// The track a level plays (0x49EAC0, each frame from the level scripts): the miniboss theme
/// while the boss bar shows (HUD element 7's timer, 0x52C832), the race theme while the laps
/// show (element 6, 0x52C830) with a race on (0x52F2F8 ≥ 2), else the level's own.
pub fn theme(level_track: usize, boss_shown: bool, laps_shown: bool, race: i32) -> usize {
    if boss_shown {
        MINIBOSS
    } else if laps_shown && race >= 2 {
        RACE
    } else {
        level_track
    }
}

/// Whether a level's frame script calls 0x49EAC0. The boss levels' scripts (internal ids 3,
/// 6, 9, 12 and 15: 0x41AA10, 0x420060, 0x424490, 0x42B3A0, 0x42FC50) never do, so their own
/// track plays throughout the fight; only the normal levels switch to the miniboss theme.
pub fn level_switches_theme(internal_id: usize) -> bool {
    internal_id % 3 != 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn boss_levels_keep_their_own_track() {
        use super::*;
        assert!(level_switches_theme(1) && level_switches_theme(14));
        assert!([3, 6, 9, 12, 15].iter().all(|&l| !level_switches_theme(l)));
    }

    #[test]
    fn theme_follows_the_boss_bar_and_the_race() {
        use super::*;
        assert_eq!(theme(3, false, false, 0), 3);
        assert_eq!(theme(3, true, true, 2), MINIBOSS);
        assert_eq!(theme(3, false, true, 1), 3, "the laps show before the start");
        assert_eq!(theme(3, false, true, 2), RACE);
    }
}

/// The name of track `index` in 0x4ECCD8: 0–14 the levels in campaign order, then the above.
pub fn track(exe: &Exe, index: usize) -> Result<String, String> {
    let u32_at = |va| exe.at(va, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).map_err(|e| e.to_string());
    if index > LEVEL_DONE {
        return Err(format!("no music track {index}"));
    }
    let ptr = u32_at(0x4ECCD8 + index as u32 * 4)?;
    let mut name = String::new();
    for i in 0..32 {
        let b = exe.at(ptr + i, 1).map_err(|e| e.to_string())?[0];
        if b == 0 && !name.is_empty() { return Ok(name); }
        if !b.is_ascii_alphanumeric() && b != b'_' { return Err("invalid music filename in executable".into()); }
        name.push(b as char);
    }
    Err("unterminated music filename".into())
}

/// Explicit override, local ignored config, then installed CD directories.
pub fn media_root(game: &Path) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("TS2_CD_DIR") { return Some(p.into()); }
    if let Ok(p) = std::fs::read_to_string("ts2-media.txt") {
        if !p.trim().is_empty() { return Some(PathBuf::from(p.trim())); }
    }
    [game.to_owned(), game.join("CD")].into_iter().find(|p| p.join("audio").is_dir())
}

pub struct Pcm {
    pub channels: u16,
    pub rate: u32,
    pub samples: Vec<i16>,
}

pub fn decode_pcm(b: &[u8]) -> Result<Pcm, String> {
    let bad = || "invalid or unsupported PCM WAV".to_string();
    if b.len() < 12 || &b[..4] != b"RIFF" || &b[8..12] != b"WAVE" { return Err(bad()); }
    let end = (u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize).checked_add(8).ok_or_else(bad)?;
    if end > b.len() || end < 12 { return Err(bad()); }
    let mut fmt = None;
    let mut data = None;
    let mut off = 12;
    while off < end {
        let header = b.get(off..off + 8).filter(|_| off + 8 <= end).ok_or_else(bad)?;
        let len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let next = off.checked_add(8).and_then(|n| n.checked_add(len)).ok_or_else(bad)?;
        if next > end { return Err(bad()); }
        let chunk = &b[off + 8..next];
        match &header[..4] {
            b"fmt " => fmt = Some(chunk),
            b"data" => data = Some(chunk),
            _ => (),
        }
        off = next.checked_add(len & 1).ok_or_else(bad)?;
        if off > end { return Err(bad()); }
    }
    let fmt = fmt.filter(|f| f.len() >= 16).ok_or_else(bad)?;
    let u16_at = |i| u16::from_le_bytes([fmt[i], fmt[i + 1]]);
    let channels = u16_at(2);
    let rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
    let bits = u16_at(14);
    if u16_at(0) != 1 || !(1..=2).contains(&channels) || rate == 0 || ![8,16].contains(&bits) { return Err(bad()); }
    let align = channels * (bits / 8);
    if u16_at(12) != align { return Err(bad()); }
    let data = data.filter(|d| !d.is_empty() && d.len() % align as usize == 0).ok_or_else(bad)?;
    let samples = if bits == 8 {
        data.iter().map(|&v| (v as i16 - 128) * 256).collect()
    } else {
        data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
    };
    Ok(Pcm { channels, rate, samples })
}
