//! Diagnostic: rows of Buzz's move table (0x4DF3F0): `moverow <state>...` (TS2_GAME_DIR).
fn main() {
    let dir = std::path::PathBuf::from(std::env::var_os("TS2_GAME_DIR").expect("TS2_GAME_DIR"));
    let exe = ts2::formats::exe::Exe::parse(std::fs::read(dir.join("toy2.exe")).unwrap()).unwrap();
    let moves = exe.buzz_moves().unwrap();
    for s in std::env::args().skip(1).filter_map(|a| a.parse::<usize>().ok()) {
        let m = &moves[s];
        println!("state {s}: overlay {} base {} rate {:#x} frames {:?} end {}", m.anim, m.base_anim, m.rate_raw, m.frames, m.end);
    }
}
