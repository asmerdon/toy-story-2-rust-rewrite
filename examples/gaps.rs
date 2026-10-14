//! Diagnostic: walkable floor in the visible level (near mesh set, upward faces) with no
//! collision floor at that height. Prints clusters of such spots.
//! cargo run --release --example gaps -- <levelNN dir>
use glam::Vec3;
use ts2::formats::{ngn, terrain};
use ts2::game::World;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("level dir"));
    let n = ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let batches = ts2::scene::level_batches_where(&n, |l, _, i| l == 0 && i.flags & 0xFFFF != 0);
    let mut gaps: Vec<(Vec3, f32)> = Vec::new();
    let (mut tested, mut area_all) = (0usize, 0.0f32);
    for b in &batches {
        for tri in b.vertices.chunks_exact(3) {
            let [a, bb, c] = [0, 1, 2].map(|k| Vec3::from(tri[k].pos));
            let nrm = (bb - a).cross(c - a);
            let area = nrm.length() / 2.0;
            if area < 400.0 {
                continue;
            }
            let nn = nrm / (2.0 * area);
            // Floors face up: −Y in game space (either winding).
            if nn.y.abs() < 0.8 {
                continue;
            }
            area_all += area;
            // Sample points inside the triangle.
            for (u, v) in [(1.0 / 3.0, 1.0 / 3.0), (0.6, 0.2), (0.2, 0.6), (0.2, 0.2)] {
                let p = a + (bb - a) * u + (c - a) * v;
                tested += 1;
                let hit = world.floor_below(p.x, p.z, p.y - 40.0).is_some_and(|y| (y - p.y).abs() < 40.0);
                if !hit {
                    gaps.push((p, area));
                }
            }
        }
    }
    // Cluster by 400-unit cells.
    let mut cells: std::collections::BTreeMap<(i32, i32, i32), (usize, f32)> = Default::default();
    for (p, a) in &gaps {
        let e = cells.entry(((p.x / 400.0).floor() as i32, (p.y / 200.0).floor() as i32, (p.z / 400.0).floor() as i32)).or_default();
        e.0 += 1;
        e.1 += a / 4.0;
    }
    let mut v: Vec<_> = cells.into_iter().collect();
    v.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap());
    for ((x, y, z), (k, a)) in v.iter().take(40) {
        println!("cell x {} y {} z {}: {k} points, ~{a:.0} sq units", x * 400, y * 200, z * 400);
    }
    println!("{} of {tested} floor points have no collision floor ({} cells); floor area {area_all:.0}", gaps.len(), v.len());
}
