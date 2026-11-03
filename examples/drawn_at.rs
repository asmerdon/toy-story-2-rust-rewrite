//! Diagnostic: every render triangle over (x, z) (level units), by layer and instance, with
//! its height there and whether it faces up: `drawn_at <levelNN dir> x z`.
use glam::Vec3;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let (x, z): (f32, f32) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    for layer in 0..n.layers.len() {
        for ii in 0..n.layers[layer].instances.len() {
            for sided in [false, true] {
                let (b, _) = ts2::scene::level_parts_sided(&n, |l, i, _| l == layer && i == ii, Some(sided));
                for t in b.iter().flat_map(|b| b.vertices.chunks_exact(3).map(|c| [0, 1, 2].map(|k| (Vec3::from(c[k].pos), c[k].color)))) {
                    let [(a, ca), (b, _), (c, _)] = t;
                    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
                    if d.abs() < 1e-6 { continue; }
                    let u = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
                    let w = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
                    if u < 0.0 || w < 0.0 || u + w > 1.0 { continue; }
                    let y = u * a.y + w * b.y + (1.0 - u - w) * c.y;
                    let nrm = (b - a).cross(c - a);
                    println!("layer {layer} instance {ii} {} y {y:.0} normal.y {:.2} colour {ca:?}", if sided { "two-sided" } else { "one-sided" }, nrm.normalize().y);
                }
            }
        }
    }
}
