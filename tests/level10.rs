//! Level 10's script. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
use ts2::game::{Player, World};
use ts2::script::Script;

#[allow(dead_code)]
struct L {
    world: World,
    specials: ts2::specials::Specials,
    script: Box<dyn Script>,
    boxes: ts2::boxes::Boxes,
    cr: Creatures,
    lists: Vec<leveldat::PointList>,
}

fn load() -> Option<L> {
    let game = env::var_os("TS2_GAME_DIR").map(PathBuf::from)?;
    let dir = game.join("data/level10");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let script = ts2::script::for_level(10, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let boxes = ts2::boxes::Boxes::new(10, &lists, Some(&e), &world, &mut specials);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.load_shapes(&game.join("data"));
    Some(L { world, specials, script, boxes, cr, lists })
}

/// Standing at the bottom of the always-open air shaft (list 0), Buzz is carried up.
#[test]
fn air_shaft_lifts_buzz() {
    let Some(mut l) = load() else { return };
    let shaft = l.lists.iter().find(|p| p.kind == 0).unwrap().points.clone();
    let (bottom, top) = (shaft[0], shaft[1]);
    let start = Vec3::new(bottom[0] as f32, (bottom[1] - 200) as f32, bottom[2] as f32);
    let mut p = Player::new(start);
    let mut highest = start.y;
    for _ in 0..600 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        p.tick(&l.world, [0.0; 2], false, false);
        highest = highest.min(p.pos.y);
    }
    eprintln!("from {} up to {highest} (top {})", start.y, top[1]);
    assert!(highest < start.y - 3000.0, "carried up the shaft");
}

/// Standing on the spider's floor starts it; beaten, token 4 appears.
#[test]
fn spider_fight() {
    let Some(mut l) = load() else { return };
    assert_eq!(l.cr.slots[8].id, 0x1F);
    let s = l.cr.slots[8].home;
    let at = Vec3::new(s[0] as f32 / 32.0, (-0x1B0000) as f32 / 32.0, s[2] as f32 / 32.0);
    let mut p = Player::new(at);
    p.on_floor = true;
    let (mut hits, mut fought, mut powered) = (0, false, false);
    for tick in 0..30000 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        l.cr.hurts.clear();
        if l.script.cutscene.is_some() {
            l.script.cutscene = None;
        }
        fought |= l.script.boss_hud;
        powered |= l.cr.boss.phase > 0x400;
        if l.cr.boss.state == 2 && tick % 200 == 0 && l.cr.slots[8].rec.state == 7 && l.cr.slots[8].health > 0 {
            l.cr.hit(8, 0, 3);
            hits += 1;
        }
        if l.script.reveals.contains(&4) {
            break;
        }
    }
    eprintln!("{hits} hits, fought {fought}, fans {powered}, state {}", l.cr.boss.state);
    assert!(fought, "the fight starts");
    assert!(powered, "the fans run");
    assert!(l.script.reveals.contains(&4), "token 4 once it's beaten");
}

/// The runaway token: asked, token 2 shows and runs along its track; Buzz in the middle
/// (x ±4500 / z ±2500 at any height) loses it.
#[test]
fn runaway_token() {
    let Some(mut l) = load() else { return };
    let mut p = Player::new(Vec3::new(-8000.0, -30000.0, 0.0));
    l.cr.slots[0x16].flags |= 0x200;
    let mut moves = Vec::new();
    for _ in 0..400 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        if l.script.cutscene.is_some() {
            l.script.cutscene = None;
        }
        moves.extend(l.script.token_moves.drain(..));
    }
    assert!(l.script.reveals.contains(&2), "token 2 shows");
    assert_eq!(l.script.race, 2, "laps {} reveals {:?}", l.script.laps, l.script.reveals);
    assert!(moves.len() > 100, "it runs ({} moves)", moves.len());
    assert_ne!(moves.first().unwrap().1, moves.last().unwrap().1);
    p.pos = Vec3::new(0.0, -100.0, 0.0);
    for _ in 0..3 {
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.script.race, 0, "lost in the middle");
    assert!(l.script.hides.contains(&2));
}

/// Riding a lift off-centre, the follow camera moves evenly. It used to be pulled in by the
/// car's walls, which the original's camera rays ignore (object flag 0x200, 0x48C860), and
/// then thrown up by the "too close to Buzz" step, a jump of 500 units every few ticks.
#[test]
fn the_camera_rides_a_lift_smoothly() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level10");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let mut l = ts2::level10::Level10::new(Some(&e));
    l.set_story(&e, &lists);
    l.solve_switches();
    let boxes = ts2::boxes::Boxes::new(10, &lists, Some(&e), &world, &mut specials);
    let at = world.mover_pos(1).unwrap() + Vec3::new(-300.0, 0.0, -300.0);
    let mut y = at.y - 9000.0;
    let floor = loop {
        let (f, o) = world.floor_below_obj(at.x, at.z, y).expect("the car's floor");
        if o == Some(1) && f > at.y - 6000.0 {
            break f;
        }
        y = f + 1.0;
    };
    let mut p = Player::new(Vec3::new(at.x, floor - 2.0, at.z));
    p.set_facing(0xC00);
    let mut cam = ts2::camera::Camera::new(&p);
    let (mut last, mut step, mut worst) = (cam.position(), Vec3::ZERO, 0.0f32);
    for k in 0..560 {
        p.update_with(&world, ts2::game::Input::default(), 1.0 / 60.0, &mut |p, w| l.step(w, &mut specials, p, &boxes));
        cam.tick(&p, &world);
        let c = cam.position();
        if k > 60 {
            worst = worst.max(((c - last) - step).length());
        }
        step = c - last;
        last = c;
    }
    assert!(p.pos.y < floor - 5000.0, "he rode up: {}", p.pos.y);
    assert!(worst < 5.0, "the camera's step changed by {worst} units in a tick");
}

/// The wall mice (0x425A80): their zone stands up on the wall, so they cross it in x and height
/// at their home's depth; the mouse on the floor (byte +0x1C ≥ 0x80) still moves in x and z.
#[test]
fn wall_mice_crawl_up_their_walls() {
    let Some(mut l) = load() else { return };
    l.cr.wall_mice = true;
    l.cr.always_active = true;
    let home = l.cr.slots[1].home;
    let (mut y_range, mut z_off) = ((i32::MAX, i32::MIN), 0);
    let walker = l.cr.slots[4].home;
    let mut walker_z = 0;
    for _ in 0..1500 {
        l.cr.tick(&l.world, home, [i32::MAX / 2; 3]);
        let c = &l.cr.slots[1];
        y_range = (y_range.0.min(c.pos[1]), y_range.1.max(c.pos[1]));
        z_off = z_off.max((c.pos[2] - home[2]).abs());
        walker_z = walker_z.max((l.cr.slots[4].pos[2] - walker[2]).abs());
    }
    assert!(l.cr.slots[1].rec.decel[0] < 0x80, "slot 1 is a wall mouse");
    assert!(y_range.1 - y_range.0 > 0x2000, "it climbs and descends: {y_range:?}");
    // It settles 0xC0 off its plane, gravity against the floor-follow (as 0x4076F0 does).
    assert!(z_off < 0x100, "it keeps to the wall's depth: {z_off}");
    assert_eq!(l.cr.slots[1].angles[0], 0xC00, "lying on the wall");
    assert!(walker_z > 0x2000, "slot 4 walks across its floor, {walker_z}");
}
