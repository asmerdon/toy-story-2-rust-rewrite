//! Animated textures (0x49B260 → 0x4CE510 → 0x4AFD30, the graphics option 0x508D70 bit 2).
//!
//! A level script copies a master tile within a texture to another place in the same texture,
//! wrapped round by an offset, so rivers and glow scroll. Each is a plain rectangle blit.

/// 0x49B260's arguments as 0x4AFD30 gets them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blit {
    /// Texture number (`texNN`).
    pub texture: u16,
    pub dst: [i32; 2],
    pub size: [i32; 2],
    /// How far the tile is scrolled, wrapped within `size`.
    pub offset: [i32; 2],
    pub src: [i32; 2],
}

impl Blit {
    /// 0x49B260(tex, x, y, w, h, ox, oy, sx, sy): the source is given from the destination.
    #[allow(clippy::too_many_arguments)]
    pub fn new(texture: u16, x: i32, y: i32, w: i32, h: i32, ox: i32, oy: i32, sx: i32, sy: i32) -> Blit {
        Blit { texture, dst: [x, y], size: [w, h], offset: [ox, oy], src: [x + sx, y + sy] }
    }

    /// The destination's new RGBA pixels, read from `rgba` (`width` × `height`, top row first).
    pub fn pixels(&self, rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
        let [w, h] = self.size;
        let (tw, th) = (width as i32, height as i32);
        if w <= 0 || h <= 0 || self.src[0] < 0 || self.src[1] < 0 || self.src[0] + w > tw || self.src[1] + h > th {
            return None;
        }
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for j in 0..h {
            for i in 0..w {
                let x = self.src[0] + (i + self.offset[0]).rem_euclid(w);
                let y = self.src[1] + (j + self.offset[1]).rem_euclid(h);
                let p = ((y * tw + x) * 4) as usize;
                out.extend_from_slice(rgba.get(p..p + 4)?);
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolls_the_master_tile_with_wrap() {
        // A 2 × 2 master at (0, 2) in a 2 × 4 texture: rows of 1s and 2s (red channel).
        let mut rgba = vec![0u8; 2 * 4 * 4];
        for (y, v) in [(2, 1u8), (3, 2u8)] {
            for x in 0..2 {
                rgba[(y * 2 + x) * 4] = v;
            }
        }
        let b = Blit::new(5, 0, 0, 2, 2, 0, 1, 0, 2);
        let p = b.pixels(&rgba, 2, 4).unwrap();
        assert_eq!([p[0], p[8]], [2, 1], "one row down, wrapped");
    }

    #[test]
    fn a_source_outside_the_texture_is_refused() {
        let rgba = vec![0u8; 2 * 2 * 4];
        assert!(Blit::new(5, 0, 0, 2, 2, 0, 0, 0, 2).pixels(&rgba, 2, 2).is_none());
    }

    #[test]
    fn the_source_is_relative_to_the_destination() {
        let b = Blit::new(5, 0x80, 0xC0, 0x40, 0x40, 0, 3, 0x40, 0);
        assert_eq!(b.src, [0xC0, 0xC0]);
    }
}
