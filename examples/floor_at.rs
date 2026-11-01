//! Diagnostic: the floors under a point (every collision triangle whose plane covers x/z),
//! with their object, mover link and height: `floor_at <levelNN dir> x z [TERRAIN.ALL]`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let (x, z): (f32, f32) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let file = a.get(4).cloned().unwrap_or_else(|| "TERRAIN.ALL".into());
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join(file)).unwrap()).unwrap();
    for (i, o) in t.objects.iter().enumerate() {
        for (p, n) in t.object_triangles(i) {
            let [a, b, c] = p.map(glam::Vec3::from);
            // Barycentric test in x/z.
            let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
            if d.abs() < 1e-6 {
                continue;
            }
            let u = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
            let v = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
            let w = 1.0 - u - v;
            if u < 0.0 || v < 0.0 || w < 0.0 {
                continue;
            }
            let y = u * a.y + v * b.y + w * c.y;
            println!(
                "object {i:4} kind {} link {:3} flags {:#06x} y {y:9.1} normal {n:?}",
                o.kind, o.linked, o.flags
            );
        }
    }
}
