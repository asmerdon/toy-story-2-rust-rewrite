//! Diagnostic: a level's special objects in the far (low-detail) set, each with the nearest
//! detailed special: `far_specials <levelNN dir>` (level units).
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let sp = ts2::specials::Specials::new(&n);
    let mut ids: Vec<_> = sp.objects.keys().copied().collect();
    ids.sort();
    for id in ids {
        let s = &sp.objects[&id];
        if s.layer == 0 {
            continue;
        }
        let near = sp.objects.iter().filter(|(_, o)| o.layer == 0).map(|(k, o)| (*k, (0..3).map(|i| (o.pos[i] - s.pos[i]).powi(2)).sum::<f32>().sqrt() as i32)).min_by_key(|x| x.1);
        println!("far {id:#x} at {:?} nearest detailed {near:x?}", s.pos.map(|v| v as i32));
    }
}
