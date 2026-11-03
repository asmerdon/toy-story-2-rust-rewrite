//! Level 4's script. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
use ts2::game::{Player, World};
use ts2::script::Script;

struct L2 {
    world: World,
    specials: ts2::specials::Specials,
    script: Box<dyn Script>,
    boxes: ts2::boxes::Boxes,
    cr: Creatures,
}

fn load() -> Option<L2> {
    let game = env::var_os("TS2_GAME_DIR").map(PathBuf::from)?;
    let dir = game.join("data/level04");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let script = ts2::script::for_level(4, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let boxes = ts2::boxes::Boxes::new(4, &lists, Some(&e), &world, &mut specials);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.load_shapes(&game.join("data"));
    Some(L2 { world, specials, script, boxes, cr })
}

/// The hanging platforms follow their paths (and stop under Buzz); the shuttles move too.
#[test]
fn platforms_follow_their_paths() {
    let Some(mut l) = load() else { return };
    let start: Vec<Vec3> = [1, 2, 3, 4, 0xE, 0x10].iter().map(|&m| l.world.mover_pos(m).unwrap()).collect();
    let mut p = Player::new(Vec3::new(0.0, -100000.0, 0.0));
    let mut far = [0.0f32; 6];
    for _ in 0..1200 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        for (k, m) in [1, 2, 3, 4, 0xE, 0x10].into_iter().enumerate() {
            far[k] = far[k].max((l.world.mover_pos(m).unwrap() - start[k]).length());
        }
    }
    eprintln!("furthest: {far:?}");
    assert!(far.iter().all(|&d| d > 50.0), "a mover didn't move");
}

/// Down in the pit near the drill, its challenge starts; after it the fight runs.
#[test]
fn drill_challenge() {
    let Some(mut l) = load() else { return };
    let d = l.cr.slots[0x18].pos;
    assert_eq!(l.cr.slots[0x18].id, 0x16);
    let mut p = Player::new(Vec3::from(d.map(|v| v as f32 / 32.0)));
    p.on_floor = true;
    eprintln!("drill at {:?}", d.map(|v| v >> 5));
    for _ in 0..3 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.cr.boss.state, 1, "challenged");
    l.script.cutscene = None;
    for _ in 0..30 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.cr.boss.state, 2);
}

/// The paint-room door's hatch (specials 0x3F / 0x40, 0x41C640) folds up to nothing while Buzz
/// is near it and drops back when he leaves, 0x200 of 0x1000 a tick.
#[test]
fn paint_room_hatch_opens_for_buzz() {
    let Some(mut l) = load() else { return };
    let tall = |l: &L2, id: i32| l.specials.get(id).unwrap().scale[1];
    let shut = [tall(&l, 0x3F), tall(&l, 0x40)];
    assert!(shut.iter().all(|&s| s > 0.0), "drawn shut at the start: {shut:?}");
    // Its bottom edge, in level units (0x51B97, −0x104FD, 0x5E58E object units).
    let at = Vec3::new(0x51B97 as f32, -0x104FD as f32, 0x5E58E as f32) / 32.0;
    let mut p = Player::new(at - Vec3::Z * 300.0);
    for _ in 0..8 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    assert_eq!([tall(&l, 0x3F), tall(&l, 0x40)], [0.0; 2], "open after 8 ticks near it");
    p.pos = at - Vec3::Z * 2500.0;
    for _ in 0..4 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    let half = [tall(&l, 0x3F), tall(&l, 0x40)];
    assert!((half[0] - shut[0] / 2.0).abs() < 1e-3 && (half[1] - shut[1] / 2.0).abs() < 1e-3, "half shut 4 ticks after leaving: {half:?} of {shut:?}");
    for _ in 0..8 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    assert_eq!([tall(&l, 0x3F), tall(&l, 0x40)], shut, "shut again");
}

/// The paint room (room 2) is drawn from outside only through its door's hatch (portal 0,
/// room 1 → 2), and the yard (room 1) from inside through the hatch and the window (0x4BC460).
#[test]
fn paint_room_seen_only_through_its_portals() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let bytes = fs::read(game.join("data/level04/level.ngn")).unwrap();
    let portals = ts2::scene::Portals::parse(&bytes);
    assert_eq!(portals.links.iter().filter(|l| l.to != 15).count(), 3, "{:?}", portals.links);
    // The viewer's camera: game (+Y down) to world, 60° reversed-depth perspective.
    let view = |at: [f32; 3], yaw: f32, pitch: f32| {
        let pos = Vec3::new(at[0], -at[1], -at[2]) / 256.0;
        let f = Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos());
        let vp = glam::Mat4::perspective_infinite_reverse_rh(60f32.to_radians(), 16.0 / 9.0, 0.05) * glam::Mat4::look_to_rh(pos, f, Vec3::Y);
        move |p: [f32; 3]| vp * glam::Vec4::new(p[0] / 256.0, -p[1] / 256.0, -p[2] / 256.0, 1.0)
    };
    let rooms = |from: u32, at: [f32; 3], yaw: f32, pitch: f32| portals.visible_rooms(from, view(at, yaw, pitch), false);
    // Outside, facing the door (+z): the hatch is in view.
    assert_eq!(rooms(1, [10500.0, -2400.0, 9000.0], 0.0, 0.0), 0b111);
    // From far off in the yard, the door in view.
    assert_eq!(rooms(1, [10500.0, -8000.0, 2000.0], 0.0, -0.35), 0b111);
    // Outside, facing away from the door.
    assert_eq!(rooms(1, [10500.0, -2400.0, 9000.0], std::f32::consts::PI, 0.0), 0b011);
    // Behind the cabin (its far side, +z beyond the room), looking back at it: no portal faces.
    assert_eq!(rooms(1, [10000.0, -4000.0, 36000.0], std::f32::consts::PI, -0.2), 0b011);
    // Inside, facing the door: the yard through the hatch and the window.
    assert_eq!(rooms(2, [9500.0, -4000.0, 19050.0], std::f32::consts::PI, 0.0), 0b111);
    // Inside, facing the back wall: the yard isn't drawn.
    assert_eq!(rooms(2, [9500.0, -4000.0, 19050.0], 0.0, 0.0), 0b101);
}

/// The hanging platforms (movers 1–4) sway as Buzz lands, stomps and hops about on them,
/// and run their paths when he's off; the shuttles (0xE–0x11) run theirs all the time.
/// With the real script moving them (inside the tick and between ticks), he never ends a
/// tick under the top of one he's over.
#[test]
fn buzz_stays_on_the_moving_platforms() {
    let Some(mut l) = load() else { return };
    let dt = 1.0 / 60.0;
    let mut failed = Vec::new();
    let mut ticks = 0;
    let mut on = [0; 0x12];
    for m in [1, 2, 3, 4, 0xE, 0xF, 0x10, 0x11] {
        let Some(at) = l.world.mover_pos(m) else { continue };
        // The highest floor of mover m under (x, z).
        let top = |w: &World, x: f32, z: f32| {
            let mut from = -1.0e9;
            while let Some((y, o)) = w.floor_below_obj(x, z, from) {
                if o == Some(m) {
                    return Some(y);
                }
                from = y + 1.0;
            }
            None
        };
        // A point on its top near where it hangs from.
        let spot = (0..12).flat_map(|k| [0.0, 200.0, 400.0, 700.0, 1000.0].map(move |r| (k as f32 / 12.0 * std::f32::consts::TAU, r))).find_map(|(a, r)| {
            let (x, z) = (at.x + a.sin() * r, at.z + a.cos() * r);
            top(&l.world, x, z).map(|y| Vec3::new(x, y, z))
        });
        let Some(spot) = spot else {
            failed.push(format!("platform {m}: no top found"));
            continue;
        };
        let mut p = Player::new(spot - Vec3::Y * 300.0);
        for t in 0..900 {
            // Standing for 600 ticks, then hopping every 40 and stomping on the way down
            // every other hop.
            let hop = t >= 600;
            let input = ts2::game::Input {
                jump: hop && t % 40 == 0,
                jump_held: hop && t % 40 < 8,
                stomp: hop && t % 80 == 20,
                ..Default::default()
            };
            let (script, specials, boxes) = (&mut l.script, &mut l.specials, &l.boxes);
            p.update_with(&l.world, input, dt, &mut |pl, w| script.step(w, specials, pl, boxes));
            l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
            ticks += 1;
            if p.on_floor && p.floor_mover.is_some_and(|(o, _)| o == m) {
                on[m as usize] += 1;
            }
            if let Some(f) = top(&l.world, p.pos.x, p.pos.z)
                && p.pos.y > f + 20.0
                && p.pos.y < f + 400.0
            {
                failed.push(format!("platform {m} tick {t}: feet {:.0} under its top {f:.0}", p.pos.y));
                break;
            }
        }
    }
    eprintln!("ticks standing on each: {on:?}");
    assert!(failed.is_empty(), "{failed:#?}");
    assert!(ticks > 1000, "{ticks} ticks");
}

/// Diagnostic (ignored): the shuttles' paths, every 20 ticks.
#[test]
#[ignore]
fn print_shuttle_paths() {
    let Some(mut l) = load() else { return };
    let mut p = Player::new(Vec3::new(0.0, -100000.0, 0.0));
    for t in 0..600 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        if t % 20 == 0 {
            let ps: Vec<String> = [0xE, 0xF, 0x10, 0x11].iter().map(|&m| format!("{:.0?}", l.world.mover_pos(m).unwrap())).collect();
            eprintln!("{t:4} {}", ps.join(" "));
        }
    }
}

/// Diagnostic (ignored): level 4's creature slots.
#[test]
#[ignore]
fn print_creatures() {
    let Some(l) = load() else { return };
    for (i, c) in l.cr.slots.iter().enumerate().filter(|(_, c)| c.id != 0) {
        eprintln!("slot {i:#x} id {:#x} pos {:?}", c.id, c.pos.map(|v| v >> 5));
    }
}

/// Diagnostic (ignored): each hanging platform's drawn box against its collision's, as the
/// script leaves them after a while.
#[test]
#[ignore]
fn print_platform_fit() {
    let Some(mut l) = load() else { return };
    let game = env::var_os("TS2_GAME_DIR").map(PathBuf::from).unwrap();
    let n = ngn::parse(&fs::read(game.join("data/level04/level.ngn")).unwrap()).unwrap();
    let mut p = Player::new(Vec3::new(0.0, -100000.0, 0.0));
    for _ in 0..200 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    for m in 1..=4 {
        let s = l.specials.get(m - 1).unwrap().clone();
        let mut bb = Vec::new();
        let b = ts2::scene::special_batches(&n, &s, &mut bb);
        let pts: Vec<Vec3> = b.iter().flat_map(|b| b.vertices.iter().map(|v| Vec3::from(v.pos))).collect();
        let lo = pts.iter().fold(Vec3::MAX, |a, p| a.min(*p));
        let hi = pts.iter().fold(Vec3::MIN, |a, p| a.max(*p));
        let at = l.world.mover_pos(m).unwrap();
        // Collision: floors of mover m under a grid over the drawn box.
        let mut top = f32::MAX;
        let mut cover = 0;
        for i in 0..=20 {
            for k in 0..=20 {
                let (x, z) = (lo.x + (hi.x - lo.x) * i as f32 / 20.0, lo.z + (hi.z - lo.z) * k as f32 / 20.0);
                let mut from = lo.y - 500.0;
                while let Some((y, o)) = l.world.floor_below_obj(x, z, from) {
                    if y > hi.y + 500.0 { break; }
                    if o == Some(m) { top = top.min(y); cover += 1; break; }
                    from = y + 1.0;
                }
            }
        }
        eprintln!("platform {m}: drawn {lo:.0}..{hi:.0}, angles {:x?}; mover at {at:.0}, turn {:?}; collision top {top:.0} over {cover}/441 of the drawn box", s.angles, l.world.mover_turn(m));
    }
}

/// Diagnostic (ignored): the drawbridge scoop (mover 0x1A, specials 0x1C / 0x1D) drawn
/// against its collision as it turns: at a few angles, the gap between the drawn top surface
/// and the collision floor over a grid.
#[test]
#[ignore]
fn print_drawbridge_fit() {
    let Some(mut l) = load() else { return };
    let game = env::var_os("TS2_GAME_DIR").map(PathBuf::from).unwrap();
    let n = ngn::parse(&fs::read(game.join("data/level04/level.ngn")).unwrap()).unwrap();
    for z in [0, -0x80, -0x130, -0x1C0, -0x266] {
        l.world.turn_mover(0x1A, [0, 0, z]);
        for id in [0x1C, 0x1D] {
            l.specials.turn_from_own(id, [0, 0, z + 0x266]);
        }
        let mut tris = Vec::new();
        for id in [0x1C] {
            let s = l.specials.get(id).unwrap().clone();
            let mut bb = Vec::new();
            for b in ts2::scene::special_batches(&n, &s, &mut bb) {
                for t in b.vertices.chunks_exact(3) {
                    tris.push([0, 1, 2].map(|k| Vec3::from(t[k].pos)));
                }
            }
        }
        let lo = tris.iter().flatten().fold(Vec3::MAX, |a, p| a.min(*p));
        let hi = tris.iter().flatten().fold(Vec3::MIN, |a, p| a.max(*p));
        let mut gaps = Vec::new();
        for i in 1..10 {
            for k in 1..10 {
                let (x, zz) = (lo.x + (hi.x - lo.x) * i as f32 / 10.0, lo.z + (hi.z - lo.z) * k as f32 / 10.0);
                let drawn = tris.iter().filter_map(|t| {
                    let [a, b, c] = *t;
                    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
                    if d.abs() < 1e-6 { return None; }
                    let u = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (zz - c.z)) / d;
                    let w = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (zz - c.z)) / d;
                    (u >= 0.0 && w >= 0.0 && u + w <= 1.0).then(|| u * a.y + w * b.y + (1.0 - u - w) * c.y)
                }).fold(f32::MAX, f32::min);
                let mut from = lo.y - 3000.0;
                let mut col = None;
                while let Some((y, o)) = l.world.floor_below_obj(x, zz, from) {
                    if o == Some(0x1A) { col = Some(y); break; }
                    from = y + 1.0;
                }
                if drawn < f32::MAX && let Some(c) = col {
                    gaps.push((c - drawn) as i32);
                }
            }
        }
        gaps.sort();
        eprintln!("z {z:#x}: drawn {lo:.0}..{hi:.0}; collision − drawn top over {} points: min {:?} median {:?} max {:?}", gaps.len(), gaps.first(), gaps.get(gaps.len() / 2), gaps.last());
    }
}
