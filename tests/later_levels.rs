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
/// Level 13's conveyor floors (surfaces 0x20–0x24) push Buzz through 0x48B660 every tick
/// (0x42CA60). The original sweeps that push with his velocity (0x488E10 folds it into the
/// motion 0x482A00 tests), and the belts end at the level's kind-0x101 boundary walls (here
/// x 12969 and x 5010; the third belt ends at a wall near z 9990). The port added the push straight to his position and had no
/// boundary walls, so a belt carried him out of the level.
#[test]
fn airport_conveyor_ends_hold_buzz_inside_the_level() {
    let Some(mut l) = load(13) else { return };
    // (start on the belt, which axis it runs along, the wall it ends at)
    for ([x, y, z], axis, wall) in
        [([7000.0, -1875.0, 7749.3], 0, 12969.0), ([11950.0, -13875.0, 1749.3], 0, 5010.0), ([11250.0, -16875.0, 5850.7], 2, 9990.0)]
    {
        let floor = l.world.floor_below(x, z, y - 50.0).unwrap();
        let mut p = Player::new(Vec3::new(x, floor, z));
        p.on_floor = true;
        let mut beyond = f32::MIN;
        for _ in 0..600 {
            l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
            p.tick(&l.world, [0.0, 0.0], false, false);
            let along = p.pos[axis];
            beyond = beyond.max(if wall > [x, 0.0, z][axis] { along - wall } else { wall - along });
        }
        assert!(beyond < -50.0, "belt from ({x}, {z}) carried Buzz {beyond:.0} past its end wall (at {:?})", p.pos);
        assert!(beyond > -200.0, "belt from ({x}, {z}) never reached its end (at {:?})", p.pos);
        let under = l.world.floor_below(p.pos.x, p.pos.z, p.pos.y - 300.0);
        assert!(p.on_floor && under.is_some_and(|f| (p.pos.y - f).abs() < 10.0), "off the floor at {:?} ({under:?})", p.pos);
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

/// Level 4: move the crane to stop `k` (1-based) of list 3.
fn crane_to(l: &mut Level, lists: &[leveldat::PointList], k: usize) {
    let p = lists.iter().find(|l| l.kind == 3).unwrap().points[k - 1];
    let at = l.world.mover_pos(0).unwrap();
    l.world.move_mover(0, Vec3::new(p[0] as f32 - at.x, 0.0, p[2] as f32 - at.z));
}

fn pour(l: &mut Level, p: &mut Player, surface: u8) {
    p.on_floor = true;
    p.stomp = -40;
    p.surface = Some(surface);
    l.script.step_creatures(&l.world, &mut l.specials, p, &mut l.cr, 0);
    p.stomp = 0;
    for _ in 0..0x40 {
        l.script.step_creatures(&l.world, &mut l.specials, p, &mut l.cr, 0);
    }
}

#[test]
fn construction_paint_mixes_and_delivers_for_token_three() {
    let Some(mut l) = load(4) else { return };
    let game = PathBuf::from(env::var_os("TS2_GAME_DIR").unwrap());
    let path = ts2::campaign::level(4).unwrap().path(&game);
    let lists = leveldat::parse(&fs::read(path.with_extension("dat")).unwrap()).unwrap();
    let mut p = Player::new(Vec3::new(1.0e6, 0.0, 0.0));
    // Stops 5, 6, 7 pour red, blue, yellow (buttons 0x21, 0x22, 0x23).
    for (a, b, target) in [(5, 6, 1), (5, 7, 2), (6, 7, 3)] {
        crane_to(&mut l, &lists, a);
        pour(&mut l, &mut p, (0x1C + a) as u8);
        crane_to(&mut l, &lists, b);
        pour(&mut l, &mut p, (0x1C + b) as u8);
        assert!(l.cr.slots[5].scale_y.unwrap() > 0x780);
        assert!(!l.script.reveals.contains(&3));
        crane_to(&mut l, &lists, target);
        // Delivered, it drains for 32 ticks; pours meanwhile are ignored.
        for _ in 0..0x21 {
            l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        }
    }
    assert!(l.script.reveals.contains(&3));
    // One delivery jingle (−5) each, not one a tick.
    let delivered = l.script.sounds.iter().filter(|(id, _)| *id == 0xFFFB).count();
    assert_eq!(delivered, 3, "delivery jingles");
}

#[test]
fn construction_paint_same_colour_twice_drains() {
    let Some(mut l) = load(4) else { return };
    let game = PathBuf::from(env::var_os("TS2_GAME_DIR").unwrap());
    let path = ts2::campaign::level(4).unwrap().path(&game);
    let lists = leveldat::parse(&fs::read(path.with_extension("dat")).unwrap()).unwrap();
    let mut p = Player::new(Vec3::new(1.0e6, 0.0, 0.0));
    crane_to(&mut l, &lists, 5);
    pour(&mut l, &mut p, 0x21);
    assert_eq!(l.cr.slots[5].light, [0x680, 0, 0]);
    pour(&mut l, &mut p, 0x21);
    for _ in 0..0x20 {
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
    }
    // Drained to a thirty-second (as the original leaves it), and no longer touchable.
    assert!(l.cr.slots[5].scale_y.unwrap() < 0x180);
    assert_eq!(l.cr.slots[5].flags & 1, 0);
}

#[test]
fn space_land_claw_catches_the_prize_and_drops_it_out() {
    let Some(mut l) = load(8) else { return };
    let claw = l.specials.pos_obj(0xA).unwrap();
    // Stand by the machine (room 4) on its button.
    let mut p = Player::new(Vec3::from(claw.map(|v| v as f32 / 32.0)) + Vec3::new(0.0, 500.0, 0.0));
    if l.world.room_at(p.pos) != Some(4) {
        eprintln!("claw test: start not in room 4 ({:?}); skipped", l.world.room_at(p.pos));
        return;
    }
    l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    assert_eq!(l.script.token_touch.first(), Some(&(3, false)));
    p.on_floor = true;
    p.surface = Some(8);
    let start = l.specials.pos_obj(0xA).unwrap();
    for _ in 0..3 {
        p.stomp = -40;
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        p.stomp = 0;
        for _ in 0..0x40 {
            l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
        }
    }
    let low = l.specials.pos_obj(0xB).unwrap();
    assert!(low[1] > start[1] + 0x2000, "the claw dropped");
    // The drop takes 160 ticks, then 60 at the bottom before it closes.
    for _ in 0..0xA0 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    assert!(!l.specials.get(0xB).unwrap().hidden, "closed at the bottom");
    l.script.token_touch.clear();
    for _ in 0..0x400 {
        l.script.step(&l.world, &mut l.specials, &mut p, &l.boxes);
    }
    // Back at the top and open: empty, or having carried the prize to the chute, in which
    // case it has fallen out and can be taken.
    let up = l.specials.pos_obj(0xA).unwrap();
    assert_eq!(up[1], start[1]);
    assert!(!l.specials.get(0xA).unwrap().hidden);
    let caught = l.script.token_touch.contains(&(3, true));
    // With rand.dat's bytes the shuffled prize lands under the claw.
    assert!(caught);
}

#[test]
fn final_arena_lod_suitcases_stay_aligned_during_intro() {
    let Some(mut l) = load(15) else { return };
    let p = Player::new(Vec3::new(-4000., -1000., 0.));
    for frame in 0..500 {
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        l.script.tick_look();
        for (near, far) in [(0, 1), (3, 4)] {
            let a = Vec3::from(l.specials.pos_obj(near).unwrap().map(|v| v as f32));
            let b = Vec3::from(l.specials.pos_obj(far).unwrap().map(|v| v as f32));
            assert!((a-b).length() < 256., "frame {frame}: near {a:?}, far {b:?}");
        }
    }
}

/// TERR1.ALL's floor ends at x = +6500 behind the start, and no polygon walls it off: the
/// arena is closed by its kind-0x101 boundary list (x about 6385–6450 at the back), vertical
/// walls the original's collision 0x484380 gathers from 0x7290F4 and tests with 0x480660.
/// Running or jumping at the back must stop him on the floor, alive. (Before the boundary
/// port he ran off and died at the fall plane 0x436220, which still applies to real falls.)
#[test]
fn final_arena_back_is_closed_by_its_boundary_wall() {
    let Some(l) = load(15) else { return };
    let game = PathBuf::from(env::var_os("TS2_GAME_DIR").unwrap());
    let exe = Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let s = exe.player_start(15).unwrap();
    let floor = l.world.floor_below(s.pos[0], s.pos[2], s.pos[1] - 600.0).unwrap();
    assert!((l.world.lowest - 82.0).abs() < 0.5, "lowest collision {}", l.world.lowest);
    for (jump, z) in [(false, s.pos[2]), (true, s.pos[2]), (false, 3000.0), (false, -3000.0)] {
        let mut p = Player::new(Vec3::new(s.pos[0], floor, z));
        p.on_floor = true;
        let mut most = f32::MIN;
        for t in 0..900 {
            let jumping = jump && t % 60 < 20;
            p.tick(&l.world, [1.0, 0.0], jumping, false);
            most = most.max(p.pos.x);
        }
        assert!(most > 6100.0, "jump {jump} z {z}: never reached the back (x {most})");
        assert!(most < 6450.0 - 120.0, "jump {jump} z {z}: went through the back wall (x {most})");
        assert!(p.dying == 0 && p.health > 0, "jump {jump} z {z}: killed at {:?}", p.pos);
        let under = l.world.floor_below(p.pos.x, p.pos.z, p.pos.y - 300.0);
        assert!(under.is_some_and(|f| p.pos.y < f + 10.0), "jump {jump} z {z}: not over the floor at {:?} ({under:?})", p.pos);
    }
}

/// One frame of the final arena as the viewer couples it: Buzz, particles touching him, the
/// creatures' shared update and contact, then the level script. Returns the particle types
/// the script and creatures made and whether a particle hurt Buzz.
struct Fight {
    l: Level,
    parts: ts2::particles::Particles,
    p: Player,
    made: Vec<ts2::creatures::ParticleRequest>,
    particle_hurts: usize,
    contact_hurts: usize,
}
impl Fight {
    fn new() -> Option<Fight> {
        let l = load(15)?;
        let game = PathBuf::from(env::var_os("TS2_GAME_DIR")?);
        let exe = Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
        let random = fs::read(game.join("data/rand.dat")).unwrap();
        let s = exe.player_start(15).unwrap();
        let floor = l.world.floor_below(s.pos[0], s.pos[2], s.pos[1] - 600.0).unwrap();
        let mut p = Player::new(Vec3::new(s.pos[0], floor, s.pos[2]));
        p.set_facing(s.angle as i32);
        p.on_floor = true;
        let parts = ts2::particles::Particles::new(Some(&exe), random);
        Some(Fight { l, parts, p, made: Vec::new(), particle_hurts: 0, contact_hurts: 0 })
    }
    fn frame(&mut self, stick: [f32; 2], spin: bool) {
        let Fight { l, parts, p, .. } = self;
        p.tick(&l.world, stick, false, spin);
        parts.camera = p.pos_obj();
        parts.tick(&l.world, (p.pos_obj(), p.facing));
        parts.touch(p.pos_obj(), p.spin > 0x14 || p.super_spinning());
        for e in parts.events.drain(..).collect::<Vec<_>>() {
            if let ts2::particles::Event::Hurt(at) = e {
                let (b, h) = (p.pos_obj(), p.health);
                p.hurt(ts2::boxes::angle12((b[0] - at[0]) as f32, (b[2] - at[2]) as f32), 3);
                self.particle_hurts += (p.health < h) as usize;
            }
        }
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        let h = p.health;
        l.cr.contact(p);
        for (angle, mode) in l.cr.hurts.drain(..).collect::<Vec<_>>() {
            p.hurt(angle, mode);
        }
        self.contact_hurts += (p.health < h) as usize;
        for r in l.cr.particles.drain(..).chain(l.script.particles.drain(..)) {
            self.made.push(r);
            parts.create(r.pos, r.vel, r.gravity, r.angle, r.spin, r.typ);
        }
        for (at, t, m, _, _) in l.script.spawns.drain(..) {
            parts.spawn(at, t, m);
        }
        for (at, t, m, _) in l.cr.spawns.drain(..) {
            parts.spawn(at, t, m);
        }
        l.script.step_creatures(&l.world, &mut l.specials, p, &mut l.cr, 0);
        l.script.tick_look();
        // Out of batteries: back in play (the viewer's respawn), so the fight can go on.
        if p.health < 0 && p.dying == 0 {
            let at = p.pos;
            p.respawn(at, p.facing);
        }
    }
}

/// 0x42F310 / 0x42F7B0: the blacksmith and the prospector start their swing only on a tick
/// their shared update reported Buzz in their zone (struct byte +6 bit 1, which needs their
/// flag 8). With it off (as their swing's end leaves it), Buzz close by and the walk animation
/// playing are not enough.
#[test]
fn final_arena_bosses_swing_only_while_chasing() {
    let Some(mut f) = Fight::new() else { return };
    let mut t = 0;
    while !f.l.script.boss_hud && t < 2000 {
        let walk = f.l.script.look.is_none() && f.p.pos.x > -3300.0;
        f.frame(if walk { [-1.0, 0.0] } else { [0.0; 2] }, false);
        t += 1;
    }
    let beside = |f: &mut Fight, k: usize| {
        let c = &f.l.cr.slots[k];
        let at = Vec3::from(c.pos.map(|v| v as f32 / 32.0)) + Vec3::new(300.0, 0.0, 0.0);
        f.p.pos = Vec3::new(at.x, f.l.world.floor_below(at.x, at.z, at.y - 600.0).unwrap_or(at.y), at.z);
        f.p.vel = [0; 3];
    };
    // Buzz beside the blacksmith; the prospector (its flag 8 kept off) walks within 300 of him.
    let (mut unchased, mut chased_swings) = (0, 0);
    for _ in 0..600 {
        f.l.cr.slots[2].flags &= !8;
        beside(&mut f, 0);
        f.p.health = 14;
        let before = [0, 2].map(|k| f.l.cr.slots[k].anim);
        f.frame([0.0; 2], false);
        for (i, k) in [0, 2].into_iter().enumerate() {
            let c = &f.l.cr.slots[k];
            let chase = c.step.chase & 1 != 0;
            if before[i] == [1, 4][i] && !chase && ts2::script::near(c.pos, f.p.pos_obj(), 300) {
                unchased += 1;
            }
            // A swing started this tick (one already under way may finish).
            if before[i] != 3 && c.anim == 3 {
                assert!(chase, "boss {k} swung on a tick it was not chasing");
                chased_swings += 1;
            }
        }
    }
    eprintln!("walk ticks near Buzz without the chase bit {unchased}, swings while chasing {chased_swings}");
    assert!(unchased > 0, "the case never arose");
    assert!(chased_swings > 0, "no boss swung while chasing");
}

/// The whole fight coupled as the viewer runs it: the bosses' real attacks (the gunslinger's
/// shots 0x61, the blacksmith's 0x66 from 0x42F310, the prospector's 0x68 from 0x42F7B0)
/// reach Buzz and cost him batteries; his spin hits each boss through 0x407440 / 0x408A60
/// until all three fall below 10 (0x42F310 / 0x42F530 / 0x42F7B0), and 0x42FC50 plays the
/// rescue ending and finishes the level.
#[test]
fn final_arena_fight_hurts_buzz_and_all_three_bosses_fall_to_hits() {
    let Some(mut f) = Fight::new() else { return };
    // Walk in (-X) until the intro starts, then wait for the fight.
    let mut t = 0;
    while !f.l.script.boss_hud && t < 2000 {
        let walk = f.l.script.look.is_none() && f.p.pos.x > -3300.0;
        f.frame(if walk { [-1.0, 0.0] } else { [0.0; 2] }, false);
        t += 1;
    }
    assert!(f.l.script.boss_hud, "fight never started");
    assert_eq!(f.l.script.boss_bar, Some(54));
    // Stand in the arena: the bosses come for him and attack.
    let h0 = f.p.health;
    for _ in 0..1500 {
        f.frame([0.0; 2], false);
    }
    let typs: Vec<u8> = f.made.iter().map(|r| r.typ).collect();
    eprintln!("made {typs:x?}; particle hurts {}, contact hurts {}, health {h0} -> {}", f.particle_hurts, f.contact_hurts, f.p.health);
    for typ in [0x61, 0x66, 0x68] {
        assert!(typs.contains(&typ), "no {typ:#x} attack");
    }
    let swing = f.made.iter().find(|r| r.typ == 0x66).unwrap();
    assert_eq!((swing.vel, swing.gravity & 3), ([0, -2, 0], 0), "0x42F310's arguments");
    assert!(f.particle_hurts > 0, "no attack hurt Buzz");
    assert!(f.p.health < h0 || f.p.lives < 5);
    // Spin into each boss in turn until it falls; health only changes through the hits.
    let gunslinger_speed = f.l.cr.slots[1].rec.speed;
    for k in 0..3 {
        let mut t = 0;
        let mut seen = vec![f.l.cr.slots[k].health];
        let mut flinched = false;
        while f.l.cr.slots[k].health >= 10 && t < 6000 {
            if t % 50 == 0 {
                let c = &f.l.cr.slots[k];
                let at = Vec3::from(c.pos.map(|v| v as f32 / 32.0)) + Vec3::new(120.0, 0.0, 0.0);
                f.p.pos = Vec3::new(at.x, f.l.world.floor_below(at.x, at.z, at.y - 600.0).unwrap_or(at.y), at.z);
                f.p.vel = [0; 3];
            }
            f.frame([0.0; 2], t % 50 == 1);
            let c = &f.l.cr.slots[k];
            if *seen.last().unwrap() != c.health {
                seen.push(c.health);
            }
            flinched |= c.rec.state == 4 && c.health >= 10;
            t += 1;
        }
        let c = &f.l.cr.slots[k];
        eprintln!("boss {k} after {t} ticks: health {seen:?}");
        assert!(c.health < 10, "boss {k} not beaten");
        assert!(seen.windows(2).all(|w| w[0] - w[1] == 2), "boss {k}: spin hits take 2 each: {seen:?}");
        assert!(flinched, "boss {k} never showed its hurt state");
        assert_eq!(c.rec.state, 4);
    }
    assert_eq!(f.l.cr.slots[1].rec.speed, gunslinger_speed, "0x42F530 leaves the speed");
    assert_eq!(f.l.script.boss_bar, Some(0));
    // The rescue ending: Woody and Jessie set down, then the level is over.
    for _ in 0..1000 {
        f.frame([0.0; 2], false);
        if f.l.script.done {
            break;
        }
    }
    assert!(f.l.script.done, "no rescue ending");
    assert_eq!((f.l.cr.slots[3].health, f.l.cr.slots[4].health), (1, 1));
    // Set down at 0x42FC50's spot (their own update moves them in height afterwards).
    assert_eq!([f.l.cr.slots[4].pos[0], f.l.cr.slots[4].pos[2]], [-0x46944, -0x1BEA]);
}

#[test]
fn final_arena_coupled_creature_simulation() {
    let Some(mut l) = load(15) else { return };
    let p = Player::new(Vec3::new(-4000., -1000., 0.));
    let mut positions = [Vec::new(), Vec::new(), Vec::new()];
    for frame in 0..1000 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.script.step_creatures(&l.world, &mut l.specials, &p, &mut l.cr, 0);
        l.script.tick_look();
        if frame > 600 {
            for k in 0..3 { positions[k].push(l.cr.slots[k].pos); }
        }
    }
    for k in 0..3 {
        let c = &l.cr.slots[k];
        eprintln!("boss {k}: pos={:?} health={} active={} anim={} state={} flags={:x}", c.pos,c.health,c.active,c.anim,c.rec.state,c.flags);
        assert!(positions[k].iter().any(|p| *p != positions[k][0]), "boss {k} never moved");
    }
    assert!(l.script.boss_hud);
}
