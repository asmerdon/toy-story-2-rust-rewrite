//! Diagnostic: how high and far Buzz's jumps go (level 5's pier).
use glam::Vec3;
use ts2::formats::terrain;
use ts2::game::{Input, Player, World};
fn main() {
    let game = std::path::PathBuf::from(std::env::var_os("TS2_GAME_DIR").unwrap());
    let t = terrain::parse(&std::fs::read(game.join("data/level05/TERRAIN.ALL")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    for (second, hold) in [(0, 0), (0, 40), (14, 0), (20, 40), (30, 60)] {
        let mut p = Player::new(Vec3::new(3750.0, 1557.0, 25500.0));
        let (mut peak, mut far) = (f32::MAX, 0.0f32);
        for tick in 0..300 {
            let jump = tick == 60 || (second > 0 && tick == 60 + second);
            let input = Input { move_xz: [1.0, 0.0], jump, jump_held: jump || (tick >= 60 && tick < 60 + hold), ..Default::default() };
            p.update_with(&world, input, 1.0 / 60.0, &mut |_, _| {});
            peak = peak.min(p.pos.y);
            far = far.max((p.pos.x - 3750.0).abs());
        }
        println!("second {second} hold {hold}: peak y {peak} (rise {}), farthest x {far}", 1557.0 - peak);
    }
}
