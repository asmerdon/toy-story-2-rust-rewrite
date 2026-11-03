//! Diagnostic: which special-object ids own render instances: `special_of <levelNN dir>
//! layer:index...`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let sp = ts2::specials::Specials::new(&n);
    for w in &a[2..] {
        let (l, i) = w.split_once(':').unwrap();
        let (l, i): (usize, usize) = (l.parse().unwrap(), i.parse().unwrap());
        let ids: Vec<String> = sp.objects.iter().filter(|(_, s)| s.layer == l && s.instance == i).map(|(id, s)| format!("{id:#x} hidden {}", s.hidden)).collect();
        println!("{w}: {ids:?}");
    }
}
