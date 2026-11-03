//! Diagnostic: one render instance's fields and its mesh's vertex bounds:
//! `instance_info <levelNN dir> layer index`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let (l, i): (usize, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let inst = &n.layers[l].instances[i];
    println!("{inst:?}");
    println!("rot as 16-bit: {:x?}", inst.rot.map(|r| r & 0xFFFF));
}
