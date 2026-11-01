//! Diagnostic: one collision object's triangles and normals: `object_tris <levelNN dir> index [TERRAIN.ALL]`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let i: usize = a[2].parse().unwrap();
    let file = a.get(3).cloned().unwrap_or_else(|| "TERRAIN.ALL".into());
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join(file)).unwrap()).unwrap();
    let o = &t.objects[i];
    println!(
        "object {i}: kind {} link {} flags {:#06x} pos {:?}",
        o.kind, o.linked, o.flags, o.pos
    );
    for (p, n) in t.object_triangles(i) {
        println!(
            "  {:?} n {:?}",
            p.map(|v| v.map(|c| c as i32)),
            n.map(|c| (c * 100.0).round() / 100.0)
        );
    }
}
