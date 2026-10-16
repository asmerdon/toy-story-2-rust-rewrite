//! Level 9's boss. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
use ts2::game::{Player, World};

/// Buzz coming in starts it; hitting the boss calls up minions each round, beating them
/// starts the next; six rounds, then the chase, and below 11 health the level ends.
#[test]
fn boss_rounds_end_the_level() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level09");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut script = ts2::script::for_level(9, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.always_active = true;
    cr.load_shapes(&game.join("data"));
    assert_eq!(cr.slots[0].id, 54, "the boss is slot 0");
    let s0 = cr.slots[0].pos;
    let mut p = Player::new(Vec3::from([s0[0], s0[1], s0[2] + 0x8000].map(|v| v as f32 / 32.0)));
    p.on_floor = true;
    let (mut hits, mut minions, mut chased) = (0, 0, false);
    for tick in 0..60000 {
        cr.tick(&world, p.pos_obj(), p.pos_obj());
        script.step_creatures(&world, &mut specials, &p, &mut cr, 0);
        cr.hurts.clear();
        chased |= cr.slots[0].rec.speed == 0x14;
        if script.look.is_none() && tick % 10 == 0 {
            if let Some(m) = (7..=11).find(|&i| cr.slots[i].id != 0 && cr.slots[i].health > 0 && cr.slots[i].scale >= 0x1000) {
                cr.hit(m, 0, if tick % 20 == 0 { 3 } else { 1 });
                minions += 1;
            } else if script.boss_hud && cr.slots[0].rec.state == 7 && cr.slots[0].health > 0 {
                cr.hit(0, 0, 3);
                hits += 1;
            }
        }
        if script.done {
            break;
        }
    }
    eprintln!("{hits} boss hits, {minions} minion hits, chased {chased}, health {}, done {}", cr.slots[0].health, script.done);
    assert!(minions > 0, "minions came");
    assert!(chased, "it chased");
    assert!(script.done, "beaten, the level ends");
}
