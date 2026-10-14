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
