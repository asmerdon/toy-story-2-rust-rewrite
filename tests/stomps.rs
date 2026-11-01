//! Every stomp switch in the game, landed on with Buzz's own physics: on the tick a script
//! sees the impact (stomp timer −0x28, 0x434D20), he must be standing on the switch's surface
//! type or mover. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::terrain;
use ts2::game::{Input, Player, World};

#[derive(Debug, Clone, Copy)]
enum Target {
    Surface(u8),
    Mover(i32),
}

/// Level (campaign number), what is stomped, and what it is.
const SWITCHES: &[(u32, Target, &str)] = &[
    (1, Target::Surface(8), "room 3 spring"),
    (2, Target::Surface(8), "switch 8"),
    (2, Target::Surface(9), "switch 9"),
    (2, Target::Surface(0xA), "spring 0xA"),
    (2, Target::Surface(0xB), "squash pad 0xB"),
    (4, Target::Surface(0x21), "paint button 0x21"),
    (4, Target::Surface(0x22), "paint button 0x22"),
    (4, Target::Surface(0x23), "paint button 0x23"),
    (4, Target::Surface(0x24), "surface 0x24"),
    (4, Target::Mover(0x16), "button mover 0x16"),
    (4, Target::Mover(0x17), "button mover 0x17"),
    (4, Target::Mover(0x18), "button mover 0x18"),
    (7, Target::Surface(8), "spring"),
    (7, Target::Mover(0xF), "hay-bale button"),
    (8, Target::Surface(8), "claw button"),
    (10, Target::Surface(0x20), "switch 0x20"),
    (10, Target::Surface(0x21), "switch 0x21"),
    (10, Target::Surface(0x22), "switch 0x22"),
    (10, Target::Mover(4), "vent 4"),
    (10, Target::Mover(5), "vent 5"),
    (11, Target::Mover(7), "valve 7"),
    (11, Target::Mover(8), "valve 8"),
    (11, Target::Mover(6), "valve 6"),
    (11, Target::Mover(9), "valve 9"),
    (11, Target::Mover(1), "pipe switch 1"),
    (11, Target::Mover(3), "pipe switch 3"),
    (11, Target::Mover(4), "pipe switch 4"),
    (14, Target::Surface(0x20), "helicopter button 0x20"),
    (14, Target::Surface(0x21), "helicopter button 0x21"),
    (14, Target::Surface(0x22), "helicopter button 0x22"),
    (14, Target::Surface(0x23), "helicopter button 0x23"),
];

fn terrain_of(game: &std::path::Path, level: u32) -> terrain::Terrain {
    let (folder, file) = if level > 10 {
        (level - 10, "TERR1.ALL")
    } else {
        (level, "TERRAIN.ALL")
    };
    terrain::parse(&fs::read(game.join(format!("data/level{folder:02}/{file}"))).unwrap()).unwrap()
}

/// The middle of the target's top (level units, y down): the object holding its highest
/// upward face, and the middle of that object's upward faces at that level.
fn top_of(t: &terrain::Terrain, target: Target) -> Option<Vec3> {
    let mut best: Option<(f32, Vec<[Vec3; 3]>)> = None;
    for (i, o) in t.objects.iter().enumerate() {
        let hit = match target {
            // Floor switches are fixed terrain.
            Target::Surface(s) => {
                o.kind != 8 && o.flags & 0x400 == 0 && (o.flags & 0xFF) as u8 == s
            }
            Target::Mover(m) => o.kind == 8 && o.linked as i32 == m + 1,
        };
        if !hit {
            continue;
        }
        let faces: Vec<[Vec3; 3]> = t
            .object_triangles(i)
            .into_iter()
            .filter(|(_, n)| Vec3::from(*n).normalize_or_zero().y < -0.9)
            .map(|(p, _)| p.map(Vec3::from))
            .collect();
        let Some(top) = faces.iter().flatten().map(|v| v.y).reduce(f32::min) else {
            continue;
        };
        if best.as_ref().is_none_or(|(y, _)| top < *y) {
            best = Some((top, faces));
        }
    }
    let (top, faces) = best?;
    let (lo, hi) = faces
        .iter()
        .filter(|f| f.iter().any(|v| v.y < top + 50.0))
        .flatten()
        .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), v| {
            (lo.min(*v), hi.max(*v))
        });
    Some(Vec3::new((lo.x + hi.x) / 2.0, top, (lo.z + hi.z) / 2.0))
}

/// Buzz stood on the target jumps, stomps at the top and lands. What a script running in the
/// tick (as `Script::step` does) sees when the stomp timer is −0x28: on the floor, the
/// surface and the mover under him.
fn stomp_on(world: &World, at: Vec3, mover: i32) -> Option<(bool, Option<u8>, bool)> {
    let mut p = Player::new(at - Vec3::Y * 4.0);
    let dt = 1.0 / 60.0;
    for _ in 0..30 {
        p.update(world, Input::default(), dt);
    }
    if std::env::var_os("STOMP_TRACE").is_some() {
        eprintln!(
            "  settled at {:?} floor {} surface {:?} mover {:?}",
            p.pos, p.on_floor, p.surface, p.floor_mover
        );
    }
    let mut seen = None;
    for tick in 0..240 {
        let input = Input {
            jump: tick == 0,
            jump_held: tick < 4,
            stomp: tick == 14,
            ..Default::default()
        };
        p.update_with(world, input, dt, &mut |p, _| {
            if p.stomp == -40 && seen.is_none() {
                seen = Some((p.on_floor, p.surface, p.touching_mover(mover)));
                if std::env::var_os("STOMP_TRACE").is_some() {
                    eprintln!("  impact at {:?}", p.pos);
                }
            }
        });
        if std::env::var_os("STOMP_TRACE").is_some() {
            eprintln!(
                "    t{tick} y {:.1} floor {} stomp {} mover {:?} vy {}",
                p.pos.y,
                p.on_floor,
                p.stomp,
                p.floor_mover.map(|m| m.0),
                p.vel[1]
            );
        }
        if seen.is_some() {
            break;
        }
    }
    seen
}

#[test]
fn every_stomp_switch_registers() {
    let Some(game) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else {
        return;
    };
    let mut failed = Vec::new();
    let mut last = None;
    let mut world = None;
    for &(level, target, what) in SWITCHES {
        if last != Some(level) {
            let t = terrain_of(&game, level);
            world = Some((World::from_terrain(&t), t));
            last = Some(level);
        }
        let (w, t) = world.as_ref().unwrap();
        let Some(at) = top_of(t, target) else {
            failed.push(format!("level {level} {what}: not in the terrain"));
            continue;
        };
        // Level 2's spring is under the old way (mover 6) until both switches are down.
        w.set_mover(6, !(level == 2 && matches!(target, Target::Surface(0xA))));
        let mover = if let Target::Mover(m) = target { m } else { -1 };
        let seen = stomp_on(w, at, mover);
        let ok = match (target, seen) {
            (Target::Surface(s), Some((true, Some(got), _))) => got == s,
            // 0x48B640 asks for contact, not the floor.
            (Target::Mover(_), Some((_, _, touching))) => touching,
            _ => false,
        };
        eprintln!(
            "level {level:2} {what:24} at {at:?}: {seen:?} {}",
            if ok { "ok" } else { "MISSED" }
        );
        if !ok {
            failed.push(format!("level {level} {what}: {seen:?}"));
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}
