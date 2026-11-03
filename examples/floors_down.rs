//! Diagnostic: every floor straight down from a point, top to bottom (level units), and the
//! mover each belongs to: `floors_down <levelNN dir> x y z`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let [x, mut y, z]: [f32; 3] = [a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap()];
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    for _ in 0..12 {
        let Some((f, m, s, _)) = world.floor_below_full(x, z, y) else { break };
        println!("floor {f:9.1} mover {m:?} surface {s:?}");
        y = f + 2.0;
    }
}
