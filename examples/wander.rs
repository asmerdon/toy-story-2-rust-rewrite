//! Diagnostic: random walks around a level; counts long falls (fell through / out of the level).
use glam::Vec3;
use ts2::formats::terrain;
use ts2::game::{Player, World};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let w = World::from_terrain(&t);
    let pk = ts2::formats::packets::parse(&std::fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let starts: Vec<Vec3> = pk.iter().flat_map(ts2::formats::packets::spawns).map(|s| Vec3::new(s.pos[0] as f32, s.pos[1] as f32 - 20.0, s.pos[2] as f32)).collect();
    let mut seed = 12345u32;
    let mut rnd = || { seed = seed.wrapping_mul(1103515245).wrapping_add(12345); (seed >> 16) as f32 / 65536.0 };
    let (mut falls, mut ticks) = (0, 0);
    for run in 0..starts.len() * 3 {
        let mut p = Player::new(starts[run % starts.len()]);
        let mut air = 0;
        let mut dir = [0.0f32; 2];
        for i in 0..3000 {
            if i % 40 == 0 { let ang = rnd() * 6.283; dir = [ang.sin(), ang.cos()]; }
            let jump = (i % 90) < 20 && rnd() < 0.5;
            p.tick(&w, dir, jump, false);
            ticks += 1;
            air = if p.on_floor { 0 } else { air + 1 };
            if air == 240 {
                falls += 1;
                eprintln!("run {run} tick {i}: falling at {:?}", p.pos.to_array().map(|v| v as i32));
                break;
            }
        }
    }
    println!("{falls} long falls in {ticks} ticks");
}
