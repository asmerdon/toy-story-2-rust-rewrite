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
