//! Diagnostic: Buzz's move table (state: overlay anim, base anim, list).
fn main() {
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").unwrap());
    let e = ts2::formats::exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    for (i, m) in e.buzz_moves().unwrap().iter().enumerate() {
        println!("state {i:#04x}: overlay {} base {} frames {:?}", m.anim, m.base_anim, &m.frames[..m.frames.len().min(8)]);
    }
}
