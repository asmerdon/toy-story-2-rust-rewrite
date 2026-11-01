//! Level 13 (Airport Infiltration, data\level03\level1.*). Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::terrain;
use ts2::game::{Input, Player, World};

/// Runs forward for a second, lets go, and returns how far he slides and whether he was on a
/// slick floor.
fn run_and_stop(world: &World, at: Vec3) -> (f32, bool) {
    let mut p = Player::new(at);
    let dt = 1.0 / 60.0;
    for _ in 0..20 {
        p.update(world, Input::default(), dt);
    }
    for _ in 0..60 {
        p.update(
            world,
            Input {
                move_xz: [0.0, 1.0],
                ..Default::default()
            },
            dt,
        );
    }
    let slick = p.slick();
    let from = p.pos;
    for _ in 0..240 {
        p.update(world, Input::default(), dt);
    }
    (
        Vec3::new(p.pos.x - from.x, 0.0, p.pos.z - from.z).length(),
        slick,
    )
}

/// The oil (surface type 5) has little grip (0x436220, types 4–7): let go at a run and Buzz
/// slides on far further than on an ordinary floor.
#[test]
fn oil_is_slippery() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else {
        return;
    };
    let t = terrain::parse(&fs::read(game.join("data/level03/TERR1.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let (oil, slick) = run_and_stop(&world, Vec3::new(6000.0, -60.0, -21500.0));
    assert!(slick, "on the oil");
    let flat = World::from_triangles(vec![
        [
            Vec3::new(-1e5, 0.0, -1e5),
            Vec3::new(1e5, 0.0, 1e5),
            Vec3::new(1e5, 0.0, -1e5),
        ],
        [
            Vec3::new(-1e5, 0.0, -1e5),
            Vec3::new(-1e5, 0.0, 1e5),
            Vec3::new(1e5, 0.0, 1e5),
        ],
    ]);
    let (floor, _) = run_and_stop(&flat, Vec3::ZERO);
    eprintln!("slides {oil:.0} on oil, {floor:.0} on a floor");
    assert!(oil > floor * 3.0, "oil {oil}, floor {floor}");
}
