//! Diagnostic: height of each level 5 pickup above the floor below it.
use ts2::formats::{exe, terrain};
fn main() {
    let game = std::path::PathBuf::from(std::env::var_os("TS2_GAME_DIR").unwrap());
    let e = exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let dir = game.join("data/level05");
    let dat = std::fs::read(dir.join("level.dat")).unwrap();
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let pk = ts2::pickups::Pickups::new(5, &dat, Some(&e)).unwrap();
    for p in &pk.items {
        let f = world.floor_below(p.pos[0] as f32, p.pos[2] as f32, p.pos[1] as f32 - 10.0);
        println!("id {:#04x} {:?} {:?}: above floor {:?}", p.id, p.kind, p.pos, f.map(|f| f - p.pos[1] as f32));
    }
}
