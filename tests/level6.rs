//! Level 6's boss. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
use ts2::game::{Player, World};

/// Buzz walking in starts the fight; hitting the boss between its recoveries beats it and
/// the level ends. (Work in progress: the boss isn't beaten yet.)
#[test]

fn boss_fight_ends_the_level() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level06");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut script = ts2::script::for_level(6, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.always_active = true;
    cr.load_shapes(&game.join("data"));
    let s0 = cr.slots[0].pos;
    let mut p = Player::new(Vec3::from([0x30000, s0[1], s0[2]].map(|v| v as f32 / 32.0)));
    p.on_floor = true;
    let mut hits = 0;
    let mut fought = false;
    for tick in 0..20000 {
        cr.tick(&world, p.pos_obj(), p.pos_obj());
        script.step_creatures(&world, &mut specials, &p, &mut cr, 0);
        cr.hurts.clear();
        fought |= script.boss_hud;
        if fought && script.look.is_none() && tick % 10 == 0 && cr.slots[0].rec.state == 7 && cr.slots[0].health > 0 {
            cr.hit(0, 0, 3);
            hits += 1;
        }
        if script.done {
            break;
        }
    }
    eprintln!("{hits} hits, fought {fought}, bar {:?}, done {}", script.boss_bar, script.done);
    assert!(fought, "the fight starts");
    assert!(script.done, "beaten, the level ends");
}
