//! Diagnostic: level 12's Zurg entrance (0x42B300 / 0x42B3A0): Zurg's height against the
//! floor under him and Buzz's platform, tick by tick. cargo run --release --example zurgintro
use std::{env, fs, path::PathBuf};
use ts2::{campaign, creatures::Creatures, formats::{exe, leveldat, ngn, packets, terrain}, game::{Player, World}};
fn main() {
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let path = campaign::level(12).unwrap().path(&game);
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(&path).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(path.with_extension("dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut script = ts2::script::for_level(12, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let pk = packets::parse(&fs::read(path.with_extension("raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    println!("Zurg spawn {:?}", spawns[0].pos);
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.always_active = true;
    cr.load_shapes(&game.join("data"));
    cr.load_animations(&n);
    let mut player = Player::new(glam::Vec3::new(-3000.0, -2400.0, 1800.0));
    for tick in 0..420 {
        cr.tick(&world, player.pos_obj(), player.pos_obj());
        script.step_creatures(&world, &mut specials, &player, &mut cr, 0);
        let c = &cr.slots[0];
        let p = c.pos.map(|v| v as f32 / 32.0);
        let fl = world.floor_below(p[0], p[2], p[1] - 100.0);
        if tick % 20 == 0 || tick > 295 && tick < 310 {
            println!("tick {tick}: zurg {p:?} home y {} target y {} flags {:#x} floor under {fl:?} buzz {:?}", c.home[1] / 32, c.target[1] / 32, c.flags, player.pos);
        }
    }
}
