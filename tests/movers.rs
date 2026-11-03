//! Moving platforms carry Buzz (0x488E10) and never let him through, whichever way a script
//! moves them. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::terrain;
use ts2::game::{Input, Player, World};

/// The middle of mover `link − 1`'s flat top, and how wide it is (level units).
fn top(t: &terrain::Terrain, i: usize) -> Option<(Vec3, f32)> {
    let faces: Vec<[Vec3; 3]> = t.object_triangles(i).into_iter().filter(|(_, n)| Vec3::from(*n).normalize_or_zero().y < -0.95).map(|(p, _)| p.map(Vec3::from)).collect();
    let y = faces.iter().flatten().map(|v| v.y).reduce(f32::min)?;
    let (lo, hi) = faces.iter().filter(|f| f.iter().all(|v| v.y < y + 2.0)).flatten().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    let size = (hi.x - lo.x).min(hi.z - lo.z);
    (size > 0.0).then(|| (Vec3::new((lo.x + hi.x) / 2.0, y, (lo.z + hi.z) / 2.0), size))
}

/// Buzz stood on mover `m` while it rises `rise` a tick for 40 ticks, moved inside the tick
/// (as `Script::step` does) or between ticks (as `step_creatures` does). Where his feet end
/// up against its top; None if he doesn't come to stand on it at rest.
fn ride(world: &World, m: i32, at: Vec3, rise: f32, inside: bool) -> Option<f32> {
    let mut p = Player::new(at - Vec3::Y * 4.0);
    let dt = 1.0 / 60.0;
    for _ in 0..20 {
        p.update(world, Input::default(), dt);
    }
    if !p.touching_mover(m) || (p.pos.y - at.y).abs() > 12.0 {
        return None;
    }
    let start = world.mover_pos(m).unwrap();
    for _ in 0..40 {
        if inside {
            p.update_with(world, Input::default(), dt, &mut |_, w| w.move_mover(m, Vec3::new(0.0, -rise, 0.0)));
        } else {
            world.move_mover(m, Vec3::new(0.0, -rise, 0.0));
            p.update(world, Input::default(), dt);
        }
    }
    let up = start.y - world.mover_pos(m).unwrap().y;
    world.place_mover(m, start);
    world.clear_motion();
    // Feet against where the top has gone.
    Some(p.pos.y - (at.y - up))
}

#[test]
fn rising_platforms_carry_buzz() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let mut failed = Vec::new();
    let mut rode = 0;
    for level in [4u32, 7, 10] {
        let t = terrain::parse(&fs::read(game.join(format!("data/level{level:02}/TERRAIN.ALL"))).unwrap()).unwrap();
        let world = World::from_terrain(&t);
        for (i, o) in t.objects.iter().enumerate().filter(|(_, o)| o.kind == 8 && o.linked != 0) {
            let m = o.linked as i32 - 1;
            let Some((at, size)) = top(&t, i) else { continue };
            // Big enough to stand on (Buzz's body is 250 across).
            if size < 300.0 {
                continue;
            }
            for (rise, inside) in [(12.0, true), (40.0, true), (12.0, false)] {
                let Some(off) = ride(&world, m, at, rise, inside) else { continue };
                rode += 1;
                if off.abs() > 12.0 {
                    failed.push(format!("level {level} mover {m:#x} rising {rise} ({}): feet {off:.0} from its top", if inside { "in the tick" } else { "between ticks" }));
                }
            }
        }
    }
    eprintln!("{rode} rides");
    assert!(rode > 20);
    assert!(failed.is_empty(), "{failed:#?}");
}

/// Buzz `above` over mover `m`'s top moving at `vy` (object units a tick, + down) while
/// it rises `rise` a tick, both for 12 ticks: whether he's jumping up slower than it comes
/// (bouncing off the pool's float as it bobs up, a lift catching him mid-jump) or dropping
/// onto it, he ends on it. Where his feet end against its top.
fn meet(world: &World, m: i32, at: Vec3, above: f32, vy: i32, rise: f32) -> Option<f32> {
    let start = world.mover_pos(m)?;
    let mut p = Player::new(at - Vec3::Y * above);
    p.vel[1] = vy;
    let dt = 1.0 / 60.0;
    for _ in 0..12 {
        p.update_with(world, Input::default(), dt, &mut |_, w| w.move_mover(m, Vec3::new(0.0, -rise, 0.0)));
    }
    let up = start.y - world.mover_pos(m).unwrap().y;
    world.place_mover(m, start);
    world.clear_motion();
    Some(p.pos.y - (at.y - up))
}

#[test]
fn a_rising_platform_meeting_buzz_puts_him_on_it() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let mut failed = Vec::new();
    let mut met = 0;
    for level in [2u32, 4, 7, 10] {
        let t = terrain::parse(&fs::read(game.join(format!("data/level{level:02}/TERRAIN.ALL"))).unwrap()).unwrap();
        let world = World::from_terrain(&t);
        for (i, o) in t.objects.iter().enumerate().filter(|(_, o)| o.kind == 8 && o.linked != 0) {
            let m = o.linked as i32 - 1;
            let Some((at, size)) = top(&t, i) else { continue };
            // Solid at the middle of its top, clear above it (no other floor within 2500 over it)
            // and big enough to land on.
            if size < 300.0 || world.floor_below(at.x, at.z, at.y - 2500.0).is_none_or(|y| (y - at.y).abs() > 2.0) {
                continue;
            }
            for (above, vy, rise) in [(400.0, 0x1000, 48.0), (20.0, -0x300, 150.0), (20.0, -0x100, 60.0)] {
                let Some(off) = meet(&world, m, at, above, vy, rise) else { continue };
                met += 1;
                if off.abs() > 12.0 {
                    failed.push(format!("level {level} mover {m:#x} (from {above} at {vy:#x}, rising {rise}): feet {off:.0} from its top"));
                }
            }
        }
    }
    eprintln!("{met} meetings");
    assert!(met > 20);
    assert!(failed.is_empty(), "{failed:#?}");
}

/// A plank tipping up into Buzz (a see-saw's end, a swaying platform's edge) puts him on it:
/// for level 2's see-saws (movers 0 / 1, yaws 0x961 / 0xB4A) and level 4's hanging
/// platforms (1–4, yaws 0xC00, 0xC00, 0, 0), Buzz just over a point 600–900 out from the
/// pivot, hopping up, while the plank turns 14 a tick (12-bit) about its own Z so that point
/// rises, for 16 ticks.
#[test]
fn a_tipping_plank_lifts_buzz() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let mut failed = Vec::new();
    let mut tipped = 0;
    for (level, planks) in [(2u32, &[(0, 0x961), (1, 0xB4A)][..]), (4, &[(1, 0xC00), (2, 0xC00), (3, 0), (4, 0)][..])] {
        let t = terrain::parse(&fs::read(game.join(format!("data/level{level:02}/TERRAIN.ALL"))).unwrap()).unwrap();
        let world = World::from_terrain(&t);
        for &(m, yaw) in planks {
            let Some(pivot) = world.mover_pos(m) else { continue };
            world.turn_mover(m, [0, yaw, 0]);
            world.clear_motion();
            // A point on its top, out from the pivot, and the way of turning that lifts it.
            let height = |x: f32, z: f32| world.floor_below_obj(x, z, pivot.y - 3000.0).filter(|(_, o)| *o == Some(m)).map(|(y, _)| y);
            let spots = (0..16).flat_map(|k| [600.0, 900.0].map(|r| (k as f32 / 16.0 * std::f32::consts::TAU, r))).filter_map(|(a, r)| {
                let (x, z) = (pivot.x + a.sin() * r, pivot.z + a.cos() * r);
                height(x, z).map(|y| (x, y, z))
            });
            for (x, y, z) in spots.take(4) {
                let lifts = |dir: i32| {
                    world.turn_mover(m, [0, yaw, dir * 40]);
                    let up = height(x, z).is_some_and(|h| h < y - 1.0);
                    world.turn_mover(m, [0, yaw, 0]);
                    up
                };
                let Some(dir) = [1, -1].into_iter().find(|&d| lifts(d)) else { continue };
                world.clear_motion();
                let mut p = Player::new(Vec3::new(x, y - 8.0, z));
                p.vel[1] = -0x100;
                let dt = 1.0 / 60.0;
                for k in 1..=16 {
                    p.update_with(&world, Input::default(), dt, &mut |_, w| w.turn_mover(m, [0, yaw, dir * 14 * k]));
                }
                tipped += 1;
                let floor = height(p.pos.x, p.pos.z);
                if floor.is_none_or(|f| p.pos.y > f + 12.0) {
                    failed.push(format!("level {level} plank {m}: feet at {:.0}, its top under him at {floor:?}", p.pos.y));
                }
                world.turn_mover(m, [0, yaw, 0]);
                world.clear_motion();
            }
        }
    }
    eprintln!("{tipped} tips");
    assert!(tipped >= 6);
    assert!(failed.is_empty(), "{failed:#?}");
}

/// A mover driven sideways into Buzz (a swinging girder, a lift's wall) shoves him along
/// rather than passing through him: for every mover at least 600 tall in levels 4, 7 and
/// 10, Buzz just off its +x side, half way up, while it moves 40 a tick towards him for 20
/// ticks; his middle never ends inside its box.
#[test]
fn a_mover_coming_sideways_pushes_buzz() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let mut failed = Vec::new();
    let mut pushed = 0;
    for level in [4u32, 7, 10] {
        let t = terrain::parse(&fs::read(game.join(format!("data/level{level:02}/TERRAIN.ALL"))).unwrap()).unwrap();
        let world = World::from_terrain(&t);
        for (i, o) in t.objects.iter().enumerate().filter(|(_, o)| o.kind == 8 && o.linked != 0) {
            let m = o.linked as i32 - 1;
            let pts: Vec<Vec3> = t.object_triangles(i).into_iter().flat_map(|(p, _)| p.map(Vec3::from)).collect();
            let lo = pts.iter().fold(Vec3::MAX, |a, p| a.min(*p));
            let hi = pts.iter().fold(Vec3::MIN, |a, p| a.max(*p));
            if hi.y - lo.y < 600.0 || hi.z - lo.z < 300.0 {
                continue;
            }
            let start = world.mover_pos(m).unwrap();
            let mid = (lo + hi) / 2.0;
            // His body (radius 125, its middle 131 above his feet) just clear of the side.
            let mut p = Player::new(Vec3::new(hi.x + 140.0, mid.y + 131.0, mid.z));
            let dt = 1.0 / 60.0;
            let mut inside = false;
            for _ in 0..20 {
                p.vel[1] = 0;
                p.update_with(&world, Input::default(), dt, &mut |_, w| w.move_mover(m, Vec3::new(-40.0, 0.0, 0.0)));
                let off = world.mover_pos(m).unwrap() - start;
                let (a, b) = (lo + off + Vec3::splat(20.0), hi + off - Vec3::splat(20.0));
                let c = p.pos - Vec3::Y * 131.0;
                inside |= c.cmpgt(a).all() && c.cmplt(b).all();
            }
            world.place_mover(m, start);
            world.clear_motion();
            pushed += 1;
            if inside {
                failed.push(format!("level {level} mover {m:#x}"));
            }
        }
    }
    eprintln!("{pushed} pushes");
    assert!(pushed > 10);
    assert!(failed.is_empty(), "went through Buzz: {failed:?}");
}
