//! Diagnostic: the drawn bounds of render instances and of collision objects (level units):
//! `instance_bounds <levelNN dir> layer:index... obj:index...`.
use glam::Vec3;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    for w in &a[2..] {
        let pts: Vec<Vec3> = if let Some(o) = w.strip_prefix("obj:") {
            t.object_triangles(o.parse().unwrap()).into_iter().flat_map(|(p, _)| p.map(Vec3::from)).collect()
        } else {
            let (l, i) = w.split_once(':').unwrap();
            let (l, i): (usize, usize) = (l.parse().unwrap(), i.parse().unwrap());
            let (b, _) = ts2::scene::level_parts_sided(&n, |ll, ii, _| ll == l && ii == i, None);
            b.iter().flat_map(|b| b.vertices.iter().map(|v| Vec3::from(v.pos))).collect()
        };
        let lo = pts.iter().fold(Vec3::MAX, |m, p| m.min(*p));
        let hi = pts.iter().fold(Vec3::MIN, |m, p| m.max(*p));
        println!("{w}: {} points, {lo:.0}..{hi:.0}", pts.len());
    }
}
