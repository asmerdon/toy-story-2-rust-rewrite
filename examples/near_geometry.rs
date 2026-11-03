//! Diagnostic: render geometry near a point (level units): triangles of each layer with a
//! vertex within `r`, their heights at the point, and billboards within `r`:
//! `near_geometry <levelNN dir> x y z r`.
use glam::Vec3;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let v: Vec<f32> = a[2..6].iter().map(|s| s.parse().unwrap()).collect();
    let (p, r) = (Vec3::new(v[0], v[1], v[2]), v[3]);
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    for layer in 0..n.layers.len() {
        for (ii, inst) in n.layers[layer].instances.iter().enumerate() {
            let (b, bb) = ts2::scene::level_parts_sided(&n, |l, i, _| l == layer && i == ii, None);
            let tris: Vec<[Vec3; 3]> = b.iter().flat_map(|b| b.vertices.chunks_exact(3).map(|c| [0, 1, 2].map(|k| Vec3::from(c[k].pos)))).collect();
            let near = tris.iter().filter(|t| t.iter().any(|q| q.distance(p) < r)).count();
            let bbs = bb.iter().filter(|b| Vec3::from(b.pivot).distance(p) < r).count();
            if near + bbs > 0 {
                let ys: Vec<i32> = tris.iter().filter(|t| t.iter().any(|q| q.distance(p) < r)).map(|t| t[0].y as i32).take(6).collect();
                println!("layer {layer} instance {ii} mesh {} flags {:#x} pos {:?}: {near} tris near (y {ys:?}), {bbs} billboards", inst.mesh, inst.flags, inst.pos);
            }
        }
    }
}
