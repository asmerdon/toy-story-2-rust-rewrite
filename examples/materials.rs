//! Diagnostic: an instance's materials (flags, mode, texture): `materials <level.ngn> layer index`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n = ts2::formats::ngn::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let (l, i): (usize, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let inst = &n.layers[l].instances[i];
    let mesh = &n.layers[l].meshes[inst.mesh as usize];
    println!("{:#?}", mesh.materials);
    let mut modes = std::collections::BTreeMap::new();
    for layer in &n.layers {
        for m in &layer.meshes {
            for mat in &m.materials {
                *modes.entry(mat.mode).or_insert(0) += 1;
            }
        }
    }
    println!("all modes in the level: {modes:?}");
}
