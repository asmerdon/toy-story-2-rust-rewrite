//! Diagnostic: every level triangle inside a box (level units), with its instance, room
//! (instance flags & 0xFF), render state ((flags >> 16) & 0xF), material mode and texture.
//! Run: cargo run --release --example hatch -- <levelNN dir> x0 y0 z0 x1 y1 z1
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let f = |i: usize| a[i].parse::<f32>().unwrap();
    let (lo, hi) = ([f(2), f(3), f(4)], [f(5), f(6), f(7)]);
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let special = |l: usize, i: usize| n.specials.iter().find(|s| s.layer == l && s.instance == i).map(|s| s.id);
    for (li, layer) in n.layers.iter().enumerate() {
        for (ii, inst) in layer.instances.iter().enumerate() {
            let Some(mesh) = layer.meshes.get(inst.mesh as usize) else { continue };
            let m = ts2::scene::instance_matrix(inst);
            for g in &mesh.groups {
                let mat = mesh.materials.get(g.material as usize);
                let mut hits = 0;
                let mut total = 0;
                let mut cols = std::collections::BTreeSet::new();
                for t in g.triangles() {
                    total += 1;
                    let p: Vec<[f32; 3]> = t.iter().map(|&i| {
                        let v = mesh.position(g, i);
                        let r = [0, 1, 2].map(|k| m[0][k] * v[0] + m[1][k] * v[1] + m[2][k] * v[2] + inst.pos[k]);
                        r
                    }).collect();
                    if p.iter().all(|q| (0..3).all(|k| q[k] >= lo[k] && q[k] <= hi[k])) {
                        hits += 1;
                        for &i in &t { cols.insert(mesh.vertices[i as usize].color); }
                    }
                }
                if hits > 0 {
                    println!("layer {li} inst {ii} pos {:?} flags {:#x} (room {}, state {}) special {:?} mesh {} group mat {} mode {:?} tex {:?}: {hits}/{total} tris; colours {:?}",
                        inst.pos, inst.flags, inst.flags & 0xFF, inst.flags >> 16 & 0xF, special(li, ii), inst.mesh, g.material,
                        mat.and_then(|m| m.mode), mesh.group_texture(g), cols.iter().take(4).collect::<Vec<_>>());
                }
            }
        }
    }
}
