//! Level 1's crib escape through first person. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, ngn, packets, terrain};
use ts2::game::{Input, Player, World};

/// In the crib (room 1: the mattress at y 1163 past x 9747), Buzz goes first person (L1),
/// targets the crib's bars (R1) and shoots each; they swing down, and the crib's barrier
/// (mover 9) goes.
#[test]
fn crib_bars_are_shot_down_in_first_person() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut cr = ts2::creatures::Creatures::new(&spawns, Some(&e), random.clone());
    cr.load_shapes(&game.join("data"));
    let mut specials = ts2::specials::Specials::new(&n);
    let mut level = ts2::level1::Level1::new(&world, &mut specials);
    let mut parts = ts2::particles::Particles::new(Some(&e), random.clone());
    let mut lasers = ts2::laser::Lasers::new(random);
    let mut look = ts2::look::FirstPerson::default();
    assert_eq!((cr.slots[0].id, cr.slots[2].id), (11, 11));

    let g = Vec3::new(10500.0, 0.0, -5300.0);
    let y = world.floor_below(g.x, g.z, 0.0).unwrap();
    assert!((y - 1163.0).abs() < 2.0, "the crib's mattress");
    let mut p = Player::new(Vec3::new(g.x, y, g.z));
    p.set_facing(0xC00);
    for _ in 0..10 {
        p.tick(&world, [0.0; 2], false, false);
    }
    eprintln!("start {:?} room {:?}, Buzz at {:?} room {:?}", g, world.room_at(g), p.pos, world.room_at(p.pos));
    let mut down = [false; 2];
    for tick in 0..3000 {
        // Under the bar still up; R1 cycles until it's the target.
        let bar = if down[0] { 2 } else { 0 };
        if tick % 200 == 0 && p.first_person == 4 {
            let spots = [(10500.0, -5300.0), (10800.0, -5000.0), (11000.0, -4600.0), (10300.0, -4800.0)];
            let (x, z) = spots[(tick / 200) % spots.len()];
            p.pos = Vec3::new(x, 1169.0, z);
        }
        let r1_pressed = tick % 200 == 5 || (tick % 200 > 5 && tick % 10 == 0 && look.target != Some(bar));
        let input = Input { look: tick == 0, fire: tick % 200 == 120, ..Default::default() };
        p.update(&world, input, 1.0 / 60.0);
        let pad = ts2::look::Pad { dpad: 0, r1: tick % 200 >= 5, r1_pressed, l1_pressed: false };
        if p.first_person != 0 {
            look.tick(&mut p, &mut cr, &mut parts, &mut specials, &world, pad);
        }
        for shot in p.shots.drain(..).collect::<Vec<_>>() {
            let ev = lasers.fire(shot, shot.origin, &mut p, &world, &mut cr, &|_| true);
            eprintln!("tick {tick}: target {:?} -> {:?}", look.target, ev);
        }
        lasers.tick();
        let cam = ts2::look::FirstPerson::eye(&p);
        parts.camera = cam;
        parts.tick(&world, (p.pos_obj(), p.facing));
        cr.tick(&world, cam, p.pos_obj());
        level.tick_creatures(&world, &mut specials, &p, &mut cr);
        for (k, &slot) in [0usize, 2].iter().enumerate() {
            if !down[k] && cr.slots[slot].rec.state == 0 {
                down[k] = true;
                eprintln!("bar {k} hit at tick {tick} (target {:?})", look.target);
            }
        }
        if level.crib_open() {
            eprintln!("crib open at tick {tick}");
            break;
        }
    }
    assert_eq!(p.first_person, 4, "should be in first person");
    assert!(down[0] && down[1], "bars down: {down:?}");
    assert!(level.crib_open() && !world.mover_enabled(9), "the crib should open");
}
