//! level.dat point lists. Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::leveldat;

#[test]
fn level_dat_lists_parse_and_poles_hang_in_the_air() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let mut total = 0;
    for nn in 1..=10 {
        for stem in ["level", "level1"] {
            let Ok(b) = fs::read(dir.join(format!("data/level{nn:02}/{stem}.dat"))) else { continue };
            let lists = leveldat::parse(&b).unwrap_or_else(|e| panic!("level{nn:02}/{stem}: {e}"));
            for p in leveldat::poles(&lists) {
                assert!(p.length > 0 && (0..=3).contains(&p.kind), "level{nn:02}/{stem}: {p:?}");
                total += 1;
            }
        }
    }
    eprintln!("{total} poles");
    assert!(total > 10);
}
