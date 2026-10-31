//! Diagnostic: per room-map room, the extent of the floors that lie in it (grid of `step`).
//! cargo run --release --example roomsurvey -- <level> [step]
use std::{collections::BTreeMap, env, fs, path::PathBuf};
use ts2::formats::terrain;
fn main() {
    let a: Vec<String> = env::args().collect();
    let n: usize = a[1].parse().unwrap();
    let step: f32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(500.0);
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let path = ts2::campaign::level(n).unwrap().path(&game);
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let w = ts2::game::World::from_terrain(&t);
    let mut rooms: BTreeMap<Option<u8>, ([f32; 3], [f32; 3], usize)> = BTreeMap::new();
    let mut x = w.lo.x;
    while x <= w.hi.x {
        let mut z = w.lo.z;
        while z <= w.hi.z {
            let mut y = w.lo.y - 10.0;
            while let Some(f) = w.floor_below(x, z, y) {
                let r = w.room_at(glam::Vec3::new(x, f, z));
                let e = rooms.entry(r).or_insert(([f32::MAX; 3], [f32::MIN; 3], 0));
                for (k, v) in [x, f, z].into_iter().enumerate() {
                    e.0[k] = e.0[k].min(v);
                    e.1[k] = e.1[k].max(v);
                }
                e.2 += 1;
                y = f + 300.0;
            }
            z += step;
        }
        x += step;
    }
    for (r, (lo, hi, c)) in rooms {
        println!("room {r:?}: {c} floor samples, {lo:?}..{hi:?}");
    }
}
