//! Diagnostic: a level file's special objects by distance from a point, with their layer and
//! whether 0x4BC460's per-instance distance tests (layer 0 < 12000, layer 1 > 1000) draw them.
//! Run: cargo run --release --example specials_near -- data/level04/level1.ngn x y z [max]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").unwrap());
    let n = ts2::formats::ngn::parse(&std::fs::read(game.join(&a[1])).unwrap()).unwrap();
    let eye: [f32; 3] = [a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap()];
    let max: f32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(f32::MAX);
    let mut rows = Vec::new();
    for s in &n.specials {
        let Some(inst) = n.layers.get(s.layer).and_then(|l| l.instances.get(s.instance)) else { continue };
        let Some(mesh) = n.layers[s.layer].meshes.get(inst.mesh as usize) else { continue };
        let c = ts2::scene::instance_centre(mesh, inst);
        let d = (0..3).map(|k| (c[k] - eye[k]).powi(2)).sum::<f32>().sqrt();
        if d <= max {
            rows.push((d, s.id, s.layer, s.instance, inst.mesh, mesh.vertices.len(), c, inst.flags & 0xFF));
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (d, id, layer, i, m, nv, c, room) in rows {
        let drawn = if layer == 0 { d < 12000.0 } else { d > 1000.0 };
        println!("id {id:#04x} layer {layer} inst {i:4} mesh {m:3} verts {nv:4} room {room:3} centre [{:.0}, {:.0}, {:.0}] dist {d:.0} drawn {drawn}", c[0], c[1], c[2]);
    }
}
