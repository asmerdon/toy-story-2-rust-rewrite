//! Level 14 (Tarmac Trouble, data\level04\level1.*). Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{exe, packets};
use ts2::game::Player;

/// The level spans over 10^6 object units. Contact tests (0x407440) used to wrap in i32 there,
/// so harmful creatures across the tarmac hurt Buzz wherever he stood.
#[test]
fn far_creatures_do_not_touch_buzz() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level04");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let pk = packets::parse(&fs::read(dir.join("level1.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.load_shapes(&game.join("data"));
    for c in cr.slots.iter_mut().filter(|c| c.id != 0) {
        c.active = true;
    }
    // Where the user was hurt with nothing near: no creature within 2000 level units.
    let at = [4000.0, 5.5, 22000.0];
    let near = cr.slots.iter().filter(|c| c.id != 0).any(|c| {
        let d = [0, 1, 2].map(|k| (c.pos[k] as f32 / 32.0 - at[k]).abs());
        d.iter().all(|&v| v < 2000.0)
    });
    assert!(!near, "pick a spot with nothing nearby");
    let mut p = Player::new(Vec3::from(at));
    p.on_floor = true;
    let health = p.health;
    cr.contact(&mut p);
    assert_eq!(p.health, health, "hurt by a creature far away");
    assert_eq!(p.hurt_timer, 0);
}

/// 0x42E600 sets each creature's draw distance (+0x42), not its flags. Writing 0x708 into
/// the flags marked them all touched: standing at the start, Hamm talked and the wing race
/// began, and every creature chased Buzz.
#[test]
fn standing_at_the_start_starts_no_talk_or_race() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level04");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ts2::formats::ngn::parse(&fs::read(dir.join("level1.ngn")).unwrap()).unwrap();
    let t = ts2::formats::terrain::parse(&fs::read(dir.join("TERR1.ALL")).unwrap()).unwrap();
    let lists = ts2::formats::leveldat::parse(&fs::read(dir.join("level1.dat")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut script = ts2::script::for_level(14, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let pk = packets::parse(&fs::read(dir.join("level1.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.load_shapes(&game.join("data"));
    let start = world.start_floor(Vec3::from(e.player_start(14).unwrap().pos));
    let mut p = Player::new(start);
    p.on_floor = true;
    for _ in 0..300 {
        cr.tick(&world, p.pos_obj(), p.pos_obj());
        cr.contact(&mut p);
        script.step_creatures(&world, &mut specials, &p, &mut cr, 0);
        assert!(script.cutscene.is_none(), "a conversation started with Buzz standing still");
    }
    assert_eq!(script.race, 0, "the wing race started by itself");
}

/// The helicopter puzzle (0x42E790): Buzz jumping and stomping on a button, with his own
/// physics and the script running inside each tick as the viewer runs it, takes one press:
/// the sparkles go out and the presses-left marker goes from 3 to 2 (specials 0x36 + n).
#[test]
fn stomping_a_helicopter_button_presses_it() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else {
        return;
    };
    let dir = game.join("data/level04");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ts2::formats::ngn::parse(&fs::read(dir.join("level1.ngn")).unwrap()).unwrap();
    let t = ts2::formats::terrain::parse(&fs::read(dir.join("TERR1.ALL")).unwrap()).unwrap();
    let lists = ts2::formats::leveldat::parse(&fs::read(dir.join("level1.dat")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut script =
        ts2::script::for_level(14, &world, &mut specials, Some(&e), &lists, Some(random)).unwrap();
    let mut boxes = ts2::boxes::Boxes::new(14, &lists, Some(&e), &world, &mut specials);
    // Button 0 (surface 0x20).
    let mut p = Player::new(Vec3::new(-25500.0, -2104.0, -19500.0));
    let dt = 1.0 / 60.0;
    let mut run = |p: &mut Player,
                   input: ts2::game::Input,
                   specials: &mut ts2::specials::Specials,
                   script: &mut Box<dyn ts2::script::Script>| {
        p.update_with(&world, input, dt, &mut |p, w| {
            boxes.tick(p, w, specials);
            script.step(w, specials, p, &boxes);
        });
    };
    for _ in 0..30 {
        run(&mut p, Default::default(), &mut specials, &mut script);
    }
    script.put_out.clear();
    assert!(!specials.get(0x4C).unwrap().hidden, "button 0 starts up");
    for tick in 0..120 {
        let input = ts2::game::Input {
            jump: tick == 0,
            jump_held: tick < 4,
            stomp: tick == 14,
            ..Default::default()
        };
        run(&mut p, input, &mut specials, &mut script);
    }
    assert_eq!(
        script.put_out.iter().filter(|&&(_, g)| g == 1).count(),
        4,
        "the press's sparkles"
    );
    assert!(
        specials.get(0x38).is_some_and(|s| !s.hidden),
        "two presses left"
    );
    assert!(specials.get(0x39).is_some_and(|s| s.hidden));
}
