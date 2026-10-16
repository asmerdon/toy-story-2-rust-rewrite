//! Diagnostic: Buzz climbs pole `n` to the top, then jumps off in each of 8 directions
//! (stick held) and walks on, with the level's script running; prints the highest point.
//! cargo run --release --example rope -- <level dir> <pole> (TS2_GAME_DIR set)
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, leveldat, ngn, terrain};
use ts2::game::{pad, Player, World};
fn main() {
    let lv = env::args().nth(1).unwrap_or("level01".into());
    let n: usize = env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(10);
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data").join(&lv);
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let ng = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let level_no = exe::level_number(&dir).unwrap_or(1);
    let poles = leveldat::poles(&lists);
    let pole = poles[n];
    for d in 0..8 {
        let world = World::from_terrain(&t);
        let mut specials = ts2::specials::Specials::new(&ng);
        let mut script = ts2::script::for_level(level_no, &world, &mut specials, Some(&e), &lists, None).unwrap();
        let boxes = ts2::boxes::Boxes::new(level_no, &lists, Some(&e), &world, &mut specials);
        let a = d as f32 / 8.0 * std::f32::consts::TAU;
        let stick = [a.sin(), a.cos()];
        let mut p = Player::new(glam::Vec3::from(pole.pos.map(|v| v as f32 / 32.0)) - glam::Vec3::Y * 10.0);
        p.set_poles(poles.clone());
        p.set_ledges(leveldat::segments(&lists, 60), leveldat::segments(&lists, 62));
        let step = |p: &mut Player, specials: &mut ts2::specials::Specials, script: &mut Box<dyn ts2::script::Script>, stick, dpad, held, jump| {
            script.step(&world, specials, p, &boxes);
            p.tick_pad(&world, stick, dpad, held, jump, false);
        };
        for _ in 0..1200 {
            step(&mut p, &mut specials, &mut script, [0.0; 2], pad::UP, false, false);
        }
        let hang = p.pos.y;
        let (mut highest, mut at) = (f32::MAX, 0);
        for tick in 0..900 {
            step(&mut p, &mut specials, &mut script, stick, 0, tick < 12, tick == 0);
            if p.pos.y < highest {
                highest = p.pos.y;
                at = tick;
            }
        }
        println!("dir {d}: hang {hang:.0}, highest {highest:.0} at tick {at} ({:.0} above the hang), ends {:?}", hang - highest, p.pos.round());
    }
}
