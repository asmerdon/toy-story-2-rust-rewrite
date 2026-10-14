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

/// The attic box (box 6, path 0: 2250 units towards −X, then 2625 towards −Z). Buzz pushes it
/// to the corner, lets go, walks round to its +Z side and pushes it on; he never gets stuck.
#[test]
fn attic_box_goes_round_its_corner() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let mut boxes = ts2::boxes::Boxes::new(1, &lists, Some(&e), &world, &mut specials);
    let b = |boxes: &ts2::boxes::Boxes| Vec3::from(boxes.boxes[6].pos.map(|v| v as f32 / 32.0));
    let start = b(&boxes);
    let y = world.floor_below(start.x + 450.0, start.z, start.y - 1500.0).expect("attic floor");
    let mut p = Player::new(Vec3::new(start.x + 450.0, y, start.z));
    p.set_facing(0xC00);
    let mut run = |p: &mut Player, boxes: &mut ts2::boxes::Boxes, stick: [f32; 2], ticks: usize| {
        let mut moved = 0.0f32;
        for _ in 0..ticks {
            let before = p.pos;
            p.update_with(&world, ts2::game::Input { move_xz: stick, ..Default::default() }, 1.0 / 60.0, &mut |p, w| {
                boxes.tick(p, w, &mut specials);
            });
            moved += (p.pos - before).length();
        }
        moved
    };
    run(&mut p, &mut boxes, [-1.0, 0.0], 400);
    let corner = b(&boxes);
    eprintln!("after pushing −X: box {:?} (start {:?}), Buzz {:?}", corner, start, p.pos);
    assert!((corner.x - 15375.0).abs() < 40.0, "box didn't reach the corner");
    // Let go, walk round: +Z, −X past the box, then push −Z.
    run(&mut p, &mut boxes, [0.0, 0.0], 5);
    assert!(run(&mut p, &mut boxes, [1.0, 0.0], 20) > 100.0, "stuck after letting go");
    // Round the corner by 500 units of +Z.
    while p.pos.z < -8850.0 {
        run(&mut p, &mut boxes, [0.0, 1.0], 1);
    }
    while p.pos.x > 15375.0 {
        run(&mut p, &mut boxes, [-1.0, 0.0], 1);
    }
    run(&mut p, &mut boxes, [0.0, -1.0], 40);
    eprintln!("Buzz round the side at {:?}", p.pos);
    run(&mut p, &mut boxes, [0.0, -1.0], 500);
    let end = b(&boxes);
    eprintln!("after pushing −Z: box {:?}, Buzz {:?}", end, p.pos);
    assert!(end.z < corner.z - 1000.0, "box didn't go round the corner");
}
