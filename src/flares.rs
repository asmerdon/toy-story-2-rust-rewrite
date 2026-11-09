//! Lens flares (the "lens flare" graphics option, 0x508D70 bit 0, on by default).
//!
//! - 0x44F200 queues up to 8 a frame: a position with a colour and base size, kept only if a
//!   ray from the camera nine tenths of the way to it is clear, and it's on screen.
//! - 0x44F420 / 0x44F580 draw each as 17 sprites (table 0x4F72D8) along the line from the light
//!   through the centre of the 512 × 256 screen, additively, weaker and smaller the further the
//!   light is from the centre.
//! - The sources are lamps in the level's point lists (each level's frame script), the fixed
//!   lamp in level 3 and the glowing particle kinds (0x410F40).

use crate::formats::exe::Exe;
use crate::formats::leveldat::PointList;
use crate::hud::Quad;
use crate::pickups::SpriteDef;

/// 0x4F72D8: 17 entries of sprite, width, height, red, green, blue (i16).
const TABLE: u32 = 0x4F72D8;
pub const COUNT: usize = 17;
/// 0x44F200: at most this many a frame.
pub const LIMIT: usize = 8;

/// A light: object units, colour (0x80 = full) and base size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flare {
    pub pos: [i32; 3],
    pub rgb: [i32; 3],
    pub size: i32,
}

/// Where a light lands on the 512 × 256 screen, and its depth in level units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub x: i32,
    pub y: i32,
    pub depth: i32,
}

/// 0x4F72D8.
pub fn table(exe: &Exe) -> Vec<[i16; 6]> {
    exe.at(TABLE, COUNT * 12)
        .map(|b| b.chunks_exact(12).map(|e| std::array::from_fn(|k| i16::from_le_bytes([e[2 * k], e[2 * k + 1]]))).collect())
        .unwrap_or_default()
}

/// The lamps of internal level `level`: (level.dat list, colour, size, reach² in the (>> 8) units
/// the scripts compare) from the frame scripts. Level 4's (list 6) are the crane puzzle's and
/// wait on it; level 14's fade with distance and are done by [`lamp`].
fn lamp_list(level: usize) -> Option<(i16, [i32; 3], i32, i32)> {
    Some(match level {
        5 => (17, [0x40, 0x30, 0x20], 0x80, 1_000_000),   // 0x41E880
        10 => (13, [0x80, 0x80, 0x80], 0x40, 1_000_000),  // 0x425F60
        11 => (19, [0x20, 0x20, 0x20], 0x80, 1_000_000),  // 0x42A130
        12 => (0, [0x80, 0x80, 0x80], 0x40, 1_000_000),   // 0x42B3A0
        13 => (10, [0x30, 0x40, 0x50], 0x40, 1_000_000),  // 0x42CA60
        15 => (0, [0x80, 0x80, 0x80], 0x80, 0x100000),    // 0x42FC50
        14 => (40, [0; 3], 0x80, 0x40000),                // 0x42E790
        4 => (6, [0; 3], 0x40, 1_000_000),                // 0x41C640, coloured by `tint`
        _ => return None,
    })
}

/// A level's lamp points (level units), with the rest of what [`lamp`] needs.
#[derive(Debug, Clone, Default)]
pub struct Lamps {
    level: usize,
    points: Vec<[i32; 3]>,
    rgb: [i32; 3],
    size: i32,
    reach: i32,
    /// Level 4's colour now (0x52F7C8); `None` while it's out.
    pub tint: Option<[i32; 3]>,
}

impl Lamps {
    pub fn level(&self) -> usize {
        self.level
    }

    pub fn new(level: usize, lists: &[PointList]) -> Lamps {
        let Some((kind, rgb, size, reach)) = lamp_list(level) else { return Lamps { level, ..Default::default() } };
        let points = lists.iter().find(|l| l.kind == kind).map(|l| l.points.clone()).unwrap_or_default();
        Lamps { level, points, rgb, size, reach, tint: None }
    }

    /// The lamps lit for a camera at `camera` (object units), in list order.
    pub fn lit(&self, camera: [i32; 3], out: &mut Vec<Flare>) {
        // Level 3's lamp is a single fixed point (0x41AA10), always queued.
        if self.level == 3 {
            out.push(Flare { pos: [0x3889, -0x36099, -0x1FBC], rgb: [0x80; 3], size: 0x80 });
        }
        for (k, p) in self.points.iter().enumerate() {
            let d = [0, 1, 2].map(|i| (camera[i] - p[i] * 0x20) >> 8);
            let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            if d2 >= self.reach {
                continue;
            }
            let rgb = match self.level {
                4 => match self.tint {
                    Some(t) => t,
                    None => continue,
                },
                // 0x42E790: fades with distance, the first 16 green, the rest blue.
                14 => {
                    let v = 0x80 - ((d2 as f32).sqrt() as i32 >> 2);
                    if v <= 0 {
                        continue;
                    }
                    if k < 16 { [v, v, 0] } else { [0, v, v] }
                }
                // 0x42FC50: the third is red.
                15 if k == 2 => [0x80, 0, 0],
                _ => self.rgb,
            };
            out.push(Flare { pos: p.map(|v| v * 0x20), rgb, size: self.size });
        }
    }
}

impl Lamps {
    /// The frame scripts' slot 1 (0x830D78…): of the lamps lit from `camera`, the nearest to
    /// Buzz's middle if within 0x100 (× 256 object units), with its light colour.
    pub fn nearest(&self, camera: [i32; 3], buzz: [i32; 3]) -> Option<([i32; 3], [i32; 3], i64)> {
        let mut best: Option<(i32, usize)> = None;
        for (k, p) in self.points.iter().enumerate() {
            let c = [0, 1, 2].map(|i| (camera[i] - p[i] * 0x20) >> 8);
            if c[0] * c[0] + c[1] * c[1] + c[2] * c[2] >= self.reach {
                continue;
            }
            let b = [(buzz[0] - p[0] * 0x20) >> 8, (buzz[1] - p[1] * 0x20 - 0x2000) >> 8, (buzz[2] - p[2] * 0x20) >> 8];
            let d = b[0] * b[0] + b[1] * b[1] + b[2] * b[2];
            if best.is_none_or(|(e, _)| d < e) {
                best = Some((d, k));
            }
        }
        let (d, k) = best.filter(|(d, _)| *d < 0x10000)?;
        let _ = d;
        let rgb = match self.level {
            4 => self.tint?,
            5 => [0x80, 0x60, 0x40],
            10 | 12 => [0xFF; 3],
            11 => [0x7F; 3],
            13 => [0x6F, 0x7F, 0x8F],
            14 if k < 16 => [0xFF, 0xFF, 0],
            14 => [0, 0xFF, 0xFF],
            15 if k == 2 => [0xFF, 0, 0],
            15 => [0xFF; 3],
            _ => return None,
        };
        Some((self.points[k].map(|v| v << 5), rgb, k as i64 + 1))
    }
}

/// 0x44F420 + 0x44F580: one flare's sprites, given where it lands. Off screen, too close to the
/// camera (depth ≤ 0x160) or nothing to draw: none.
pub fn quads(f: &Flare, at: Screen, table: &[[i16; 6]], sprites: &[Option<SpriteDef>]) -> Vec<Quad> {
    let mut out = Vec::new();
    if !(0..=0x200).contains(&at.x) || !(0..=0x100).contains(&at.y) || at.depth - 0x60 <= 0x100 {
        return out;
    }
    let (cx, cy) = (0x100 - at.x, 0x80 - at.y);
    let weight = 0x103FF - (cx * cx + cy * cy).min(0xFFFF);
    let scale = weight / (at.depth / 2 + 1) + f.size;
    let (mut px, mut py) = (at.x * 8, at.y * 8);
    for e in table.iter().take(COUNT) {
        if e[0] != 0
            && let Some(Some(def)) = sprites.get(e[0] as usize)
        {
            let (w, h) = (e[1] as i32 * scale, e[2] as i32 * scale);
            let c = |k: usize, v: i32| (e[3 + k] as i64 * v as i64 * weight as i64 >> 23).clamp(0, 0xFF) as u8;
            out.push(Quad {
                sprite: e[0] as usize,
                cell: 0,
                pos: [((px >> 3) - w / 256) as f32, ((py >> 3) - h / 256) as f32],
                size: [def.size[0] as f32 * w as f32 / 4096.0, def.size[1] as f32 * h as f32 / 4096.0],
                width: 512.0,
                color: [c(0, f.rgb[0]), c(1, f.rgb[1]), c(2, f.rgb[2])],
                alpha: 0xFF,
            });
        }
        px += cx;
        py += cy;
    }
    out
}

/// 0x44DD80, colours over 0x80: a full-screen wash added to the picture (screen 0..1).
pub fn white_wash(c: i32) -> crate::scene::Batch {
    let v = (c * 2 - 0x100).clamp(0, 0xFF) as u8;
    let color = [0xFF - v, v / 2, v / 2, v / 2];
    let p = |x: f32, y: f32| crate::scene::GpuVertex { pos: [x, y, 0.0], uv: [0.0; 2], color };
    crate::scene::Batch { texture: None, blend: crate::scene::Blend::Additive, vertices: vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0), p(0.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprites() -> Vec<Option<SpriteDef>> {
        (0..16).map(|_| Some(SpriteDef { texture: 31, size: [16, 16], cells: vec![[0, 0]] })).collect()
    }

    const FLARE: Flare = Flare { pos: [0; 3], rgb: [0x80; 3], size: 0x40 };

    #[test]
    fn sprites_run_through_the_centre() {
        let mut t = [[0i16; 6]; COUNT];
        t[0] = [5, 72, 48, 96, 96, 96];
        t[8] = [7, 42, 28, 4, 4, 56];
        let q = quads(&FLARE, Screen { x: 0x140, y: 0x90, depth: 1000 }, &t, &sprites());
        assert_eq!(q.len(), 2);
        // The first at the light, the ninth stepped 8 times along (0x100 − x, 0x80 − y) / 8.
        let scale = (0x103FF - (0x40 * 0x40 + 0x10 * 0x10)) / 501 + 0x40;
        assert_eq!(q[0].pos, [(0x140 - 72 * scale / 256) as f32, (0x90 - 48 * scale / 256) as f32]);
        assert_eq!(q[1].pos, [(0x140 - 0x40 - 42 * scale / 256) as f32, (0x90 - 0x10 - 28 * scale / 256) as f32]);
    }

    #[test]
    fn nothing_off_screen_or_too_close() {
        let t = [[5i16, 72, 48, 96, 96, 96]; COUNT];
        assert!(quads(&FLARE, Screen { x: 0x201, y: 0x80, depth: 1000 }, &t, &sprites()).is_empty());
        assert!(quads(&FLARE, Screen { x: 0x100, y: 0x80, depth: 0x160 }, &t, &sprites()).is_empty());
        assert_eq!(quads(&FLARE, Screen { x: 0x100, y: 0x80, depth: 0x161 }, &t, &sprites()).len(), COUNT);
    }

    /// Lightning at 0xDD: v = 0xBA, alpha 0x45 (0x44DD80).
    #[test]
    fn the_white_wash_follows_the_colour() {
        let b = white_wash(0xDD);
        assert_eq!(b.blend, crate::scene::Blend::Additive);
        assert_eq!(b.vertices[0].color, [0x45, 0x5D, 0x5D, 0x5D]);
    }

    #[test]
    fn the_nearest_lamp_lights_buzz() {
        let lists = [PointList { kind: 13, points: vec![[1000, 0, 0], [10, 0, 0]], quads: vec![] }];
        let l = Lamps::new(10, &lists);
        let (at, rgb, _) = l.nearest([0; 3], [0, 0x2000, 0]).unwrap();
        assert_eq!((at, rgb), ([10 << 5, 0, 0], [0xFF; 3]));
    }

    #[test]
    fn level_4s_lamps_follow_the_tint() {
        let lists = [PointList { kind: 6, points: vec![[0, 0, 0]], quads: vec![] }];
        let mut l = Lamps::new(4, &lists);
        let mut out = Vec::new();
        l.lit([0; 3], &mut out);
        assert!(out.is_empty(), "out");
        l.tint = Some([0x20, 0x20, 0]);
        l.lit([0; 3], &mut out);
        assert_eq!(out[0].rgb, [0x20, 0x20, 0]);
    }

    #[test]
    fn lamps_light_within_reach() {
        let lists = [PointList { kind: 13, points: vec![[0, 0, 0], [100000, 0, 0]], quads: vec![] }];
        let l = Lamps::new(10, &lists);
        let mut out = Vec::new();
        l.lit([0; 3], &mut out);
        assert_eq!(out, vec![Flare { pos: [0; 3], rgb: [0x80; 3], size: 0x40 }]);
    }
}
