//! Diagnostic: a level's creature slot tick by tick with Buzz held at a point: animation,
//! frame time, state, angles and the script's look-at. `bossanim <campaign level> slot x y z [ticks]`
use std::{env, fs, path::PathBuf};
use ts2::{
    campaign,
    creatures::Creatures,
    formats::{exe, leveldat, ngn, packets, terrain},
    game::{Player, World},
};
fn main() {
    let a: Vec<String> = env::args().collect();
    let level: usize = a[1].parse().unwrap();
    let slot: usize = a[2].parse().unwrap();
    let at: [f32; 3] = [
        a[3].parse().unwrap(),
        a[4].parse().unwrap(),
        a[5].parse().unwrap(),
    ];
    let ticks: u32 = a.get(6).and_then(|s| s.parse().ok()).unwrap_or(400);
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let path = campaign::level(level).unwrap().path(&game);
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(&path).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(path.with_extension("dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let id = campaign::level(level).unwrap().internal_id;
    let mut script = ts2::script::for_level(
        id,
        &world,
        &mut specials,
        Some(&e),
        &lists,
        Some(random.clone()),
    )
    .unwrap();
    let pk = packets::parse(&fs::read(path.with_extension("raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.always_active = true;
    cr.load_shapes(&game.join("data"));
    cr.load_animations(&n);
    let mut p = Player::new(glam::Vec3::from(at));
    p.on_floor = true;
    for tick in 0..ticks {
        cr.tick(&world, p.pos_obj(), p.pos_obj());
        script.step_creatures(&world, &mut specials, &p, &mut cr, 0);
        cr.hurts.clear();
        let c = &cr.slots[slot];
        if tick % 10 == 0 {
            println!(
                "tick {tick}: id {} anim {} time {:#x} state {} angles {:?} pos {:?} look {:?}",
                c.id,
                c.anim,
                c.time,
                c.rec.state,
                c.angles,
                c.pos.map(|v| v / 32),
                script.look.map(|l| l.ticks)
            );
        }
    }
}
