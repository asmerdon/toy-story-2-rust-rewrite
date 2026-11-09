//! Diagnostic: the follow camera while Buzz climbs each pole of a level, from four facings.
//! Per pole, the ticks the camera ended behind a wall (more than 8 units through one, looking
//! at Buzz) and how deep it went. `camera_poles --level 1`
use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::{leveldat, terrain};
use ts2::game::{Player, World, pad};

fn main() {
    let a: Vec<String> = env::args().collect();
    let only: usize = a.iter().position(|x| x == "--level").map_or(1, |i| a[i + 1].parse().unwrap());
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let lv = ts2::campaign::LEVELS.iter().find(|l| l.number == only).unwrap();
    let path = lv.path(&game);
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let w = World::from_terrain(&t);
    let lists = leveldat::parse(&fs::read(path.with_file_name("level.dat")).unwrap()).unwrap();
    let poles = leveldat::poles(&lists);
    let (mut total, mut ticks) = (0, 0);
    for (i, pole) in poles.iter().enumerate() {
        let (mut behind, mut deepest) = (0, 0.0f32);
        for k in 0..4 {
            let at = Vec3::from(pole.pos.map(|v| v as f32 / 32.0));
            let mut p = Player::new(at + Vec3::new(0.0, -2.0, 0.0));
            p.set_poles(poles.clone());
            p.set_facing(k * 0x400);
            p.on_floor = true;
            let mut cam = ts2::camera::Camera::new(&p);
            for tick in 0..600 {
                p.tick_pad(&w, [0.0; 2], pad::UP, false, false, false);
                cam.tick(&p, &w);
                ticks += 1;
                let c = cam.position();
                if a.iter().position(|x| x == "--trace").is_some_and(|n| a[n + 1].parse() == Ok(i)) && k == 0 && tick % 20 == 0 {
                    println!("{tick:3} buzz {:.0?} state {:#x} climb {} cam {c:.0?} dist {} behind {:?}", p.pos, p.state, p.climb, cam.distance, w.camera_behind_wall(c, p.pos - Vec3::Y * 192.0 - c));
                }
                let to = p.pos - Vec3::Y * (0x1800 as f32 / 32.0) - c;
                if let Some(s) = w.camera_behind_wall(c, to) {
                    let d = s * to.length();
                    if d > 8.0 {
                        behind += 1;
                        deepest = deepest.max(d);
                    }
                }
                if p.climb <= 0 {
                    break;
                }
            }
        }
        total += behind;
        println!("pole {i} kind {} at {:?} length {}: {behind} ticks behind a wall, deepest {deepest:.0}", pole.kind, pole.pos.map(|v| v / 32), pole.length / 32);
    }
    println!("level {only}: {total} of {ticks} ticks behind a wall");
}
