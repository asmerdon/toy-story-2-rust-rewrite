//! Checks the exe's player-start table against level geometry. Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::{anim, exe, ngn};
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

#[test]
fn buzz_move_table_matches_his_animations() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let exe = exe::Exe::parse(fs::read(dir.join("toy2.exe")).unwrap()).unwrap();
    let moves = exe.buzz_moves().unwrap();
    assert_eq!(moves.len(), exe::BUZZ_STATES);
    let n = ngn::parse(&fs::read(dir.join("data/level01/level.ngn")).unwrap()).unwrap();
    let buzz = n.creatures.iter().find(|c| c.id == 0).unwrap();
    let anims: Vec<anim::Animation> = buzz.animations.iter().map(|a| anim::parse(a).unwrap()).collect();
    let mut used = std::collections::BTreeSet::new();
    for (s, m) in moves.iter().enumerate() {
        for a in [m.anim, m.base_anim] {
            assert!((0..anims.len() as i32).contains(&a), "state {s}: animation {a} of {}", anims.len());
            used.insert(a);
        }
        let frames = anims[m.anim as usize].frame_count;
        for &f in m.frames.iter().filter(|&&f| f < 0x80) {
            assert!((f as u16) < frames, "state {s}: frame {f} >= {frames} in animation {}", m.anim);
        }
    }
    // The table uses every one of Buzz's animations.
    assert_eq!(used.len(), anims.len(), "unused animations: {used:?}");
}
