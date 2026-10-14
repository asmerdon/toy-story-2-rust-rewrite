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
//!   otherwise          -> normal block for the part that just ended:
//!                         u16 count, u16 flags, count * (i16 x,y,z, u16 w),
//!                         one u16 normal index per vertex of that part's polygons
//!                         (0xFFFF = none), padded to 4 bytes
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

pub const NO_NORMAL: u16 = 0xFFFF;

#[derive(Debug, Clone, Default)]
pub struct Part {
    pub polys: Vec<Poly>,
    /// Smooth-shading normals (4.12 fixed point). Empty for flat/prelit parts.
    pub normals: Vec<[i16; 3]>,
    /// One entry per polygon vertex, in polygon order, indexing `normals` (or [`NO_NORMAL`]).
    /// Empty when the part has no normal block.
    pub normal_indices: Vec<u16>,
    /// Upper u16 of the normal block header (0, 0x800, 0x1000, 0x4CC seen). Unknown.
    pub normal_flags: u16,
}

impl Part {
    pub fn vertex_count(&self) -> usize {
        self.polys.iter().map(|p| p.verts.len()).sum()
    }
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

/// Validates a normal block for a part with `vertex_count` vertices and returns its byte length.
/// Every normal must have unit length in 4.12 fixed point and every index must be in range,
/// which keeps us from mistaking trailer data for geometry.
fn normal_block_len(b: &[u8], o: usize, vertex_count: usize) -> Option<usize> {
    let rd16 = |p: usize| u16::from_le_bytes([b[p], b[p + 1]]);
    let count = rd16(o) as usize;
    let idx_start = o + 4 + 8 * count;
    let len = (idx_start - o + 2 * vertex_count).next_multiple_of(4);
    if vertex_count == 0 || o + len > b.len() {
        return None;
    }
    for i in 0..count {
        let e = o + 4 + 8 * i;
        let c = |k: usize| rd16(e + 2 * k) as i16 as f64;
        let mag = (c(0) * c(0) + c(1) * c(1) + c(2) * c(2)).sqrt();
        if (mag - 4096.0).abs() > 40.0 {
            return None;
        }
    }
    let indices_ok = (0..vertex_count).map(|i| rd16(idx_start + 2 * i)).all(|i| i == NO_NORMAL || (i as usize) < count);
    indices_ok.then_some(len)
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

        // Normal blocks follow the terminator of the part they belong to.
        let prev = parts.last_mut().filter(|p: &&mut Part| cur.polys.is_empty() && p.normals.is_empty() && p.normal_indices.is_empty());
        if let Some(prev) = prev
            && let Some(len) = normal_block_len(b, r.o, prev.vertex_count())
        {
            let end = r.o + len;
            let count = r.u16() as usize;
            prev.normal_flags = r.u16();
            for _ in 0..count {
                prev.normals.push([r.i16(), r.i16(), r.i16()]);
                let _w = r.u16();
            }
            for _ in 0..prev.vertex_count() {
                let i = r.u16();
                prev.normal_indices.push(i);
            }
            r.o = end;
            continue;
        }

        break Stop::Unknown(w);
    };

    if !cur.polys.is_empty() || !cur.normals.is_empty() {
        parts.push(cur);
    }
    Ok(Model { header, parts, stop, stop_offset: r.o, file_len: b.len() })
}

/// A creature's hit shape for one animation: an ellipsoid, as 0x407440 (touching Buzz) and
/// 0x4A5870 (laser rays) test it. Offset in object units from the creature's feet (rotated
/// by its facing), axis scales in 1/256ths, radius in level units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ellipsoid {
    pub offset: [i16; 3],
    pub scale: [i16; 3],
    pub radius: i16,
}

/// Hit shapes from the file's directory: a bounding sphere (offset in object units, radius in
/// level units) and one ellipsoid per animation, indexed by animation number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitShapes {
    pub sphere: [i16; 4],
    pub ellipsoids: Vec<Ellipsoid>,
}

impl HitShapes {
    /// The ellipsoid for `anim` (the last one for animations past the end of the table).
    pub fn ellipsoid(&self, anim: usize) -> Ellipsoid {
        self.ellipsoids[anim.min(self.ellipsoids.len() - 1)]
    }

    /// From eight i16 words: offset x, y, z, a count, scale x, y, z, radius.
    pub fn entry(w: &[i16]) -> Ellipsoid {
        Ellipsoid { offset: [w[0], w[1], w[2]], scale: [w[4], w[5], w[6]], radius: w[7] }
    }
}

/// The hit shapes in a `.all` file, as the creature loader 0x43D820 / 0x43ACA0 finds them. The
/// first word, doubled, is the offset of a directory: u32 count, then 0x4C-byte entries {u32
/// size in 16-bit words, 3 × u32, u32 type, …, 4 × i16 sphere at +0x2C}; their data blocks
/// follow each other from offset 4. The type-9 entry's block is the ellipsoid table.
pub fn hit_shapes(b: &[u8]) -> Option<HitShapes> {
    let u32_at = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
    let i16_at = |o: usize| b.get(o..o + 2).map(|s| i16::from_le_bytes([s[0], s[1]]));
    let dir = u32_at(0)? as usize * 2;
    let count = u32_at(dir)? as usize;
    if count > 4096 {
        return None;
    }
    let mut data = 4usize;
    for k in 0..count {
        let e = dir + 4 + k * 0x4C;
        let size = u32_at(e)? as usize * 2;
        if u32_at(e + 0x10)? == 9 {
            let sphere = [0, 1, 2, 3].map(|j| i16_at(e + 0x2C + 2 * j).unwrap_or(0));
            let ellipsoids: Vec<Ellipsoid> = (0..size / 16)
                .filter_map(|j| {
                    let w: Option<Vec<i16>> = (0..8).map(|i| i16_at(data + 16 * j + 2 * i)).collect();
                    w.map(|w| HitShapes::entry(&w))
                })
                .collect();
            return (!ellipsoids.is_empty()).then_some(HitShapes { sphere, ellipsoids });
        }
        data += size;
    }
    None
}
