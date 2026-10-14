//! Level 1's room gadgets: room 2's lift and room 3's spring. Skipped unless TS2_GAME_DIR is
//! set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, leveldat, ngn, terrain};
use ts2::game::{Player, World};

struct Level {
    world: World,
    specials: ts2::specials::Specials,
    level: ts2::level1::Level1,
    boxes: ts2::boxes::Boxes,
    poles: Vec<leveldat::Pole>,
}

fn load() -> Option<Level> {
    let game = env::var_os("TS2_GAME_DIR").map(PathBuf::from)?;
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let mut level = ts2::level1::Level1::new(&world, &mut specials);
    level.set_story(&e, &lists);
    let boxes = ts2::boxes::Boxes::new(1, &lists, Some(&e), &world, &mut specials);
    Some(Level { world, specials, level, boxes, poles: leveldat::poles(&lists) })
}

/// Climbing pole 8 in room 2 sets the lift going: special 6 and the pole sink together until
/// 0xB800 below where they started.
#[test]
fn lift_sinks_with_its_pole() {
    let Some(mut l) = load() else { return };
    let pole = l.poles[8];
    let lift = l.specials.pos_obj(6).unwrap();
    let at = Vec3::from(pole.pos.map(|v| v as f32 / 32.0));
    eprintln!("pole 8 at {at:?} length {}, room {:?}; special 6 at {:?}", pole.length >> 5, l.world.room_at(at), lift.map(|v| v >> 5));
    let mut p = Player::new(at - Vec3::Y * 200.0);
    p.set_poles(l.poles.clone());
    for _ in 0..300 {
        p.tick(&l.world, [0.0; 2], false, false);
        l.level.tick(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    let now = l.specials.pos_obj(6).unwrap();
    eprintln!("on pole {:?}; special 6 now {:?}, pole bottom {}", p.pole_held(), now.map(|v| v >> 5), p.pole(8).unwrap().pos[1] >> 5);
    assert_eq!(now[1] - lift[1], 0xB800, "sunk all the way");
    assert_eq!(p.pole(8).unwrap().pos[1] - pole.pos[1], 0xB800, "the pole went with it");
}

/// Stomping onto room 3's spring throws Buzz high and far along 0x11E.
#[test]
fn spring_throws_buzz() {
    let Some(mut l) = load() else { return };
    let s = l.specials.pos_obj(0x17).unwrap().map(|v| v as f32 / 32.0);
    let g = Vec3::from(s);
    let y = l.world.floor_below(g.x, g.z, g.y - 300.0).unwrap();
    eprintln!("spring special at {g:?}, floor {y} room {:?}", l.world.room_at(Vec3::new(g.x, y, g.z)));
    let mut p = Player::new(Vec3::new(g.x, y, g.z));
    let (mut top, mut thrown) = (f32::MAX, false);
    for tick in 0..400 {
        // Jump, then stomp back down onto it.
        p.tick(&l.world, [0.0; 2], (2..12).contains(&tick), tick == 20);
        l.level.tick(&l.world, &mut l.specials, &mut p, &l.boxes);
        thrown |= p.launched;
        top = top.min(p.pos.y);
    }
    let moved = Vec3::new(p.pos.x - g.x, 0.0, p.pos.z - g.z).length();
    eprintln!("thrown {thrown}, highest {top} ({} above), landed {moved} away at {:?}", y - top, p.pos);
    assert!(thrown);
    assert!(y - top > 800.0, "high");
    assert!(moved > 500.0, "far");
}
