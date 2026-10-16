//! Diagnostic: each level's background picture (level.raw packet 36): size, horizon offset
//! and the sky / ground colours the PC version clears with.
use std::{env, fs, path::PathBuf};
fn main() {
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    for lv in 1..=10 {
        let dir = game.join(format!("data/level{lv:02}"));
        let Ok(raw) = fs::read(dir.join("level.raw")) else { continue };
        let pk = ts2::formats::packets::parse(&raw).unwrap();
        for p in pk.iter().filter(|p| p.kind() == 36) {
            println!("level {lv}: {:?}", ts2::formats::packets::backdrop(p));
        }
    }
}
