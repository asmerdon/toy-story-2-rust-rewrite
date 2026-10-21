//! Diagnostic: writes a level's background picture (level.raw packet 36) as a PNG.
//! cargo run --release --example backdrop -- <levelNN dir> <out.png>
use std::{env, fs};
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
    fs::write(&out, ts2::png::encode_rgb(w as u32, h as u32, &rgb)).unwrap();
    println!("{w}x{h}, palette {pal} bytes, {} bytes of data", d.len());
}
