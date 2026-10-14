//! Diagnostic: terrain objects whose bounds come within `r` of a point.
//! cargo run --example near -- <levelNN dir> x y z r
use ts2::formats::terrain;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let p: Vec<f32> = a[2..6].iter().map(|s| s.parse().unwrap()).collect();
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    for (i, o) in t.objects.iter().enumerate() {
        let tris: Vec<_> = t.shapes[o.shape].iter().flat_map(|q| q.triangles(o.pos.map(|v| v as f32))).collect();
        if tris.is_empty() {
            continue;
        }
        let lo = tris.iter().flatten().fold([f32::MAX; 3], |a, q| [0, 1, 2].map(|k| a[k].min(q[k])));
        let hi = tris.iter().flatten().fold([f32::MIN; 3], |a, q| [0, 1, 2].map(|k| a[k].max(q[k])));
        if (0..3).all(|k| p[k] >= lo[k] - p[3] && p[k] <= hi[k] + p[3]) {
            println!("obj {i:3} kind {} link {:3} flags {:#06x} polys {:3} box {:?}..{:?}", o.kind, o.linked, o.flags, tris.len(), lo.map(|v| v as i32), hi.map(|v| v as i32));
        }
    }
}
