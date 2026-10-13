//! Character animations: `.ngn` creature tag 0x204 (same encoding as PS1 `.anm`).
//!
//! Reverse-engineered from the PC exe's sampler (see `docs/formats/anim.md`). Summary:
//!
//! ```text
//! header (little-endian i16/u16):
//!   [0]  i16  if negative, header size in bytes = -value; otherwise 12
//!   [2]  u16  frame count
//!   [4]  u16  frame stride in 16-bit words
//!   [5]  u16  node table length (entries)
//!   [6]  u16  scale bitmask length in bytes
//!   [7]  i16  special/root node index
//! node table: i16 per node = word offset of that node's record inside a frame (<0: not animated)
//! scale bitmask: bit n set = node n has scale words
//! frames: frame_count × stride words
//! node record (words): tx ty tz, rot_lo, rot_hi, [sx sy sz]
//! ```
//!
//! Each translation word holds the value in bits 2..15 (`>> 2`) and two extra rotation bits in
//! bits 0..1. The rotation dword `rot_hi << 16 | rot_lo` packs three 10-bit fields. Together they
//! form three 12-bit angles (4096 = 360°) for X, Y and Z. Scale is 4.12 fixed point.
//!
//! Node transforms are in model space and are not chained through a hierarchy: the game
//! multiplies each node's matrix directly by the object's root matrix.

use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "animation: {}", self.0)
    }
}

impl std::error::Error for Error {}

/// One node's pose at a frame, in the game's units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodePose {
    /// Translation in model units (same units as mesh vertices).
    pub translation: [f32; 3],
    /// Euler angles in 12-bit units (4096 = 360°). The matrix is built Rz · Ry · Rx (row vectors:
    /// Z applied first, then Y, then X).
    pub angles: [u16; 3],
    pub scale: [f32; 3],
}

impl NodePose {
    pub fn angles_radians(&self) -> [f32; 3] {
        self.angles.map(|a| a as f32 / 4096.0 * std::f32::consts::TAU)
    }

    /// Row-major 4×4 matrix for row vectors (`v' = v · M`), as Direct3D uses:
    /// scale, then rotate Z, Y, X, then translate.
    pub fn matrix(&self) -> [[f32; 4]; 4] {
        let [ax, ay, az] = self.angles_radians();
        let mut m = identity();
        // The game pre-multiplies X, then Y, then Z into an identity matrix.
        m = mul(&rot_x(ax), &m);
        m = mul(&rot_y(ay), &m);
        m = mul(&rot_z(az), &m);
        let s = self.scale;
        let mut out = mul(&[[s[0], 0., 0., 0.], [0., s[1], 0., 0.], [0., 0., s[2], 0.], [0., 0., 0., 1.]], &m);
        out[3][0] += self.translation[0];
        out[3][1] += self.translation[1];
        out[3][2] += self.translation[2];
        out
    }
}

fn identity() -> [[f32; 4]; 4] {
    let mut m = [[0.0; 4]; 4];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    m
}

fn mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut r = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

// Matches the game's helpers: rows 1/2 (X), rows 0/2 (Y), rows 0/1 (Z) of a row-vector matrix.
fn rot_x(t: f32) -> [[f32; 4]; 4] {
    let (s, c) = t.sin_cos();
    [[1., 0., 0., 0.], [0., c, s, 0.], [0., -s, c, 0.], [0., 0., 0., 1.]]
}
fn rot_y(t: f32) -> [[f32; 4]; 4] {
    let (s, c) = t.sin_cos();
    [[c, 0., -s, 0.], [0., 1., 0., 0.], [s, 0., c, 0.], [0., 0., 0., 1.]]
}
fn rot_z(t: f32) -> [[f32; 4]; 4] {
    let (s, c) = t.sin_cos();
    [[c, s, 0., 0.], [-s, c, 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]]
}

#[derive(Debug, Clone)]
pub struct Animation {
    pub frame_count: u16,
    /// Frame stride in 16-bit words.
    pub stride: u16,
    /// Per node: word offset of its record within a frame, or None if the node isn't animated.
    pub node_offsets: Vec<Option<u16>>,
    /// Per node: whether its record carries scale.
    pub has_scale: Vec<bool>,
    /// The node table as stored: record offsets, or -1/-2/-3 markers.
    pub raw_table: Vec<i16>,
    pub root_node: i16,
    /// All frame words, `frame_count * stride` long.
    pub words: Vec<i16>,
}

fn rd16(b: &[u8], o: usize) -> Result<u16, Error> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| Error(format!("read past end at {o:#x}")))
}

pub fn parse(b: &[u8]) -> Result<Animation, Error> {
    let h = |i: usize| rd16(b, 2 * i);
    let first = h(0)? as i16;
    let header = if first < 0 { (-(first as i32)) as usize } else { 12 };
    let frame_count = h(2)?;
    let stride = h(4)?;
    let table_len = h(5)? as usize;
    let mask_len = h(6)? as usize;
    let root_node = h(7)? as i16;

    let raw_table = (0..table_len).map(|i| rd16(b, header + 2 * i).map(|v| v as i16)).collect::<Result<Vec<_>, _>>()?;
    let node_offsets = raw_table.iter().map(|&v| (v >= 0).then_some(v as u16)).collect();
    let mask_start = header + 2 * table_len;
    let mask = b.get(mask_start..mask_start + mask_len).ok_or_else(|| Error("scale mask past end".into()))?;
    let has_scale: Vec<bool> = (0..table_len).map(|n| mask.get(n / 8).is_some_and(|m| m & (1 << (n % 8)) != 0)).collect();

    let frames_start = mask_start + mask_len;
    let full = frame_count as usize * stride as usize;
    let available = b.len().saturating_sub(frames_start) / 2;
    // Some shipped animations (e.g. level03 SLIME) are a few words short in their final frame.
    // The original reads past the buffer; we instead repeat the previous frame's words.
    if available + stride as usize <= full && frame_count > 1 {
        return Err(Error(format!("frames need {full} words at {frames_start:#x}, have {available}")));
    }
    let take = available.min(full);
    let mut words: Vec<i16> =
        b[frames_start..frames_start + 2 * take].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    while words.len() < full {
        let prev = words.len() - stride as usize;
        words.push(words[prev]);
    }
    Ok(Animation { frame_count, stride, node_offsets, has_scale, raw_table, root_node, words })
}

impl Animation {
    /// Node matrix at fractional frame `t`, interpolated as the engine does: translation and
    /// scale lerped, rotation by lerping the two frames' matrices element-wise.
    pub fn matrix_at(&self, node: usize, t: f32) -> Option<[[f32; 4]; 4]> {
        let last = self.frame_count.saturating_sub(1) as usize;
        let t = t.clamp(0.0, last as f32);
        let f0 = (t.floor() as usize).min(last.saturating_sub(1));
        let f1 = (f0 + 1).min(last);
        let frac = t - f0 as f32;
        let (a, b) = (self.pose(node, f0)?.matrix(), self.pose(node, f1)?.matrix());
        let mut m = a;
        for i in 0..4 {
            for j in 0..4 {
                m[i][j] = a[i][j] + (b[i][j] - a[i][j]) * frac;
            }
        }
        Some(m)
    }

    /// Pose of `node` at integer `frame`. None if the node isn't animated.
    pub fn pose(&self, node: usize, frame: usize) -> Option<NodePose> {
        let off = (*self.node_offsets.get(node)?)? as usize;
        let frame = frame.min(self.frame_count.saturating_sub(1) as usize);
        let base = frame * self.stride as usize + off;
        let w = |k: usize| self.words.get(base + k).copied();
        let (t0, t1, t2) = (w(0)?, w(1)?, w(2)?);
        let packed = (w(4)? as u16 as u32) << 16 | w(3)? as u16 as u32;
        let angle = |field: u32, low: i16| (((field & 0x3FF) << 2) as u16) | (low as u16 & 3);
        let angles = [angle(packed >> 20, t0), angle(packed >> 10, t1), angle(packed, t2)];
        let scale = if self.has_scale.get(node).copied().unwrap_or(false) {
            [w(5)?, w(6)?, w(7)?].map(|s| s as f32 / 4096.0)
        } else {
            [1.0; 3]
        };
        Some(NodePose { translation: [t0, t1, t2].map(|t| (t >> 2) as f32), angles, scale })
    }
}
