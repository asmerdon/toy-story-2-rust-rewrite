//! `levelNN\level.dat` section 1: typed point lists, as the parser 0x43E6E0 walks them (it
//! stores list `type` at 0x559C70 + 4 × type). Only what the game code ported so far needs.
//!
//! ```text
//! u32 list_count
//! list_count × { i16 count, i16 type, points… }
//!   type < 0:     ((count × 3 + 1) / 2 + 4) words in all (packed, not read here)
//!   type 0x3F:    count × 4 i32
//!   other types:  count × 3 i32 (x, y, z in level units; +Y down)
//! ```
//! Known types: 0–40 paths (0–8 challenge routes), 58 reward / box spots, 61 poles, 63 pickups
//! (x, y, z, id), 64 five-point zones (0x40), ≥ 0x41 room outlines (halved in place by the
//! parser) that feed the room table.
//!
//! After section 1: section 2 (u32 n, n × 128-byte cells), section 3 (20-byte object records
//! until one with flags 0), then the special-object table; see [`special_objects`].

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "level.dat: {}", self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq)]
pub struct PointList {
    pub kind: i16,
    /// Points for the 3-value list types; empty for packed (negative) and 4-value types.
    pub points: Vec<[i32; 3]>,
    /// Points for the 4-value type (0x3F, the pickups): x, y, z and a word whose low byte is
    /// the pickup id.
    pub quads: Vec<[i32; 4]>,
}

fn rd32(b: &[u8], o: usize) -> Result<i32, Error> {
    b.get(o..o + 4).map(|s| i32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| Error(format!("read past end at {o:#x}")))
}

pub fn parse(b: &[u8]) -> Result<Vec<PointList>, Error> {
    Ok(walk_lists(b)?.0)
}

/// Section 1, plus the offset just past it.
fn walk_lists(b: &[u8]) -> Result<(Vec<PointList>, usize), Error> {
    let rd = |o: usize| rd32(b, o);
    let n = rd(0)?;
    if !(0..=4096).contains(&n) {
        return Err(Error(format!("implausible list count {n}")));
    }
    let mut o = 4usize;
    let mut out = Vec::new();
    for _ in 0..n {
        let head = rd(o)?;
        let count = (head & 0xFFFF) as i16 as i32;
        let kind = (head >> 16) as i16;
        let words = if kind < 0 {
            (count * 3 + 1) / 2 + 4
        } else if kind == 0x3F {
            count * 4 + 1
        } else {
            count * 3 + 1
        };
        let points = if kind >= 0 && kind != 0x3F {
            (0..count as usize).map(|k| Ok([rd(o + 4 + 12 * k)?, rd(o + 8 + 12 * k)?, rd(o + 12 + 12 * k)?])).collect::<Result<_, Error>>()?
        } else {
            Vec::new()
        };
        let quads = if kind == 0x3F {
            (0..count as usize).map(|k| Ok([rd(o + 4 + 16 * k)?, rd(o + 8 + 16 * k)?, rd(o + 12 + 16 * k)?, rd(o + 16 + 16 * k)?])).collect::<Result<_, Error>>()?
        } else {
            Vec::new()
        };
        out.push(PointList { kind, points, quads });
        o += words.max(1) as usize * 4;
    }
    Ok((out, o))
}

/// One entry of list 63, the pickups as 0x447DB0 copies them for 0x4A0F80. Level units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickupPoint {
    pub pos: [i32; 3],
    /// Below 0x30: a sprite (index into the sprite table at 0x4EC0A4; 16 is the coin).
    /// Otherwise a special-object id (`.ngn` 0x105).
    pub id: u8,
}

pub const PICKUPS: i16 = 63;

pub fn pickup_points(lists: &[PointList]) -> Vec<PickupPoint> {
    let Some(list) = lists.iter().find(|l| l.kind == PICKUPS) else { return Vec::new() };
    list.quads.iter().map(|q| PickupPoint { pos: [q[0], q[1], q[2]], id: q[3] as u8 }).collect()
}

/// A level object record from `level.dat` sections 2–3 (20 bytes each, walked by 0x43E6E0).
/// The PC game still reads these for gameplay; the geometry they point at is the PS1 model,
/// which the PC draws from the `.ngn` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Object {
    /// Level units.
    pub pos: [i32; 3],
    /// Bounding size (absolute value of the stored i16); pickups use size / 8 as their radius.
    pub size: i16,
    /// Flags byte after the loader's fix-up (0x80 also sets 0x40).
    pub flags: u8,
    /// Polygon count of the PS1 model (0x43E430). The pickup code identifies pickup kinds by it.
    pub polys: u16,
}

/// The special-object table (`0x54F628`): index = special id, as level scripts and the
/// pickup code (0x447DB0, 0x44E520) look objects up.
pub fn special_objects(b: &[u8]) -> Result<Vec<Option<Object>>, Error> {
    let (_, end) = walk_lists(b)?;
    // Section 2: 128-byte cells.
    let cells = rd32(b, end)?;
    if !(0..=65536).contains(&cells) {
        return Err(Error(format!("implausible cell count {cells}")));
    }
    let mut o = end + 4 + cells as usize * 128;
    // Section 3a: object records until one whose flags byte is 0.
    loop {
        let flags = *b.get(o + 0xE).ok_or_else(|| Error("object list runs past the end".into()))?;
        if flags == 0 {
            break;
        }
        o += 20;
    }
    let table = o + 20;
    let count = rd32(b, table)?;
    // The table holds count + 1 entries; −1 (level 17) means none.
    if !(-1..=4096).contains(&count) {
        return Err(Error(format!("implausible special count {count}")));
    }
    (0..count + 1)
        .map(|i| i as usize)
        .map(|i| {
            let r = rd32(b, table + 4 + 4 * i)? as usize;
            if r == 0 {
                return Ok(None);
            }
            object(b, r).map(Some)
        })
        .collect()
}

fn object(b: &[u8], r: usize) -> Result<Object, Error> {
    let pos = [rd32(b, r)?, rd32(b, r + 4)?, rd32(b, r + 8)?];
    let size = i16::from_le_bytes([b[r + 0xC], b[r + 0xD]]).wrapping_abs();
    let mut flags = *b.get(r + 0xE).ok_or_else(|| Error(format!("object at {r:#x} past end")))?;
    if flags & 0x80 != 0 {
        flags = (flags | 0x50).wrapping_sub(0x10);
    }
    let model = rd32(b, r + 0x10)? as usize;
    let polys = poly_count(b, model, flags)?;
    Ok(Object { pos, size, flags, polys })
}

/// 0x43E430: the model's polygon count. The mesh pointer is at model +0x14 (+0x1C when
/// flags bit 8 adds a scale); flags & 0x6F picks between two mesh encodings.
fn poly_count(b: &[u8], model: usize, flags: u8) -> Result<u16, Error> {
    let mesh_at = |off: usize| -> Result<usize, Error> { Ok(rd32(b, model + off)? as usize) };
    let kind = flags & 0x6F;
    let n = match kind & 0x27 {
        1 | 4 => strip_polys(b, mesh_at(if kind & 8 != 0 { 0x1C } else { 0x14 })?)?,
        2 | 3 => {
            let m = mesh_at(if kind & 8 != 0 { 0x1C } else { 0x14 })?;
            let groups = rd32(b, m)?;
            let mut p = m + (groups as usize * 3 + 1) * 4;
            let mut total = 0;
            for _ in 0..groups.max(0) {
                let c = rd32(b, p)?;
                p += 4;
                if c > 0 {
                    total += c;
                    p += c as usize * 44;
                }
            }
            total
        }
        _ => 0,
    };
    Ok(n as u16)
}

/// 0x43E2D0: a vertex block (n > 0: 2n + 1 words; n < 0: −3n + 2 words), then a stream of
/// `{u16 op, i16 count, data}` primitive runs until op 0xFFFF.
fn strip_polys(b: &[u8], m: usize) -> Result<i32, Error> {
    let n = rd32(b, m)?;
    let skip = if n < 0 { -3 * n + 2 } else { 2 * n + 1 };
    let mut p = m + skip as usize * 4;
    let rd16 = |o: usize| -> Result<u16, Error> { b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| Error(format!("mesh stream past end at {o:#x}"))) };
    let mut total = 0;
    for _ in 0..100_000 {
        let op = rd16(p)?;
        if op == 0xFFFF {
            return Ok(total);
        }
        let c = rd16(p + 2)? as i16 as i32;
        let (per, polys) = match op & 0x1F {
            0..=4 | 6 => (12, 1),
            8..=0xC | 0xE => (12, 2),
            0x10 | 0x12 | 0x14 | 0x16 | 0x18 | 0x1A | 0x1C | 0x1E | 0x11 | 0x13 | 0x19 | 0x1B => (4, 1),
            o => return Err(Error(format!("unknown primitive op {o:#x} at {p:#x}"))),
        };
        p += 4;
        if c > 0 {
            total += c * polys;
            p += c as usize * per;
        }
    }
    Err(Error("primitive stream never ends".into()))
}

/// A climbable pole or rope (list type 61), as 0x414550 prepares them for the climb code
/// 0x4354E0. Object units (level units × 32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pole {
    /// Bottom point.
    pub pos: [i32; 3],
    /// 0 / 1 climbable (1: climbing out of the top hops off), 2 slide-only, 3 ignored.
    pub kind: i32,
    /// Bottom − top, positive.
    pub length: i32,
}

pub const POLES: i16 = 61;

/// Pairs of points (bottom, top). A point whose |x| = |y| = |z| = v before a pair is a marker
/// setting the kind (v / 50) for the following poles.
pub fn poles(lists: &[PointList]) -> Vec<Pole> {
    let Some(list) = lists.iter().find(|l| l.kind == POLES) else { return Vec::new() };
    let p = &list.points;
    let mut out = Vec::new();
    let (mut i, mut kind) = (0, 0);
    while i + 1 < p.len() {
        let [x, y, z] = p[i];
        if x.abs() == y.abs() && y.abs() == z.abs() {
            kind = x.abs() / 50;
            i += 1;
            continue;
        }
        let top = p[i + 1];
        out.push(Pole { pos: [x, y, z].map(|v| v * 32), kind, length: (y - top[1]) * 32 });
        i += 2;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_set_the_kind() {
        let l = PointList { kind: POLES, points: vec![[0, 10, 0], [0, -90, 0], [100, 100, 100], [5, 20, 5], [5, -80, 5]], quads: Vec::new() };
        let p = poles(&[l]);
        assert_eq!(p, vec![Pole { pos: [0, 320, 0], kind: 0, length: 3200 }, Pole { pos: [160, 640, 160], kind: 2, length: 3200 }]);
    }
}
