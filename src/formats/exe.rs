//! Reads data tables from the user's own `toy2.exe` at runtime (never shipped in the repo).
//! Addresses are virtual addresses in the 1999-10-29 PC build; see `docs/re/functions.md`.

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "toy2.exe: {}", self.0)
    }
}

impl std::error::Error for Error {}

pub struct Exe {
    bytes: Vec<u8>,
    image_base: u32,
    /// (virtual address, virtual size, file offset, raw size)
    sections: Vec<(u32, u32, u32, u32)>,
}

fn rd32(b: &[u8], o: usize) -> Result<u32, Error> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| Error(format!("read past end at {o:#x}")))
}

impl Exe {
    pub fn parse(bytes: Vec<u8>) -> Result<Exe, Error> {
        if bytes.get(0..2) != Some(b"MZ") {
            return Err(Error("not an MZ executable".into()));
        }
        let pe = rd32(&bytes, 0x3C)? as usize;
        if bytes.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err(Error("no PE header".into()));
        }
        let count = u16::from_le_bytes([bytes[pe + 6], bytes[pe + 7]]) as usize;
        let opt = u16::from_le_bytes([bytes[pe + 20], bytes[pe + 21]]) as usize;
        let image_base = rd32(&bytes, pe + 24 + 28)?;
        let table = pe + 24 + opt;
        let sections = (0..count)
            .map(|i| {
                let s = table + 40 * i;
                Ok((rd32(&bytes, s + 12)?, rd32(&bytes, s + 8)?, rd32(&bytes, s + 20)?, rd32(&bytes, s + 16)?))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Exe { bytes, image_base, sections })
    }

    /// Bytes at virtual address `va`.
    pub fn at(&self, va: u32, len: usize) -> Result<&[u8], Error> {
        let rva = va.checked_sub(self.image_base).ok_or_else(|| Error(format!("{va:#x} below image base")))?;
        for &(vaddr, _vsize, raw, rsize) in &self.sections {
            if rva >= vaddr && rva + len as u32 <= vaddr + rsize {
                let o = (raw + rva - vaddr) as usize;
                return self.bytes.get(o..o + len).ok_or_else(|| Error(format!("{va:#x} outside file")));
            }
        }
        Err(Error(format!("{va:#x} not in any section")))
    }

    /// Player start for game level `level` (1..=15): position in level units and facing
    /// angle in 12-bit units (4096 = 360°). Table at 0x4F59A4, 16 bytes per level, read by
    /// 0x433D50 (`ResetPlayer`), which then snaps the height to the floor.
    pub fn player_start(&self, level: usize) -> Result<PlayerStart, Error> {
        if !(1..=15).contains(&level) {
            return Err(Error(format!("no player start for level {level}")));
        }
        let b = self.at(0x4F59A4 + 16 * level as u32, 16)?;
        let i = |k: usize| i32::from_le_bytes(b[4 * k..4 * k + 4].try_into().unwrap());
        Ok(PlayerStart {
            // Object positions are level units × 32.
            pos: [i(0), i(1), i(2)].map(|v| v as f32 / 32.0),
            angle: i16::from_le_bytes([b[12], b[13]]),
        })
    }
}

/// Game logic ticks per second: the frame timer converts elapsed ms × 60 / 1000 (0x490860).
pub const TICKS_PER_SECOND: f32 = 60.0;

/// Buzz's jump launch speed in object units per tick, upward (`StartJump` 0x434090 callers).
pub const BUZZ_JUMP_SPEED_OBJ: i32 = 0xC00;

/// One row of Buzz's move table (0x4DF3F0, 20 bytes per state, read by 0x4011D0).
#[derive(Debug, Clone, PartialEq)]
pub struct BuzzMove {
    /// Frame list: frame numbers (< 0x80) and event markers (≥ 0x80), up to the 0xFF.
    pub frames: Vec<u8>,
    /// Byte after 0xFF: 1 = hold the last frame, n > 1 = loop back n entries, 0 = stop
    /// (also used for 0xFE).
    pub end: u8,
    /// The whole list as stored, through its end code: 0xFF n, or 0xFE (back to state 0).
    pub list: Vec<u8>,
    /// Overlay animation (object slot +0x12): applied on top, only to the nodes it animates.
    pub anim: i32,
    /// Base animation (object slot +0x16): applied to every node first.
    pub base_anim: i32,
    /// Frame-list entries per tick, 16.16. Negative means "scale by horizontal speed":
    /// entries per tick = -raw × speed (object units per tick) / 65536.
    pub rate_raw: i32,
}

pub const BUZZ_STATES: usize = 33;

impl Exe {
    pub fn buzz_moves(&self) -> Result<Vec<BuzzMove>, Error> {
        (0..BUZZ_STATES as u32)
            .map(|s| {
                let row = self.at(0x4DF3F0 + 20 * s, 20)?;
                let w = |k: usize| u32::from_le_bytes(row[4 * k..4 * k + 4].try_into().unwrap());
                let mut frames = Vec::new();
                let mut va = w(0);
                loop {
                    let b = self.at(va, 1)?[0];
                    if b == 0xFF || b == 0xFE || frames.len() > 255 {
                        break;
                    }
                    frames.push(b);
                    va += 1;
                }
                // 0xFE ends the list and returns Buzz to state 0 (0x401000); treat as "stop".
                let fe = self.at(va, 1)?[0] == 0xFE;
                let end = if fe { 0 } else { self.at(va + 1, 1)?[0] };
                let mut list = frames.clone();
                list.extend_from_slice(if fe { &[0xFE][..] } else { self.at(va, 2)? });
                Ok(BuzzMove { frames, end, list, anim: w(1) as i32, base_anim: w(2) as i32, rate_raw: w(3) as i32 })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerStart {
    pub pos: [f32; 3],
    pub angle: i16,
}

impl PlayerStart {
    pub fn yaw_radians(&self) -> f32 {
        self.angle as f32 / 4096.0 * std::f32::consts::TAU
    }
}

/// Game level number for a level file: `levelNN\level.*` is level NN and `levelNN\level1.*` is
/// NN + 10 (the exe's InitLevelPlay subtracts 10 and switches to the `level1` files).
pub fn level_number(path: &std::path::Path) -> Option<usize> {
    let dir = path.parent()?.file_name()?.to_str()?.to_ascii_lowercase();
    let nn: usize = dir.strip_prefix("level")?.parse().ok()?;
    match path.file_stem()?.to_str()?.to_ascii_lowercase().as_str() {
        "level" => Some(nn),
        "level1" => Some(nn + 10),
        _ => None,
    }
}
