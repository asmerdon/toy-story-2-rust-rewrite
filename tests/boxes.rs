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

/// Level 1's ledges (level.dat list 60, 2750 units up the basement walls): falling just below
/// one, Buzz grabs it (0x435100), hangs 512 below it for 53 ticks without moving, then pulls
/// himself up (vy -0x600).
#[test]
fn level_1_ledge_is_grabbed_and_climbed() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let ledges = leveldat::segments(&lists, leveldat::LEDGES);
    assert_eq!(ledges.len(), 7);
    let mut climbed = 0;
    for (k, [a, b]) in ledges.iter().enumerate() {
        let mid = Vec3::new((a[0] + b[0]) as f32 / 64.0, a[1] as f32 / 32.0, (a[2] + b[2]) as f32 / 64.0);
        let along = Vec3::new((b[0] - a[0]) as f32, 0.0, (b[2] - a[2]) as f32).normalize();
        for side in [1.0f32, -1.0] {
            let n = Vec3::new(-along.z, 0.0, along.x) * side;
            let mut p = Player::new(mid + n * 40.0 + Vec3::Y * 300.0);
            p.set_ledges(ledges.clone(), Vec::new());
            p.set_facing(ts2::boxes::angle12(-n.x, -n.z));
            let (mut grabbed, mut hung, mut pulled) = (false, 0, false);
            for _ in 0..150 {
                p.update(&world, ts2::game::Input::default(), 1.0 / 60.0);
                grabbed |= p.ledge > 0;
                if p.ledge > 0x10 && p.vel == [0; 3] && p.pos.y > mid.y + 300.0 && p.pos.y < mid.y + 513.0 {
                    hung += 1;
                }
                pulled |= p.ledge > 0 && p.ledge < 0x10 && p.vel[1] < 0;
            }
            eprintln!("ledge {k} side {side}: grabbed {grabbed}, hung {hung} ticks, pulled up {pulled}");
            climbed += (grabbed && hung > 10 && pulled) as usize;
        }
    }
    assert_eq!(climbed, 14, "only {climbed} climbs");
}

/// Level 1's zip lines (list 62): dropping onto one near its top end, Buzz is caught under
/// it, rides it down, speeding up, and lets go at the far end with an upward kick.
#[test]
fn level_1_zip_line_is_ridden() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let zips = leveldat::segments(&lists, leveldat::ZIP_LINES);
    assert_eq!(zips.len(), 2);
    for (k, [a, b]) in zips.iter().enumerate() {
        let (a, b) = (Vec3::from(a.map(|v| v as f32 / 32.0)), Vec3::from(b.map(|v| v as f32 / 32.0)));
        let start = a + (b - a) * 0.1 + Vec3::Y * 200.0;
        let mut p = Player::new(start);
        p.set_ledges(Vec::new(), leveldat::segments(&lists, leveldat::ZIP_LINES));
        let (mut rode, mut let_go) = (0, false);
        for _ in 0..600 {
            let before = p.zip;
            p.update(&world, ts2::game::Input::default(), 1.0 / 60.0);
            rode += (p.zip == 2) as usize;
            let_go |= before == 2 && p.zip == 0 && p.vel[1] < 0;
            if let_go {
                break;
            }
        }
        let end = (p.pos - b).length();
        eprintln!("zip {k}: rode {rode} ticks, let go {let_go}, {end:.0} from its end");
        assert!(rode > 20 && let_go && end < 800.0, "zip line {k} not ridden");
    }
}
