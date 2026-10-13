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
//! Known types: 0–40 paths (0–8 challenge routes), 58 challenge reward positions, 61 poles,
//! 63 four values per point, 64 five-point zones (0x40), ≥ 0x41 room outlines (halved in place
//! by the parser) that feed the room table.

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
}

pub fn parse(b: &[u8]) -> Result<Vec<PointList>, Error> {
    let rd = |o: usize| -> Result<i32, Error> { b.get(o..o + 4).map(|s| i32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| Error(format!("read past end at {o:#x}"))) };
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
        out.push(PointList { kind, points });
        o += words.max(1) as usize * 4;
    }
    Ok(out)
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
        let l = PointList { kind: POLES, points: vec![[0, 10, 0], [0, -90, 0], [100, 100, 100], [5, 20, 5], [5, -80, 5]] };
        let p = poles(&[l]);
        assert_eq!(p, vec![Pole { pos: [0, 320, 0], kind: 0, length: 3200 }, Pole { pos: [160, 640, 160], kind: 2, length: 3200 }]);
    }
}
