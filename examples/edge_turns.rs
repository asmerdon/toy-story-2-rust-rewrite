//! Diagnostic: Buzz turning on the edge of a surface. Starts him on every floor point with a
//! drop within his body's reach (`--step S`, default 80 level units), then for each heading
//! either runs and reverses (a skid) or taps the stick round on the spot, and flags a tick when:
//! - face: his centre crossed a face from its front;
//! - floor: he ends a tick below a floor that was under him at his new x/z;
//! - pop: he rose more than 30 units in one tick while on the floor;
//! - inside: his centre ends less than 100 units from a floor or wall he's facing the front of.
//! `edge_turns --level 1 [--step 80] [--box x0 y0 z0 x1 y1 z1]`
use glam::Vec3;
use std::collections::HashSet;
use std::{env, fs, path::PathBuf};
use ts2::formats::terrain;
use ts2::game::{Player, World};

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn main() {
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let only: usize = arg("--level").and_then(|v| v.parse().ok()).unwrap_or(1);
    let step: f32 = arg("--step").and_then(|v| v.parse().ok()).unwrap_or(80.0);
    let bx: Option<Vec<f32>> = {
        let a: Vec<String> = env::args().collect();
        a.iter().position(|x| x == "--box").map(|i| a[i + 1..i + 7].iter().map(|v| v.parse().unwrap()).collect())
    };
    let lift = Vec3::Y * (6.0 + 125.0);
    let lv = ts2::campaign::LEVELS.iter().find(|l| l.number == only).unwrap();
    let path = lv.path(&game);
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let w = World::from_terrain(&t);
    let (lo, hi) = match &bx {
        Some(b) => (Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5])),
        None => (w.lo, w.hi),
    };
    let mut starts = Vec::new();
    let mut x = lo.x;
    while x <= hi.x {
        let mut z = lo.z;
        while z <= hi.z {
            let mut y = lo.y - 10.0;
            while let Some(f) = w.floor_below(x, z, y)
                && f <= hi.y
            {
                let c = Vec3::new(x, f - 131.0, z);
                let room = [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z].iter().all(|&d| w.sweep(c, d * 130.0, 0.0).is_none());
                let edge = (0..8).any(|k| {
                    let a = k as f32 * std::f32::consts::TAU / 8.0;
                    let (px, pz) = (x + a.sin() * 130.0, z + a.cos() * 130.0);
                    w.floor_below(px, pz, f - 16.0).is_none_or(|g| g - f > 48.0)
                });
                if room && edge && w.room_at(c).is_some() {
                    starts.push(Vec3::new(x, f, z));
                }
                y = f + 300.0;
            }
            z += step;
        }
        x += step;
    }
    // `--trace x y z try`: every tick of one run.
    let trace: Option<Vec<f32>> = {
        let a: Vec<String> = env::args().collect();
        a.iter().position(|x| x == "--trace").map(|i| a[i + 1..i + 5].iter().map(|v| v.parse().unwrap()).collect())
    };
    let (mut seen, mut counts) = (HashSet::new(), [0usize; 4]);
    let starts = match &trace {
        Some(t) => vec![Vec3::new(t[0], t[1], t[2])],
        None => starts,
    };
    for s in &starts {
        for k in 0..16 {
            if trace.as_ref().is_some_and(|t| t[3] as i32 != k) {
                continue;
            }
            let a = (k / 2) as f32 * std::f32::consts::TAU / 8.0;
            let dir = |o: f32| [(a + o).sin(), (a + o).cos()];
            let mut p = Player::new(*s);
            p.on_floor = true;
            for _ in 0..3 {
                p.tick(&w, [0.0; 2], false, false);
            }
            for tick in 0..90 {
                let stick = if k % 2 == 0 {
                    // Run a moment, then reverse: the skid.
                    if tick < 10 { dir(0.0) } else if tick < 50 { dir(std::f32::consts::PI) } else { [0.0; 2] }
                } else if tick % 8 < 4 {
                    // Taps round on the spot.
                    dir(tick as f32 / 8.0 * 2.1)
                } else {
                    [0.0; 2]
                };
                let before = p.pos;
                let was_floor = p.on_floor;
                p.tick(&w, stick, false, false);
                if trace.is_some() {
                    println!("{tick:3} stick {:.1?} pos {:.1?} vel {:?} floor {} coyote {} facing {:#x} skid {} n {:.2?}", stick, p.pos, p.vel, p.on_floor, p.coyote, p.facing, p.skid, p.contact_normal);
                    continue;
                }
                let mut flag = None;
                if let Some(h) = w.sweep(before - lift, p.pos - before, 0.0)
                    && h.t < 1.0
                    && h.depth == 0.0
                {
                    flag = Some((0, format!("face n {:.2?}", h.normal)));
                } else if let Some(f) = w.floor_below(p.pos.x, p.pos.z, before.y - 16.0)
                    && p.pos.y > f + 24.0
                {
                    flag = Some((1, format!("floor at y {f:.0}")));
                } else if was_floor && before.y - p.pos.y > 30.0 {
                    flag = Some((2, format!("rose {:.0}", before.y - p.pos.y)));
                } else {
                    let c = p.pos - lift;
                    for d in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z, Vec3::Y] {
                        if let Some(h) = w.sweep(c, d * 100.0, 0.0)
                            && h.t * 100.0 < 95.0
                        {
                            flag = Some((3, format!("{:.0} from face n {:.2?}", h.t * 100.0, h.normal)));
                            break;
                        }
                    }
                }
                if let Some((kind, what)) = flag {
                    let cell = (kind, (p.pos.x / 100.0) as i32, (p.pos.y / 100.0) as i32, (p.pos.z / 100.0) as i32);
                    if seen.insert(cell) {
                        counts[kind] += 1;
                        println!("{:<6} room {:?} from {:?} try {k} tick {tick}: {:?} -> {:?} ({what})", ["face", "floor", "pop", "inside"][kind], w.room_at(*s), s.round(), before.round(), p.pos.round());
                    }
                    break;
                }
            }
        }
    }
    println!("level {only}: {} edge starts, {} face, {} floor, {} pop, {} inside (distinct cells)", starts.len(), counts[0], counts[1], counts[2], counts[3]);
}
