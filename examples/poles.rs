//! Diagnostic: Buzz climbs each pole of a level to the top (UP held), then lets go; prints
//! how high he gets above the pole's top and where he lands.
//! cargo run --release --example poles -- <level dir> (TS2_GAME_DIR set)
use std::{env, fs, path::PathBuf};
use ts2::formats::{leveldat, terrain};
use ts2::game::{pad, Player, World};
fn main() {
    let lv = env::args().nth(1).unwrap_or("level01".into());
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data").join(&lv);
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let poles = leveldat::poles(&lists);
    for (i, pole) in poles.iter().enumerate() {
        let bottom = glam::Vec3::from(pole.pos.map(|v| v as f32 / 32.0));
        let top = (pole.pos[1] - pole.length) as f32 / 32.0;
        let mut p = Player::new(bottom - glam::Vec3::Y * 10.0);
        p.set_poles(poles.clone());
        let (mut highest, mut exit, mut room) = (f32::MAX, None, None);
        for tick in 0..900 {
            let up = exit.is_none();
            p.tick_pad(&world, [0.0; 2], if up { pad::UP } else { 0 }, false, false, false);
            if exit.is_none() && p.climb < 0 {
                exit = Some(tick);
            }
            if exit.is_some() {
                highest = highest.min(p.pos.y);
            }
            room = world.room_at(p.pos);
        }
        println!(
            "pole {i:2} kind {} bottom {:?} top y {top:.0}: exit {:?}, highest {:.0} ({:+.0} over the top), ends {:?} room {room:?}",
            pole.kind, pole.pos.map(|v| v / 32), exit, highest, top - highest, p.pos.round()
        );
    }
}
