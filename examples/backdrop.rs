//! Diagnostic: writes a level's background picture (level.raw packet 36) as a PNG.
//! cargo run --release --example backdrop -- <levelNN dir> <out.png>
use std::{env, fs};
fn crc(d: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in d {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}
fn png(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * w as usize * 3..(y + 1) * w as usize * 3]);
    }
    let mut z = vec![0x78, 1];
    for (i, c) in raw.chunks(65535).enumerate() {
        let last = (i + 1) * 65535 >= raw.len();
        z.push(last as u8);
        z.extend((c.len() as u16).to_le_bytes());
        z.extend((!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |t: &[u8], d: &[u8]| {
        out.extend((d.len() as u32).to_be_bytes());
        let mut td = t.to_vec();
        td.extend_from_slice(d);
        out.extend_from_slice(&td);
        out.extend(crc(&td).to_be_bytes());
    };
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}
fn main() {
    let dir = std::path::PathBuf::from(env::args().nth(1).unwrap());
    let out = env::args().nth(2).unwrap();
    let pk = ts2::formats::packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let p = pk.iter().find(|p| p.kind() == 36).unwrap();
    let d = &p.data;
    let w = i16::from_le_bytes([d[8], d[9]]) as usize;
    let h = i16::from_le_bytes([d[10], d[11]]) as usize;
    let pal = i16::from_le_bytes([d[12], d[13]]) as usize / 2 * 2;
    let px = 0x10 + pal;
    let mut rgb = Vec::new();
    for i in 0..w * h {
        let c = d[px + i] as usize;
        rgb.extend_from_slice(&d[0x10 + 3 * c..0x13 + 3 * c]);
    }
    fs::write(&out, png(w as u32, h as u32, &rgb)).unwrap();
    println!("{w}x{h}, palette {pal} bytes, {} bytes of data", d.len());
}
