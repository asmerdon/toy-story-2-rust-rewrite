//! Diagnostic: level instances (both layers) whose centres lie within a cone ahead of an eye,
//! nearest first, with mesh name, room byte, and whether each is a special object.
//! Run: cargo run --release --example instances_ahead -- <file.ngn> x y z dx dz [half-angle°]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").unwrap());
    let n = ts2::formats::ngn::parse(&std::fs::read(game.join(&a[1])).unwrap()).unwrap();
    let f: Vec<f32> = a[2..7].iter().map(|s| s.parse().unwrap()).collect();
    let (eye, dir) = ([f[0], f[1], f[2]], [f[3], f[4]]);
    let cone: f32 = a.get(7).and_then(|s| s.parse().ok()).unwrap_or(10.0);
    let dl = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    let mut rows = Vec::new();
    for (l, layer) in n.layers.iter().enumerate() {
        for (i, inst) in layer.instances.iter().enumerate() {
            let Some(mesh) = layer.meshes.get(inst.mesh as usize) else { continue };
            let c = ts2::scene::instance_centre(mesh, inst);
            let (dx, dz) = (c[0] - eye[0], c[2] - eye[2]);
            let h = (dx * dx + dz * dz).sqrt();
            let ang = ((dx * dir[0] + dz * dir[1]) / (h * dl)).clamp(-1.0, 1.0).acos().to_degrees();
            if ang > cone {
                continue;
            }
            let d = (h * h + (c[1] - eye[1]).powi(2)).sqrt();
            let special = n.specials.iter().find(|s| s.layer == l && s.instance == i).map(|s| s.id as i32).unwrap_or(-1);
            rows.push((d, l, i, mesh.name.clone(), inst.flags, c, special, ang));
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (d, l, i, name, flags, c, sp, ang) in rows {
        println!("L{l} inst {i:4} {name:12} flags {flags:#08x} special {sp:4} centre [{:.0}, {:.0}, {:.0}] dist {d:.0} off {ang:.1}°", c[0], c[1], c[2]);
    }
}
