//! Level 2's script. Skipped unless TS2_GAME_DIR is set.

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
    let dir = game.join("data/level02");
    let e = exe::Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = ts2::specials::Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let boxes = ts2::boxes::Boxes::new(2, &lists, Some(&e), &world, &mut specials);
    let script = ts2::script::for_level(2, &world, &mut specials, Some(&e), &lists, Some(random.clone())).unwrap();
    let pk = packets::parse(&fs::read(dir.join("level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&e), random);
    cr.load_shapes(&game.join("data"));
    Some(L2 { world, specials, script, boxes, cr })
}

/// The see-saws' collision is turned by their yaw at rest: there's floor along the plank
/// through the special's position.
#[test]
fn see_saws_lie_along_their_yaw() {
    let Some(l) = load() else { return };
    for id in [0, 1] {
        let p = Vec3::from(l.specials.pos_obj(id).unwrap().map(|v| v as f32 / 32.0));
        let yaw = [0x961, 0xB4A][id as usize] as f32 / 4096.0 * std::f32::consts::TAU;
        let along = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let floor = |q: Vec3| l.world.floor_below(q.x, q.z, q.y - 400.0);
        let hits = [-900.0, -400.0, 0.0, 400.0, 900.0].iter().filter(|&&d| floor(p + along * d).is_some_and(|y| (y - p.y).abs() < 300.0)).count();
        eprintln!("see-saw {id} at {p:?}: {hits} of 5 along the plank");
        assert!(hits >= 4, "see-saw {id} isn't lying along its yaw");
    }
}

/// Touching the sarge (slot 6) asks for the troops; with five found he thanks Buzz and
/// gives token 1.
#[test]
fn sarge_wants_his_troops() {
    let Some(mut l) = load() else { return };
    let troops: Vec<usize> = (0..l.cr.slots.len()).filter(|&i| l.cr.slots[i].id == 13 && l.cr.slots[i].health == 0x66).collect();
    eprintln!("troops: {troops:?}");
    assert_eq!(troops.len(), 5);
    let visit = |l: &mut L2, slot: usize| {
        let at = Vec3::from(l.cr.slots[slot].pos.map(|v| v as f32 / 32.0));
        let mut p = Player::new(at);
        for _ in 0..3 {
            l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
            p.pos = Vec3::from(l.cr.slots[slot].pos.map(|v| v as f32 / 32.0));
            l.cr.contact(&mut p);
            l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        }
    };
    visit(&mut l, 6);
    assert!(l.script.cutscene.is_some(), "the sarge asks");
    l.script.cutscene = None;
    // Troop 4 starts untouchable (record flags 0x20) until something in the level frees it.
    l.cr.slots[4].flags |= 0x80;
    for &t in &troops {
        visit(&mut l, t);
    }
    assert_eq!(l.cr.sheep, 5);
    visit(&mut l, 6);
    assert_eq!(l.script.cutscene.as_ref().map(|c| c.token), Some(Some(1)));
}

/// On the roof the kite's challenge starts; after it the fight shows the boss bar.
#[test]
fn kite_challenge_on_the_roof() {
    let Some(mut l) = load() else { return };
    let at = Vec3::new(0x2B000 as f32 / 32.0, 0.0, 0x3D000 as f32 / 32.0);
    let y = l.world.floor_below(at.x, at.z, -0x76000 as f32 / 32.0).expect("the roof");
    eprintln!("roof at {y}");
    let mut p = Player::new(Vec3::new(at.x, y, at.z));
    p.on_floor = true;
    for _ in 0..3 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.cr.boss.state, 1, "challenged");
    l.script.cutscene = None;
    for _ in 0..60 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    eprintln!("kite at {:?}, bar {:?}, hud {}", l.cr.slots[0x1A].pos.map(|v| v >> 5), l.script.boss_bar, l.script.boss_hud);
    assert_eq!(l.cr.boss.state, 2);
    assert_eq!(l.script.boss_bar, Some(54));
}

/// In the pool Buzz sinks slowly.
#[test]
fn pool_water_slows_falls() {
    let Some(mut l) = load() else { return };
    let mut p = Player::new(Vec3::new(-0x30000 as f32 / 32.0, 0x7000 as f32 / 32.0, 0.0));
    let mut top = 0;
    for _ in 0..20 {
        p.tick(&l.world, [0.0; 2], false, false);
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        top = top.max(p.vel[1]);
    }
    eprintln!("in water {}, fastest fall {top:#x}", p.in_water);
    assert!(p.in_water);
    assert!(top <= 0x400);
}


#[test]
fn pool_ball_deflates_then_pumping_launches_it_with_collision() {
    let Some(mut l) = load() else { return };
    let mut p = Player::new(Vec3::ZERO);
    let home = l.specials.get(6).unwrap().clone();
    for _ in 0..400 { l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes); }
    assert!((l.specials.get(6).unwrap().scale[1] - home.scale[1] / 4.0).abs() < 0.001);
    let mut launched = false;
    for _ in 0..20 {
        p.surface = Some(11); p.coyote = 6; p.stomp = -40;
        for _ in 0..30 { l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes); }
        p.surface = None; p.coyote = 0; p.stomp = 0;
        for _ in 0..30 { l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes); }
        if l.specials.get(6).unwrap().scale[1] == home.scale[1] { launched = true; break; }
    }
    assert!(launched, "repeated pump presses should fill the ball");
    assert_ne!(l.specials.get(6).unwrap().pos, home.pos);
    assert!(l.script.put_out.contains(&(3,1)));
    assert!(l.script.sounds.iter().any(|(id,_)| *id == 0xB));
    for _ in 0..500 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        let pos = l.specials.get(6).unwrap().pos;
        assert!((pos[1] - 0x4400 as f32 / 32.0).abs() < 2500.0, "ball escaped water: {pos:?}");
        let collision = l.world.mover_pos(7).unwrap();
        assert!((collision.y - (pos[1] - 128.0)).abs() < 60.0);
    }
    assert!(l.script.spawns.iter().any(|(_,kind,_,_,_)| *kind == 0x39));
}


/// Supply the collision contact a player walking into a box would produce; exercise the
/// real box path update and level script together, including releasing the push latch.
fn push_obstacle(l: &mut L2, p: &mut Player, index: usize) {
    let angle = l.boxes.push_state().2[index];
    p.set_facing(angle);
    p.target = angle;
    p.stick_pushed = true;
    p.on_floor = true;
    p.coyote = 6;
    let a = angle as f32 * std::f32::consts::TAU / 4096.0;
    p.wall_mover = Some((l.boxes.boxes[index].mover, Vec3::new(-a.sin(),0.0,-a.cos())));
    l.boxes.tick(p, &l.world, &mut l.specials);
    l.script.step(&l.world, &mut l.specials, p, &l.boxes);
}

#[test]
fn pushed_drawbridge_falls_and_swaps_collision() {
    let Some(mut l) = load() else { return };
    assert_eq!(l.boxes.boxes.len(),2);
    let home = l.specials.angles(5).unwrap()[2] - 0x2EE;
    let mut p = Player::new(Vec3::from(l.boxes.boxes[0].pos.map(|v| v as f32 / 32.0)));
    assert!(!l.world.mover_enabled(4));
    for _ in 0..50 {
        push_obstacle(&mut l, &mut p, 0);
        if l.world.mover_enabled(4) { break; }
    }
    assert!(l.world.mover_enabled(4));
    assert!(!l.world.mover_enabled(2) && !l.world.mover_enabled(3));
    assert_eq!(l.boxes.pushing_index(),0);
    for _ in 0..300 { l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes); }
    assert_eq!(l.specials.angles(5).unwrap()[2], home & 0xFFF);
    assert!(l.script.sounds.iter().any(|(id,_)| *id == 0x34));
}

#[test]
fn tree_only_tips_when_pushed_from_its_upper_platform() {
    let Some(mut l) = load() else { return };
    let home = l.specials.angles(0x1E).unwrap()[2];
    let mut p = Player::new(Vec3::from(l.boxes.boxes[1].pos.map(|v| v as f32 / 32.0)));
    p.pos.y = 0.0;
    for _ in 0..20 { push_obstacle(&mut l, &mut p, 1); }
    assert_eq!(l.boxes.progress(1),0);
    assert!(!l.world.mover_enabled(0xC));
    p.pos.y = -0x10300 as f32 / 32.0;
    push_obstacle(&mut l, &mut p, 1);
    assert!(l.world.mover_enabled(0xC));
    assert!(!l.world.mover_enabled(0xB));
    assert_eq!(l.boxes.pushing_index(),0);
    assert_eq!([p.vel[0],p.vel[2]],[0,0]);
    for _ in 0..100 { l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes); }
    assert_eq!(l.specials.angles(0x1E).unwrap()[2], (home + 0xFFF - 0x245) & 0xFFF);
    assert!(l.script.sounds.iter().any(|(id,_)| *id == 0x34));
}
