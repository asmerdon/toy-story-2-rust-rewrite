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

/// Square raises the arm and fires after 11 ticks; the beam auto-aims at the ZURG1 ahead and
/// each shot costs it one of its 4 health (hit kind 2). Its hit shapes come from zurg1.all.
#[test]
fn laser_shots_hit_a_robot() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut c = Creatures::new(&spawns, Some(&e), random.clone());
    c.load_shapes(&game.join("data"));
    assert_eq!(c.shape(3).sphere, [0, -6560, 0, 298]);
    let k = 9;
    let cam = c.slots[k].pos;
    c.tick(&world, cam, [0; 3]);
    let robot = glam::Vec3::from(c.slots[k].pos.map(|v| v as f32 / 32.0));
    // 600 units away along +x, facing it (−x is 12-bit angle 0xC00), slightly off-aim.
    let mut buzz = ts2::game::Player::new(robot + glam::Vec3::new(600.0, 0.0, 0.0));
    buzz.on_floor = true;
    buzz.coyote = 6;
    buzz.set_facing(0xC20);
    let mut lasers = ts2::laser::Lasers::new(random);
    let health = c.slots[k].health;
    let mut fired = 0;
    for tick in 0..120 {
        let input = ts2::game::Input { fire: tick == 0 || tick == 30, ..Default::default() };
        buzz.update(&world, input, 1.0 / 60.0);
        buzz.pos = robot + glam::Vec3::new(600.0, 0.0, 0.0);
        for shot in buzz.shots.drain(..).collect::<Vec<_>>() {
            fired += 1;
            assert!(tick == 11 || tick >= 30, "fired at tick {tick}");
            lasers.fire(shot, shot.origin, &mut buzz, &world, &mut c, &|_| true);
        }
        lasers.tick();
        c.tick(&world, cam, buzz.pos_obj());
        c.slots[k].pos = robot.to_array().map(|v| (v * 32.0) as i32);
    }
    eprintln!("{fired} shots, robot health {health} -> {}", c.slots[k].health);
    assert_eq!(fired, 2);
    assert_eq!(c.slots[k].health, health - 2);
}

/// A ZURG3 (id 4) with Buzz in its zone fires a pair of shots (particle 0x26) per 360-tick
/// cycle; a shot reaching Buzz hurts him, but bounces off him while he spins.
#[test]
fn zurg3_fires_and_shots_hurt_unless_spinning() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut c = Creatures::new(&spawns, Some(&e), random.clone());
    let k = c.slots.iter().position(|s| s.id == 4).expect("a ZURG3 in level 1");
    let home = c.slots[k].pos;
    let mut shots = Vec::new();
    for tick in 0..800 {
        c.tick(&world, home, home);
        for r in c.particles.drain(..) {
            shots.push((tick, r));
        }
    }
    eprintln!("ZURG3 slot {k} shots at ticks {:?}", shots.iter().map(|s| s.0).collect::<Vec<_>>());
    assert!(shots.len() >= 2 && shots.iter().all(|s| s.1.typ == 0x26), "no shots");
    assert_eq!(shots[1].0 - shots[0].0, 16, "the pair is 16 ticks apart");

    // A shot flying into Buzz.
    let mut p = ts2::particles::Particles::new(Some(&e), random);
    let buzz = [0, 0, 0];
    p.camera = buzz;
    let fire = |p: &mut ts2::particles::Particles| p.create([0, -0x1CC0, -0x2000], [0, 0, 0x400], 0, 0, 0, 0x26).unwrap();
    fire(&mut p);
    let mut hurt = false;
    for _ in 0..30 {
        p.tick(&world, (buzz, 0));
        p.touch(buzz, false);
        hurt |= p.events.iter().any(|ev| matches!(ev, ts2::particles::Event::Hurt(_)));
    }
    assert!(hurt, "the shot should hurt Buzz");
    let mut p2 = ts2::particles::Particles::new(Some(&e), Vec::new());
    p2.camera = buzz;
    let k = fire(&mut p2);
    let mut batted = false;
    for _ in 0..30 {
        p2.tick(&world, (buzz, 0));
        p2.touch(buzz, true);
        batted |= p2.events.iter().any(|ev| matches!(ev, ts2::particles::Event::Sound(7, _)));
        assert!(!p2.events.iter().any(|ev| matches!(ev, ts2::particles::Event::Hurt(_))), "hurt while spinning");
    }
    assert!(batted && p2.slots[k].vel[2] < 0, "the shot should bounce back");
}

/// Level 7: a Zurg pod with Buzz under it sweeps its laser at the ground (ribbons, and Buzz
/// hurt when it lands near him); touching a chick (health 0x66) counts it and it's gone.
#[test]
fn level_7_pods_fire_and_chicks_count() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level07");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut c = Creatures::new(&spawns, Some(&e), fs::read(game.join("data/rand.dat")).unwrap());
    c.load_shapes(&game.join("data"));
    let pod = c.slots.iter().position(|s| s.id == 0x14).expect("a pod");
    let under = c.slots[pod].home;
    let (mut ribbons, mut hurts) = (0, 0);
    for _ in 0..800 {
        let buzz = [under[0], c.slots[pod].pos[1] + 0x8000, under[2]];
        c.tick(&world, under, buzz);
        ribbons += c.ribbons.drain(..).count();
        hurts += c.hurts.drain(..).count();
    }
    eprintln!("pod {pod}: {ribbons} ribbon frames, {hurts} hurts");
    assert!(ribbons > 100, "the pod fires");

    let chick = c.slots.iter().position(|s| s.id == 0x1C && s.health == 0x66).expect("a chick");
    let at = c.slots[chick].pos;
    let mut buzz = ts2::game::Player::new(glam::Vec3::from(at.map(|v| v as f32 / 32.0)));
    for _ in 0..3 {
        c.tick(&world, at, at);
        buzz.pos = glam::Vec3::from(c.slots[chick].pos.map(|v| v as f32 / 32.0));
        c.contact(&mut buzz);
    }
    assert_eq!(c.sheep, 1, "found a chick");
    assert!(c.slots[chick].health <= 0 || c.slots[chick].id == 0, "gone");
}
