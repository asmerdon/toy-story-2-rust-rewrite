//! `levelNN/level*.raw` and `gfx/*.raw`: chains of LZ-compressed "packets".
//!
//! The PC exe still loads these (routine "Loading packet data %s"): besides PS1 texture pages
//! they carry gameplay data such as the creature spawn list (type 35) and the map (type 36).
//! See `docs/formats/packets.md`.
//!
//! ```text
//! repeat until u32 0xFFFFFFFF:
//!   u32be unpacked_size, u32be packed_len, u8[6] unknown, u8[packed_len] LZ stream
//! unpacked data starts with u32le packet type
//! ```

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "packets: {}", self.0)
    }
}

impl std::error::Error for Error {}

pub mod kind {
    pub const CREATURE_LIST: u32 = 35;
    pub const MAP: u32 = 36;
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub offset: usize,
    pub header: [u8; 6],
    /// Decompressed bytes, including the leading u32 type.
    pub data: Vec<u8>,
}

impl Packet {
    pub fn kind(&self) -> u32 {
        self.data.get(0..4).map_or(u32::MAX, |b| u32::from_le_bytes(b.try_into().unwrap()))
    }
}

/// MSB-first bit reader with a sentinel bit, refilled one byte at a time.
struct Bits<'a> {
    b: &'a [u8],
    p: usize,
    reg: u32,
}

impl Bits<'_> {
    fn byte(&mut self) -> Result<u8, Error> {
        let v = *self.b.get(self.p).ok_or_else(|| Error(format!("stream overrun at {:#x}", self.p)))?;
        self.p += 1;
        Ok(v)
    }
    fn bit(&mut self) -> Result<u32, Error> {
        let mut v = (self.reg >> 7) & 1;
        self.reg = (self.reg << 1) & 0xFF;
        if self.reg == 0 {
            self.reg = (self.byte()? as u32) << 1 | 1;
            v = (self.reg >> 8) & 1;
            self.reg &= 0x1FF;
        }
        Ok(v)
    }
}

/// Decompresses one LZ stream starting at `b[start]`. Returns the data and the end offset.
///
/// Codes (after the first control bit of the stream, which is skipped):
/// `0` literal byte · `10` + byte: 2-byte match · `110`: 3-byte match ·
/// `111` + byte n: n>0 → (n+8)-byte match, n=0 → next bit 0 = end of stream ·
/// `100x0` / `100x1y`: 4..8-byte matches, or a run of literal 4-byte blocks when the length code is 9.
/// Long matches carry a prefix-coded high offset (0..15) before the low offset byte;
/// the copy distance is offset + 1 (so offset 0 repeats the previous byte).
pub fn decompress(b: &[u8], start: usize, size_hint: usize) -> Result<(Vec<u8>, usize), Error> {
    let first = *b.get(start).ok_or_else(|| Error("empty stream".into()))?;
    let mut bits = Bits { b, p: start + 1, reg: (first as u32) << 1 | 1 };
    bits.bit()?;
    let mut out: Vec<u8> = Vec::with_capacity(size_hint);
    let copy = |out: &mut Vec<u8>, dist: usize, len: usize| -> Result<(), Error> {
        if dist > out.len() {
            return Err(Error(format!("match distance {dist} beyond output {}", out.len())));
        }
        for _ in 0..len {
            out.push(out[out.len() - dist]);
        }
        Ok(())
    };
    loop {
        if bits.bit()? == 0 {
            out.push(bits.byte()?);
            continue;
        }
        let len;
        if bits.bit()? == 0 {
            let b2 = bits.bit()?;
            let mut l = b2 + 4;
            if bits.bit()? == 1 {
                l = bits.bit()? + (b2 + 3) * 2;
                if l == 9 {
                    let mut n = 0;
                    for _ in 0..4 {
                        n = n * 2 + bits.bit()?;
                    }
                    for _ in 0..(n + 3) * 4 {
                        out.push(bits.byte()?);
                    }
                    continue;
                }
            }
            len = l as usize;
        } else {
            if bits.bit()? == 0 {
                let off = bits.byte()? as usize;
                copy(&mut out, off + 1, 2)?;
                continue;
            }
            if bits.bit()? == 1 {
                let n = bits.byte()?;
                if n == 0 {
                    if bits.bit()? == 0 {
                        return Ok((out, bits.p));
                    }
                    continue;
                }
                len = n as usize + 8;
            } else {
                len = 3;
            }
        }
        let mut hi = 0;
        if bits.bit()? == 1 {
            let u = bits.bit()?;
            if bits.bit()? == 0 {
                hi = if u == 0 { 2 + bits.bit()? } else { 1 };
            } else {
                let u = (bits.bit()? + u * 2) | 4;
                hi = if bits.bit()? == 1 { u } else { bits.bit()? + u * 2 };
            }
        }
        let off = bits.byte()? as usize | (hi as usize) << 8;
        copy(&mut out, off + 1, len)?;
    }
}

pub fn parse(b: &[u8]) -> Result<Vec<Packet>, Error> {
    let mut p = 0;
    let mut out = Vec::new();
    while p + 14 <= b.len() && b[p..p + 4] != [0xFF; 4] {
        let unpacked = u32::from_be_bytes(b[p..p + 4].try_into().unwrap()) as usize;
        let packed = u32::from_be_bytes(b[p + 4..p + 8].try_into().unwrap()) as usize;
        let header: [u8; 6] = b[p + 8..p + 14].try_into().unwrap();
        let (data, end) = decompress(b, p + 14, unpacked).map_err(|e| Error(format!("packet at {p:#x}: {}", e.0)))?;
        if data.len() != unpacked || end != p + 14 + packed {
            return Err(Error(format!(
                "packet at {p:#x}: unpacked {} (header {unpacked}), ended at {end:#x} (expected {:#x})",
                data.len(),
                p + 14 + packed
            )));
        }
        out.push(Packet { offset: p, header, data });
        p += 14 + packed;
    }
    Ok(out)
}

/// One entry of the type-35 creature list (32 bytes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spawn {
    /// World position in level units (same space as `.ngn` instance positions; +Y down).
    pub pos: [i32; 3],
    /// Creature id as in `data/creatures.cfg` and the `.ngn` creature slots.
    pub creature: u8,
    /// Remaining 19 bytes; not decoded yet (facing, behaviour parameters, ...).
    pub params: [u8; 19],
}

pub fn spawns(packet: &Packet) -> Vec<Spawn> {
    if packet.kind() != kind::CREATURE_LIST {
        return Vec::new();
    }
    packet.data[4..]
        .chunks_exact(32)
        .take_while(|r| r.iter().any(|&x| x != 0))
        .map(|r| Spawn {
            pos: [0, 1, 2].map(|k| i32::from_le_bytes(r[4 * k..4 * k + 4].try_into().unwrap())),
            creature: r[12],
            params: r[13..32].try_into().unwrap(),
        })
        .collect()
}

/// The type-36 packet: the level's PS1 background picture, 8-bit with an RGB palette. The PC
/// version only takes two colours from it (0x452310): the palette colour of its first pixel
/// (the sky, 0x4F73B4) and of its last (the ground, 0x4F73C0); it clears the screen with them
/// above and below the horizon, `horizon` (0x4F73B0) lines below where the view's horizon
/// falls (in 240-line PS1 screen units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backdrop {
    pub width: i32,
    pub height: i32,
    pub horizon: i32,
    pub sky: [u8; 3],
    pub ground: [u8; 3],
}

/// Reads a type-36 packet: u32 type, i32 horizon, i16 width, i16 height, i16 palette bytes,
/// u16, then the palette (3 bytes a colour) and width × height indices.
pub fn backdrop(packet: &Packet) -> Option<Backdrop> {
    if packet.kind() != kind::MAP {
        return None;
    }
    let d = &packet.data;
    let i16_at = |o: usize| d.get(o..o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]) as i32);
    let horizon = i32::from_le_bytes(d.get(4..8)?.try_into().ok()?);
    let (width, height, pal) = (i16_at(8)?, i16_at(10)?, i16_at(12)?);
    let pixels = 0x10 + (pal / 2 * 2) as usize;
    let colour = |at: usize| -> Option<[u8; 3]> {
        let i = *d.get(at)? as usize;
        Some([*d.get(0x10 + 3 * i)?, *d.get(0x11 + 3 * i)?, *d.get(0x12 + 3 * i)?])
    };
    let sky = colour(pixels)?;
    let ground = colour(pixels + (width * height) as usize - 1)?;
    Some(Backdrop { width, height, horizon, sky, ground })
}

/// The type-36 picture itself as RGBA (width, height, pixels top row first).
pub fn backdrop_rgba(packet: &Packet) -> Option<(u32, u32, Vec<u8>)> {
    let b = backdrop(packet)?;
    let d = &packet.data;
    let pal = i16::from_le_bytes([d[12], d[13]]) as usize / 2 * 2;
    let px = 0x10 + pal;
    let n = (b.width * b.height) as usize;
    let mut rgba = Vec::with_capacity(n * 4);
    for i in 0..n {
        let c = *d.get(px + i)? as usize;
        rgba.extend_from_slice(d.get(0x10 + 3 * c..0x13 + 3 * c)?);
        rgba.push(255);
    }
    Some((b.width as u32, b.height as u32, rgba))
}
