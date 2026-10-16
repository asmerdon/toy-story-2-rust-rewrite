//! Diagnostic: the Tin Man's fight with Buzz standing at a distance from his home.
//! cargo run --release --example tinman -- <distance> (TS2_GAME_DIR set)
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
fn main() {
    let d: f32 = env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(1500.0);
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = ts2::creatures::Creatures::new(&spawns, Some(&e), fs::read(game.join("data/rand.dat")).unwrap());
    cr.load_shapes(&game.join("data"));
    let mut specials = ts2::specials::Specials::new(&n);
    let mut level = ts2::level1::Level1::new(&world, &mut specials);
    level.set_story(&e, &lists);
    let home = glam::Vec3::from(cr.slots[8].home.map(|v| v as f32 / 32.0));
    let mut at = home + glam::Vec3::new(d, 0.0, 0.0);
    at.y = world.floor_below(at.x, at.z, at.y - 400.0).unwrap_or(at.y);
    let mut p = ts2::game::Player::new(at);
    p.on_floor = true;
    for tick in 0..2000 {
        let steps = cr.tick(&world, p.pos_obj(), p.pos_obj());
        level.tick_characters(&world, &p, &mut cr, 0);
        if level.cutscene.is_some() {
            level.cutscene = None;
        }
        let c = &cr.slots[8];
        let chase = steps.iter().find(|(i, _)| *i == 8).map(|(_, s)| s.chase);
        if tick % 50 == 0 {
            let pos = glam::Vec3::from(c.pos.map(|v| v as f32 / 32.0));
            println!("{tick}: state {} anim {} chase {chase:?} flags {:#x} dist to Buzz {:.0} from home {:.0} pc {}", cr.boss.state, c.anim, c.flags, (pos - at).length(), (pos - home).length(), 0);
        }
    }
}
