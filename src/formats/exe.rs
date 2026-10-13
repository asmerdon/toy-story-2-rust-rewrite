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
