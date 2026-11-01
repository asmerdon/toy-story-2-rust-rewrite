//! Uses the user's recovered level02/level1.* data, not synthetic boss records.
use std::{env, fs, path::PathBuf};
use ts2::{
    campaign,
    creatures::Creatures,
    formats::{exe, leveldat, ngn, packets, terrain},
    game::{Player, World},
};

#[test]
fn zurg_enters_fires_recovers_and_finishes() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else {
        return;
    };
    let path = campaign::level(12).unwrap().path(&game);
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(&path).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(path.with_extension("dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut script = ts2::script::for_level(
        12,
        &world,
        &mut specials,
        Some(&e),
        &lists,
        Some(random.clone()),
    )
    .unwrap();
    let pk = packets::parse(&fs::read(path.with_extension("raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    assert_eq!(spawns[0].creature, 57);
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.always_active = true;
    cr.load_shapes(&game.join("data"));
    cr.load_animations(&n);
    assert!(cr.node_point(0, 1, [-250.0, -250.0, 0.0]).is_some());
    let mut player = Player::new(glam::Vec3::new(-4000.0, -2400.0, 1800.0));
    // Waiting outside the trigger must not start the introduction.
    script.step_creatures(&world, &mut specials, &player, &mut cr, 0);
    assert!(script.look.is_none());
    player.pos.x = -3000.0;
    script.step_creatures(&world, &mut specials, &player, &mut cr, 0);
    assert_eq!(script.look_ticks(), 299);
    let mut shots = std::collections::BTreeSet::new();
    let mut hits = 0;
    let mut recovered = false;
    let mut saw_fight = false;
    for tick in 0..12000 {
        cr.tick(&world, player.pos_obj(), player.pos_obj());
        script.step_creatures(&world, &mut specials, &player, &mut cr, 0);
        saw_fight |= script.boss_hud;
        for p in script.particles.drain(..) {
            shots.insert(p.typ);
        }
        // Allow a full initial attack, then hit after each recovery/retaliation cycle.
        if tick > 850 && tick % 400 == 0 && cr.slots[0].rec.state == 6 && cr.slots[0].health >= 10 {
            cr.hit(0, 0, 1);
            hits += 1;
        }
        if hits > 0 && cr.slots[0].rec.state == 6 {
            recovered = true;
        }
        if script.done {
            break;
        }
    }
    assert!(saw_fight);
    assert!(shots.contains(&0x6c), "lobbed shots");
    assert!(shots.contains(&0x6d), "retaliation shots");
    assert!(recovered, "hit recovery restores the combat state");
    assert_eq!(
        hits, 10,
        "29 health down to the original death threshold of 9"
    );
    assert_eq!(cr.slots[0].health, 9);
    assert!(script.done, "death scene ends the level");
    assert_eq!(script.boss_bar, Some(0));
}

/// The entrance (0x42B300 / 0x42B3A0) lifts Zurg 0x28000 and lowers him 0x200 a tick for the
/// 300-tick scene, writing his height to slot +0x60 (0x52C8A0, the hover target) each time.
/// The port wrote it to the home instead, so the hover code pulled him down to his spawn on
/// the floor Buzz stands on and he sank about 60 units into it. He must stay above the floor
/// throughout and end the scene 0x2800 over his spawn.
#[test]
fn zurg_entrance_stays_above_the_floor() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else {
        return;
    };
    let path = campaign::level(12).unwrap().path(&game);
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(&path).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(path.with_extension("dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut script = ts2::script::for_level(12, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let pk = packets::parse(&fs::read(path.with_extension("raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let spawn_y = spawns[0].pos[1] * 32;
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.always_active = true;
    cr.load_shapes(&game.join("data"));
    cr.load_animations(&n);
    let player = Player::new(glam::Vec3::new(-3000.0, -2400.0, 1800.0));
    for tick in 0..300 {
        cr.tick(&world, player.pos_obj(), player.pos_obj());
        script.step_creatures(&world, &mut specials, &player, &mut cr, 0);
        let c = &cr.slots[0];
        let floor = world.floor_below(c.pos[0] as f32 / 32.0, c.pos[2] as f32 / 32.0, c.pos[1] as f32 / 32.0 - 100.0).unwrap();
        assert!(c.pos[1] < (floor * 32.0) as i32, "tick {tick}: Zurg at y {} is inside the floor at {floor}", c.pos[1] / 32);
    }
    assert_eq!(cr.slots[0].pos[1], spawn_y - 0x2800, "the scene ends 0x2800 above his spawn");
}

/// 0x4076F0's chase keeps Zurg (type 0x39) 2000 units from Buzz, pushing him out a sixteenth
/// of the way a tick; before, he flew straight onto Buzz and stayed there.
#[test]
fn zurg_keeps_his_distance_while_chasing() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else {
        return;
    };
    let path = campaign::level(12).unwrap().path(&game);
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(&path).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(path.with_extension("dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut script = ts2::script::for_level(
        12,
        &world,
        &mut specials,
        Some(&e),
        &lists,
        Some(random.clone()),
    )
    .unwrap();
    let pk = packets::parse(&fs::read(path.with_extension("raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.always_active = true;
    cr.load_shapes(&game.join("data"));
    cr.load_animations(&n);
    let mut player = Player::new(glam::Vec3::new(-1500.0, -2400.0, 1800.0));
    player.on_floor = true;
    let mut nearest = f32::MAX;
    for _ in 0..2400 {
        cr.tick(&world, player.pos_obj(), player.pos_obj());
        script.step_creatures(&world, &mut specials, &player, &mut cr, 0);
        cr.hurts.clear();
        if script.boss_hud && script.look.is_none() {
            let (c, b) = (cr.slots[0].pos, player.pos_obj());
            nearest = nearest.min(((c[0] - b[0]) as f32).hypot((c[2] - b[2]) as f32) / 32.0);
        }
    }
    eprintln!("nearest {nearest:.0}");
    assert!(nearest > 1200.0 && nearest < 2000.0, "nearest {nearest}");
}
