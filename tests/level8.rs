//! Level 8's script. Skipped unless TS2_GAME_DIR is set.

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
    let dir = game.join("data/level08");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let script = ts2::script::for_level(8, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let boxes = ts2::boxes::Boxes::new(8, &lists, Some(&e), &world, &mut specials);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.load_shapes(&game.join("data"));
    Some(L2 { world, specials, script, boxes, cr })
}

/// In room 5 the boss's challenge starts; it fires and, beaten, gives token 4.
#[test]
fn boss_fight() {
    let Some(mut l) = load() else { return };
    assert_eq!(l.cr.slots[40].id, 0x2F);
    let at = Vec3::from(l.cr.slots[40].pos.map(|v| v as f32 / 32.0));
    eprintln!("boss at {at:?} room {:?}", l.world.room_at(at));
    let mut p = Player::new(at);
    p.on_floor = true;
    let (mut hits, mut beams) = (0, 0);
    for tick in 0..8000 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        l.cr.hurts.clear();
        beams += l.cr.ribbons.drain(..).count();
        if l.script.cutscene.is_some() {
            l.script.cutscene = None;
        }
        if beams > 0 && l.cr.boss.state == 2 && tick % 20 == 0 && l.cr.slots[40].rec.state == 7 {
            l.cr.hit(40, 0, 3);
            hits += 1;
        }
        if l.script.reveals.contains(&4) {
            break;
        }
    }
    eprintln!("{hits} hits, {beams} beams, state {}", l.cr.boss.state);
    assert!(beams > 0, "it fires");
    assert!(l.script.reveals.contains(&4), "token 4");
}

/// Touching the racer starts the race; it flies the track.
#[test]
fn race_starts() {
    let Some(mut l) = load() else { return };
    let at = Vec3::from(l.cr.slots[1].pos.map(|v| v as f32 / 32.0));
    let mut p = Player::new(at);
    for _ in 0..3 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        p.pos = Vec3::from(l.cr.slots[1].pos.map(|v| v as f32 / 32.0));
        l.cr.contact(&mut p);
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.script.race, 1);
    l.script.cutscene = None;
    let start = l.cr.slots[1].pos;
    p.pos.y -= 5000.0;
    p.on_floor = false;
    for _ in 0..300 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    let moved = (Vec3::from(l.cr.slots[1].pos.map(|v| v as f32)) - Vec3::from(start.map(|v| v as f32))).length() / 32.0;
    eprintln!("racer moved {moved}");
    assert!(moved > 500.0);
}
