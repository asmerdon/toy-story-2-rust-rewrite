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
