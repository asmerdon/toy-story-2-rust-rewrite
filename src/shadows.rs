//! Floor shadows: the list at 0x54F098, drawn by 0x445720.
//!
//! - Each entry is a position (object units, the floor height in y), a half size and, for one
//!   laid on a slope, the height offsets of three of its corners and an alpha.
//! - Flat entries (size > 0), written straight into the list by the particles (0x410F40), the
//!   creatures that keep to a set height (0x4076F0) and a few level scripts, are drawn with
//!   alpha 0x20.
//! - Sloped entries (0x485680 for creatures and anything else asking through 0x486280, 0x4857E0
//!   for Buzz) are stored with a negative size. The floor's normal tilts them, a floor steeper
//!   than 0x3000 of 0x4000 shrinks them, one steeper than 0x1000 gets none, and more than
//!   0x10000 object units above the floor gets none either.
//! - All are cell (96, 96)–(127, 127) of texture 31, black, 10 units above the floor.

use crate::scene::{self, Batch, GpuVertex};

/// The list holds 0x30 entries; everything but 0x485680's own check stops at 0x2F.
pub const LIMIT: usize = 0x2F;

/// Drawn this many level units above the floor (0x4DC038).
const LIFT: f32 = 10.0;

/// 0x54F098: one shadow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shadow {
    /// Object units.
    pub at: [i32; 3],
    /// Half size, level units: > 0 flat, < 0 on a slope.
    pub size: i32,
    /// 0x54DEC8 / 0x54DECA / 0x54DECC: corner heights, level units.
    pub dy: [i32; 3],
    /// 0x54DECE.
    pub alpha: u8,
}

/// A flat shadow, as the particles and 0x4076F0 write them.
pub fn flat(out: &mut Vec<Shadow>, at: [i32; 3], size: i32) {
    if out.len() < LIMIT {
        out.push(Shadow { at, size, dy: [0; 3], alpha: 0x20 });
    }
}

/// 0x485680 / 0x4857E0: a shadow on the floor at (`at`), whose normal (× 0x4000, game axes) is
/// `n`, for something `height` object units up at `from_y`.
pub fn on_floor(out: &mut Vec<Shadow>, at: [i32; 3], size: i32, from_y: i32, n: [i32; 3]) {
    if (from_y - at[1]) / 1024 + 0x40 < 0 || n[1] >= -0x1000 || out.len() >= LIMIT {
        return;
    }
    let size = if n[1] > -0x3000 { (-0x800 - n[1]) * size / 0x2800 } else { size };
    let base = (n[2] + n[0]) * size / n[1];
    out.push(Shadow {
        at: [at[0], base * 0x20 + at[1], at[2]],
        size: -size,
        dy: [(n[2] - n[0]) * size / n[1] - base, (n[0] - n[2]) * size / n[1] - base, base * -2],
        alpha: if n[1] < -0x2000 { 0x30 } else { (((-0x1000 - n[1]) * 3) as u32 >> 8) as u8 },
    });
}

/// 0x445720: the four corners of a shadow, level units, in the original's order.
pub fn corners(s: &Shadow) -> [[f32; 3]; 4] {
    let (x, y, z) = ((s.at[0] >> 5) as f32, (s.at[1] >> 5) as f32 - LIFT, (s.at[2] >> 5) as f32);
    let r = s.size as f32;
    if s.size < 1 {
        let d = s.dy.map(|v| v as f32);
        [[x - r, y + d[2], z - r], [x - r, y + d[0], z + r], [x + r, y + d[1], z - r], [x + r, y, z + r]]
    } else {
        [[x - r, y, z + r], [x - r, y, z - r], [x + r, y, z + r], [x + r, y, z - r]]
    }
}

/// The list as triangles in texture `tex` (`size` wide and high).
pub fn batches(list: &[Shadow], tex: usize, size: [f32; 2]) -> Vec<Batch> {
    let mut out = Vec::new();
    let (u0, v0, u1, v1) = (96.0 / size[0], 96.0 / size[1], 127.0 / size[0], 127.0 / size[1]);
    for s in list {
        let c = corners(s);
        let color = [s.alpha, 0, 0, 0];
        let v = |k: usize, uv: [f32; 2]| GpuVertex { pos: c[k], uv, color };
        let (a, b, d, e) = (v(0, [u0, v0]), v(1, [u1, v0]), v(2, [u0, v1]), v(3, [u1, v1]));
        scene::push(&mut out, Some(tex), scene::Blend::Alpha, [a, b, e]);
        scene::push(&mut out, Some(tex), scene::Blend::Alpha, [a, e, d]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOOR: [i32; 3] = [0, -0x4000, 0];

    /// 0x485680 on level ground: the corners level, alpha 0x30, a negative size.
    #[test]
    fn level_floor_is_full_size() {
        let mut l = Vec::new();
        on_floor(&mut l, [320, 3200, 640], 100, 0, FLOOR);
        assert_eq!(l, vec![Shadow { at: [320, 3200, 640], size: -100, dy: [0; 3], alpha: 0x30 }]);
        assert_eq!(corners(&l[0])[3], [10.0 - 100.0, 90.0, 20.0 - 100.0]);
    }

    /// Steep floors shrink it, then lose it; so does being too far above the floor.
    #[test]
    fn slopes_and_height() {
        let mut l = Vec::new();
        on_floor(&mut l, [0; 3], 100, 0, [0, -0x2000, 0]);
        assert_eq!((l[0].size, l[0].alpha), (-60, ((0x1000 * 3) as u32 >> 8) as u8));
        l.clear();
        on_floor(&mut l, [0; 3], 100, 0, [0, -0x0FFF, 0]);
        assert!(l.is_empty(), "too steep");
        on_floor(&mut l, [0, 0x10400, 0], 100, 0, FLOOR);
        assert!(l.is_empty(), "too far above the floor");
    }

    /// A tilted floor tilts the corners by the normal.
    #[test]
    fn corners_follow_the_slope() {
        let mut l = Vec::new();
        on_floor(&mut l, [0; 3], 100, 0, [0x1000, -0x3000, 0]);
        let s = l[0];
        assert_eq!(s.size, -100);
        assert_eq!(s.dy, [(-0x1000 * 100) / -0x3000 - s.at[1] / 0x20, (0x1000 * 100) / -0x3000 - s.at[1] / 0x20, s.at[1] / 0x20 * -2]);
    }

    #[test]
    fn the_list_is_capped() {
        let mut l = Vec::new();
        for _ in 0..0x40 {
            flat(&mut l, [0; 3], 10);
        }
        assert_eq!(l.len(), LIMIT);
        assert_eq!(l[0].alpha, 0x20);
    }
}
