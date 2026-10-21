//! `.ngn` PC data containers (`data/gfx/*.ngn`, `data/levelNN/*.ngn`), written by TT's `pconv.exe`.
//!
//! See `docs/formats/ngn.md`. The file is a flat list of chunks:
//!
//! ```text
//! u32 id, u32 len, u8[len]       repeated; id 0 / len 0 terminates level files
//! ```
//!
//! | id    | contents                                                         |
//! |-------|------------------------------------------------------------------|
//! | 0x104 | textures: named Windows BMP files                                |
//! | 0x106 | creature table (names + PC-converted models), not decoded yet    |
//! | 0x100 | mesh set (tagged records 0x40..0x44 per mesh)                    |
//! | 0x101 | instance set: one placement per mesh in the preceding 0x100      |
//! | 0x105 | special objects: script id → instance (see `Special`)              |
//! | 0x102, 0x103 | not decoded yet                                             |

use std::fmt;
use std::ops::Range;

pub mod chunk {
    pub const END: u32 = 0x000;
    pub const MESHES: u32 = 0x100;
    pub const INSTANCES: u32 = 0x101;
    pub const TEXTURES: u32 = 0x104;
    /// Special objects: level-script handles for instances (0x4C3EB0).
    pub const SPECIALS: u32 = 0x105;
    pub const CREATURES: u32 = 0x106;
}

#[derive(Debug)]
pub enum Error {
    Truncated { at: usize, what: &'static str },
    Malformed { at: usize, what: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { at, what } => write!(f, "truncated {what} at {at:#x}"),
            Error::Malformed { at, what } => write!(f, "malformed data at {at:#x}: {what}"),
        }
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

/// Little-endian cursor with bounds checks that report file offsets.
struct Cur<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Cur<'a> {
    fn new(b: &'a [u8], o: usize) -> Self {
        Cur { b, o }
    }
    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8]> {
        let s = self.b.get(self.o..self.o + n).ok_or(Error::Truncated { at: self.o, what })?;
        self.o += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1, "u8")?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2, "u16")?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4, "u32")?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4, "f32")?.try_into().unwrap()))
    }
    fn f32x3(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    /// u8 length followed by that many bytes, trailing NULs stripped.
    fn pstr(&mut self) -> Result<String> {
        let n = self.u8()? as usize;
        let s = self.take(n, "string")?;
        Ok(String::from_utf8_lossy(s).trim_end_matches('\0').to_string())
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: u32,
    /// Byte range of the chunk payload within the file.
    pub data: Range<usize>,
}

pub fn chunks(b: &[u8]) -> Result<Vec<Chunk>> {
    let mut c = Cur::new(b, 0);
    let mut out = Vec::new();
    while c.o + 8 <= b.len() {
        let id = c.u32()?;
        let len = c.u32()? as usize;
        let start = c.o;
        c.take(len, "chunk payload")?;
        out.push(Chunk { id, data: start..start + len });
        if id == chunk::END && len == 0 {
            break;
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub name: String,
    /// A complete Windows BMP file (usually 256x256 24-bit; load screens 800x600).
    pub bmp: Vec<u8>,
}

impl Texture {
    pub fn dimensions(&self) -> Option<(i32, i32, u16)> {
        let b = &self.bmp;
        if b.len() < 30 || &b[0..2] != b"BM" {
            return None;
        }
        let w = i32::from_le_bytes(b[18..22].try_into().unwrap());
        let h = i32::from_le_bytes(b[22..26].try_into().unwrap());
        let bpp = u16::from_le_bytes(b[28..30].try_into().unwrap());
        Some((w, h, bpp))
    }
}

fn parse_textures(b: &[u8], r: Range<usize>) -> Result<Vec<Texture>> {
    let mut c = Cur::new(b, r.start);
    let n = c.u32()?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let size = c.u32()? as usize;
        let name_len = c.u32()? as usize;
        let name = String::from_utf8_lossy(c.take(name_len, "texture name")?).trim_end_matches('\0').to_string();
        let bmp = c.take(size, "texture data")?.to_vec();
        out.push(Texture { name, bmp });
    }
    if c.o != r.end {
        return Err(Error::Malformed { at: c.o, what: format!("texture chunk has {} unread bytes", r.end - c.o) });
    }
    Ok(out)
}

/// 36-byte vertex (`0x43` header says format 0x17, stride 36).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    /// Real normals on creature meshes; always (1, 0, 0) on level geometry (prelit).
    pub normal: [f32; 3],
    /// Raw bytes. Byte 0 is 0x80 or 0xFF; bytes 1..=3 vary like a light colour. Order not confirmed.
    pub color: [u8; 4],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prim {
    /// Engine code 1.
    Triangles,
    /// Engine code 4. Each quad is in perimeter order.
    Quads,
    /// Engine code 5. Billboard: pivot + 4 corners in strip order; the corners face the camera
    /// (0x4B70E0 type 5: the camera matrix).
    Strip,
    /// Engine code 6. Billboard, same layout as 5, turned only about the vertical axis (type 4:
    /// a Y rotation by the camera's heading), e.g. banister spindles.
    Fan,
    Other(u32),
}

impl From<u32> for Prim {
    fn from(v: u32) -> Self {
        match v {
            1 => Prim::Triangles,
            4 => Prim::Quads,
            5 => Prim::Strip,
            6 => Prim::Fan,
            v => Prim::Other(v),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Group {
    pub prim: Prim,
    /// Index into the mesh's `materials`.
    pub material: u16,
    pub indices: Vec<u16>,
}

impl Group {
    /// Triangulate into (a, b, c) index triples.
    pub fn triangles(&self) -> Vec<[u16; 3]> {
        let i = &self.indices;
        match self.prim {
            Prim::Triangles => i.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect(),
            Prim::Quads => i.chunks_exact(4).flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect(),
            // Codes 5 and 6 are flat sprite quads: index 0 is a pivot point (black, uv 0,0, shared
            // by every quad of the mesh, not part of the shape) and the other four are the
            // corners in strip order, as offsets from the pivot (see `Mesh::position`).
            // Drawing the pivot gave the spikes seen on rails and webs.
            Prim::Strip | Prim::Fan if i.len() == 5 => vec![[i[1], i[2], i[3]], [i[2], i[4], i[3]]],
            Prim::Strip => (2..i.len())
                .map(|k| if k % 2 == 0 { [i[k - 2], i[k - 1], i[k]] } else { [i[k - 1], i[k - 2], i[k]] })
                .collect(),
            Prim::Fan => (2..i.len()).map(|k| [i[0], i[k - 1], i[k]]).collect(),
            Prim::Other(_) => Vec::new(),
        }
    }
}

/// One material from the `0x42` record. Group `material` fields index this list.
#[derive(Debug, Clone)]
pub struct Material {
    /// 0x81 or 0xC1 seen. Bit 0x40 adds `mode`.
    pub flags: u32,
    /// Always (250, 250, 250) in shipped data so far.
    pub color: [u8; 3],
    /// Present when `flags & 0x40`. Values 0x02, 0x06, 0x10, 0x18 seen; probably blend/transparency mode.
    pub mode: Option<u32>,
    /// Index into the mesh's texture list, or None if untextured (0xFFFF).
    pub texture: Option<u16>,
}

fn parse_material(at: usize, flags: u32, d: &[u8]) -> Result<Material> {
    let has_mode = flags & 0x40 != 0;
    let want = if has_mode { 9 } else { 5 };
    if d.len() != want {
        return Err(Error::Malformed { at, what: format!("material flags {flags:#x} with {} bytes", d.len()) });
    }
    let color = [d[0], d[1], d[2]];
    let (mode, t) = if has_mode {
        (Some(u32::from_le_bytes(d[3..7].try_into().unwrap())), u16::from_le_bytes([d[7], d[8]]))
    } else {
        (None, u16::from_le_bytes([d[3], d[4]]))
    };
    Ok(Material { flags, color, mode, texture: (t != 0xFFFF).then_some(t) })
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub name: String,
    /// Texture names from the `0x41` record (index = material).
    pub textures: Vec<String>,
    /// Bytes after the texture names in `0x41`: 26 per material, not decoded.
    pub material_extra: Vec<u8>,
    pub materials: Vec<Material>,
    pub vertices: Vec<Vertex>,
    pub groups: Vec<Group>,
}

/// How a material is blended, from its mode flags as the exe maps them (0x4B6760 → 0x4B6320).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Blend {
    #[default]
    Opaque,
    /// Mode 0x02 (or 0x200): source alpha / inverse source alpha, no depth write.
    Alpha,
    /// Mode 0x10 (or 0x400): source alpha / one (e.g. Buzz's visor), no depth write.
    Additive,
    /// Mode 0x20 (or 0x800): zero / inverse source colour, no depth write.
    Darken,
}

impl Material {
    /// Drawn from both sides: mode bit 8, which 0x4B6760 turns into cull-none (render state
    /// 0x40, clearing the clockwise / counter-clockwise bits). Fences, boards and cut-outs.
    pub fn two_sided(&self) -> bool {
        self.mode.unwrap_or(0) & 8 != 0
    }

    pub fn blend(&self) -> Blend {
        let m = self.mode.unwrap_or(0);
        if m & 0x410 != 0 {
            Blend::Additive
        } else if m & 0x820 != 0 {
            Blend::Darken
        } else if m & 0x202 != 0 {
            Blend::Alpha
        } else {
            Blend::Opaque
        }
    }
}

impl Group {
    /// The pivot vertex of a sprite quad (codes 5/6), if this group is one.
    pub fn sprite_pivot(&self) -> Option<u16> {
        (matches!(self.prim, Prim::Strip | Prim::Fan) && self.indices.len() == 5).then(|| self.indices[0])
    }
}

impl Mesh {
    /// Mesh-space position of vertex `i` as drawn in group `g`: sprite corners are offsets
    /// from the sprite's pivot.
    pub fn position(&self, g: &Group, i: u16) -> [f32; 3] {
        let p = self.vertices[i as usize].pos;
        match g.sprite_pivot().map(|k| self.vertices[k as usize].pos) {
            Some(o) => [p[0] + o[0], p[1] + o[1], p[2] + o[2]],
            None => p,
        }
    }

    pub fn group_blend(&self, g: &Group) -> Blend {
        self.materials.get(g.material as usize).map(Material::blend).unwrap_or_default()
    }

    /// Whether a group's material is drawn from both sides ([`Material::two_sided`]).
    pub fn group_two_sided(&self, g: &Group) -> bool {
        self.materials.get(g.material as usize).is_some_and(Material::two_sided)
    }

    /// Texture name used by a group, if any.
    pub fn group_texture(&self, g: &Group) -> Option<&str> {
        let t = self.materials.get(g.material as usize)?.texture?;
        self.textures.get(t as usize).map(String::as_str)
    }
}

/// Body of a `0x41` texture list (also used by creature tag 0x205).
fn read_texture_list(d: &mut Cur, end: usize) -> Result<(Vec<String>, Vec<u8>)> {
    let n = d.u16()?;
    let _n2 = d.u16()?;
    let _unknown = d.take(2, "texture list header")?;
    let names = (0..n).map(|_| d.pstr()).collect::<Result<_>>()?;
    // 26 bytes per material follow the names; meaning unknown.
    let extra = d.take(end - d.o, "texture list extra")?.to_vec();
    Ok((names, extra))
}

/// Body of a `0x42` material list (also used by creature tag 0x206).
fn read_materials(d: &mut Cur, at: usize) -> Result<Vec<Material>> {
    let n = d.u32()?;
    (0..n)
        .map(|_| {
            let flags = d.u32()?;
            let len = d.u32()? as usize;
            parse_material(at, flags, d.take(len, "material")?)
        })
        .collect()
}

/// Body of a `0x43` vertex buffer (also used by creature tag 0x207).
fn read_vertices(d: &mut Cur, at: usize, len: usize) -> Result<Vec<Vertex>> {
    let _format = d.u32()?;
    let stride = d.u32()? as usize;
    let n = d.u32()? as usize;
    if stride != 36 || 12 + stride * n != len {
        return Err(Error::Malformed { at, what: format!("vertex block stride {stride} count {n} len {len}") });
    }
    (0..n)
        .map(|_| {
            let pos = d.f32x3()?;
            let normal = d.f32x3()?;
            let color: [u8; 4] = d.take(4, "vertex colour")?.try_into().unwrap();
            let uv = [d.f32()?, d.f32()?];
            Ok(Vertex { pos, normal, color, uv })
        })
        .collect()
}

/// Reads mesh tags (0x40..0x44) until and including the 0 tag that ends the mesh.
fn read_mesh(c: &mut Cur, limit: usize) -> Result<Mesh> {
    let mut mesh = Mesh::default();
    loop {
        if c.o + 8 > limit {
            return Err(Error::Truncated { at: c.o, what: "mesh (no end tag)" });
        }
        let at = c.o;
        let tag = c.u32()?;
        let len = c.u32()? as usize;
        let end = c.o + len;
        if end > limit {
            return Err(Error::Truncated { at, what: "mesh tag" });
        }
        let mut d = Cur::new(c.b, c.o);
        match tag {
            0x00 => return Ok(mesh),
            0x40 => mesh.name = d.pstr()?,
            0x41 => (mesh.textures, mesh.material_extra) = read_texture_list(&mut d, end)?,
            0x42 => mesh.materials = read_materials(&mut d, at)?,
            0x43 => mesh.vertices = read_vertices(&mut d, at, len)?,
            0x44 => {
                let groups = d.u32()?;
                for _ in 0..groups {
                    let prim = Prim::from(d.u32()?);
                    let material = d.u16()?;
                    let n = d.u16()? as usize;
                    let indices = (0..n).map(|_| d.u16()).collect::<Result<Vec<_>>>()?;
                    if let Some(&bad) = indices.iter().find(|&&i| i as usize >= mesh.vertices.len()) {
                        return Err(Error::Malformed { at, what: format!("index {bad} out of range in mesh {:?}", mesh.name) });
                    }
                    mesh.groups.push(Group { prim, material, indices });
                }
            }
            _ => return Err(Error::Malformed { at, what: format!("unknown mesh tag {tag:#x}") }),
        }
        if d.o != end {
            return Err(Error::Malformed { at, what: format!("tag {tag:#x} has {} unread bytes", end as isize - d.o as isize) });
        }
        c.o = end;
    }
}

fn parse_mesh_set(b: &[u8], r: Range<usize>) -> Result<Vec<Mesh>> {
    let mut c = Cur::new(b, r.start);
    let count = c.u32()? as usize;
    let meshes = (0..count).map(|_| read_mesh(&mut c, r.end)).collect::<Result<Vec<_>>>()?;
    if c.o != r.end {
        return Err(Error::Malformed { at: c.o, what: format!("mesh set has {} unread bytes", r.end - c.o) });
    }
    Ok(meshes)
}

/// One skeleton node of a creature.
#[derive(Debug, Clone)]
pub struct CreatureNode {
    pub name: String,
    /// Low u16 of the node header in tag 0x203. Bit 0: has mesh. Bit 2: only on smooth-jointed
    /// characters (Buzz, Bo Peep); probably skinned from the shared vertex buffer.
    pub flags: u16,
    /// Already in model space (rest pose), like the PS1 `.all` parts.
    pub mesh: Option<Mesh>,
    /// Not drawn (bit 0 of the part flags 0x4CAB80 sets; Mr Potato Head's missing parts).
    pub hidden: bool,
}

/// Connecting geometry from creature tag 0x208 (the exe's `hobjload.c` calls it a patch).
/// Every vertex is a copy of a shared vertex (tag 0x207) that follows one node's transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Patch {
    /// 0: a single quad, corner k = shared vertex `verts[k]` following `nodes[k]`.
    /// n > 0: two rows of n vertices; row A = shared `verts[0]..+n` following `nodes[0]`,
    /// row B = shared `verts[1]..+n` following `nodes[1]`.
    pub count: i16,
    /// Index into the creature's shared materials (tag 0x206).
    pub material: i16,
    pub nodes: [i16; 4],
    pub verts: [i16; 4],
}

impl Patch {
    /// The patch's vertices as (shared vertex index, node index), in the engine's order:
    /// row A then row B for strips, corners 0..3 for quads.
    pub fn vertices(&self) -> Vec<(usize, usize)> {
        let at = |v: i16, n: i16| (v as usize, n as usize);
        if self.count == 0 {
            (0..4).map(|k| at(self.verts[k], self.nodes[k])).collect()
        } else {
            let n = self.count;
            let a = (0..n).map(|i| at(self.verts[0] + i, self.nodes[0]));
            let b = (0..n).map(|i| at(self.verts[1] + i, self.nodes[1]));
            a.chain(b).collect()
        }
    }

    /// Triangles over `vertices()` indices.
    pub fn triangles(&self) -> Vec<[usize; 3]> {
        if self.count == 0 {
            return vec![[0, 1, 2], [0, 2, 3]];
        }
        let n = self.count as usize;
        (0..n.saturating_sub(1)).flat_map(|i| [[i, i + 1, n + i + 1], [i, n + i + 1, n + i]]).collect()
    }
}

fn read_patches(d: &mut Cur) -> Result<Vec<Patch>> {
    let n = d.u32()?;
    (0..n)
        .map(|_| {
            let count = d.u16()? as i16;
            let material = d.u16()? as i16;
            let nodes = [d.u16()? as i16, d.u16()? as i16, d.u16()? as i16, d.u16()? as i16];
            let verts = [d.u16()? as i16, d.u16()? as i16, d.u16()? as i16, d.u16()? as i16];
            Ok(Patch { count, material, nodes, verts })
        })
        .collect()
}

/// One creature from chunk 0x106: the PC-converted model, skeleton and animations.
#[derive(Debug, Clone)]
pub struct Creature {
    /// Slot in the creature table = id in `data/creatures.cfg`.
    pub id: usize,
    pub name: String,
    pub nodes: Vec<CreatureNode>,
    /// Tags 0x205/0x206/0x207: textures, materials and vertices shared by smooth joints.
    pub shared_textures: Vec<String>,
    pub shared_materials: Vec<Material>,
    pub shared_vertices: Vec<Vertex>,
    /// Tag 0x208: connecting geometry built from `shared_vertices`, each vertex following a node.
    pub patches: Vec<Patch>,
    /// Tag 0x204 records. Same header (F0 FF 03 00) as the PS1 `.anm` files; not decoded.
    pub animations: Vec<Vec<u8>>,
}

fn parse_creatures(b: &[u8], r: Range<usize>) -> Result<Vec<Creature>> {
    let mut c = Cur::new(b, r.start);
    let slots = c.u32()?;
    if slots > 1024 {
        return Err(Error::Malformed { at: r.start, what: format!("implausible creature count {slots}") });
    }
    let names = (0..slots).map(|_| c.pstr()).collect::<Result<Vec<_>>>()?;
    let mut present = names.iter().enumerate().filter(|(_, n)| !n.is_empty());
    let mut out = Vec::new();
    let mut cur: Option<Creature> = None;
    let mut node_names = Vec::new();
    while c.o + 8 <= r.end {
        let at = c.o;
        let tag = c.u32()?;
        let len = c.u32()? as usize;
        let end = c.o + len;
        if end > r.end {
            return Err(Error::Truncated { at, what: "creature tag" });
        }
        let mut d = Cur::new(b, c.o);
        if tag == 0x200 {
            let (id, name) = present.next().ok_or(Error::Malformed { at, what: "more creatures than names".into() })?;
            cur = Some(Creature {
                id,
                name: name.clone(),
                nodes: Vec::new(),
                shared_textures: Vec::new(),
                shared_materials: Vec::new(),
                shared_vertices: Vec::new(),
                patches: Vec::new(),
                animations: Vec::new(),
            });
        }
        let cr = cur.as_mut().ok_or(Error::Malformed { at, what: format!("tag {tag:#x} before creature header") })?;
        match tag {
            0x000 => out.push(cur.take().unwrap()),
            0x200 => node_names = vec![String::new(); d.u32()? as usize],
            0x202 => {
                for n in node_names.iter_mut() {
                    *n = d.pstr()?;
                }
            }
            // Runtime matrix buffer (64 bytes per node, all zero on disk).
            0x201 => d.o = end,
            0x203 => {
                for name in &node_names {
                    let flags = d.u16()?;
                    let _ffff = d.u16()?;
                    let mesh = if flags & 1 != 0 { Some(read_mesh(&mut d, end)?) } else { None };
                    cr.nodes.push(CreatureNode { name: name.clone(), flags, mesh, hidden: false });
                }
            }
            0x204 => cr.animations.push(d.take(len, "animation")?.to_vec()),
            0x205 => cr.shared_textures = read_texture_list(&mut d, end)?.0,
            0x206 => cr.shared_materials = read_materials(&mut d, at)?,
            0x207 => cr.shared_vertices = read_vertices(&mut d, at, len)?,
            0x208 => cr.patches = read_patches(&mut d)?,
            _ => return Err(Error::Malformed { at, what: format!("unknown creature tag {tag:#x}") }),
        }
        if d.o != end {
            return Err(Error::Malformed { at, what: format!("creature tag {tag:#x} has {} unread bytes", end as isize - d.o as isize) });
        }
        c.o = end;
    }
    if c.o != r.end || cur.is_some() {
        return Err(Error::Malformed { at: c.o, what: "creature chunk did not end cleanly".into() });
    }
    Ok(out)
}

/// 44-byte placement record from chunk 0x101 (`u32 count, u32 stride=44, records`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instance {
    pub pos: [f32; 3],
    /// Angles about X, Y, Z, 16-bit (0x4000 = 90°); see `scene::instance_matrix`.
    pub rot: [u32; 3],
    /// Applied before the rotation.
    pub scale: [f32; 3],
    /// Index into the mesh set this instance set belongs to.
    pub mesh: u32,
    /// Flags or type; values like 0x20001, 0x40000, 0x40008 seen. Unknown.
    pub flags: u32,
}

fn parse_instances(b: &[u8], r: Range<usize>) -> Result<Vec<Instance>> {
    let mut c = Cur::new(b, r.start);
    let n = c.u32()? as usize;
    let stride = c.u32()? as usize;
    if stride != 44 || 8 + stride * n != r.len() {
        return Err(Error::Malformed { at: r.start, what: format!("instance chunk len {} for {n} records of {stride}", r.len()) });
    }
    (0..n)
        .map(|_| {
            Ok(Instance {
                pos: c.f32x3()?,
                rot: [c.u32()?, c.u32()?, c.u32()?],
                scale: c.f32x3()?,
                mesh: c.u32()?,
                flags: c.u32()?,
            })
        })
        .collect()
}

/// A mesh set paired with the instance set that places it.
#[derive(Debug, Clone)]
pub struct Layer {
    pub meshes: Vec<Mesh>,
    pub instances: Vec<Instance>,
}

/// A special object (chunk 0x105, loaded by 0x4C3EB0): a handle level scripts use to move,
/// rotate and scale one instance (0x4CCE30 position, 0x4CCC70 rotation, 0x4CCB20 scale).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Special {
    pub id: u16,
    /// Index into `Ngn::layers` (a negative instance word in the file selects the second set).
    pub layer: usize,
    pub instance: usize,
}

#[derive(Debug, Clone)]
pub struct Ngn {
    pub chunks: Vec<Chunk>,
    pub specials: Vec<Special>,
    pub textures: Vec<Texture>,
    pub creatures: Vec<Creature>,
    pub layers: Vec<Layer>,
}

/// `u32 count, u32 max_id`, then count × `{i16 id, i16 instance}`; a negative instance is
/// `-v & 0x3FFF` in the second instance set. Ids outside 0..=max_id are ignored, as the loader
/// does.
fn parse_specials(b: &[u8], r: Range<usize>) -> Result<Vec<Special>> {
    let mut c = Cur::new(b, r.start);
    let count = c.u32()? as usize;
    let max_id = c.u32()? as i32;
    let mut out = Vec::new();
    for _ in 0..count {
        let id = c.u16()? as i16;
        let inst = c.u16()? as i16;
        if id < 0 || id as i32 > max_id {
            continue;
        }
        let (layer, instance) = if inst < 0 { (1, (-(inst as i32) & 0x3FFF) as usize) } else { (0, inst as usize) };
        out.push(Special { id: id as u16, layer, instance });
    }
    if c.o > r.end {
        return Err(Error::Malformed { at: r.start, what: "special-object chunk overruns".into() });
    }
    Ok(out)
}

pub fn parse(b: &[u8]) -> Result<Ngn> {
    let chunks = chunks(b)?;
    let mut textures = Vec::new();
    let mut creatures = Vec::new();
    let mut layers: Vec<Layer> = Vec::new();
    let mut specials = Vec::new();
    for ch in &chunks {
        match ch.id {
            chunk::TEXTURES => textures.extend(parse_textures(b, ch.data.clone())?),
            chunk::CREATURES => creatures = parse_creatures(b, ch.data.clone())?,
            chunk::MESHES => layers.push(Layer { meshes: parse_mesh_set(b, ch.data.clone())?, instances: Vec::new() }),
            chunk::INSTANCES => {
                let layer = layers
                    .last_mut()
                    .filter(|l| l.instances.is_empty())
                    .ok_or(Error::Malformed { at: ch.data.start, what: "instance chunk without mesh chunk".into() })?;
                layer.instances = parse_instances(b, ch.data.clone())?;
            }
            chunk::SPECIALS => specials = parse_specials(b, ch.data.clone())?,
            _ => {}
        }
    }
    Ok(Ngn { chunks, specials, textures, creatures, layers })
}
