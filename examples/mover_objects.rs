//! Diagnostic: a level's terrain objects linked to mover `m` (link m + 1), with their kind,
//! flags, triangle count and the heights of their upward faces: `mover_objects <dir> m`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let m: i16 = a[2].parse().unwrap();
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    for (i, o) in t.objects.iter().enumerate().filter(|(_, o)| o.linked == m + 1) {
        let tris = t.object_triangles(i);
        let mut ups: Vec<i32> = tris.iter().filter(|(_, n)| glam::Vec3::from(*n).normalize_or_zero().y < -0.7).map(|(p, _)| p[0][1] as i32).collect();
        ups.sort();
        ups.dedup();
        let (lo, hi) = tris.iter().flat_map(|(p, _)| p.iter()).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), v| ([0, 1, 2].map(|k| lo[k].min(v[k])), [0, 1, 2].map(|k| hi[k].max(v[k]))));
        println!("object {i} kind {} flags {:#x} {} tris, box {lo:?}..{hi:?}, up faces at {ups:?}", o.kind, o.flags, tris.len());
    }
}
