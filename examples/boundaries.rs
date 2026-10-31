//! Diagnostic: every non-collision record (kind >= 0x100) in every TERRAIN.ALL / TERR*.ALL,
//! with the 0x12345678 lists 0x489980 chains at 0x7290F0 + (kind - 0x100) * 4, dumped raw.
//! cargo run --release --example boundaries (TS2_GAME_DIR set)
use std::{env, fs, path::PathBuf};
fn main() {
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let mut files: Vec<PathBuf> = Vec::new();
    for e in fs::read_dir(game.join("data")).unwrap().flatten() {
        if e.path().is_dir() {
            for f in fs::read_dir(e.path()).unwrap().flatten() {
                let n = f.file_name().to_string_lossy().to_ascii_uppercase();
                if n.starts_with("TERR") && n.ends_with(".ALL") {
                    files.push(f.path());
                }
            }
        }
    }
    files.sort();
    for f in files {
        let raw = fs::read(&f).unwrap();
        let d = u32::from_le_bytes(raw[0..4].try_into().unwrap()) as usize * 2;
        let n = i32::from_le_bytes(raw[d..d + 4].try_into().unwrap());
        let mut cursor = 4usize;
        for k in 0..n as usize {
            let r = d + 4 + 76 * k;
            let i = |o: usize| i32::from_le_bytes(raw[r + o..r + o + 4].try_into().unwrap());
            let size = i(0) as usize * 2;
            let kind = i(16);
            if kind >= 0x100 {
                let b = &raw[cursor..cursor + size];
                let w = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
                let magic = if b.len() >= 4 { w(0) as u32 } else { 0 };
                println!("{}: rec {k} kind {kind:#x} size {size} bytes magic {magic:#x}", f.display());
                if magic == 0x12345678 && b.len() >= 8 {
                    let count = u16::from_le_bytes([b[4], b[5]]) as usize;
                    let pts = (b.len() - 8) / 12;
                    println!("   count {count} ({:#06x} after), {pts} points", u16::from_le_bytes([b[6], b[7]]));
                    for p in 0..pts {
                        let o = 8 + 12 * p;
                        println!("   {p:>3}: {:>8} {:>12} {:>8}", w(o), w(o + 4), w(o + 8));
                    }
                }
            }
            cursor += size;
        }
    }
}
