//! Creatures running their behaviour scripts. Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{exe, packets, terrain};
use ts2::game::World;

/// Spinning into a ZURG1 (4 health, spins do 2) kills it in two spins and drops a coin;
/// standing in its way costs Buzz a battery and a knock-back.
#[test]
fn spin_attack_beats_a_robot_and_robots_hurt() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut c = Creatures::new(&spawns, Some(&e), fs::read(game.join("data/rand.dat")).unwrap());
    let k = 9;
    assert_eq!((c.slots[k].id, c.slots[k].health), (3, 4));
    let at = |c: &Creatures| glam::Vec3::from(c.slots[k].pos.map(|v| v as f32 / 32.0));
    let input = |stomp: bool| ts2::game::Input { stomp, ..Default::default() };

    // Standing still against it.
    let mut buzz = ts2::game::Player::new(at(&c) + glam::Vec3::new(60.0, 0.0, 0.0));
    let cam = c.slots[k].pos;
    c.tick(&world, cam, buzz.pos_obj());
    buzz.pos = at(&c) + glam::Vec3::new(60.0, 0.0, 0.0);
    c.contact(&mut buzz);
    assert_eq!(buzz.health, 13, "robot contact should cost a battery");
    assert!(buzz.hurt_timer > 0);

    // Spinning: two spins.
    let mut buzz = ts2::game::Player::new(at(&c) + glam::Vec3::new(60.0, 0.0, 0.0));
    buzz.on_floor = true;
    buzz.coyote = 6;
    let mut died = false;
    for tick in 0..400 {
        buzz.update(&world, input(tick % 60 == 0), 1.0 / 60.0);
        buzz.pos = at(&c) + glam::Vec3::new(60.0, 0.0, 0.0);
        c.tick(&world, cam, buzz.pos_obj());
        c.contact(&mut buzz);
        if c.events.iter().any(|e| matches!(e, ts2::creatures::Event::Died { coin: true, .. })) {
            died = true;
            break;
        }
    }
    eprintln!("robot health {} after the spins, Buzz health {}", c.slots[k].health, buzz.health);
    // (Pinned against it between spins, Buzz does get hurt, as in the original.)
    assert!(died, "two spins should beat a ZURG1");
}

/// Level 1's hopping robots (ZURG1, slot 9 and others) wander inside their zones near the
/// camera, keep to the floor, and animate; far ones don't move.
#[test]
fn level_1_robots_wander_in_their_zones() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut c = Creatures::new(&spawns, Some(&e), random);
    let k = 9;
    let home = c.slots[k].pos;
    let far = 26;
    let far_home = c.slots[far].pos;
    let camera = [home[0], home[1] - 0x4000, home[2] + 0x8000];
    let buzz = [home[0] + 0x40000, home[1], home[2]];
    let (mut moved, mut frames, mut landed, mut highest) = (0, std::collections::HashSet::new(), false, 0.0f32);
    for _ in 0..1200 {
        c.tick(&world, camera, buzz);
        let s = &c.slots[k];
        let d = [0, 2].map(|i| (s.pos[i] - home[i]).abs());
        moved = moved.max(d[0].max(d[1]));
        frames.insert(s.time >> 16);
        // Zone: 25 × 155 half sizes (× 256 object units), unrotated.
        assert!(d[0] <= 25 * 256 + 64 && d[1] <= 155 * 256 + 64, "left its zone: {:?}", s.pos);
        let y = s.pos[1] as f32 / 32.0;
        let floor = world.floor_below(s.pos[0] as f32 / 32.0, s.pos[2] as f32 / 32.0, y - 1500.0).unwrap();
        assert!(y <= floor + 40.0, "below the floor: {y} vs {floor}");
        landed |= (floor - y).abs() < 40.0;
        highest = highest.max(floor - y);
    }
    let s = &c.slots[k];
    eprintln!("slot {k}: moved up to {} level units, {} frames seen, hops up to {highest} units, at {:?}", moved / 32, frames.len(), s.pos.map(|v| v / 32));
    assert!(moved > 32 * 32, "robot did not move");
    assert!(frames.len() > 2, "no animation");
    assert!(landed && highest < 1200.0, "not hopping along the floor");
    assert_eq!(c.slots[far].pos, far_home, "a far creature moved");
}
