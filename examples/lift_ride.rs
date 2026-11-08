//! Diagnostic: Buzz riding level 10's lift under the real script, tick by tick: his height,
//! the camera's position, aim and distance. `lift_ride <lift 0|1> [ticks] [skip]`.
use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, leveldat, ngn, terrain};
use ts2::game::{Input, Player, World};
use ts2::script::Script;

fn main() {
    let a: Vec<String> = env::args().collect();
    let m: i32 = a.get(1).map_or(1, |v| v.parse().unwrap());
    let ticks: usize = a.get(2).map_or(400, |v| v.parse().unwrap());
    let skip: usize = a.get(3).map_or(0, |v| v.parse().unwrap());
    let game = PathBuf::from(env::var_os("TS2_GAME_DIR").unwrap());
    let dir = game.join("data/level10");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut l = ts2::level10::Level10::new(Some(&e));
    l.set_story(&e, &lists);
    l.solve_switches();
    let at0 = world.mover_pos(m).unwrap();
    let scan = a.get(4).is_some_and(|v| v == "scan");
    let one: Option<Vec<f32>> = a.get(4).filter(|_| !scan).map(|v| v.split(',').map(|x| x.parse().unwrap()).collect());
    let cases: Vec<(f32, f32, i32)> = if scan {
        let mut v = Vec::new();
        for dx in [-300.0f32, -150.0, 0.0, 150.0, 300.0] {
            for dz in [-300.0f32, -150.0, 0.0, 150.0, 300.0] {
                for f in [0, 0x400, 0x800, 0xC00, 0x200, 0xA00] {
                    v.push((dx, dz, f));
                }
            }
        }
        v
    } else {
        vec![one.map_or((0.0, 0.0, 0), |o| (o[0], o[1], o[2] as i32))]
    };
    let dt = 1.0 / 60.0;
    let fresh = || {
        let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
        let world = World::from_terrain(&t);
        let mut specials = ts2::specials::Specials::new(&n);
        let mut l = ts2::level10::Level10::new(Some(&e));
        l.set_story(&e, &lists);
        l.solve_switches();
        let boxes = ts2::boxes::Boxes::new(10, &lists, Some(&e), &world, &mut specials);
        (world, specials, l, boxes)
    };
    for (dx, dz, facing) in cases {
        let (world, mut specials, mut l, boxes) = fresh();
        let mut y = at0.y - 9000.0;
        let (x, z) = (at0.x + dx, at0.z + dz);
        let mut found = None;
        for _ in 0..20 {
            let Some((f, o)) = world.floor_below_obj(x, z, y) else { break };
            if o == Some(m) && f > at0.y - 6000.0 {
                found = Some(f);
                break;
            }
            y = f + 1.0;
        }
        let Some(floor) = found else { continue };
        let mut p = Player::new(Vec3::new(x, floor - 2.0, z));
        p.set_facing(facing);
        let mut cam = ts2::camera::Camera::new(&p);
        let (mut py, mut pc, mut pd) = (p.pos.y, cam.position(), Vec3::ZERO);
        let (mut worst, mut worst_k, mut sum) = (0.0f32, 0, 0.0f32);
        for k in 0..ticks {
            p.update_with(&world, Input::default(), dt, &mut |p, w| l.step(w, &mut specials, p, &boxes));
            cam.tick(&p, &world);
            let c = cam.position();
            let d = c - pc;
            if k >= skip && k < 560 {
                let j = (d - pd).length();
                sum += j;
                if j > worst {
                    worst = j;
                    worst_k = k;
                }
            }
            if !scan && k >= skip {
                println!(
                    "{k:4} y {:9.1} dy {:6.1} floor {} | cam {:8.1} {:9.1} {:8.1} d {:6.1} {:6.1} {:6.1} | look {:5} {:5} dist {:5} above {:6.1}",
                    p.pos.y, p.pos.y - py, p.on_floor as u8, c.x, c.y, c.z, d.x, d.y, d.z, cam.look_yaw, cam.look_pitch, cam.distance, p.pos.y - c.y
                );
            }
            py = p.pos.y;
            pc = c;
            pd = d;
        }
        if scan {
            println!("off {dx:6.0} {dz:6.0} facing {facing:4x}: jerk sum {sum:8.1} worst {worst:6.1} at {worst_k}  buzz y {:.0}", p.pos.y);
        }
    }
}
