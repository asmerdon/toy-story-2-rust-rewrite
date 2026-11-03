//! Diagnostic: coins high up and what is under each: the collision floor and the nearest
//! render surface below it (any layer): `coins_over <levelNN dir> level min` (TS2_GAME_DIR).
use glam::Vec3;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let level: usize = a[2].parse().unwrap();
    let min: f32 = a[3].parse().unwrap();
    let game = std::path::PathBuf::from(std::env::var_os("TS2_GAME_DIR").unwrap());
    let e = ts2::formats::exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let pk = ts2::pickups::Pickups::new(level, &std::fs::read(dir.join("level.dat")).unwrap(), Some(&e)).unwrap();
    let (b, _) = ts2::scene::level_parts_sided(&n, |_, _, _| true, None);
    let tris: Vec<[Vec3; 3]> = b.iter().flat_map(|b| b.vertices.chunks_exact(3).map(|c| [0, 1, 2].map(|k| Vec3::from(c[k].pos)))).collect();
    for p in pk.items.iter().filter(|p| p.is_sprite() && (p.pos[1] as f32) < -min) {
        let (x, y, z) = (p.pos[0] as f32, p.pos[1] as f32, p.pos[2] as f32);
        let floor = world.floor_below(x, z, y);
        let drawn = tris.iter().filter_map(|t| {
            let [a, b, c] = *t;
            let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
            if d.abs() < 1e-6 { return None; }
            let u = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
            let w = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
            (u >= 0.0 && w >= 0.0 && u + w <= 1.0).then(|| u * a.y + w * b.y + (1.0 - u - w) * c.y)
        }).filter(|h| *h >= y).fold(f32::MAX, f32::min);
        println!("coin {:?}: floor {floor:?}, drawn below at {}", p.pos, if drawn == f32::MAX { "none".into() } else { format!("{drawn:.0}") });
    }
}
