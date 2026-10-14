//! Diagnostic: floor heights along a line. cargo run --example probe -- <levelNN dir> x z0 z1 step from_y [xaxis]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let w = ts2::game::World::from_terrain(&t);
    let f = |i: usize| a[i].parse::<f32>().unwrap();
    let (c, s0, s1, step, from) = (f(2), f(3), f(4), f(5), f(6));
    let xaxis = a.len() > 7;
    let mut s = s0;
    while (step > 0.0 && s <= s1) || (step < 0.0 && s >= s1) {
        let (x, z) = if xaxis { (s, c) } else { (c, s) };
        println!("{x:7.0} {z:7.0}: below {:?}  near {:?}", w.floor_below(x, z, from), w.floor_near(x, z, from));
        s += step;
    }
}
