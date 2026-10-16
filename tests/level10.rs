//! Level 10's script. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::creatures::Creatures;
use ts2::formats::{exe, leveldat, ngn, packets, terrain};
use ts2::game::{Player, World};
use ts2::script::Script;

#[allow(dead_code)]
struct L {
    world: World,
    specials: ts2::specials::Specials,
    script: Box<dyn Script>,
    boxes: ts2::boxes::Boxes,
    cr: Creatures,
    lists: Vec<leveldat::PointList>,
}

fn load() -> Option<L> {
    let game = env::var_os("TS2_GAME_DIR").map(PathBuf::from)?;
    let dir = game.join("data/level10");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let script = ts2::script::for_level(10, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let boxes = ts2::boxes::Boxes::new(10, &lists, Some(&e), &world, &mut specials);
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.load_shapes(&game.join("data"));
    Some(L { world, specials, script, boxes, cr, lists })
}

/// Standing at the bottom of the always-open air shaft (list 0), Buzz is carried up.
#[test]
fn air_shaft_lifts_buzz() {
    let Some(mut l) = load() else { return };
    let shaft = l.lists.iter().find(|p| p.kind == 0).unwrap().points.clone();
    let (bottom, top) = (shaft[0], shaft[1]);
    let start = Vec3::new(bottom[0] as f32, (bottom[1] - 200) as f32, bottom[2] as f32);
    let mut p = Player::new(start);
    let mut highest = start.y;
    for _ in 0..600 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        p.tick(&l.world, [0.0; 2], false, false);
        highest = highest.min(p.pos.y);
    }
    eprintln!("from {} up to {highest} (top {})", start.y, top[1]);
    assert!(highest < start.y - 3000.0, "carried up the shaft");
}

/// Standing on the spider's floor starts it; beaten, token 4 appears.
#[test]
fn spider_fight() {
    let Some(mut l) = load() else { return };
    assert_eq!(l.cr.slots[8].id, 0x1F);
    let s = l.cr.slots[8].home;
    let at = Vec3::new(s[0] as f32 / 32.0, (-0x1B0000) as f32 / 32.0, s[2] as f32 / 32.0);
    let mut p = Player::new(at);
    p.on_floor = true;
    let (mut hits, mut fought, mut powered) = (0, false, false);
    for tick in 0..30000 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        l.cr.hurts.clear();
        if l.script.cutscene.is_some() {
            l.script.cutscene = None;
        }
        fought |= l.script.boss_hud;
        powered |= l.cr.boss.phase > 0x400;
        if l.cr.boss.state == 2 && tick % 200 == 0 && l.cr.slots[8].rec.state == 7 && l.cr.slots[8].health > 0 {
            l.cr.hit(8, 0, 3);
            hits += 1;
        }
        if l.script.reveals.contains(&4) {
            break;
        }
    }
    eprintln!("{hits} hits, fought {fought}, fans {powered}, state {}", l.cr.boss.state);
    assert!(fought, "the fight starts");
    assert!(powered, "the fans run");
    assert!(l.script.reveals.contains(&4), "token 4 once it's beaten");
}

/// The runaway token: asked, token 2 shows and runs along its track; Buzz in the middle
/// (x ±4500 / z ±2500 at any height) loses it.
#[test]
fn runaway_token() {
    let Some(mut l) = load() else { return };
    let mut p = Player::new(Vec3::new(-8000.0, -30000.0, 0.0));
    l.cr.slots[0x16].flags |= 0x200;
    let mut moves = Vec::new();
    for _ in 0..400 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        if l.script.cutscene.is_some() {
            l.script.cutscene = None;
        }
        moves.extend(l.script.token_moves.drain(..));
    }
    assert!(l.script.reveals.contains(&2), "token 2 shows");
    assert_eq!(l.script.race, 2, "laps {} reveals {:?}", l.script.laps, l.script.reveals);
    assert!(moves.len() > 100, "it runs ({} moves)", moves.len());
    assert_ne!(moves.first().unwrap().1, moves.last().unwrap().1);
    p.pos = Vec3::new(0.0, -100.0, 0.0);
    for _ in 0..3 {
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.script.race, 0, "lost in the middle");
    assert!(l.script.hides.contains(&2));
}
