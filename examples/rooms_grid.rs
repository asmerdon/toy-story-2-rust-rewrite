//! Diagnostic: a level's portals, and up to three reachable spots (pickups) in each room.
//! `rooms_grid <level.ngn> <TERRAIN.ALL> <level.dat>`.
use glam::Vec3;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let p = ts2::scene::Portals::parse(&std::fs::read(&a[1]).unwrap());
    eprintln!("{} portal polygons, {} links", p.polys.len(), p.links.len());
    let t = ts2::formats::terrain::parse(&std::fs::read(&a[2]).unwrap()).unwrap();
    let w = ts2::game::World::from_terrain(&t);
    let lists = ts2::formats::leveldat::parse(&std::fs::read(&a[3]).unwrap()).unwrap();
    let mut seen: std::collections::BTreeMap<u8, Vec<[i32; 3]>> = Default::default();
    for l in lists.iter().filter(|l| l.kind == 0x3F) {
        for q in &l.quads {
            let at = Vec3::new(q[0] as f32, q[1] as f32, q[2] as f32);
            let Some(y) = w.floor_below(at.x, at.z, at.y - 100.0) else { continue };
            if let Some(r) = w.room_at(Vec3::new(at.x, y - 50.0, at.z)) {
                let v = seen.entry(r as u8).or_default();
                if v.len() < 3 {
                    v.push([q[0], y as i32, q[2]]);
                }
            }
        }
    }
    for (r, v) in seen {
        for s in v {
            println!("{r} {},{},{}", s[0], s[1], s[2]);
        }
    }
}
