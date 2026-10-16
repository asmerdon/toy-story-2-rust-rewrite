//! Diagnostic: triangles whose three vertex colours are near black, near a point:
//! `blackfaces <level.ngn> x y z radius`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n = ts2::formats::ngn::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let c = glam::Vec3::new(a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap());
    let r: f32 = a[5].parse().unwrap();
    for (li, layer) in n.layers.iter().enumerate() {
        for ii in 0..layer.instances.len() {
            for batch in ts2::scene::level_batches_where(&n, |x, y, _| x == li && y == ii) {
                for t in batch.vertices.chunks_exact(3) {
                    let p: Vec<glam::Vec3> = t.iter().map(|v| glam::Vec3::from(v.pos)).collect();
                    let m = (p[0] + p[1] + p[2]) / 3.0;
                    if m.distance(c) > r || !t.iter().all(|v| v.color[1] < 12 && v.color[2] < 12 && v.color[3] < 12) {
                        continue;
                    }
                    let nrm = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
                    println!("L{li}#{ii} flags {:#x} tex {:?} {:?} centre {:?} normal {:.2?} colours {:?}", layer.instances[ii].flags, batch.texture.map(|k| n.textures[k].name.clone()), batch.blend, m.round(), nrm, t.iter().map(|v| v.color).collect::<Vec<_>>());
                }
            }
        }
    }
}
