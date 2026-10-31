//! Diagnostic: the final arena's (level05/level1, campaign 15) collision and the edges of its
//! walkable floor. cargo run --release --example finalarena (TS2_GAME_DIR set)
use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, terrain};
fn main() {
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data/level05");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    // Every level's fall plane (0x436220) against its start floor.
    for lv in ts2::campaign::LEVELS {
        let path = lv.path(&game);
        let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
        let w = ts2::game::World::from_terrain(&t);
        let s = e.player_start(lv.internal_id).unwrap();
        let f = w.floor_below(s.pos[0], s.pos[2], s.pos[1] - 600.0);
        println!("level {:>2}: lowest {:>7.0} (hi.y {:>7.0}), start floor {f:?}", lv.number, w.lowest, w.hi.y);
    }
    let start = e.player_start(15).unwrap();
    println!("start {:?} facing {:#x}", start.pos, start.angle);
    let raw = fs::read(dir.join("TERR1.ALL")).unwrap();
    let d = u32::from_le_bytes(raw[0..4].try_into().unwrap()) as usize * 2;
    let n = i32::from_le_bytes(raw[d..d + 4].try_into().unwrap());
    for k in 0..n as usize {
        let r = d + 4 + 76 * k;
        let i = |o: usize| i32::from_le_bytes(raw[r + o..r + o + 4].try_into().unwrap());
        let h = |o: usize| i16::from_le_bytes(raw[r + o..r + o + 2].try_into().unwrap());
        println!(
            "rec {k}: size {} pos ({}, {}, {}) kind {:#x} link {} flags {:#x} tmpl {} inst {} raw {:02x?}",
            i(0), i(4), i(8), i(12), i(16), h(0x1A), h(0x28) as u16, h(0x44), h(0x46), &raw[r..r + 76]
        );
    }
    let t = terrain::parse(&raw).unwrap();
    for (i, o) in t.objects.iter().enumerate() {
        let tris = t.object_triangles(i);
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (p, _) in &tris {
            for v in p {
                lo = lo.min(Vec3::from(*v));
                hi = hi.max(Vec3::from(*v));
            }
        }
        println!("object {i}: kind {} flags {:#x} {} tris, bounds {lo:?}..{hi:?}", o.kind, o.flags, tris.len());
        if env::args().any(|a| a == "-v") {
            for (p, nrm) in &tris {
                println!("   {:?} n {:?}", p.map(|v| v.map(|c| c as i32)), nrm.map(|c| (c * 100.0).round() / 100.0));
            }
        }
    }
    let pk = ts2::formats::packets::parse(&fs::read(dir.join("level1.raw")).unwrap()).unwrap();
    for (k, s) in pk.iter().flat_map(ts2::formats::packets::spawns).enumerate() {
        println!("spawn {k}: creature {} at {:?}", s.creature, s.pos);
    }
    let world = ts2::game::World::from_terrain(&t);
    // Floor map: '#' floor within 300 of the start height, '.' lower floor, ' ' none.
    let s = Vec3::from(start.pos);
    let y0 = world.floor_below(s.x, s.z, s.y - 2000.0);
    println!("floor under start: {y0:?}");
    let step = 200.0;
    let (x0, x1, z0, z1) = (world.lo.x, world.hi.x, world.lo.z, world.hi.z);
    println!("world bounds {:?}..{:?}", world.lo, world.hi);
    let mut z = z1;
    while z >= z0 {
        let mut line = String::new();
        let mut x = x0;
        while x <= x1 {
            let near_start = (x - s.x).abs() < step / 2.0 && (z - s.z).abs() < step / 2.0;
            let c = match world.floor_below(x, z, s.y - 3000.0) {
                _ if near_start => 'S',
                Some(y) if (y - y0.unwrap_or(s.y)).abs() < 300.0 => '#',
                Some(y) if y < y0.unwrap_or(s.y) => '^',
                Some(_) => '.',
                None => ' ',
            };
            line.push(c);
            x += step;
        }
        println!("z {z:>8.0} |{line}|");
        z -= step;
    }
}
