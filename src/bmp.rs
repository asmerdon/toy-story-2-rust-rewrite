//! Minimal BMP decoder for the textures embedded in `.ngn` files (24-bit and 8-bit palettised,
//! uncompressed). Output is RGBA8, top row first. Pure black becomes transparent, following the
//! PS1 convention that colour 0 is see-through.

#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

pub fn decode(b: &[u8], black_is_transparent: bool) -> Option<Image> {
    if b.get(0..2)? != b"BM" {
        return None;
    }
    let data_off = u32_at(b, 10)? as usize;
    let header_size = u32_at(b, 14)? as usize;
    let width = u32_at(b, 18)? as i32;
    let raw_height = u32_at(b, 22)? as i32;
    let bpp = u16_at(b, 28)?;
    let compression = u32_at(b, 30)?;
    if compression != 0 || width <= 0 || raw_height == 0 {
        return None;
    }
    let (w, h) = (width as usize, raw_height.unsigned_abs() as usize);
    let bottom_up = raw_height > 0;
    let palette: Vec<[u8; 3]> = if bpp == 8 {
        let colours = match u32_at(b, 46)? {
            0 => 256,
            n => n as usize,
        };
        let p = 14 + header_size;
        (0..colours).map(|i| b.get(p + 4 * i..p + 4 * i + 3).map(|c| [c[2], c[1], c[0]])).collect::<Option<_>>()?
    } else {
        Vec::new()
    };
    let row_bytes = (w * bpp as usize / 8).next_multiple_of(4);
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        let src_row = if bottom_up { h - 1 - y } else { y };
        let row = b.get(data_off + src_row * row_bytes..data_off + (src_row + 1) * row_bytes)?;
        for x in 0..w {
            let [r, g, bl] = match bpp {
                24 => [row[3 * x + 2], row[3 * x + 1], row[3 * x]],
                8 => *palette.get(row[x] as usize)?,
                _ => return None,
            };
            let a = if black_is_transparent && r == 0 && g == 0 && bl == 0 { 0 } else { 255 };
            rgba[(y * w + x) * 4..][..4].copy_from_slice(&[r, g, bl, a]);
        }
    }
    Some(Image { width: w as u32, height: h as u32, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_2x2_24bit_bottom_up() {
        // 2x2, rows padded to 8 bytes. Bottom row: blue, black. Top row: red, white.
        let mut b = vec![0u8; 54];
        b[0..2].copy_from_slice(b"BM");
        b[10..14].copy_from_slice(&54u32.to_le_bytes());
        b[14..18].copy_from_slice(&40u32.to_le_bytes());
        b[18..22].copy_from_slice(&2i32.to_le_bytes());
        b[22..26].copy_from_slice(&2i32.to_le_bytes());
        b[28..30].copy_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(&[255, 0, 0, 0, 0, 0, 0, 0]); // bottom: blue (BGR), black
        b.extend_from_slice(&[0, 0, 255, 255, 255, 255, 0, 0]); // top: red, white
        let img = decode(&b, true).unwrap();
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(&img.rgba[0..4], &[255, 0, 0, 255]); // top-left red
        assert_eq!(&img.rgba[4..8], &[255, 255, 255, 255]);
        assert_eq!(&img.rgba[8..12], &[0, 0, 255, 255]); // bottom-left blue
        assert_eq!(&img.rgba[12..16], &[0, 0, 0, 0]); // black -> transparent
    }
}
