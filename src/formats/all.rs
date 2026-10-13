//! `.all` character model files (`data/chars*/*.all`).
//!
//! Layout as reverse-engineered so far (see `docs/formats/all.md`):
//!
//! ```text
//! u32               header (meaning unknown; not the file or geometry size)
//! repeat:
//!   u32 == 0          -> end of current part
//!   u32 == 0x12345678 -> end of geometry; trailer section follows (unparsed)
//!   byte[3] is a PS1 GPU polygon code -> polygon record
//!   otherwise          -> normal block: u16 count, u16 unknown, count * (i16 x,y,z, u16 w)
//! ```
//!
//! Polygon record (n = 3 for triangles, 4 for quads):
//!
//! ```text
//! u8 r0, g0, b0, code
//! n * (i16 x, i16 y, i16 z, u8 u, u8 v)
//! (n - 1) * (u8 r, g, b, tag)
//! ```
//!
//! Coordinates are PS1 model space: +Y is down. Normals are 4.12 fixed point (4096 = 1.0).

use std::fmt;

pub const GEOMETRY_END: u32 = 0x1234_5678;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vertex {
    pub pos: [i16; 3],
    pub uv: [u8; 2],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Poly {
    /// PS1 GPU primitive code (0x34 = gouraud textured tri, 0x3C = gouraud textured quad, ...).
    pub code: u8,
    pub verts: Vec<Vertex>,
    pub colors: Vec<[u8; 3]>,
    /// Fourth byte of colour words 1..n. Probably CLUT / texture page / flags; not yet decoded.
    pub tags: Vec<u8>,
}

impl Poly {
    pub fn is_quad(&self) -> bool {
        self.code & 0x08 != 0
    }
    pub fn is_textured(&self) -> bool {
        self.code & 0x04 != 0
    }
    pub fn is_semi_transparent(&self) -> bool {
        self.code & 0x02 != 0
    }
}

#[derive(Debug, Clone, Default)]
pub struct Part {
    pub polys: Vec<Poly>,
    /// Normal list that preceded this part's polygons, if any.
    pub normals: Vec<[i16; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Reached the 0x12345678 marker; trailer data starts after it.
    GeometryEnd,
    /// Hit a block type we don't understand yet.
    Unknown(u32),
    Eof,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub header: u32,
    pub parts: Vec<Part>,
    pub stop: Stop,
    /// Byte offset where parsing stopped.
    pub stop_offset: usize,
    pub file_len: usize,
}

impl Model {
    pub fn poly_count(&self) -> usize {
        self.parts.iter().map(|p| p.polys.len()).sum()
    }
}

#[derive(Debug)]
pub enum Error {
    TooShort,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooShort => write!(f, "file shorter than header"),
        }
    }
}

impl std::error::Error for Error {}

fn vertex_count(code: u8) -> Option<usize> {
    // Polygon primitives are 0x20..=0x3F on the PS1 GPU; bit 3 selects quad.
    if code & 0xE0 != 0x20 {
        return None;
    }
    Some(if code & 0x08 != 0 { 4 } else { 3 })
}

struct Reader<'a> {
    b: &'a [u8],
    o: usize,
}

impl Reader<'_> {
    fn remaining(&self) -> usize {
        self.b.len() - self.o
    }
    fn u8(&mut self) -> u8 {
        let v = self.b[self.o];
        self.o += 1;
        v
    }
    fn u16(&mut self) -> u16 {
        let v = u16::from_le_bytes([self.b[self.o], self.b[self.o + 1]]);
        self.o += 2;
        v
    }
    fn i16(&mut self) -> i16 {
        self.u16() as i16
    }
    fn peek_u32(&self) -> u32 {
        u32::from_le_bytes(self.b[self.o..self.o + 4].try_into().unwrap())
    }
}

/// A normal block is accepted only if every entry has unit length in 4.12 fixed point,
/// which keeps us from mistaking trailer data for geometry.
fn normal_block_len(b: &[u8], o: usize) -> Option<usize> {
    let count = u16::from_le_bytes([b[o], b[o + 1]]) as usize;
    let len = 4 + 8 * count;
    if count == 0 || o + len > b.len() {
        return None;
    }
    for i in 0..count {
        let e = o + 4 + 8 * i;
        let c = |k: usize| i16::from_le_bytes([b[e + 2 * k], b[e + 2 * k + 1]]) as f64;
        let mag = (c(0) * c(0) + c(1) * c(1) + c(2) * c(2)).sqrt();
        if (mag - 4096.0).abs() > 40.0 {
            return None;
        }
    }
    Some(len)
}

pub fn parse(b: &[u8]) -> Result<Model, Error> {
    if b.len() < 4 {
        return Err(Error::TooShort);
    }
    let header = u32::from_le_bytes(b[0..4].try_into().unwrap());
    let mut r = Reader { b, o: 4 };
    let mut parts = Vec::new();
    let mut cur = Part::default();

    let stop = loop {
        if r.remaining() < 4 {
            break Stop::Eof;
        }
        let w = r.peek_u32();
        if w == 0 {
            r.o += 4;
            parts.push(std::mem::take(&mut cur));
            continue;
        }
        if w == GEOMETRY_END {
            break Stop::GeometryEnd;
        }

        let code = b[r.o + 3];
        if let Some(n) = vertex_count(code) {
            let len = 4 + 8 * n + 4 * (n - 1);
            if r.remaining() < len {
                break Stop::Unknown(w);
            }
            let c0 = [r.u8(), r.u8(), r.u8()];
            r.o += 1;
            let verts = (0..n)
                .map(|_| Vertex { pos: [r.i16(), r.i16(), r.i16()], uv: [r.u8(), r.u8()] })
                .collect();
            let mut colors = vec![c0];
            let mut tags = Vec::with_capacity(n - 1);
            for _ in 1..n {
                colors.push([r.u8(), r.u8(), r.u8()]);
                tags.push(r.u8());
            }
            cur.polys.push(Poly { code, verts, colors, tags });
            continue;
        }

        if let Some(len) = normal_block_len(b, r.o) {
            let count = r.u16() as usize;
            let _unknown = r.u16();
            for _ in 0..count {
                cur.normals.push([r.i16(), r.i16(), r.i16()]);
                let _w = r.u16();
            }
            debug_assert_eq!(len, 4 + 8 * count);
            continue;
        }

        break Stop::Unknown(w);
    };

    if !cur.polys.is_empty() || !cur.normals.is_empty() {
        parts.push(cur);
    }
    Ok(Model { header, parts, stop, stop_offset: r.o, file_len: b.len() })
}
