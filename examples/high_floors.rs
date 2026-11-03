//! Diagnostic: floors more than `min` units above the ground in a box, on a 250-unit grid:
//! `high_floors <levelNN dir> x0 x1 z0 z1 min` (level units, prints x z height object).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let v: Vec<f32> = a[2..7].iter().map(|s| s.parse().unwrap()).collect();
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let mut x = v[0];
    while x <= v[1] {
        let mut z = v[2];
        while z <= v[3] {
            let mut y = -40000.0;
            while let Some((f, m, s, _)) = world.floor_below_full(x, z, y) {
                if f > -v[4] {
                    break;
                }
                println!("{x} {z} {f:.0} mover {m:?} surface {s:?}");
                y = f + 2.0;
            }
            z += 250.0;
        }
        x += 250.0;
    }
}
