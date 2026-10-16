//! Diagnostic: touch level 1's Mr Potato Head and Bo Peep through the level script; prints the
//! speech box's character count over time. (TS2_GAME_DIR)
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
fn main() {
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    for slot in [0x1Fusize, 1] {
        let mut specials = ts2::specials::Specials::new(&n);
        let mut script = ts2::script::for_level(1, &world, &mut specials, Some(&e), &lists, Some(fs::read(game.join("data/rand.dat")).unwrap())).unwrap();
        script.item = env::var("ITEM").ok().and_then(|v| v.parse().ok()).unwrap_or(script.item);
        let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
        let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
        let mut cr = ts2::creatures::Creatures::new(&spawns, Some(&e), Vec::new());
        cr.load_shapes(&game.join("data"));
        let at = glam::Vec3::from(cr.slots[slot].pos.map(|v| v as f32 / 32.0));
        let p = ts2::game::Player::new(at);
        cr.tick(&world, p.pos_obj(), p.pos_obj());
        cr.slots[slot].flags |= 0x200;
        let mut counts = Vec::new();
        for tick in 0..300 {
            script.step_creatures(&world, &mut specials, &p, &mut cr, 0);
            if let Some(c) = script.cutscene.as_mut() {
                c.tick(false, false);
                if tick % 30 == 0 {
                    counts.push(c.dialog.quads().len());
                }
            } else if tick % 30 == 0 {
                counts.push(0);
            }
        }
        println!("slot {slot:#x} (id {}): quads {counts:?}", cr.slots[slot].id);
    }
}
