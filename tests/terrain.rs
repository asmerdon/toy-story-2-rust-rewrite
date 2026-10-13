//! Collision files (`TERRAIN.ALL` / `TERR1.ALL`). Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, ngn, terrain};
use ts2::{game, scene};

#[test]
fn every_terrain_file_parses_and_lies_under_the_start() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else {
        eprintln!("TS2_GAME_DIR not set; skipping");
        return;
    };
    let exe = exe::Exe::parse(fs::read(dir.join("toy2.exe")).unwrap()).unwrap();
    let mut checked = 0;
    for nn in 1..=10 {
        for stem in ["level", "level1"] {
            let level_path = dir.join(format!("data/level{nn:02}/{stem}.ngn"));
            if !level_path.exists() {
                continue;
            }
            let path = terrain::path_for(&level_path).unwrap();
            let Ok(bytes) = fs::read(&path) else { continue };
            let t = terrain::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let tris = t.static_triangles();
            assert!(!tris.is_empty(), "{}: no collision", path.display());
            let level = exe::level_number(&level_path).unwrap();
            let Ok(start) = exe.player_start(level) else { continue };
            let [x, y, z] = start.pos;
            let world = game::World::from_terrain(&t);
            let render = game::World::from_batches(&scene::level_batches(&ngn::parse(&fs::read(&level_path).unwrap()).unwrap()));
            let (cf, rf) = (world.floor_below(x, z, y - 2000.0), render.floor_below(x, z, y - 2000.0));
            eprintln!("level {level:2}: {} objects, {} triangles; floor under start: collision {cf:?}, render {rf:?}", t.objects.len(), tris.len());
            let cf = cf.unwrap_or_else(|| panic!("level {level}: no collision floor under the start"));
            if let Some(rf) = rf {
                assert!((cf - rf).abs() < 64.0, "level {level}: collision floor {cf} vs render floor {rf}");
            }
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} levels checked");
}
