//! Diagnostic: a map of floor surface types around a point, one letter a cell:
//! `surfaces_grid <terrain file> x z step [from_y]`. `.` no floor; `-` ordinary (0xFF).
use ts2::formats::terrain;
use ts2::game::World;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = terrain::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let w = World::from_terrain(&t);
    let (x, z, step): (f32, f32, f32) = (a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap());
    let from: f32 = a.get(5).map_or(-400.0, |v| v.parse().unwrap());
    for j in -14..=14 {
        let mut s = String::new();
        for i in -30..=30 {
            s.push(match w.floor_below_full(x + i as f32 * step, z + j as f32 * step, from).map(|f| f.2) {
                None => '.',
                Some(None) => '-',
                Some(Some(k)) => char::from_digit((k % 36) as u32, 36).unwrap(),
            });
        }
        println!("{s}");
    }
}
