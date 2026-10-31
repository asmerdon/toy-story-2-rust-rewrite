//! Diagnostic: level 13's conveyor floors (surfaces 0x20–0x24, pushed by 0x42CA60 through
//! 0x48B660). Drops Buzz on many points of each belt, lets the belt carry him for 10 seconds
//! and reports where he ends: on a floor inside the level, or not.
//! cargo run --release --example belts (TS2_GAME_DIR set)
use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::terrain;
use ts2::game::{Player, World};
fn force(s: Option<u8>) -> [i32; 3] {
    match s {
        Some(0x20) => [0, 0, 0x400],
        Some(0x21) => [0x400, 0, 0],
        Some(0x22) => [-0x200, 0, 0x376],
        Some(0x23) => [-0x292, 0, -0x310],
        Some(0x24) => [-0x400, 0, 0],
        _ => [0; 3],
    }
}
fn main() {
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let lv = ts2::campaign::level(13).unwrap();
    let path = lv.path(&game);
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let w = World::from_terrain(&t);
    let (mut bad, mut total) = (0, 0);
    for (i, o) in t.objects.iter().enumerate() {
        let s = o.flags as u8;
        if !(0x20..=0x24).contains(&s) {
            continue;
        }
        for (tri, n) in t.object_triangles(i) {
            if n[1] > -0.5 {
                continue;
            }
            let c = Vec3::from(tri[0]) / 3.0 + Vec3::from(tri[1]) / 3.0 + Vec3::from(tri[2]) / 3.0;
            let Some(fy) = w.floor_below(c.x, c.z, c.y - 50.0) else { continue };
            let mut p = Player::new(Vec3::new(c.x, fy, c.z));
            p.on_floor = true;
            let mut worst = None;
            for tick in 0..600 {
                p.push_by(force(p.surface));
                let before = p.pos;
                p.tick(&w, [0.0, 0.0], false, false);
                // The body's centre (0xC0 + radius above the feet) must never come within 100
                // of a face on the way: the resolver keeps it 125 off.
                let lift = Vec3::Y * (6.0 + 125.0);
                if let Some(h) = w.sweep(before - lift, p.pos - before, 100.0)
                    && h.t < 1.0
                    && h.depth == 0.0
                {
                    worst = Some((tick, p.pos, Some(-h.t)));
                    break;
                }
                let under = w.floor_below(p.pos.x, p.pos.z, p.pos.y - 200.0);
                if under.is_none() || p.dying > 0 {
                    worst = Some((tick, p.pos, under));
                    break;
                }
            }
            total += 1;
            if let Some((tick, at, under)) = worst {
                bad += 1;
                println!("object {i} surface {s:#x} from {c:?}: lost at tick {tick} at {at:?} (floor under {under:?}, dying {})", p.dying);
            }
        }
    }
    println!("{bad} of {total} belt starts lost Buzz");
}
