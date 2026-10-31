//! Particles from the original tables. Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::{exe::Exe, ngn, terrain};
use ts2::particles::Particles;

/// The spin's rings, laser sparks and a ZURG's fireballs: they spawn from the exe's tables,
/// live, move and fade, draw with level 1's sprites, and the fireballs burst on the floor.
#[test]
fn particles_spawn_live_and_draw() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let e = Exe::parse(fs::read(dir.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("data/level01/level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("data/level01/TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let random = fs::read(dir.join("data/rand.dat")).unwrap();
    let mut p = Particles::new(Some(&e), random);
    let start = e.player_start(1).unwrap();
    let feet = start.pos.map(|v| (v * 32.0) as i32);
    let feet = [feet[0], (world.floor_below(start.pos[0], start.pos[2], start.pos[1] - 600.0).unwrap() * 32.0) as i32, feet[2]];
    p.camera = feet;
    let ring = p.spawn(feet, 0x16, 2).expect("ring");
    assert_eq!(p.slots[ring].life, 16);
    assert_ne!(p.slots[ring].flags & 0x10, 0, "rings lie flat");
    for _ in 0..5 {
        p.spawn([feet[0], feet[1] - 0x3000, feet[2]], 4, 4);
    }
    let fire = p.spawn([feet[0], feet[1] - 0x4000, feet[2]], 0x23, 0xE).expect("fireball");
    let before = p.slots[ring];
    p.tick(&world, (feet, 0));
    let after = p.slots[ring];
    assert!(after.pos[1] < feet[1] && after.size[0] < before.size[0], "ring should ride up and shrink: {:?}", after);
    let sprites = ts2::hud::sprite_table(&e, 1);
    let texture = |slot: i16| {
        let name = format!("tex{slot:02}");
        let i = n.textures.iter().position(|t| t.name.eq_ignore_ascii_case(&name))?;
        Some((i, [256.0, 256.0]))
    };
    let tris: usize = p.batches(&sprites, &texture, glam::Vec3::X, glam::Vec3::NEG_Y).iter().map(|b| b.vertices.len() / 3).sum();
    assert_eq!(tris, 2 * p.live(), "every live particle draws");
    // The fireball falls and bursts into dust with a thud (death action 3, sound 0xB).
    let mut burst = false;
    for _ in 0..200 {
        p.tick(&world, (feet, 0));
        burst |= p.events.iter().any(|ev| matches!(ev, ts2::particles::Event::Sound(0xB, _)));
        p.events.clear();
    }
    eprintln!("fireball slot {fire}: burst {burst}, {} live", p.live());
    assert!(burst, "fireball didn't burst on the floor");
}

/// Level 1's list 58 sparkles twinkle near the camera (0x71 box spots, 0x73 rewards) and a
/// put-out reward stops.
#[test]
fn reward_sparkles_twinkle_near_the_camera() {
    let Some(game) = std::env::var_os("TS2_GAME_DIR").map(std::path::PathBuf::from) else { return };
    let e = ts2::formats::exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let lists = ts2::formats::leveldat::parse(&std::fs::read(game.join("data/level01/level.dat")).unwrap()).unwrap();
    let mut r = ts2::rewards::Rewards::new(&lists);
    let pts = &lists.iter().find(|l| l.kind == 58).unwrap().points;
    eprintln!("list 58: {} points, split {}", pts.len(), r.split);
    assert!(r.split > 0 && r.split < pts.len());
    let mut parts = ts2::particles::Particles::new(Some(&e), std::fs::read(game.join("data/rand.dat")).unwrap());
    // Camera at reward 0.
    let at = pts[r.split + 1].map(|v| v * 32);
    parts.camera = at;
    for _ in 0..64 {
        r.tick(at, &mut parts);
    }
    let live = |p: &ts2::particles::Particles| p.slots.iter().filter(|s| s.life > 0 && s.typ == 0x73).count();
    assert!(live(&parts) > 0, "reward sparkles");
    for k in 0..pts.len() - r.split - 1 {
        r.put_out(k, 1, &mut parts);
    }
    for _ in 0..400 {
        parts.slots.iter_mut().for_each(|s| s.life = (s.life - 1).max(0));
        r.tick(at, &mut parts);
    }
    assert_eq!(live(&parts), 0, "all put out");
}

/// Zurg's shot metadata must not be interpreted as vertical velocity/gravity.
#[test]
fn zurg_projectiles_home_bounce_and_end_in_the_hole() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let e = Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(game.join("data/level02/TERR1.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let mut parts = Particles::new(Some(&e), vec![128; 2048]);
    let origin = [0, -100000, 65536];
    let buzz = [50000, -80000, 65536];
    parts.camera = origin;
    // Launched as 0x42B3A0 does, facing (roughly) the way he is: yaw 0x300 against 0x400 to him.
    let homing = parts.create(origin, [0, -2, 0], 0x300 << 2, 0, 0, 0x6d).unwrap();
    assert_ne!(parts.slots[homing].flags & 8, 0);
    let distance = |at: [i32; 3]| (0..3).map(|k| (at[k] as f64 - buzz[k] as f64).powi(2)).sum::<f64>().sqrt();
    let start = distance(origin);
    // 0x410F40: type 0x6d has frame period byte 0xC4, so its yaw turns by error / ((life/16)^2
    // + 4) at speed 1/8: weak steering at first, firmer as its life runs down.
    let mut closest = f64::MAX;
    for i in 0..150 {
        parts.tick(&world, (buzz, 0));
        if i == 29 {
            assert!(parts.slots[homing].life > 0);
            assert!(parts.slots[homing].pos[0] > origin[0], "shot turns towards Buzz");
        }
        closest = closest.min(distance(parts.slots[homing].pos));
    }
    assert!(closest < start * 0.5, "shot approaches Buzz: {closest} vs {start}");
    let ball = parts.create([0, -77000, 65536], [0, 1024, 0], 256, 0, 0, 0x6c).unwrap();
    let mut bounced = false;
    for _ in 0..10 {
        parts.tick(&world, (buzz, 0));
        bounced |= parts.slots[ball].vel[1] < 0;
    }
    assert!(bounced, "ball bounces on arena floor");
    let hole = parts.create([-0xbd7c, -100000, 0x99], [0; 3], 0, 0, 0, 0x6c).unwrap();
    parts.tick(&world, (buzz, 0));
    assert_eq!(parts.slots[hole].life, 0, "central hole consumes shots");
}
