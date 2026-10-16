//! Diagnostic: a creature's zone against the floor of its room around it.
//! cargo run --release --example arena -- <level dir> <slot> (TS2_GAME_DIR set)
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, packets, terrain};
fn main() {
    let lv = env::args().nth(1).unwrap_or("level01".into());
    let slot: usize = env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(8);
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data").join(&lv);
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let cr = ts2::creatures::Creatures::new(&spawns, Some(&e), fs::read(game.join("data/rand.dat")).unwrap());
    let c = &cr.slots[slot];
    let home = glam::Vec3::from(c.home.map(|v| v as f32 / 32.0));
    let room = world.room_at(home);
    println!("home {home:?} room {room:?} zone {:?} angle {}", c.rec.zone, c.rec.zone_angle);
    let (mut lo, mut hi) = (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN));
    let (mut n, mut out) = (0, 0);
    for i in -80..=80 {
        for j in -80..=80 {
            let (x, z) = (home.x + i as f32 * 100.0, home.z + j as f32 * 100.0);
            let Some(y) = world.floor_below(x, z, home.y - 1500.0) else { continue };
            if (y - home.y).abs() > 1500.0 || world.room_at(glam::Vec3::new(x, y, z)) != room {
                continue;
            }
            n += 1;
            lo = lo.min(glam::Vec2::new(x, z));
            hi = hi.max(glam::Vec2::new(x, z));
            let p = [(x * 32.0) as i32, (y * 32.0) as i32, (z * 32.0) as i32];
            if !ts2::creatures::in_zone(c, p) {
                out += 1;
            }
        }
    }
    println!("floor of the room near it: x {}..{}, z {}..{}; {n} samples, {out} outside the zone", lo.x, hi.x, lo.y, hi.y);
}
