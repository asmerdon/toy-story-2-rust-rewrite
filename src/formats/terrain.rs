//! Collision geometry: `levelNN\TERRAIN.ALL` (level NN) and `TERR1.ALL` (level NN + 10),
//! loaded by 0x489C30 / 0x489980 and queried by the floor and wall routines (0x486520,
//! 0x484380). See `docs/formats/terrain.md`.
//!
//! ```text
//! u32 dir_offset            in 16-bit words from the file start
//! data blocks               object polygon data, back to back from offset 4
//! directory (at dir_offset):
//!   u32 count
//!   count × 76-byte object records:
//!     +0x00 i32 data size (16-bit words)   +0x04 i32 x, y, z (level units)
//!     +0x10 i32 type (6/7 static, 8 moved by a level object; ≥ 0x100 not collision)
//!     +0x1A i16 linked level object (moving platforms)   +0x28 u16 flags
//!     +0x44 i16 template id   +0x46 i16 1 = instance of the object that defined the template
//! object data: groups until a negative tag:
//!   i16 tag, i16 n, i16 x, w, z, d (bounds), n × 44-byte polygons
//! polygon (i16 words):
//!   0–1 x min/size, 2–3 coarse y/z bounds (bytes), 4–6 v0, 7–9 e1, 10–12 e2, 13–15 e3,
//!   16–18 normal (2.14), 19–21 second normal, or word 20 = 0x7FFF for a triangle
//! ```

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "terrain: {}", self.0)
    }
}

impl std::error::Error for Error {}

/// One collision polygon, relative to its object, in level units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Poly {
    pub v0: [i16; 3],
    pub e1: [i16; 3],
    pub e2: [i16; 3],
    pub e3: [i16; 3],
    /// Unit normal × 16384 (game axes, +Y down: floors point to −Y).
    pub normal: [i16; 3],
    /// Quads: the second half's normal. Triangles: other precomputed data.
    pub normal2: [i16; 3],
}

impl Poly {
    pub fn is_triangle(&self) -> bool {
        self.normal2[1] == 0x7FFF
    }

    /// The polygon as triangles of absolute points (object position added).
    pub fn triangles(&self, at: [f32; 3]) -> Vec<[[f32; 3]; 3]> {
        self.triangles_with_normals(at).into_iter().map(|(t, _)| t).collect()
    }

    /// Triangles with their unit normals, split as the sweep test 0x481FB0 does: (v0, v0+e1,
    /// v0+e2) with `normal`, and for quads (v0+e3, v0+e2, v0+e1) with `normal2`.
    pub fn triangles_with_normals(&self, at: [f32; 3]) -> Vec<([[f32; 3]; 3], [f32; 3])> {
        let p = |e: [i16; 3]| [0, 1, 2].map(|k| at[k] + self.v0[k] as f32 + e[k] as f32);
        let unit = |n: [i16; 3]| n.map(|v| v as f32 / 16384.0);
        let (a, b, c) = (p([0; 3]), p(self.e1), p(self.e2));
        let mut out = vec![([a, b, c], unit(self.normal))];
        if !self.is_triangle() {
            out.push(([p(self.e3), c, b], unit(self.normal2)));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub pos: [i32; 3],
    pub kind: i32,
    pub flags: u16,
    /// Level object that moves this one (type 8), 0 = none.
    pub linked: i16,
    /// Index into `Terrain::shapes`.
    pub shape: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Terrain {
    /// Polygon lists, shared by instanced objects.
    pub shapes: Vec<Vec<Poly>>,
    pub objects: Vec<Object>,
    /// Boundary walls: the kind-0x101 lists (see `boundaries` below), each a polyline of
    /// (x, z) points in level units, `count + 1` points for `count` segments.
    pub boundaries: Vec<Vec<[i32; 2]>>,
}

fn i16_at(b: &[u8], o: usize) -> Result<i16, Error> {
    b.get(o..o + 2).map(|s| i16::from_le_bytes([s[0], s[1]])).ok_or_else(|| Error(format!("read past end at {o:#x}")))
}

fn i32_at(b: &[u8], o: usize) -> Result<i32, Error> {
    b.get(o..o + 4).map(|s| i32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| Error(format!("read past end at {o:#x}")))
}

fn read_shape(b: &[u8], mut o: usize) -> Result<Vec<Poly>, Error> {
    let mut polys = Vec::new();
    while i16_at(b, o)? >= 0 {
        let n = i16_at(b, o + 2)?.max(0) as usize;
        let mut p = o + 12;
        for _ in 0..n {
            let w = |k: usize| i16_at(b, p + 2 * k);
            let v = |k: usize| -> Result<[i16; 3], Error> { Ok([w(k)?, w(k + 1)?, w(k + 2)?]) };
            polys.push(Poly { v0: v(4)?, e1: v(7)?, e2: v(10)?, e3: v(13)?, normal: v(16)?, normal2: v(19)? });
            p += 44;
        }
        o = p;
    }
    Ok(polys)
}

/// Records of this kind hold a boundary list. 0x489980 chains every record of kind ≥ 0x100
/// whose data starts 0x12345678 into a list per kind (heads at 0x728CF0 + kind × 4, i.e.
/// 0x7290F0 + (kind − 0x100) × 4, cleared first). Only kind 0x101's head, 0x7290F4, is read:
/// by Buzz's collision 0x484380 and the ray cast 0x48C860, which copy the segments near the
/// motion into 0x728DA0 for the circle-against-line tests 0x480660 / 0x487B20 (vertical walls
/// of unlimited height). Kind 0x104 (0x729100) is chained but never read.
pub const BOUNDARY_KIND: i32 = 0x101;

/// One boundary list block: u32 0x12345678 (overwritten by the chain link at load), u16
/// segment count, u16 0, then `count + 1` points of three i32 (x, y, z, level units). The
/// walls only use x and z: the y fields of points 16k..16k+3 hold group k's bounds (x min,
/// x size, z min, z size) for 0x484380's broad test, or 0x80000000 for "always test".
fn read_boundary(b: &[u8], at: usize, size: usize) -> Result<Option<Vec<[i32; 2]>>, Error> {
    if size < 8 || i32_at(b, at)? as u32 != 0x1234_5678 {
        return Ok(None);
    }
    let count = i16_at(b, at + 4)? as u16 as usize;
    if count == 0 {
        return Ok(None);
    }
    if 8 + 12 * (count + 1) > size {
        return Err(Error(format!("boundary list of {count} segments overruns its {size}-byte record")));
    }
    (0..=count).map(|k| Ok([i32_at(b, at + 8 + 12 * k)?, i32_at(b, at + 16 + 12 * k)?])).collect::<Result<Vec<_>, _>>().map(Some)
}

pub fn parse(b: &[u8]) -> Result<Terrain, Error> {
    let dir = i32_at(b, 0)? as usize * 2;
    let count = i32_at(b, dir)?;
    if !(0..=4096).contains(&count) {
        return Err(Error(format!("implausible object count {count}")));
    }
    let mut t = Terrain::default();
    // Template id → shape, as 0x489980 keeps them (ids < 100).
    let mut templates = [None::<usize>; 100];
    let mut cursor = 4usize;
    for k in 0..count as usize {
        let r = dir + 4 + 76 * k;
        let size = i32_at(b, r)?;
        let pos = [i32_at(b, r + 4)?, i32_at(b, r + 8)?, i32_at(b, r + 12)?];
        let kind = i32_at(b, r + 16)?;
        let linked = i16_at(b, r + 0x1A)?;
        let flags = i16_at(b, r + 0x28)? as u16;
        let template = i16_at(b, r + 0x44)?;
        let instance = i16_at(b, r + 0x46)? == 1;
        if kind < 0x100 {
            let slot = usize::try_from(template).ok().filter(|&i| i < templates.len());
            let shape = if instance {
                slot.and_then(|i| templates[i])
            } else {
                t.shapes.push(read_shape(b, cursor)?);
                let s = t.shapes.len() - 1;
                if let Some(i) = slot {
                    templates[i] = Some(s);
                }
                Some(s)
            };
            if let Some(shape) = shape {
                t.objects.push(Object { pos, kind, flags, linked, shape });
            }
        } else if kind == BOUNDARY_KIND {
            if let Some(list) = read_boundary(b, cursor, size.max(0) as usize * 2)? {
                t.boundaries.push(list);
            }
        }
        cursor += size.max(0) as usize * 2;
    }
    if cursor != dir {
        return Err(Error(format!("object data ends at {cursor:#x}, directory at {dir:#x}")));
    }
    Ok(t)
}

impl Terrain {
    /// Collision triangles in absolute level units: static objects (types 6/7), plus the
    /// moving ones (type 8) at their starting positions, since the level objects that move them
    /// aren't ported yet. Objects flagged 0x100 start disabled and are skipped, as the
    /// gatherer 0x485940 does; 0x400 marks the quarter-scale room map, not collision.
    pub fn static_triangles(&self) -> Vec<[[f32; 3]; 3]> {
        self.objects
            .iter()
            .filter(|o| matches!(o.kind, 6..=8) && o.flags & 0x500 == 0)
            .flat_map(|o| self.shapes[o.shape].iter().flat_map(move |p| p.triangles(o.pos.map(|v| v as f32))))
            .collect()
    }

    /// Object `i`'s triangles with their unit normals, at its stored position.
    pub fn object_triangles(&self, i: usize) -> Vec<([[f32; 3]; 3], [f32; 3])> {
        let o = &self.objects[i];
        self.shapes[o.shape].iter().flat_map(|p| p.triangles_with_normals(o.pos.map(|v| v as f32))).collect()
    }

    /// As `static_triangles`, with each triangle's stored unit normal (its solid side faces it).
    pub fn collision_triangles(&self) -> Vec<([[f32; 3]; 3], [f32; 3])> {
        self.objects
            .iter()
            .filter(|o| matches!(o.kind, 6..=8) && o.flags & 0x500 == 0)
            .flat_map(|o| self.shapes[o.shape].iter().flat_map(move |p| p.triangles_with_normals(o.pos.map(|v| v as f32))))
            .collect()
    }
}

/// The collision file for a level file: `level.*` → `TERRAIN.ALL`, `level1.*` → `TERR1.ALL`
/// (0x489C30 picks terrain/terr1/terr2/terr3 by the level's mode bits).
pub fn path_for(level_file: &std::path::Path) -> Option<std::path::PathBuf> {
    let stem = level_file.file_stem()?.to_str()?.to_ascii_lowercase();
    let name = match stem.as_str() {
        "level" => "TERRAIN.ALL",
        "level1" => "TERR1.ALL",
        _ => return None,
    };
    Some(level_file.with_file_name(name))
}
