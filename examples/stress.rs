//! Diagnostic: level 1 played by random input with everything the viewer runs (boxes, the
//! level script, creatures, stomps, spins, the laser). Reports Buzz ending up under a floor
//! he was standing on a moment before, and long falls. Run with TS2_GAME_DIR set:
//! cargo run --release --example stress [runs] [ticks]
use glam::Vec3;
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
use ts2::game::{Input, Player, World};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let runs: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(40);
    let ticks: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(4000);
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").unwrap());
    let dir = game.join("data/level01");
    let e = exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&std::fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let random = std::fs::read(game.join("data/rand.dat")).unwrap();
    let pk = packets::parse(&std::fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let start = e.player_start(1).unwrap();
    let poles = leveldat::poles(&lists);
    let mut seed = 777u32;
    let mut rnd = move || {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        (seed >> 16) as f32 / 65536.0
    };
    let (mut through, mut falls) = (0, 0);
    for run in 0..runs {
        let world = World::from_terrain(&t);
        let mut sp = ts2::specials::Specials::new(&n);
        let mut boxes = ts2::boxes::Boxes::new(1, &lists, Some(&e), &world, &mut sp);
        let mut script = ts2::level1::Level1::new(&world, &mut sp);
        let mut cr = ts2::creatures::Creatures::new(&spawns, Some(&e), random.clone());
        cr.load_shapes(&game.join("data"));
        // Start at the level start or near a random creature spawn.
        let g = if run % 2 == 0 {
            Vec3::from(start.pos)
        } else {
            let s = &spawns[(rnd() * spawns.len() as f32) as usize % spawns.len()];
            Vec3::new(s.pos[0] as f32 + 300.0, s.pos[1] as f32, s.pos[2] as f32)
        };
        let Some(y) = world.floor_below(g.x, g.z, g.y - 600.0) else { continue };
        let mut p = Player::new(Vec3::new(g.x, y, g.z));
        p.set_random_table(random.clone());
        p.set_poles(poles.clone());
        let mut dir = [0.0f32; 2];
        let mut air = 0;
        let mut last_floor: Option<(Vec3, f32)> = None;
        for i in 0..ticks {
            if i % 37 == 0 {
                let ang = rnd() * std::f32::consts::TAU;
                dir = if rnd() < 0.15 { [0.0; 2] } else { [ang.sin(), ang.cos()] };
            }
            let r = rnd();
            let input = Input {
                move_xz: dir,
                jump: r < 0.03,
                jump_held: (i % 50) < 25,
                stomp: (0.03..0.05).contains(&r),
                stomp_held: (i % 200) < 90,
                fire: (0.05..0.06).contains(&r),
                ..Default::default()
            };
            p.update_with(&world, input, 1.0 / 60.0, &mut |p, w| {
                boxes.tick(p, w, &mut sp);
                script.tick(w, &mut sp, p, &boxes);
            });
            p.shots.clear();
            cr.tick(&world, p.pos_obj(), p.pos_obj());
            cr.contact(&mut p);
            if p.health < 0 && p.dying == 0 {
                p.respawn(Vec3::new(g.x, y, g.z), 0);
                last_floor = None;
            }
            if p.on_floor {
                air = 0;
                last_floor = Some((p.pos, p.pos.y));
            } else {
                air += 1;
            }
            // Under a floor he stood on: the floor he was on is still there, above him now.
            if let Some((at, fy)) = last_floor
                && !p.on_floor
                && p.pos.y > fy + 60.0
                && (p.pos.x - at.x).hypot(p.pos.z - at.z) < 40.0
                && world.floor_below(at.x, at.z, fy - 5.0).is_some_and(|y2| (y2 - fy).abs() < 3.0)
            {
                through += 1;
                eprintln!(
                    "run {run} tick {i}: through the floor at {:?} (floor {fy:.0}, now {:.0}); state {} stomp {} charge {} vel {:?}",
                    at.to_array().map(|v| v as i32),
                    p.pos.y,
                    p.state,
                    p.stomp,
                    p.charge,
                    p.vel
                );
                last_floor = None;
            }
            if air == 300 {
                falls += 1;
                eprintln!("run {run} tick {i}: long fall at {:?}", p.pos.to_array().map(|v| v as i32));
                break;
            }
        }
    }
    println!("{through} fall-throughs, {falls} long falls in {runs} runs");
}
