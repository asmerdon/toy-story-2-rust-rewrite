//! Diagnostic: the fault class "Buzz passes through a wall or falls through a floor". For each
//! level (or `--level N`), drops Buzz on every floor found on a grid (`--step S`, default 300
//! level units), then runs him for `--ticks T` (default 150) at full stick in 8 headings
//! (half of them jumping now and then), with the level's movers at rest. Flags a tick when:
//! - wall: the body's centre came within 100 units of a face on its way (the resolver keeps it
//!   125 off), crossing it from the front;
//! - floor: he ends a tick below a floor that was under him, at his new x/z, at the height he
//!   started the tick;
//! - fell: the fall plane (0x436220) killed him.
//! Starts must fit the body and, where the level has boundary walls, lie inside them.
//! Starts must fit the body and, where the level has boundary walls, lie inside them.
//! Prints each flag once per 200-unit cell. cargo run --release --example clipsweep
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
    let only: Option<usize> = arg("--level").and_then(|v| v.parse().ok());
    let step: f32 = arg("--step").and_then(|v| v.parse().ok()).unwrap_or(300.0);
    let ticks: usize = arg("--ticks").and_then(|v| v.parse().ok()).unwrap_or(150);
    let lift = Vec3::Y * (6.0 + 125.0);
    // `--box x0 y0 z0 x1 y1 z1`: only starts inside this box.
    let bx: Option<Vec<f32>> = {
        let a: Vec<String> = env::args().collect();
        a.iter().position(|x| x == "--box").map(|i| a[i + 1..i + 7].iter().map(|v| v.parse().unwrap()).collect())
    };
    let headings: usize = arg("--headings").and_then(|v| v.parse().ok()).unwrap_or(8);
    for lv in ts2::campaign::LEVELS {
        if only.is_some_and(|n| n != lv.number) {
            continue;
        }
        let path = lv.path(&game);
        let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
        let w = World::from_terrain(&t);
        let mut starts = Vec::new();
        let (lo, hi) = match &bx {
            Some(b) => (Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5])),
            None => (w.lo, w.hi),
        };
        let mut x = lo.x;
        while x <= hi.x {
            let mut z = lo.z;
            while z <= hi.z {
                let mut y = lo.y - 10.0;
                while let Some(f) = w.floor_below(x, z, y)
                    && f <= hi.y
                {
                    // Only starts where the body fits: no face within 130 of its centre
                    // horizontally (a ray from 130 out back to the centre hits nothing).
                    let c = Vec3::new(x, f - 131.0, z);
                    let fits = [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z]
                        .iter()
                        .all(|&d| w.sweep(c + d * 130.0, -d * 130.0, 0.0).is_none() && w.sweep(c, d * 130.0, 0.0).is_none());
                    // Levels with boundary walls: only starts inside them (crossing parity over
                    // every segment), the rest is outside the play area.
                    let segs = w.boundary_segments();
                    let inside = segs.is_empty()
                        || segs.iter().filter(|[a, b]| (a[1] > z) != (b[1] > z) && x < a[0] + (z - a[1]) / (b[1] - a[1]) * (b[0] - a[0])).count() % 2 == 1;
                    if fits && inside {
                        starts.push(Vec3::new(x, f, z));
                    }
                    y = f + 300.0;
                }
                z += step;
            }
            x += step;
        }
        let (mut seen, mut counts) = (HashSet::new(), [0usize; 3]);
        for s in &starts {
            for k in 0..headings {
                let a = k as f32 * std::f32::consts::TAU / headings as f32;
                let stick = [a.sin(), a.cos()];
                let mut p = Player::new(*s);
                p.on_floor = true;
                for tick in 0..ticks {
                    let before = p.pos;
                    let jump = k % 2 == 1 && tick % 50 < 15;
                    p.tick(&w, stick, jump, false);
                    let mut flag = None;
                    // The centre itself crossing a face from its front (r = 0): a pass-through.
                    if let Some(h) = w.sweep(before - lift, p.pos - before, 0.0)
                        && h.t < 1.0
                        && h.depth == 0.0
                    {
                        flag = Some((0, format!("face n {:.2?}", h.normal)));
                    } else if let Some(f) = w.floor_below(p.pos.x, p.pos.z, before.y - 16.0)
                        && p.pos.y > f + 24.0
                    {
                        flag = Some((1, format!("floor at y {f:.0}")));
                    } else if p.dying > 0 && p.pos.y > w.lowest {
                        flag = Some((2, "fell".to_string()));
                    }
                    if let Some((kind, what)) = flag {
                        let cell = (kind, (p.pos.x / 200.0) as i32, (p.pos.y / 200.0) as i32, (p.pos.z / 200.0) as i32);
                        if seen.insert(cell) {
                            counts[kind] += 1;
                            println!(
                                "level {:>2} {:<5} room {:?} from {:?} heading {k} tick {tick}: {:?} -> {:?} ({what})",
                                lv.number,
                                ["wall", "floor", "fell"][kind],
                                w.room_at(*s),
                                s.round(),
                                before.round(),
                                p.pos.round()
                            );
                        }
                        break;
                    }
                }
            }
        }
        println!(
            "level {:>2}: {} starts, {} wall, {} floor, {} fell (distinct cells)",
            lv.number,
            starts.len(),
            counts[0],
            counts[1],
            counts[2]
        );
    }
}
