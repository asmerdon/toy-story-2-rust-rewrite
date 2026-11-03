//! Diagnostic: collision floors with nothing drawn there. For a grid over a box of the level
//! (level units, 100 apart), every floor more than `min` above the ground whose height no
//! layer-0 render triangle over the same spot comes within 60 of: `bare_floors <levelNN dir>
//! x0 x1 z0 z1 min`. Prints x z height object.
use glam::Vec3;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let v: Vec<f32> = a[2..7].iter().map(|s| s.parse().unwrap()).collect();
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let layer: usize = std::env::var("LAYER").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let (batches, _) = ts2::scene::level_parts_sided(&n, |l, _, _| l == layer, None);
    let tris: Vec<[Vec3; 3]> = batches.iter().flat_map(|b| b.vertices.chunks_exact(3).map(|c| [0, 1, 2].map(|k| Vec3::from(c[k].pos)))).collect();
    let height = |t: &[Vec3; 3], x: f32, z: f32| -> Option<f32> {
        let [a, b, c] = *t;
        let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
        if d.abs() < 1e-6 { return None; }
        let u = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
        let w = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
        (u >= 0.0 && w >= 0.0 && u + w <= 1.0).then(|| u * a.y + w * b.y + (1.0 - u - w) * c.y)
    };
    let mut x = v[0];
    while x <= v[1] {
        let mut z = v[2];
        while z <= v[3] {
            let mut y = -60000.0;
            while let Some((f, m, _, _)) = world.floor_below_full(x, z, y) {
                if f > -v[4] { break; }
                let drawn = tris.iter().any(|t| height(t, x, z).is_some_and(|h| (h - f).abs() < 60.0));
                if !drawn { println!("{x} {z} {f:.0} mover {m:?}"); }
                y = f + 2.0;
            }
            z += 100.0;
        }
        x += 100.0;
    }
}
