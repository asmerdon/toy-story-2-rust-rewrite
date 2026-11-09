//! Diagnostic: flights of stairs in a level: from each floor point, walks along ±x and ±z
//! climbing steps of 30–200 units, and prints runs of at least 5 steps (start, direction, end).
//! `find_stairs --level 1`
use std::{env, fs, path::PathBuf};
use ts2::formats::terrain;
use ts2::game::World;

fn main() {
    let a: Vec<String> = env::args().collect();
    let only: usize = a.iter().position(|x| x == "--level").map_or(1, |i| a[i + 1].parse().unwrap());
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let lv = ts2::campaign::LEVELS.iter().find(|l| l.number == only).unwrap();
    let t = terrain::parse(&fs::read(terrain::path_for(&lv.path(&game)).unwrap()).unwrap()).unwrap();
    let w = World::from_terrain(&t);
    let mut x = w.lo.x;
    let mut seen = std::collections::HashSet::new();
    while x <= w.hi.x {
        let mut z = w.lo.z;
        while z <= w.hi.z {
            let mut y = w.lo.y - 10.0;
            while let Some(f) = w.floor_below(x, z, y) {
                for (dx, dz) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    let (mut px, mut pz, mut h, mut steps, mut go) = (x, z, f, 0, 0);
                    while go < 300 {
                        go += 1;
                        px += dx * 20.0;
                        pz += dz * 20.0;
                        // The next floor at most 200 above where he stands.
                        let Some(g) = w.floor_below(px, pz, h - 200.0) else { break };
                        let up = h - g;
                        if up > 200.0 || up < -2.0 {
                            break;
                        }
                        if up >= 30.0 {
                            steps += 1;
                        }
                        h = g;
                    }
                    let (ex, ez) = (px - dx * 20.0, pz - dz * 20.0);
                    if steps >= 5 && seen.insert(((ex / 400.0) as i32, (h / 200.0) as i32, (ez / 400.0) as i32, dx as i32, dz as i32)) {
                        println!("{steps} steps from ({x:.0} {f:.0} {z:.0}) dir ({dx} {dz}) to ({ex:.0} {h:.0} {ez:.0}) room {:?}", w.room_at(glam::Vec3::new(ex, h - 50.0, ez)));
                    }
                }
                y = f + 300.0;
            }
            z += 80.0;
        }
        x += 80.0;
    }
}
