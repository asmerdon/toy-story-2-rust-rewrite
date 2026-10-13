//! Diagnostic: level 1's pushable objects with their paths. Run with TS2_GAME_DIR set:
//! cargo run --release --example boxes
use ts2::formats::{exe, leveldat, ngn, terrain};

fn main() {
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&std::fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let mut sp = ts2::specials::Specials::new(&n);
    let b = ts2::boxes::Boxes::new(1, &lists, Some(&e), &world, &mut sp);
    for (k, x) in b.boxes.iter().enumerate() {
        let pts = lists.iter().find(|l| l.kind == x.path).map(|l| l.points.clone()).unwrap_or_default();
        println!("box {k}: special {} mover {} path {} at {:?}", x.special, x.mover, x.path, x.pos.map(|v| v / 32));
        println!("   path: {pts:?}");
    }
}
