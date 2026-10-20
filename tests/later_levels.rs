//! Real-data integration checks for the recovered later campaign scripts.
use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::{
    boxes::Boxes,
    creatures::Creatures,
    formats::{exe::Exe, leveldat, ngn, packets, terrain},
    game::{Player, World},
    script::{self, Script},
    specials::Specials,
};
struct Level {
    world: World,
    specials: Specials,
    cr: Creatures,
    boxes: Boxes,
    script: Box<dyn Script>,
}
fn load(number: usize) -> Option<Level> {
    let game = PathBuf::from(env::var_os("TS2_GAME_DIR")?);
    let lv = ts2::campaign::level(number).unwrap();
    let path = lv.path(&game);
    let exe = Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(&path).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(path.with_extension("dat")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = Specials::new(&n);
    let boxes = Boxes::new(lv.internal_id, &lists, Some(&exe), &world, &mut specials);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let script = script::for_level(
        lv.internal_id,
        &world,
        &mut specials,
        Some(&exe),
        &lists,
        Some(random.clone()),
    )
    .unwrap();
    let pk = packets::parse(&fs::read(path.with_extension("raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&exe), random);
    cr.load_shapes(&game.join("data"));
    cr.load_animations(&n);
    cr.always_active = true;
    Some(Level {
        world,
        specials,
        cr,
        boxes,
        script,
    })
}
fn touch(l: &mut Level, slot: usize, p: &Player) {
    l.cr.slots[slot].flags |= 0x200;
    l.script
        .step_creatures(&l.world, &mut l.specials, p, &mut l.cr, 0);
}
#[test]
fn airport_platforms_move_and_stomp_springs_launch() {
    let Some(mut l) = load(13) else { return };
    let mut p = Player::new(Vec3::ZERO);
    let start = l.world.mover_pos(8).unwrap();
    for _ in 0..90 {
        l.world.clear_motion();
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    assert!((l.world.mover_pos(8).unwrap() - start).length() > 100.0);
    for (mover, model) in [(0, 9), (14, 0x1C)] {
        p.stomp = -40;
        p.floor_mover = Some((mover, -Vec3::Y));
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        assert_eq!(p.stomp, 0);
        assert_eq!(p.vel[1], -0xC00);
        assert_ne!(l.specials.angles(model).unwrap()[2], 0);
    }
}
#[test]
fn airport_weights_challenge_times_out_and_can_be_won_on_retry() {
    let Some(mut l) = load(13) else { return };
    let mut p = Player::new(Vec3::ZERO);
    touch(&mut l, 5, &p);
    assert_eq!(l.script.race, 1);
    assert_eq!(l.script.show_collectables, Some(true));
    l.script.cutscene = None;
    for _ in 0..4000 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        l.script
            .step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.script.race, 0);
    assert_eq!(l.script.show_collectables, Some(false));
    touch(&mut l, 5, &p);
    l.script.cutscene = None;
    l.script.collected = 5;
    touch(&mut l, 5, &p);
    assert_eq!(l.script.race, 3);
    assert_eq!(l.script.cutscene.as_ref().unwrap().token, Some(2));
}
#[test]
fn airport_prospector_intro_damage_and_reward() {
    let Some(mut l) = load(13) else { return };
    let at = l.cr.slots[32].pos;
    eprintln!(
        "airport boss id {} health {} at {:?}",
        l.cr.slots[32].id, l.cr.slots[32].health, at
    );
    let mut p = Player::new(Vec3::from(at.map(|v| v as f32 / 32.0)));
    p.on_floor = true;
    l.script
        .step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    assert!(l.script.cutscene.is_some());
    l.script.cutscene = None;
    l.script
        .step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    assert!(l.script.boss_hud);
    assert_eq!(l.script.boss_bar, Some(54));
    l.cr.slots[32].health = 9;
    for _ in 0..130 {
        l.script
            .step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.script.boss_bar, Some(0));
    assert!(l.script.reveals.contains(&4));
}

#[test]
fn final_arena_requires_all_three_bosses_before_rescue_ending() {
    let Some(mut l) = load(15) else { return };
    let mut p = Player::new(Vec3::new(-4000.0, -1000.0, 0.0));
    p.on_floor = true;
    for _ in 0..600 {
        l.script
            .step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        l.script.tick_look();
    }
    assert!(l.script.boss_hud);
    assert_eq!(l.script.boss_bar, Some(54));
    for k in 0..3 {
        l.cr.slots[k].health = 9;
        l.script
            .step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        assert!(!l.script.done);
        assert_eq!(l.script.boss_bar, Some((2 - k as i32) * 18));
    }
    for _ in 0..430 {
        l.script.tick_look();
        l.script
            .step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert!(l.script.done);
    assert_eq!(l.cr.slots[3].health, 1);
    assert_eq!(l.cr.slots[4].health, 1);
}

#[test]
fn tarmac_switch_sets_its_lamp_and_clears_the_previous_one() {
    use ts2::level14::switch_bits;
    assert_eq!(switch_bits(0b0000, 0), 0b0001);
    assert_eq!(switch_bits(0b1000, 0), 0b0001);
    assert_eq!(switch_bits(0b0001, 1), 0b0010);
    assert_eq!(switch_bits(0b0111, 2), 0b0101);
}

#[test]
fn tarmac_plane_turns_and_boss_intro_damage_and_reward() {
    let Some(mut l) = load(14) else { return };
    let mut p = Player::new(Vec3::ZERO);
    for _ in 0..60 {
        l.world.clear_motion();
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    assert_eq!(l.world.mover_turn(0)[1], 60);
    let at = l.cr.slots[46].pos;
    let p = Player::new(Vec3::from(at.map(|v| v as f32 / 32.0)));
    l.script
        .step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    assert!(l.script.cutscene.is_some());
    l.script.cutscene = None;
    l.script
        .step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    assert!(l.script.boss_hud);
    l.cr.slots[46].health = 9;
    for _ in 0..130 {
        l.script
            .step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert_eq!(l.script.boss_bar, Some(0));
    assert!(l.script.reveals.contains(&4));
}

fn stomp_on(p: &mut Player, mover: i32) {
    p.stomp = -40;
    p.on_floor = true;
    p.floor_mover = Some((mover, -Vec3::Y));
}

#[test]
fn penthouse_valves_raise_the_water_and_the_plug_drains_it() {
    let Some(mut l) = load(11) else { return };
    let mut p = Player::new(Vec3::ZERO);
    // Valve 3 (mover 6) fills to −0xFA00 at 0x40 a tick.
    stomp_on(&mut p, 6);
    l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    p.stomp = 0;
    p.floor_mover = None;
    for _ in 0..0x400 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    assert_eq!(p.water, (1, -0xFA00 + 0x2EA18));
    assert!(l.script.put_out.contains(&(1, 1)));
    // Special 0x5B has grown in at that level.
    assert!(!l.specials.get(0x5B).unwrap().hidden);
    // The plug drains it at 0x200 a tick.
    p.pos = Vec3::new(0x2CC2E as f32, 0x2EE2B as f32, -0x2FC03 as f32) / 32.0;
    for _ in 0..0x80 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    assert_eq!(p.water, (0, 0));
}

#[test]
fn penthouse_turret_button_looks_then_wrecks_its_turret() {
    let Some(mut l) = load(11) else { return };
    let mut p = Player::new(Vec3::ZERO);
    stomp_on(&mut p, 2);
    l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    assert_eq!(l.script.look.map(|l| l.ticks), Some(0xB4));
    p.stomp = 0;
    p.floor_mover = None;
    for _ in 0..0x3C {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    assert!(l.specials.get(0x32).unwrap().hidden);
    assert!(!l.specials.get(0x42).unwrap().hidden);
}

#[test]
fn penthouse_pipe_switch_moves_flow_and_the_boat_sails() {
    let Some(mut l) = load(11) else { return };
    let mut p = Player::new(Vec3::new(1.0e6, 0.0, 0.0));
    let boat = l.specials.pos_obj(0x26).unwrap();
    // Pipes start 1, 4 and 0x20; switch 2 (mover 3) moves 4 on to 8.
    assert!(!l.specials.get(0x1F).unwrap().hidden);
    stomp_on(&mut p, 3);
    l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    assert!(l.specials.get(0x1F).unwrap().hidden);
    assert!(!l.specials.get(0x20).unwrap().hidden);
    p.stomp = 0;
    p.floor_mover = None;
    for _ in 0..120 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    let now = l.specials.pos_obj(0x26).unwrap();
    let moved = (0..3).map(|k| ((now[k] - boat[k]) as f64).powi(2)).sum::<f64>().sqrt();
    assert!(moved > 1000.0, "boat moved {moved}");
}

#[test]
fn penthouse_gunslinger_intro_fight_and_token() {
    let Some(mut l) = load(11) else { return };
    let mut p = Player::new(Vec3::new(-0xA8000 as f32, 0x1F000 as f32, -0x30000 as f32) / 32.0);
    l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    assert!(l.script.cutscene.is_some());
    l.script.cutscene = None;
    p.pos.y = 0x1F000 as f32 / 32.0;
    l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    assert!(l.script.boss_hud);
    assert_eq!(l.script.boss_bar, Some((29 - 9) * 0x36 / 0x14));
    l.cr.slots[11].health = 9;
    l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    assert_eq!(l.script.boss_bar, Some(0));
    assert!(!l.script.reveals.contains(&4));
    for _ in 0..0x78 {
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    assert!(l.script.reveals.contains(&4));
}
