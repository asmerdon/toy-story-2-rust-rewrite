//! Level 1's Bo Peep and her sheep. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
use ts2::game::{Player, World};

/// Touching each of the five sheep finds it; Bo Peep (room 4) then thanks Buzz in a
/// conversation, and when its box is closed token 1 appears. Before that she asks for them.
#[test]
fn five_sheep_for_bo_peep_give_a_token() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = ts2::creatures::Creatures::new(&spawns, Some(&e), fs::read(game.join("data/rand.dat")).unwrap());
    cr.load_shapes(&game.join("data"));
    let mut specials = ts2::specials::Specials::new(&n);
    let mut level = ts2::level1::Level1::new(&world, &mut specials);
    level.set_story(&e, &lists);
    let sheep: Vec<usize> = (0..cr.slots.len()).filter(|&i| cr.slots[i].id == 6).collect();
    assert_eq!(sheep.len(), 5);

    // Visit Bo Peep first: she asks.
    let bo = Vec3::from(cr.slots[1].pos.map(|v| v as f32 / 32.0));
    assert_eq!(world.room_at(bo), Some(4), "Bo Peep in room 4");
    let visit = |cr: &mut ts2::creatures::Creatures, level: &mut ts2::level1::Level1, at: Vec3| {
        let mut p = Player::new(at);
        for _ in 0..3 {
            cr.tick(&world, p.pos_obj(), p.pos_obj());
            p.pos = at;
            cr.contact(&mut p);
            level.tick_characters(&world, &p, cr, 0);
        }
    };
    visit(&mut cr, &mut level, bo);
    assert!(level.cutscene.is_some(), "Bo Peep should talk");
    level.cutscene = None;
    for &s in &sheep {
        let at = Vec3::from(cr.slots[s].pos.map(|v| v as f32 / 32.0));
        visit(&mut cr, &mut level, at);
    }
    assert_eq!(cr.sheep, 5, "all five sheep found");
    visit(&mut cr, &mut level, bo);
    let cs = level.cutscene.as_mut().expect("the thank-you conversation");
    assert_eq!(cs.token, Some(1));
    let mut ended = None;
    for tick in 0..3000 {
        for ev in cs.tick(tick % 30 == 29, false) {
            if let ts2::cutscene::Event::Ended(t) = ev {
                ended = Some(t);
            }
        }
        if ended.is_some() {
            break;
        }
    }
    assert_eq!(ended, Some(Some(1)), "the box closes and token 1 is handed over");
    assert_eq!(cr.sheep, -1);
}

/// Hamm (room 3) asks for fifty coins, and with them hands over token 0.
#[test]
fn fifty_coins_for_hamm_give_a_token() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = ts2::creatures::Creatures::new(&spawns, Some(&e), Vec::new());
    cr.load_shapes(&game.join("data"));
    let mut specials = ts2::specials::Specials::new(&n);
    let mut level = ts2::level1::Level1::new(&world, &mut specials);
    level.set_story(&e, &lists);
    assert_eq!(cr.slots[0x1E].id, 10, "Hamm in slot 30");
    let at = Vec3::from(cr.slots[0x1E].pos.map(|v| v as f32 / 32.0));
    assert_eq!(world.room_at(at), Some(3));
    let talk = |cr: &mut ts2::creatures::Creatures, level: &mut ts2::level1::Level1, coins: i32| {
        level.cutscene = None;
        let mut p = Player::new(at);
        for _ in 0..3 {
            cr.tick(&world, p.pos_obj(), p.pos_obj());
            p.pos = at;
            cr.contact(&mut p);
            level.tick_characters(&world, &p, cr, coins);
        }
        level.cutscene.as_ref().map(|c| c.token)
    };
    assert_eq!(talk(&mut cr, &mut level, 12), Some(None), "asks for coins");
    assert_eq!(talk(&mut cr, &mut level, 50), Some(Some(0)), "hands over token 0");
}

/// The RC race (room 2): touching the car starts the challenge; after the conversation it
/// drives its waypoints, lapping. Buzz going round the finish line counts laps.
#[test]
fn rc_race_runs_and_counts_laps() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = ts2::creatures::Creatures::new(&spawns, Some(&e), fs::read(game.join("data/rand.dat")).unwrap());
    cr.load_shapes(&game.join("data"));
    let mut specials = ts2::specials::Specials::new(&n);
    let mut level = ts2::level1::Level1::new(&world, &mut specials);
    level.set_story(&e, &lists);
    let rc = Vec3::from(cr.slots[0x1D].pos.map(|v| v as f32 / 32.0));
    eprintln!("RC at {rc:?} room {:?}", world.room_at(rc));
    assert_eq!(world.room_at(rc), Some(2), "the RC car in room 2");
    let mut p = Player::new(rc);
    for _ in 0..3 {
        cr.tick(&world, p.pos_obj(), p.pos_obj());
        p.pos = rc;
        cr.contact(&mut p);
        level.tick_characters(&world, &p, &mut cr, 0);
    }
    assert_eq!(level.race, 1);
    level.cutscene = None;
    // Buzz stands aside in room 2 while the car drives.
    let start = cr.slots[0x1D].pos;
    let mut moved = 0;
    for _ in 0..6000 {
        cr.tick(&world, p.pos_obj(), p.pos_obj());
        level.tick_characters(&world, &p, &mut cr, 0);
        let d = [0, 2].map(|k| ((cr.slots[0x1D].pos[k] - start[k]) >> 5).abs());
        moved = moved.max(d[0].max(d[1]));
        if level.race == 3 {
            break;
        }
    }
    eprintln!("race state {}, RC moved up to {moved} units, at {:?}", level.race, cr.slots[0x1D].pos.map(|v| v >> 5));
    assert!(moved > 1000, "the RC car didn't drive");
    assert_eq!(level.race, 3, "the car finishes (Buzz didn't race)");
    assert!(!level.given[2]);
}
