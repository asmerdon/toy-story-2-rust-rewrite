//! Diagnostic: dump the 5-index primitive groups (codes 5/6) of a level's meshes with their
//! vertices, raw "normal" words and UVs. Run: cargo run --release --example sprites --
//! <level.ngn> [max groups]
use ts2::formats::ngn::{self, Prim};

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let max: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(6);
    let n = ngn::parse(&std::fs::read(&path).unwrap()).unwrap();
    let mut shown = 0;
    let mut kinds: std::collections::HashMap<(String, usize), usize> = std::collections::HashMap::new();
    for (li, layer) in n.layers.iter().enumerate() {
        for (ii, inst) in layer.instances.iter().enumerate() {
            let Some(mesh) = layer.meshes.get(inst.mesh as usize) else { continue };
            for g in &mesh.groups {
                if !matches!(g.prim, Prim::Strip | Prim::Fan) {
                    continue;
                }
                *kinds.entry((format!("{:?}", g.prim), g.indices.len())).or_default() += 1;
                if shown >= max {
                    continue;
                }
                shown += 1;
                println!("L{li} #{ii} {} pos {:?} rot {:x?} scale {:?} prim {:?} mat {:?}", mesh.name, inst.pos, inst.rot, inst.scale, g.prim, mesh.materials.get(g.material as usize).map(|m| (m.flags, m.mode)));
                for &i in &g.indices {
                    let v = &mesh.vertices[i as usize];
                    let nb = v.normal.map(|f| format!("{:08x}", f.to_bits()));
                    println!("   v{i:4} pos {:9.2?} n {:?} col {:02x?} uv {:.3?}", v.pos, nb, v.color, v.uv);
                }
            }
        }
    }
    println!("{kinds:?}");
}
