//! Diagnostic: where a floor of surface `kind` meets ordinary floor at the same height:
//! `oil_edges <terrain file> kind`.
use ts2::formats::terrain;
use ts2::game::World;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = terrain::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let w = World::from_terrain(&t);
    let kind: u8 = a[2].parse().unwrap();
    let mut n = 0;
    for ix in -300..300 {
        for iz in -300..300 {
            let (x, z) = (ix as f32 * 120.0, iz as f32 * 120.0);
            let mut y = -60000.0f32;
            for _ in 0..12 {
                let Some((f, _, s, _)) = w.floor_below_full(x, z, y) else { break };
                if s == Some(kind) {
                    for (dx, dz) in [(120.0, 0.0), (-120.0, 0.0), (0.0, 120.0), (0.0, -120.0)] {
                        if let Some((g, _, None, _)) = w.floor_below_full(x + dx, z + dz, f - 200.0)
                            && (g - f).abs() < 40.0
                            && w.sweep(glam::Vec3::new(x, f - 70.0, z), glam::Vec3::new(dx, 0.0, dz), 30.0).is_none()
                            && n < 30
                        {
                            println!("oil at {x} {f} {z} meets ordinary floor at {} {g} {}", x + dx, z + dz);
                            n += 1;
                        }
                    }
                }
                y = f + 1.0;
            }
        }
    }
    println!("{n} edges listed");
}
