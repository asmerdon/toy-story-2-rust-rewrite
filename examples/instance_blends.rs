//! Diagnostic: the blend modes and textures of a render instance's batches:
//! `instance_blends <levelNN dir> layer index`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let (l, i): (usize, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    for sided in [false, true] {
        let (b, _) = ts2::scene::level_parts_sided(&n, |ll, ii, _| ll == l && ii == i, Some(sided));
        for b in &b {
            println!("{} {:?} texture {:?} ({}) {} tris", if sided { "two-sided" } else { "one-sided" }, b.blend, b.texture, b.texture.map_or("-".into(), |t| n.textures[t].name.clone()), b.vertices.len() / 3);
        }
    }
}
