//! Level 7's script. Skipped unless TS2_GAME_DIR is set.

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
    let dir = game.join("data/level07");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let script = ts2::script::for_level(7, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let boxes = ts2::boxes::Boxes::new(7, &lists, Some(&e), &world, &mut specials);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.load_shapes(&game.join("data"));
    Some(L2 { world, specials, script, boxes, cr })
}

/// In room 4 the dino's challenge starts; beaten (below 10 health) its fight ends and token
/// 4 appears.
#[test]
fn dino_fight() {
    let Some(mut l) = load() else { return };
    assert_eq!(l.cr.slots[0].id, 0x1A);
    let d = l.cr.slots[0].pos;
    let at = Vec3::from(d.map(|v| v as f32 / 32.0));
    eprintln!("dino at {at:?} room {:?}", l.world.room_at(at));
    let mut p = Player::new(at);
    p.on_floor = true;
    let mut hits = 0;
    for tick in 0..6000 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        l.cr.hurts.clear();
        if l.script.cutscene.is_some() {
            l.script.cutscene = None;
        }
        if l.cr.boss.state == 2 && tick % 20 == 0 && l.cr.slots[0].rec.state == 7 {
            l.cr.hit(0, 0, 3);
            hits += 1;
        }
        if l.script.reveals.contains(&4) {
            break;
        }
    }
    eprintln!("{hits} hits, state {}, health {}", l.cr.boss.state, l.cr.slots[0].health);
    assert!(l.script.reveals.contains(&4), "token 4");
}

/// The first three path movers run from the start.
#[test]
fn paths_run() {
    let Some(mut l) = load() else { return };
    let p = Player::new(Vec3::new(0.0, -100000.0, 0.0));
    let start: Vec<Vec3> = [7, 8, 9].iter().map(|&m| l.world.mover_pos(m).unwrap()).collect();
    let mut far = [0.0f32; 3];
    for _ in 0..1200 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        for (k, m) in [7, 8, 9].into_iter().enumerate() {
            far[k] = far[k].max((l.world.mover_pos(m).unwrap() - start[k]).length());
        }
    }
    eprintln!("furthest {far:?}");
    assert!(far.iter().all(|&d| d > 50.0));
}
