//! Diagnostic: level 1's living-room spring (surface 8, 0x417680 launches at vy −0x1280,
//! heading 0x11E): where the launch takes Buzz. cargo run --release --example spring1 [x z]
use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::terrain;
use ts2::game::{Player, World};
fn main() {
    let a: Vec<f32> = env::args().skip(1).filter_map(|s| s.parse().ok()).collect();
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let path = ts2::campaign::level(1).unwrap().path(&game);
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let w = World::from_terrain(&t);
    let (x, z) = if a.len() >= 2 { (a[0], a[1]) } else { (17360.0, -8566.0) };
    let y = w.floor_below(x, z, a.get(4).copied().unwrap_or(6000.0)).unwrap();
    let y = a.get(5).copied().unwrap_or(y);
    let mut p = Player::new(Vec3::new(x, y, z));
    p.on_floor = true;
    if a.len() <= 5 {
        p.tick(&w, [0.0; 2], false, false);
    }
    println!("on {:?} surface {:?}", p.pos, p.surface);
    // With a third number: walk at that angle (radians) instead of the launch.
    let stick = a.get(2).map(|r| [r.sin(), r.cos()]);
    if stick.is_none() {
        p.launch(-0x1280, 0x11E);
    }
    for tick in 0..240 {
        p.tick(&w, stick.unwrap_or([0.0; 2]), false, false);
        if tick % 5 == 0 || p.dying > 0 || a.len() > 3 && (a[3]..a[3] + 30.0).contains(&(tick as f32)) {
            println!("tick {tick}: {:?} vel {:?} floor {} room {:?} dying {}", p.pos.round(), p.vel, p.on_floor, w.room_at(p.pos), p.dying);
        }
        if p.dying > 0 {
            break;
        }
    }
}
