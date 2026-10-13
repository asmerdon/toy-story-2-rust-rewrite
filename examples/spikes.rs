//! Diagnostic: find long, thin triangles per primitive type (decoding errors show as spikes).
use ts2::formats::ngn;
fn main() {
    let n = ngn::parse(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let mut stats = std::collections::BTreeMap::<String, (usize, usize)>::new();
    let mut shown = 0;
    for (li, l) in n.layers.iter().enumerate() {
        for (mi, m) in l.meshes.iter().enumerate() {
            for g in &m.groups {
                let key = format!("{:?}", g.prim);
                for t in g.triangles() {
                    let p = t.map(|i| glam::Vec3::from(m.vertices[i as usize].pos));
                    let e = [(p[1] - p[0]).length(), (p[2] - p[1]).length(), (p[0] - p[2]).length()];
                    let area = (p[1] - p[0]).cross(p[2] - p[0]).length() / 2.0;
                    let long = e.iter().cloned().fold(0.0, f32::max);
                    let thin = long > 200.0 && area / (long * long) < 0.02;
                    let s = stats.entry(key.clone()).or_default();
                    s.0 += 1;
                    if thin { s.1 += 1; }
                    if thin && shown < 8 && !matches!(g.prim, ngn::Prim::Triangles | ngn::Prim::Quads) {
                        shown += 1;
                        println!("layer {li} mesh {mi} {:?} count {} idx {:?}", g.prim, g.indices.len(), &g.indices[..g.indices.len().min(16)]);
                    }
                }
            }
        }
    }
    for (k, v) in stats { println!("{k}: {} tris, {} thin+long", v.0, v.1); }
    // Per mesh: share of thin+long triangles, with an instance position.
    for (li, l) in n.layers.iter().enumerate() {
        for (mi, m) in l.meshes.iter().enumerate() {
            let (mut all, mut bad) = (0, 0);
            let mut prims = std::collections::BTreeSet::new();
            for g in &m.groups {
                for t in g.triangles() {
                    let p = t.map(|i| glam::Vec3::from(m.vertices[i as usize].pos));
                    let e = [(p[1] - p[0]).length(), (p[2] - p[1]).length(), (p[0] - p[2]).length()];
                    let area = (p[1] - p[0]).cross(p[2] - p[0]).length() / 2.0;
                    let long = e.iter().cloned().fold(0.0, f32::max);
                    all += 1;
                    if long > 200.0 && area / (long * long) < 0.02 { bad += 1; prims.insert(format!("{:?}", g.prim)); }
                }
            }
            if bad >= 8 {
                let inst = l.instances.iter().find(|i| i.mesh as usize == mi).unwrap();
                println!("layer {li} mesh {mi}: {bad}/{all} bad {:?} verts {} at {:?} rot {:x?} scale {:?}", prims, m.vertices.len(), inst.pos.map(|v| v as i32), inst.rot, inst.scale);
            }
        }
    }
}
