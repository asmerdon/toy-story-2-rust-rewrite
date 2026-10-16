//! Diagnostic: Buzz jumps up under the middle of each ledge bar (list 60); prints whether he
//! grabs it and the animation states he goes through. `bars <levelNN dir>` (TS2_GAME_DIR).
use ts2::formats::{leveldat, terrain};
use ts2::game::{Player, World};
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&std::fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let ledges = leveldat::segments(&lists, 60);
    for (i, [a, b]) in ledges.iter().enumerate() {
        let m = [(a[0] + b[0]) / 2, a[1], (a[2] + b[2]) / 2].map(|v| v as f32 / 32.0);
        let Some(floor) = world.floor_below(m[0], m[2], m[1] + 10.0) else {
            println!("bar {i}: no floor below");
            continue;
        };
        let mut p = Player::new(glam::Vec3::new(m[0], floor, m[2]));
        p.set_ledges(ledges.clone(), Vec::new());
        p.on_floor = true;
        let mut states = Vec::new();
        let mut peak = f32::MAX;
        for tick in 0..200 {
            p.tick_pad(&world, [0.0; 2], 0, tick < 20 || (30..50).contains(&tick), tick == 0 || tick == 30, false);
            peak = peak.min(p.pos.y);
            if states.last() != Some(&p.state) {
                states.push(p.state);
            }
        }
        println!("bar {i}: {:?}→{:?}, floor {floor:.0} (bar {:.0} above), states {states:?}, peak {:.0} above the floor", a, b, floor - m[1], floor - peak);
    }
}
