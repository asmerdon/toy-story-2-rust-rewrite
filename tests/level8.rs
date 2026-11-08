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

fn level8() -> Option<(World, ts2::specials::Specials, Box<dyn Script>, ts2::boxes::Boxes, Creatures, Vec<leveldat::PointList>)> {
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
    let cr = Creatures::new(&spawns, Some(&e), random);
    Some((world, specials, script, boxes, cr, lists))
}

/// The two arcs (0x422D20) draw bolts while Buzz is in their area, and strike him when he's
/// within 500 and above the floor limit.
#[test]
fn arcs_strike_buzz_in_their_area() {
    let Some((world, mut specials, mut script, _boxes, mut cr, _)) = level8() else { return };
    let (x, z) = (0x40000 as f32 / 32.0, -0x30000 as f32 / 32.0);
    let p = Player::new(Vec3::new(x, 0.0, z));
    let (mut bolts, mut hurts) = (0, 0);
    for _ in 0..200 {
        cr.ribbons.clear();
        script.step_creatures(&world, &mut specials, &p, &mut cr, 0);
        bolts += cr.ribbons.iter().filter(|r| r.3 == 0x40).count();
        hurts += cr.hurts.drain(..).filter(|h| h.1 == 2).count();
    }
    assert!(bolts >= 200, "both arcs draw: {bolts}");
    eprintln!("bolts {bolts}, strikes {hurts}");
    let far = Player::new(Vec3::new(-50000.0, 0.0, 0.0));
    cr.ribbons.clear();
    script.step_creatures(&world, &mut specials, &far, &mut cr, 0);
    assert!(!cr.ribbons.iter().any(|r| r.3 == 0x40), "only in their area");
}

/// Room 2's lobbers (0x423200, list 16): after 200 ticks each point in turn throws a 0x60 at
/// Buzz when he's within 0x300 of it.
#[test]
fn room_2_lobbers_throw_at_buzz() {
    let Some((world, mut specials, mut script, boxes, _cr, lists)) = level8() else { return };
    let l = lists.iter().find(|l| l.kind == 16).expect("list 16");
    let at = Vec3::from(l.points[0].map(|v| v as f32));
    let spot = [at, at + Vec3::new(0.0, 0.0, 200.0), at + Vec3::new(200.0, 0.0, 0.0), at - Vec3::new(200.0, 0.0, 0.0)]
        .into_iter()
        .find(|p| world.room_at(*p) == Some(2) && (p.x * 32.0) < 0x1B467 as f32)
        .expect("a place in room 2 beside the list");
    let mut p = Player::new(spot);
    let mut shots = 0;
    for k in 0..260 {
        p.pos = spot;
        script.step(&world, &mut specials, &mut p, &boxes);
        let n = script.particles.drain(..).filter(|r| r.typ == 0x60).count();
        assert!(k > 199 || n == 0, "none in the first 200 ticks");
        shots += n;
    }
    assert!(shots > 0, "the lobbers threw");
}

/// Room 4's specials 0x14–0x17 sway about their homes (0x423200).
#[test]
fn room_4_specials_sway() {
    let Some((world, mut specials, mut script, boxes, _cr, _)) = level8() else { return };
    let home = specials.home_obj(0x14).unwrap().map(|v| v as f32 / 32.0);
    let spot = [0.0f32, 400.0, -400.0, 800.0, -800.0].into_iter().flat_map(|d| [Vec3::new(home[0] + d, home[1], home[2]), Vec3::new(home[0], home[1], home[2] + d)])
        .find(|p| world.room_at(*p) == Some(4)).expect("room 4 near special 0x14");
    let mut p = Player::new(spot);
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for _ in 0..600 {
        p.pos = spot;
        script.step(&world, &mut specials, &mut p, &boxes);
        let y = specials.pos_obj(0x14).unwrap()[1];
        lo = lo.min(y);
        hi = hi.max(y);
    }
    assert!(hi - lo > 0x2000, "it swayed: {lo} {hi}");
}

/// Level 8's boxes (0x423020 → 0x4335D0 list 0x4F2F44): specials 0, 15, 19 on movers 0, 3, 4,
/// pushed along lists 1, 2, 3.
#[test]
fn level_8_boxes_are_loaded_and_push() {
    let Some((world, mut specials, _s, mut boxes, _cr, lists)) = level8() else { return };
    assert_eq!(boxes.boxes.iter().map(|b| (b.special, b.mover, b.path)).collect::<Vec<_>>(), vec![(0, 0, 1), (15, 3, 2), (19, 4, 3)]);
    // Walk box 0 along its path from behind it.
    let l = lists.iter().find(|l| l.kind == 1).expect("list 1");
    let (a, b) = (Vec3::from(l.points[0].map(|v| v as f32)), Vec3::from(l.points[1].map(|v| v as f32)));
    let dir = (b - a).normalize();
    let start = a - dir * 500.0;
    let y = world.floor_below(start.x, start.z, a.y - 600.0).expect("floor behind the box");
    let mut p = Player::new(Vec3::new(start.x, y, start.z));
    let before = boxes.boxes[0].pos;
    for _ in 0..300 {
        p.update_with(&world, ts2::game::Input { move_xz: [dir.x, dir.z], ..Default::default() }, 1.0 / 60.0, &mut |p, w| {
            boxes.tick(p, w, &mut specials);
        });
    }
    let moved = (0..3).map(|k| (boxes.boxes[0].pos[k] - before[k]).abs()).max().unwrap();
    assert!(moved > 32 * 32, "box 0 moved {moved}; Buzz at {:?}", p.pos);
}
