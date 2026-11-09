//! Diagnostic: which creature nodes carry flags 1, 2 and 4 (0x4CAA60: 1 skipped, 2 billboard,
//! 4 lit by the hardware lights), with their vertex colours and normals.
//! `node_flags <level.ngn>...`.
fn main() {
    for path in std::env::args().skip(1) {
        let n = ts2::formats::ngn::parse(&std::fs::read(&path).unwrap()).unwrap();
        println!("{path}");
        for c in &n.creatures {
            let mut lit = 0;
            let mut parts = Vec::new();
            for node in &c.nodes {
                if node.flags & 4 != 0 {
                    lit += 1;
                }
                if let Some(m) = &node.mesh {
                    let v = &m.vertices;
                    let (lo, hi) = v.iter().fold(([255u8; 3], [0u8; 3]), |(mut a, mut b), x| {
                        for k in 0..3 {
                            a[k] = a[k].min(x.color[k + 1]);
                            b[k] = b[k].max(x.color[k + 1]);
                        }
                        (a, b)
                    });
                    parts.push(format!("{}:{:x}:{:?}-{:?}", node.name, node.flags, lo, hi));
                }
            }
            println!("  {} id {}: {} nodes, {lit} with flag 4; patch verts {}", c.name, c.id, c.nodes.len(), c.shared_vertices.len());
            for p in parts.iter().take(6) {
                println!("    {p}");
            }
        }
    }
}
