//! Diagnostic: Buzz walks into each push box of a level along its first path segment and
//! pushes; prints whether it moved and what he touched. `pushsim <levelNN dir>` (TS2_GAME_DIR).
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, leveldat, ngn, terrain};
fn main() {
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let dir = PathBuf::from(env::args().nth(1).unwrap());
    let level = exe::level_number(&dir.join("level.ngn")).unwrap_or(1);
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let mut boxes = ts2::boxes::Boxes::new(level, &lists, Some(&e), &world, &mut specials);
    for k in 0..boxes.boxes.len() {
        let b = boxes.boxes[k].clone();
        let path = lists.iter().find(|l| l.kind == b.path).unwrap().points.clone();
        let (p0, p1) = (path[0], path[1]);
        let d = glam::Vec2::new((p1[0] - p0[0]) as f32, (p1[2] - p0[2]) as f32).normalize();
        let mut touched = std::collections::BTreeSet::new();
        let mut moved = 0.0f32;
        for back in [400.0f32, 600.0, 800.0] {
            let start = glam::Vec3::new(p0[0] as f32 - d.x * back, p0[1] as f32 - 50.0, p0[2] as f32 - d.y * back);
            let floor = world.floor_below(start.x, start.z, start.y - 200.0).unwrap_or(start.y);
            let mut p = ts2::game::Player::new(glam::Vec3::new(start.x, floor, start.z));
            p.on_floor = true;
            for _ in 0..240 {
                boxes.tick(&mut p, &world, &mut specials);
                p.tick_pad(&world, [d.x, d.y], 0, false, false, false);
                if let Some((m, _)) = p.wall_mover {
                    touched.insert(m);
                }
                let (pushing, ang, angles) = boxes.push_state();
                if k == 4 && back == 400.0 && p.wall_mover.is_some() {
                    println!("  touch {:?} floor {} stick {} busy {} facing {:#x} y {:.0}", p.wall_mover, p.on_floor, p.stick_pushed, p.busy(), p.facing, p.pos.y);
                }
                if pushing != 0 && k == 4 && back == 400.0 {
                    println!("  pushing {pushing} angle {ang:#x} box heading {:#x} facing {:#x}", angles[k], p.facing);
                }
            }
            let now = boxes.boxes[k].pos;
            moved = moved.max((((now[0] - b.pos[0]) as f32).hypot((now[2] - b.pos[2]) as f32)) / 32.0);
        }
        println!("box {k} (special {}, mover {}): moved {moved:.0}, walls touched {touched:?}, floor at start {:?}", b.special, b.mover, world.floor_below(p0[0] as f32 - d.x * 400.0, p0[2] as f32 - d.y * 400.0, p0[1] as f32 - 250.0));
    }
}
