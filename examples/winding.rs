//! Diagnostic: winding of level triangles lying on walkable floor (TERRAIN.ALL): if their
//! normals nearly all point up, front faces can be told by winding. `winding <levelNN dir>`.
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let (mut up, mut down, mut down_black) = (0, 0, 0);
    for b in ts2::scene::level_batches_where(&n, |l, _, _| l == 0) {
        for tri in b.vertices.chunks_exact(3) {
            let p: Vec<glam::Vec3> = tri.iter().map(|v| glam::Vec3::from(v.pos)).collect();
            let nrm = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            if nrm.y.abs() < 0.9 {
                continue;
            }
            let c = (p[0] + p[1] + p[2]) / 3.0;
            let Some(f) = world.floor_below(c.x, c.z, c.y - 30.0) else { continue };
            if (f - c.y).abs() > 20.0 {
                continue;
            }
            let black = tri.iter().all(|v| v.color[1] < 4 && v.color[2] < 4 && v.color[3] < 4);
            if nrm.y < 0.0 {
                up += 1
            } else {
                down += 1;
                if black {
                    down_black += 1;
                }
                if down < 8 {
                    println!("down: centre {:?} black {black} blend {:?}", c.round(), b.blend);
                }
            }
        }
    }
    println!("on walkable floor: winding normal up {up}, down {down} ({down_black} black)");
}
