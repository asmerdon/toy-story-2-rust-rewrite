//! Level 3's slime boss. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
use ts2::game::{Player, World};

/// Meeting the slime grows it; each hit shrinks it, four rounds of shrinking it to nothing
/// win, and the level ends.
#[test]
fn slime_shrinks_over_four_rounds() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level03");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut script = ts2::script::for_level(3, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.load_shapes(&game.join("data"));
    let s0 = cr.slots[0].pos;
    // Buzz a little way off (the slime moves 0x10000 back at the start).
    let mut p = Player::new(Vec3::from([s0[0], s0[1], s0[2] + 0x10000 - 0x8000].map(|v| v as f32 / 32.0)));
    p.on_floor = true;
    let (mut looks, mut hits, mut max_scale) = (0, 0, 0);
    for tick in 0..30000 {
        cr.tick(&world, p.pos_obj(), p.pos_obj());
        script.step_creatures(&world, &mut specials, &p, &mut cr, 0);
        cr.hurts.clear();
        if script.look.is_some() {
            looks += 1;
        }
        max_scale = max_scale.max(cr.slots[0].scale);
        if script.look.is_none() && tick % 5 == 0 && cr.slots[0].rec.state != 4 && cr.slots[0].health > 0 {
            cr.hit(0, 0, 3);
            hits += 1;
        }
        if script.done {
            break;
        }
    }
    eprintln!("{looks} look ticks, {hits} hits, biggest scale {max_scale:#x}, bar {:?}, done {}", script.boss_bar, script.done);
    assert!(max_scale > 0x1400, "it grows round by round");
    assert!(script.done, "the level ends");
    assert_eq!(script.boss_bar, Some(0));
}
