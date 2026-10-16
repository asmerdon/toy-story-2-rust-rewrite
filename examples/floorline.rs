//! Diagnostic: floor heights along a line: `floorline <levelNN dir> x y z dx dz steps step`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let f = |i: usize| a[i].parse::<f32>().unwrap();
    let (x, y, z, dx, dz, n, s) = (f(2), f(3), f(4), f(5), f(6), f(7) as i32, f(8));
    for k in 0..n {
        let (px, pz) = (x + dx * s * k as f32, z + dz * s * k as f32);
        println!("{px:.0},{pz:.0}: floor {:?}", world.floor_below(px, pz, y - 300.0));
    }
}
