//! Diagnostic: every mover (kind-8 object with a link) of a level: id, position, size:
//! `movers_list <levelNN dir>`.
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    for (i, o) in t.objects.iter().enumerate().filter(|(_, o)| o.linked != 0) {
        let tris = t.object_triangles(i);
        let (lo, hi) = tris.iter().flat_map(|(p, _)| p.iter()).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), v| ([0, 1, 2].map(|k| lo[k].min(v[k])), [0, 1, 2].map(|k| hi[k].max(v[k]))));
        println!("mover {:#x} object {i} kind {} pos {:?} size {:?}", o.linked - 1, o.kind, o.pos, [0, 1, 2].map(|k| (hi[k] - lo[k]) as i32));
    }
}
