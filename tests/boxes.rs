//! Pushable objects and level 1's door. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, leveldat, ngn, terrain};
use ts2::game::{Player, World};

/// Buzz walks into level 1's basement door (box 3, special 4) from its +Z side and pushes it
/// along path 4 (479 units towards −Z); it slides off its ledge and the level script tips it
/// open, swapping its collision from mover 1 to mover 14.
#[test]
fn level_1_door_is_pushed_over_and_opens() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let mut boxes = ts2::boxes::Boxes::new(1, &lists, Some(&e), &world, &mut specials);
    let mut level = ts2::level1::Level1::new(&world, &mut specials);
    assert!(world.mover_enabled(1) && !world.mover_enabled(14));

    let (x, z) = (4152.0, -7700.0);
    let y = world.floor_below(x, z, -1500.0).expect("floor in front of the door");
    let mut p = Player::new(Vec3::new(x, y, z));
    p.set_facing(0x800);
    let start = boxes.boxes[3].pos;
    let mut went_over = false;
    for tick in 0..900 {
        p.update_with(&world, ts2::game::Input { move_xz: [0.0, -1.0], ..Default::default() }, 1.0 / 60.0, &mut |p, w| {
            boxes.tick(p, w, &mut specials);
            level.tick(w, &mut specials, p, &boxes);
        });
        if boxes.going_over(3) && !went_over {
            went_over = true;
            eprintln!("over the edge after {tick} ticks, box at {:?}", boxes.boxes[3].pos.map(|v| v / 32));
        }
    }
    let end = boxes.boxes[3].pos;
    eprintln!("box 3 from {:?} to {:?}; Buzz at {:?}", start.map(|v| v / 32), end.map(|v| v / 32), p.pos);
    assert!(end[2] < start[2] - 400 * 32, "box 3 was not pushed along its path");
    assert!(went_over);
    assert!(world.mover_enabled(14) && !world.mover_enabled(1), "door collision not swapped");
    let angles = specials.angles(4).unwrap();
    let own = n.layers[0].instances[38].rot[0] as i32 >> 4;
    assert_eq!((angles[0] - own) & 0xFFF, 0x200, "door not tipped open");
}
