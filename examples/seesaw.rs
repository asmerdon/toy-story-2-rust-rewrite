//! Diagnostic: level 1's attic see-saw (mover / special 0xF) tipped each way: collision floor
//! against the drawn plank's top, along the plank. (TS2_GAME_DIR)
use std::{env, fs, path::PathBuf};
fn main() {
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data/level01");
    let n = ts2::formats::ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = ts2::formats::terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let piv = specials.pos_obj(0xF).unwrap().map(|v| v as f32 / 32.0);
    for tilt in [0x1C0i32, 0, -0x1C0] {
        let a = [0, 0, tilt];
        world.turn_mover(0xF, a);
        specials.set_angles(0xF, a);
        let s = specials.get(0xF).unwrap().clone();
        let mut bb = Vec::new();
        let drawn = ts2::scene::special_batches(&n, &s, &mut bb);
        print!("tilt {tilt:#x}:");
        for dx in (-8..=8).map(|k| k as f32 * 200.0) {
            let (x, z) = (piv[0] + dx, piv[2]);
            let col = world.floor_below(x, z, piv[1] - 1500.0);
            // Top of the drawn plank at this point (highest drawn triangle over it).
            let mut top = f32::MAX;
            for b in &drawn {
                for tri in b.vertices.chunks_exact(3) {
                    let p: Vec<glam::Vec3> = tri.iter().map(|v| glam::Vec3::from(v.pos)).collect();
                    let (u, v) = (p[1] - p[0], p[2] - p[0]);
                    let det = u.x * v.z - u.z * v.x;
                    if det.abs() < 1e-3 {
                        continue;
                    }
                    let (qx, qz) = (x - p[0].x, z - p[0].z);
                    let s = (qx * v.z - qz * v.x) / det;
                    let w = (u.x * qz - u.z * qx) / det;
                    if s >= 0.0 && w >= 0.0 && s + w <= 1.0 {
                        top = top.min(p[0].y + s * u.y + w * v.y);
                    }
                }
            }
            print!(" {dx:+}:{:?}/{:.0}", col.map(|v| v.round() as i32), if top == f32::MAX { 0.0 } else { top });
        }
        println!();
    }
}
