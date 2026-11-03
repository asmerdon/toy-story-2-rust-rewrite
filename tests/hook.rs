//! The grappling hook in first person (0x4038E0 / 0x434990 / 0x4A5370) on level 10's hook
//! points. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{leveldat, ngn, terrain};
use ts2::game::{Input, Player, World};
use ts2::look::{FirstPerson, Pad};

/// Mouse movement this tick that turns the first-person view towards `at` (level units):
/// a hook point is only a target while it's on screen.
fn look_at(p: &Player, fp: &FirstPerson, at: [i32; 3]) -> [i32; 2] {
    let wrap = |a: i32| (a + 0x800 & 0xFFF) - 0x800;
    let eye = FirstPerson::eye(p);
    let d = [0, 1, 2].map(|k| (at[k] * 32 - eye[k]) as f32);
    let yaw = ts2::boxes::angle12(d[0], d[2]);
    let pitch = wrap(ts2::boxes::angle12(d[0].hypot(d[2]), -d[1]) - 0x400);
    [wrap(yaw - p.facing).clamp(-20, 20), wrap(pitch - fp.cam_pitch).clamp(-20, 20)]
}

/// Stands Buzz on `from`'s floor (or `step` level units off it, on the same floor), turns the
/// first-person view to `to`, holds target (R1, pressed again until `to` is the one) with a
/// hand jittering on the mouse once locked, fires, and gives where he ends up. None if the
/// hook point was never locked on.
#[allow(clippy::too_many_arguments)]
fn climb(world: &World, fp: &mut FirstPerson, specials: &mut ts2::specials::Specials, cr: &mut Creatures, from: [i32; 3], step: [f32; 2], to: [i32; 3]) -> Option<Vec3> {
    let at = Vec3::new(from[0] as f32 + step[0], from[1] as f32 - 1.0, from[2] as f32 + step[1]);
    let floor = world.floor_below(at.x, at.z, at.y - 50.0)?;
    if (floor - from[1] as f32).abs() > 20.0 {
        return None;
    }
    let mut particles = ts2::particles::Particles::new(None, Vec::new());
    let mut p = Player::new(Vec3::new(at.x, floor, at.z));
    p.hook_ammo = 5;
    let dt = 1.0 / 60.0;
    for _ in 0..20 {
        p.update(world, Input::default(), dt);
    }
    fp.target = None;
    p.first_person = 3;
    let mut locked = false;
    for t in 0..400 {
        let jitter = [[1, 0], [0, -1], [-1, 1], [0, 0]][t % 4];
        let mouse = if t < 60 { look_at(&p, fp, to) } else { jitter };
        let pad = Pad { r1: t > 60, r1_pressed: t > 60 && t % 30 == 1 && !locked, mouse, ..Default::default() };
        p.update(world, Input::default(), dt);
        fp.tick(&mut p, cr, &mut particles, specials, world, pad);
        locked = p.hook_target == Some(to);
        if locked && t % 30 == 29 {
            break;
        }
    }
    if !locked {
        return None;
    }
    for t in 0..300 {
        p.update(world, Input { fire: t == 0, fire_held: t < 3, ..Default::default() }, dt);
        fp.tick(&mut p, cr, &mut particles, specials, world, Pad { r1: true, ..Default::default() });
    }
    Some(p.pos)
}

/// Level 10's hook points come in climbs (0→1, corridor→2→3, 5→6, 8→9→10→11), each 2500
/// units up from the ledge before. From somewhere on each ledge (as 0x447DB0 leaves the
/// points: on their floors), first person targets the next point, locks it through mouse
/// jitter, and the hook pulls Buzz up onto its ledge.
#[test]
fn every_hook_climb_on_level_10_reaches_the_next_ledge() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level10");
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let mut cr = Creatures::new(&[], None, Vec::new());
    let mut fp = FirstPerson::default();
    fp.hooks = lists.iter().find(|l| l.kind == 59).unwrap().points.clone();
    fp.settle_hooks(&world);
    let hooks = fp.hooks.clone();
    let steps = [[0.0, 0.0], [400.0, 0.0], [-400.0, 0.0], [0.0, 400.0], [0.0, -400.0], [800.0, 0.0], [-800.0, 0.0], [0.0, 800.0], [0.0, -800.0]];
    let mut failed = Vec::new();
    // Hook 2 is reached from the end of the corridor (floor −5001) past hook 1's ledge.
    let corridor = [-3700, -5001, 9250];
    for (from, to) in [(0, 1), (99, 2), (2, 3), (5, 6), (8, 9), (9, 10), (10, 11)] {
        let ledge = hooks[to][1] as f32;
        let start = hooks.get(from).copied().unwrap_or(corridor);
        let made = steps.iter().filter_map(|&s| climb(&world, &mut fp, &mut specials, &mut cr, start, s, hooks[to])).find(|end| (end.y - ledge).abs() < 200.0);
        match made {
            Some(end) => eprintln!("{from}→{to}: up to {end:?}"),
            None => failed.push(format!("{from}→{to}")),
        }
    }
    assert!(failed.is_empty(), "no way up: {failed:?}");
}

/// 0x440F70 draws every hook point as sprite 0x1E (a quad of two triangles) within 13312
/// units of the camera, whether or not Buzz has hooks.
#[test]
fn hook_points_are_drawn() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let dir = game.join("data/level10");
    let e = ts2::formats::exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let pk = ts2::pickups::Pickups::new(10, &fs::read(dir.join("level.dat")).unwrap(), Some(&e)).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let hooks = lists.iter().find(|l| l.kind == 59).unwrap().points.clone();
    let tris = |cam: [f32; 3]| pk.hook_batches(&n, &hooks, cam, [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]).iter().map(|b| b.vertices.len() / 3).sum::<usize>();
    let h = hooks[0].map(|v| v as f32);
    assert!(tris([h[0], h[1], h[2] - 3000.0]) >= 2);
    assert_eq!(tris([h[0], h[1] + 200_000.0, h[2]]), 0);
}
