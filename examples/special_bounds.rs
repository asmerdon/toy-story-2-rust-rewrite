//! Diagnostic: a special object's drawn bounds at its home (level units):
//! `special_bounds <levelNN dir> id`.
use glam::Vec3;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let id: u16 = a[2].parse().unwrap();
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let sp = ts2::specials::Specials::new(&n);
    let s = &sp.objects[&id];
    let (b, _) = ts2::scene::level_parts_sided(&n, |l, i, _| l == s.layer && i == s.instance, None);
    let pts: Vec<Vec3> = b.iter().flat_map(|b| b.vertices.iter().map(|v| Vec3::from(v.pos))).collect();
    let lo = pts.iter().fold(Vec3::MAX, |m, p| m.min(*p));
    let hi = pts.iter().fold(Vec3::MIN, |m, p| m.max(*p));
    println!("special {id} layer {} instance {} pos {:?}: {lo:.0}..{hi:.0}", s.layer, s.instance, s.pos);
}
