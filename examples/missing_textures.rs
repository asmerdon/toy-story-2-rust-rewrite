//! Diagnostic: texture names meshes use that the level's texture block lacks, with how many
//! materials and instances refer to each: `missing_textures <level.ngn>`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n = ts2::formats::ngn::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let have: std::collections::BTreeSet<String> = n.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
    println!("{} textures in the file: {:?}", have.len(), have);
    let mut missing: std::collections::BTreeMap<String, (usize, Vec<(usize, usize)>)> = Default::default();
    for (li, layer) in n.layers.iter().enumerate() {
        for (mi, m) in layer.meshes.iter().enumerate() {
            for mat in &m.materials {
                let Some(t) = mat.texture else { continue };
                let name = m.textures.get(t as usize).cloned().unwrap_or_else(|| format!("<index {t} of {}>", m.textures.len()));
                if !have.contains(&name.to_ascii_lowercase()) {
                    let e = missing.entry(name).or_default();
                    e.0 += 1;
                    if !e.1.contains(&(li, mi)) {
                        e.1.push((li, mi));
                    }
                }
            }
        }
    }
    for (name, (count, meshes)) in missing {
        println!("missing {name}: {count} materials in meshes {:?}", &meshes[..meshes.len().min(12)]);
    }
}
