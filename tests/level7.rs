//! Level 7's script. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
use ts2::game::{Player, World};
use ts2::script::Script;

#[allow(dead_code)]
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

/// Drops Buzz onto mover `m`'s top (above `at`, level units), running the script's pre-move
/// part each tick; returns him once he stands on it.
fn land_on(l: &mut L2, m: i32, at: Vec3) -> Player {
    let mut p = Player::new(at + Vec3::new(0.0, -300.0, 0.0));
    for _ in 0..240 {
        p.update_with(&l.world, ts2::game::Input::default(), 1.0 / 60.0, &mut |p, w| {
            l.script.step(w, &mut l.specials, p, &l.boxes);
        });
        if p.on_floor && p.floor_mover.is_some_and(|(id, _)| id == m) {
            return p;
        }
    }
    panic!("Buzz never stood on mover {m}: at {:?} on_floor {} {:?}", p.pos, p.on_floor, p.floor_mover);
}

/// Skateboard 1 (mover 10, special 0xB): Buzz landing on it starts it rolling towards −z; the
/// special follows it; from z = 0x42C60 it falls and bounces; below 0x104C0 it stops and
/// throws a rider.
#[test]
fn skateboard_one_rolls_when_landed_on() {
    let Some(mut l) = load() else { return };
    let home = l.world.mover_pos(10).unwrap();
    let mut p = land_on(&mut l, 10, home);
    let mut low = home;
    let mut thrown = false;
    for tick in 0..2400 {
        p.update_with(&l.world, ts2::game::Input::default(), 1.0 / 60.0, &mut |p, w| {
            l.script.step(w, &mut l.specials, p, &l.boxes);
        });
        let at = l.world.mover_pos(10).unwrap();
        low = low.max(at);
        if tick == 120 {
            assert!(at.z < home.z - 100.0, "the skateboard did not roll: {at:?}");
            let s = l.specials.pos_obj(0xB).unwrap().map(|v| v >> 5);
            assert!((s[2] as f32 - at.z).abs() < 80.0, "special 0xB did not follow: {s:?} vs {at:?}");
        }
        thrown |= !p.on_floor && p.vel[1] < -0x800 && at.z * 32.0 < 0x104C0 as f32 + 200.0;
    }
    let end = l.world.mover_pos(10).unwrap();
    eprintln!("skateboard 1: {home:?} -> {end:?}, thrown {thrown}");
    assert!(end.z * 32.0 < 0x104C0 as f32 + 0x6A4 as f32, "it did not reach its stop: {end:?}");
    assert!(end.y > home.y + 1000.0, "it did not fall: {end:?}");
    assert!(thrown, "Buzz was not thrown at the end");
}

/// Skateboard 2 (mover 0xE, special 0x18) rolls to z = 0x2300 (level units), turns round, rolls
/// back and rests again.
#[test]
fn skateboard_two_rolls_and_returns() {
    let Some(mut l) = load() else { return };
    let home = l.world.mover_pos(0xE).unwrap();
    let mut p = land_on(&mut l, 0xE, home);
    let mut far = home;
    for _ in 0..2400 {
        p.update_with(&l.world, ts2::game::Input::default(), 1.0 / 60.0, &mut |p, w| {
            l.script.step(w, &mut l.specials, p, &l.boxes);
        });
        let at = l.world.mover_pos(0xE).unwrap();
        far = far.min(at);
    }
    let end = l.world.mover_pos(0xE).unwrap();
    eprintln!("skateboard 2: {home:?} -> far {far:?} -> {end:?}");
    assert!(far.z < 0x2300 as f32 + 60.0, "it did not roll far enough: {far:?}");
    assert!(end.z > far.z + 1000.0, "it did not come back: {end:?}");
}
