//! Level 4's script. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
use ts2::game::{Player, World};
use ts2::script::Script;

struct L2 {
    world: World,
    specials: ts2::specials::Specials,
    script: Box<dyn Script>,
    boxes: ts2::boxes::Boxes,
    cr: Creatures,
}

fn load() -> Option<L2> {
    let game = env::var_os("TS2_GAME_DIR").map(PathBuf::from)?;
    let dir = game.join("data/level04");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let script = ts2::script::for_level(4, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let boxes = ts2::boxes::Boxes::new(4, &lists, Some(&e), &world, &mut specials);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.load_shapes(&game.join("data"));
    Some(L2 { world, specials, script, boxes, cr })
}

/// The hanging platforms follow their paths (and stop under Buzz); the shuttles move too.
#[test]
fn platforms_follow_their_paths() {
    let Some(mut l) = load() else { return };
    let start: Vec<Vec3> = [1, 2, 3, 4, 0xE, 0x10].iter().map(|&m| l.world.mover_pos(m).unwrap()).collect();
    let mut p = Player::new(Vec3::new(0.0, -100000.0, 0.0));
    let mut far = [0.0f32; 6];
    for _ in 0..1200 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        for (k, m) in [1, 2, 3, 4, 0xE, 0x10].into_iter().enumerate() {
            far[k] = far[k].max((l.world.mover_pos(m).unwrap() - start[k]).length());
        }
    }
    eprintln!("furthest: {far:?}");
    assert!(far.iter().all(|&d| d > 50.0), "a mover didn't move");
}

/// Down in the pit near the drill, its challenge starts; after it the fight runs.
#[test]
fn drill_challenge() {
    let Some(mut l) = load() else { return };
    let d = l.cr.slots[0x18].pos;
    assert_eq!(l.cr.slots[0x18].id, 0x16);
    let mut p = Player::new(Vec3::from(d.map(|v| v as f32 / 32.0)));
    p.on_floor = true;
    eprintln!("drill at {:?}", d.map(|v| v >> 5));
    for _ in 0..3 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.cr.boss.state, 1, "challenged");
    l.script.cutscene = None;
    for _ in 0..30 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.cr.boss.state, 2);
}

/// The paint-room door's hatch (specials 0x3F / 0x40, 0x41C640) folds up to nothing while Buzz
/// is near it and drops back when he leaves, 0x200 of 0x1000 a tick.
#[test]
fn paint_room_hatch_opens_for_buzz() {
    let Some(mut l) = load() else { return };
    let tall = |l: &L2, id: i32| l.specials.get(id).unwrap().scale[1];
    let shut = [tall(&l, 0x3F), tall(&l, 0x40)];
    assert!(shut.iter().all(|&s| s > 0.0), "drawn shut at the start: {shut:?}");
    // Its bottom edge, in level units (0x51B97, −0x104FD, 0x5E58E object units).
    let at = Vec3::new(0x51B97 as f32, -0x104FD as f32, 0x5E58E as f32) / 32.0;
    let mut p = Player::new(at - Vec3::Z * 300.0);
    for _ in 0..8 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    assert_eq!([tall(&l, 0x3F), tall(&l, 0x40)], [0.0; 2], "open after 8 ticks near it");
    p.pos = at - Vec3::Z * 2500.0;
    for _ in 0..4 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    let half = [tall(&l, 0x3F), tall(&l, 0x40)];
    assert!((half[0] - shut[0] / 2.0).abs() < 1e-3 && (half[1] - shut[1] / 2.0).abs() < 1e-3, "half shut 4 ticks after leaving: {half:?} of {shut:?}");
    for _ in 0..8 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    assert_eq!([tall(&l, 0x3F), tall(&l, 0x40)], shut, "shut again");
}

/// The paint room (room 2) is drawn from outside only through its door's hatch (portal 0,
/// room 1 → 2), and the yard (room 1) from inside through the hatch and the window (0x4BC460).
#[test]
fn paint_room_seen_only_through_its_portals() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let bytes = fs::read(game.join("data/level04/level.ngn")).unwrap();
    let portals = ts2::scene::Portals::parse(&bytes);
    assert_eq!(portals.links.iter().filter(|l| l.to != 15).count(), 3, "{:?}", portals.links);
    // The viewer's camera: game (+Y down) to world, 60° reversed-depth perspective.
    let view = |at: [f32; 3], yaw: f32, pitch: f32| {
        let pos = Vec3::new(at[0], -at[1], -at[2]) / 256.0;
        let f = Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos());
        let vp = glam::Mat4::perspective_infinite_reverse_rh(60f32.to_radians(), 16.0 / 9.0, 0.05) * glam::Mat4::look_to_rh(pos, f, Vec3::Y);
        move |p: [f32; 3]| vp * glam::Vec4::new(p[0] / 256.0, -p[1] / 256.0, -p[2] / 256.0, 1.0)
    };
    let rooms = |from: u32, at: [f32; 3], yaw: f32, pitch: f32| portals.visible_rooms(from, view(at, yaw, pitch), false);
    // Outside, facing the door (+z): the hatch is in view.
    assert_eq!(rooms(1, [10500.0, -2400.0, 9000.0], 0.0, 0.0), 0b111);
    // From far off in the yard, the door in view.
    assert_eq!(rooms(1, [10500.0, -8000.0, 2000.0], 0.0, -0.35), 0b111);
    // Outside, facing away from the door.
    assert_eq!(rooms(1, [10500.0, -2400.0, 9000.0], std::f32::consts::PI, 0.0), 0b011);
    // Behind the cabin (its far side, +z beyond the room), looking back at it: no portal faces.
    assert_eq!(rooms(1, [10000.0, -4000.0, 36000.0], std::f32::consts::PI, -0.2), 0b011);
    // Inside, facing the door: the yard through the hatch and the window.
    assert_eq!(rooms(2, [9500.0, -4000.0, 19050.0], std::f32::consts::PI, 0.0), 0b111);
    // Inside, facing the back wall: the yard isn't drawn.
    assert_eq!(rooms(2, [9500.0, -4000.0, 19050.0], 0.0, 0.0), 0b101);
}
