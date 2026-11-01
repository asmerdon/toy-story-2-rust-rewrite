//! Diagnostic: level 9's first minion call-up, tick by tick: the pair's distance from the boss
//! (level units, horizontal), heights, the flail's health and the scene timer.
use std::{env, fs, path::PathBuf};
use ts2::{
    creatures::Creatures,
    formats::{exe, leveldat, ngn, packets, terrain},
    game::{Player, World},
};
fn main() {
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data/level09");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let mut script = ts2::script::for_level(
        9,
        &world,
        &mut specials,
        Some(&e),
        &lists,
        Some(random.clone()),
    )
    .unwrap();
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.always_active = true;
    cr.load_shapes(&game.join("data"));
    cr.load_animations(&n);
    let s0 = cr.slots[0].pos;
    let mut p = Player::new(glam::Vec3::from(
        [s0[0], s0[1], s0[2] + 0x8000].map(|v| v as f32 / 32.0),
    ));
    p.on_floor = true;
    let mut hit = false;
    let mut shown = 0;
    for tick in 0..4000 {
        cr.tick(&world, p.pos_obj(), p.pos_obj());
        script.step_creatures(&world, &mut specials, &p, &mut cr, 0);
        cr.hurts.clear();
        if tick == 450 {
            let b = cr.slots[0].pos;
            let eye = cr.node_point(0, 0, [0.0, 0.0, -400.0]);
            println!(
                "boss {:?} eye {eye:?} (level units off: {:?})",
                b,
                eye.map(|e| [0, 1, 2].map(|k| (e[k] - b[k]) / 32))
            );
        }
        if !hit
            && script.look.is_none()
            && script.boss_hud
            && cr.slots[0].rec.state == 7
            && tick > 400
        {
            cr.hit(0, 0, 3);
            hit = true;
        }
        let m = &cr.slots[8];
        let b = cr.slots[0].pos;
        if m.id != 0 && m.health > 0 && shown < 260 {
            shown += 1;
            let d = ((m.pos[0] - b[0]) as f32).hypot((m.pos[2] - b[2]) as f32) / 32.0;
            println!(
                "tick {tick}: d {d:.0} y {} boss y {} flail1 id {} health {} anim {} look {:?}",
                m.pos[1] / 32,
                b[1] / 32,
                cr.slots[1].id,
                cr.slots[1].health,
                cr.slots[1].anim,
                script
                    .look
                    .map(|l| (l.ticks, l.from.map(|v| v / 32), l.at.map(|v| v / 32)))
            );
        }
    }
}
