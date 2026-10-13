//! Sound effects: sound ids → samples, loaded per level from the user's install, with the PC
//! version's volume and pan (PlaySound 0x49E660, positional 0x4A3810, DirectSound 0x47DE50).
//! Output is the caller's business; this module only decides what to play and how loud.
//!
//! - Sample slots are filled per level by 0x47EC20 from name lists in the exe: the global list
//!   (table 0x4FD140, entry 0, slots from 1), then the level's effects (0x4FD140[level]) and
//!   voices (0x4FD5E0[level]); each name is `data\sfx\<name>.wav`.
//! - A sound id indexes 16-byte entries at 0x502950: i16 sample slot (bit 0x4000: per-level
//!   remap list at 0x5028E8; bit 0x8000: another play routine, treated the same here), i16
//!   pitch (a PS1 leftover: the PC plays every sample at its own rate), i16 volume (0..0x7F),
//!   i16 second volume, i16 pitch-variation shift, …
//! - Volume: the louder side indexes a dB table at 0x4FD668 (hundredths of a dB, as DirectSound
//!   takes them); pan = (right − left) × 10000 / 128.

use crate::formats::exe::{Error, Exe};
use glam::Vec3;
use std::collections::HashMap;
use std::path::Path;

pub struct Sample {
    pub rate: u32,
    /// Mono 16-bit samples.
    pub data: Vec<i16>,
}

pub struct SoundBank {
    pub level: usize,
    slots: HashMap<u16, Sample>,
    exe_defs: Vec<[i16; 8]>,
    remap: Vec<(i16, i16)>,
    db_table: Vec<i16>,
}

const DEFS: u32 = 0x502950;
const DEF_COUNT: u32 = 0x100;
const REMAP: u32 = 0x5028E8;
const DB_TABLE: u32 = 0x4FD668;
const EFFECT_LISTS: u32 = 0x4FD140;
const VOICE_LISTS: u32 = 0x4FD5E0;

fn u32_at(exe: &Exe, va: u32) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(exe.at(va, 4)?.try_into().unwrap()))
}

fn cstr(exe: &Exe, mut va: u32) -> Result<String, Error> {
    let mut s = Vec::new();
    loop {
        let b = exe.at(va, 1)?[0];
        if b == 0 || s.len() > 64 {
            return Ok(String::from_utf8_lossy(&s).into_owned());
        }
        s.push(b);
        va += 1;
    }
}

/// (first slot, names) for entry `index` of a list table.
fn name_list(exe: &Exe, table: u32, index: u32) -> Result<(u16, Vec<String>), Error> {
    let list = u32_at(exe, table + 8 * index)?;
    let base = u32_at(exe, table + 8 * index + 4)? as u16;
    let mut names = Vec::new();
    if list != 0 {
        let mut p = list;
        loop {
            let s = u32_at(exe, p)?;
            if s == 0 || names.len() > 256 {
                break;
            }
            names.push(cstr(exe, s)?);
            p += 4;
        }
    }
    Ok((base, names))
}

/// The slots 0x47EC20 loads for `level` (1..=16): slot → sample name.
pub fn slot_names(exe: &Exe, level: usize) -> Result<Vec<(u16, String)>, Error> {
    let mut lists = vec![name_list(exe, EFFECT_LISTS, 0)?];
    if (1..=16).contains(&level) {
        lists.push(name_list(exe, EFFECT_LISTS, level as u32)?);
        lists.push(name_list(exe, VOICE_LISTS, level as u32)?);
    }
    Ok(lists
        .into_iter()
        .flat_map(|(base, names)| names.into_iter().enumerate().map(move |(i, n)| (base + i as u16, n)))
        .filter(|(_, n)| !n.is_empty())
        .collect())
}

/// Minimal RIFF WAVE reader: PCM 8- or 16-bit, mono or stereo (mixed down to mono).
pub fn parse_wav(b: &[u8]) -> Option<Sample> {
    if b.get(0..4)? != b"RIFF" || b.get(8..12)? != b"WAVE" {
        return None;
    }
    let (mut o, mut fmt, mut data) = (12usize, None, None);
    while o + 8 <= b.len() {
        let id = &b[o..o + 4];
        let len = u32::from_le_bytes(b[o + 4..o + 8].try_into().ok()?) as usize;
        let body = b.get(o + 8..(o + 8 + len).min(b.len()))?;
        match id {
            b"fmt " => fmt = Some(body),
            b"data" => data = Some(body),
            _ => {}
        }
        o += 8 + len + (len & 1);
    }
    let fmt = fmt?;
    let channels = u16::from_le_bytes(fmt.get(2..4)?.try_into().ok()?).max(1) as usize;
    let rate = u32::from_le_bytes(fmt.get(4..8)?.try_into().ok()?);
    let bits = u16::from_le_bytes(fmt.get(14..16)?.try_into().ok()?);
    let data = data?;
    let frames: Vec<i16> = match bits {
        16 => data.chunks_exact(2 * channels).map(|f| (f.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]) as i32).sum::<i32>() / channels as i32) as i16).collect(),
        8 => data.chunks_exact(channels).map(|f| ((f.iter().map(|&s| s as i32 - 128).sum::<i32>() / channels as i32) << 8) as i16).collect(),
        _ => return None,
    };
    Some(Sample { rate, data: frames })
}

impl SoundBank {
    /// Loads the level's samples from `sfx_dir` (`data\sfx`). Missing files are skipped.
    pub fn load(exe: &Exe, sfx_dir: &Path, level: usize) -> Result<SoundBank, Error> {
        let files: HashMap<String, std::path::PathBuf> = std::fs::read_dir(sfx_dir)
            .map(|d| d.flatten().map(|e| (e.file_name().to_string_lossy().to_ascii_lowercase(), e.path())).collect())
            .unwrap_or_default();
        let mut slots = HashMap::new();
        for (slot, name) in slot_names(exe, level)? {
            let Some(path) = files.get(&format!("{}.wav", name.to_ascii_lowercase())) else { continue };
            if let Some(s) = std::fs::read(path).ok().and_then(|b| parse_wav(&b)) {
                slots.insert(slot, s);
            }
        }
        let exe_defs = (0..DEF_COUNT)
            .map(|i| {
                let b = exe.at(DEFS + 16 * i, 16)?;
                Ok(std::array::from_fn(|k| i16::from_le_bytes([b[2 * k], b[2 * k + 1]])))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let remap_bytes = exe.at(REMAP, (DEFS - REMAP) as usize)?;
        let remap = remap_bytes.chunks_exact(4).map(|c| (i16::from_le_bytes([c[0], c[1]]), i16::from_le_bytes([c[2], c[3]]))).collect();
        let db_table = exe.at(DB_TABLE, 129 * 2)?.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        Ok(SoundBank { level, slots, exe_defs, remap, db_table })
    }

    pub fn sample_count(&self) -> usize {
        self.slots.len()
    }

    /// The sample and its (left, right) volumes for sound `id`, as 0x49E660 resolves them.
    pub fn sound(&self, id: u16) -> Option<(&Sample, i32, i32)> {
        let def = self.exe_defs.get(id as usize)?;
        let mut slot = def[0] as u16;
        if slot & 0x4000 != 0 {
            // Pairs of (level, slot) from word (slot & 0x3FFF) of the remap area.
            let start = (slot & 0x3FFF) as usize / 2;
            slot = self.remap.get(start..)?.iter().take(32).find(|(lv, _)| *lv as usize == self.level)?.1 as u16;
        }
        let slot = slot & 0x7FFF;
        if slot == 0 {
            return None;
        }
        Some((self.slots.get(&slot)?, def[2] as i32, def[3] as i32))
    }

    /// Left and right gains (0..1) for DirectSound volume/pan as 0x47DE50 sets them.
    pub fn gains(&self, left: i32, right: i32) -> (f32, f32) {
        let (left, right) = (left.clamp(0, 128), right.clamp(0, 128));
        let db = self.db_table[left.max(right) as usize] as f32;
        let pan = ((right - left) * 10000 / 128).clamp(-10000, 10000) as f32;
        let amp = |hundredths_db: f32| 10f32.powf(hundredths_db / 2000.0);
        let l = amp(db) * if pan > 0.0 { amp(-pan) } else { 1.0 };
        let r = amp(db) * if pan < 0.0 { amp(pan) } else { 1.0 };
        (l, r)
    }
}

/// Per-ear volumes (0..=128) for a sound of volume `vol` at `rel` (sound position − listener,
/// level units), heard with the listener's `right` and `forward` unit vectors (0x4A3810).
/// Ears sit 0x400 either side; each fades out linearly over 0xC00.
pub fn positional(vol: i32, rel: Vec3, right: Vec3, forward: Vec3) -> (i32, i32) {
    // Object units (× 32) through the camera's 4.12 matrix (GTE style, 4096 = 1), >> 17:
    // level units; the forward axis is halved again.
    let x = rel.dot(right) as i32;
    let z = (rel.dot(forward) / 2.0) as i32;
    let ear = |dx: i32| {
        let d = ((dx as f64).powi(2) + (z as f64).powi(2)).sqrt() as i32;
        (((0xC00 - d) / 24) * vol / 128).clamp(0, 0x80)
    };
    (ear(x + 0x400), ear(x - 0x400))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positional_is_centred_ahead_and_fades() {
        let (r, f) = (Vec3::X, Vec3::Z);
        let (l0, r0) = positional(0x50, Vec3::new(0.0, 0.0, 10.0), r, f);
        assert_eq!(l0, r0);
        let (l1, r1) = positional(0x50, Vec3::new(800.0, 0.0, 0.0), r, f);
        assert!(r1 > l1, "{l1} {r1}");
        assert_eq!(positional(0x50, Vec3::new(0.0, 0.0, 9000.0), r, f), (0, 0));
        // At the camera's usual 1200 units, Buzz is clearly audible on both sides.
        let (l2, r2) = positional(0x50, Vec3::new(0.0, 0.0, 1200.0), r, f);
        assert!(l2 > 40 && l2 == r2, "{l2} {r2}");
    }
}
