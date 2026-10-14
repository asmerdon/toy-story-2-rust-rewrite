//! Level 1's see-saw in room 5 (mover and special 0xF). Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, leveldat, ngn, terrain};
use ts2::game::{Player, World};

/// Left alone the plank leans to its positive end; Buzz standing out on the other end tips it
/// his way, and he rides it down.
#[test]
fn see_saw_tips_towards_buzz() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let mut level = ts2::level1::Level1::new(&world, &mut specials);
    let boxes = ts2::boxes::Boxes::new(1, &lists, Some(&e), &world, &mut specials);
    let pivot = specials.pos_obj(0xF).map(|p| p.map(|v| v >> 5)).expect("special 0xF");
    eprintln!("see-saw special at {pivot:?}");
    // Someone in room 5 away from the plank: it settles leaning one way.
    let mut idle = Player::new(Vec3::new(15000.0, -5925.0, -4499.0));
    idle.on_floor = true;
    for _ in 0..600 {
        level.tick(&world, &mut specials, &mut idle, &boxes);
    }
    let rest = specials.angles(0xF).unwrap()[2];
    eprintln!("at rest: tilt {rest:#x}");
    assert!(rest > 0x100, "leans to its positive end");
    let height = |x: f32| world.floor_below(x, pivot[2] as f32, pivot[1] as f32 - 2000.0);
    let (lo, hi) = (height(pivot[0] as f32 - 1200.0), height(pivot[0] as f32 + 1200.0));
    eprintln!("ends: -x {lo:?}, +x {hi:?}");
    // +y is down: the +x end is the lower one.
    assert!(hi.unwrap() > lo.unwrap() + 300.0);

    // Buzz drops onto the raised (−x) end and stays there.
    let x = pivot[0] as f32 - 900.0;
    let top = height(x).unwrap();
    let mut p = Player::new(Vec3::new(x, top - 200.0, pivot[2] as f32));
    for _ in 0..400 {
        p.tick(&world, [0.0; 2], false, false);
        level.tick(&world, &mut specials, &mut p, &boxes);
    }
    let after = (specials.angles(0xF).unwrap()[2] << 20) >> 20;
    eprintln!("with Buzz on the -x end: tilt {after:#x}, Buzz at {:?} on {:?}", p.pos, p.floor_mover);
    assert!(after < -0x100, "tipped his way");
    assert!(p.pos.y > lo.unwrap() + 300.0, "he rode it down");
}
