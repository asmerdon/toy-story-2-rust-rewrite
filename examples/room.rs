//! Diagnostic: the room-map room at points. cargo run --example room -- <levelNN dir> x y z
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let w = ts2::game::World::from_terrain(&t);
    let f = |i: usize| a[i].parse::<f32>().unwrap();
    for dy in [-400.0, -100.0, -20.0, 0.0, 20.0] {
        let p = glam::Vec3::new(f(2), f(3) + dy, f(4));
        println!("{p:?}: room {:?}", w.room_at(p));
    }
}
