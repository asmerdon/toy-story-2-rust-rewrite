//! Checks the exe's player-start table against level geometry. Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, ngn};
use ts2::{game, scene};

#[test]
fn every_player_start_is_above_a_floor() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else {
        eprintln!("TS2_GAME_DIR not set; skipping");
        return;
    };
    let exe = exe::Exe::parse(fs::read(dir.join("toy2.exe")).unwrap()).unwrap();
    let mut checked = 0;
    for nn in 1..=10 {
        for stem in ["level", "level1"] {
            let path = dir.join(format!("data/level{nn:02}/{stem}.ngn"));
            let Ok(bytes) = fs::read(&path) else { continue };
            let level = exe::level_number(&path).unwrap();
            let Ok(start) = exe.player_start(level) else { continue };
            let n = ngn::parse(&bytes).unwrap();
            let world = game::World::from_batches(&scene::level_batches(&n));
            let [x, y, z] = start.pos;
            // The game snaps to the floor below; allow a generous search window above the point.
            let floor = world.floor_below(x, z, y - 2000.0);
            eprintln!("level {level:2} ({}) start {:?} floor {:?}", path.display(), start.pos, floor);
            assert!(floor.is_some_and(|f| (f - y).abs() < 2000.0), "level {level}: no floor near start {:?}", start.pos);
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} levels checked");
}
