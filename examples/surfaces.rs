//! Diagnostic: collision objects near a point and their surface types (flags low byte):
//! `surfaces <levelNN dir> x y z radius`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let c = glam::Vec3::new(a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap());
    let r: f32 = a[5].parse().unwrap();
    for (i, o) in t.objects.iter().enumerate() {
        let tris = t.object_triangles(i);
        let near = tris.iter().any(|(tr, _)| tr.iter().any(|v| glam::Vec3::from(*v).distance(c) < r));
        if near {
            println!("object {i}: kind {} flags {:#x} (surface {}) linked {} at {:?}, {} tris", o.kind, o.flags, o.flags & 0xFF, o.linked, o.pos, tris.len());
        }
    }
}
