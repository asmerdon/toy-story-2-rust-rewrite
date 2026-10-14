//! Diagnostic: from where inside level 1's crib a laser ray reaches each crib bar.
use glam::Vec3;
fn main() {
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data/level01");
    let e = ts2::formats::exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let pk = ts2::formats::packets::parse(&std::fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(ts2::formats::packets::spawns).collect();
    let mut cr = ts2::creatures::Creatures::new(&spawns, Some(&e), Vec::new());
    cr.load_shapes(&game.join("data"));
    cr.tick(&world, cr.slots[0].pos, cr.slots[0].pos);
    world.set_mover(9, false);
    for bar in [0usize, 2] {
        cr.slots[bar].rec.state = 5;
        let (c, _) = cr.ellipsoid_at(bar);
        let mut hits = 0;
        for x in (9850..11400).step_by(100) {
            for z in (-6600..-3950).step_by(100) {
                let Some(y) = world.floor_below(x as f32, z as f32, 0.0) else { continue };
                let o = [x * 32, ((y - 384.0) * 32.0) as i32, z * 32];
                let mut d = [0, 1, 2].map(|k| c[k] - o[k]);
                let ov = Vec3::from(o.map(|v| v as f32 / 32.0));
                let dv = Vec3::from(d.map(|v| v as f32 / 32.0));
                if let Some(h) = world.sweep(ov, dv * 1.2, 1.0) {
                    d = (dv * 1.2 * h.t * 32.0).to_array().map(|v| v as i32);
                } else {
                    d = (dv * 1.2 * 32.0).to_array().map(|v| v as i32);
                }
                let saved = cr.slots[bar].health;
                if cr.laser_ray(o, &mut d, 0, 0) >= 2 {
                    hits += 1;
                }
                cr.slots[bar].health = saved;
            }
        }
        println!("bar {bar} centre {:?}: hit from {hits} spots", c.map(|v| v / 32));
    }
}
