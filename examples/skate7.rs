//! Diagnostic: level 7's skateboard movers (10 and 0xE) and their specials (0xB and 0x18).
use ts2::formats::{ngn, terrain};
fn main() {
    let game = std::path::PathBuf::from(std::env::var_os("TS2_GAME_DIR").unwrap());
    let dir = game.join("data/level07");
    let n = ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let specials = ts2::specials::Specials::new(&n);
    for (m, s) in [(10, 0xB), (0xE, 0x18)] {
        println!("mover {m}: {:?} enabled {} special {s}: {:?}", world.mover_pos(m), world.mover_enabled(m), specials.pos_obj(s).map(|p| p.map(|v| v >> 5)));
    }
}
