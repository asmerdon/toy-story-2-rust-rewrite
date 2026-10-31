//! Boss and miniboss attacks against the original routines, on the real data. Each test
//! pins one behaviour the port had wrong. Skipped unless TS2_GAME_DIR is set.

use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::{
    creatures::{Creatures, Event},
    formats::{exe::Exe, leveldat, ngn, packets, terrain},
    game::{Player, World},
    particles::Particles,
    script::{self, sin_table, Script},
    specials::Specials,
};

struct Level {
    world: World,
    specials: Specials,
    cr: Creatures,
    script: Box<dyn Script>,
}

/// Level `internal` (the script number), from `data/levelNN` (the boss levels 3–9) or, with
/// `campaign`, from the campaign table's path (the later levels).
fn load(internal: usize, campaign: bool) -> Option<Level> {
    let game = PathBuf::from(env::var_os("TS2_GAME_DIR")?);
    let exe = Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let (ngn_path, id) = if campaign {
        let lv = ts2::campaign::level(internal).unwrap();
        (lv.path(&game), lv.internal_id)
    } else {
        (game.join(format!("data/level{internal:02}/level.ngn")), internal)
    };
    let n = ngn::parse(&fs::read(&ngn_path).unwrap()).unwrap();
    let lists = leveldat::parse(&fs::read(ngn_path.with_extension("dat")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(terrain::path_for(&ngn_path).unwrap()).unwrap()).unwrap();
    let world = World::from_terrain(&t);
    let mut specials = Specials::new(&n);
    let random = fs::read(game.join("data/rand.dat")).unwrap();
    let script = script::for_level(id, &world, &mut specials, Some(&exe), &lists, Some(random.clone())).unwrap();
    let pk = packets::parse(&fs::read(ngn_path.with_extension("raw")).unwrap()).unwrap();
    let spawns: Vec<_> = pk.iter().flat_map(packets::spawns).collect();
    let mut cr = Creatures::new(&spawns, Some(&exe), random);
    cr.load_shapes(&game.join("data"));
    cr.load_animations(&n);
    cr.always_active = true;
    Some(Level { world, specials, cr, script })
}

fn player_at(pos: [i32; 3]) -> Player {
    let mut p = Player::new(Vec3::from(pos.map(|v| v as f32 / 32.0)));
    p.on_floor = true;
    p
}

impl Level {
    fn step(&mut self, p: &Player) {
        self.script.step_creatures(&self.world, &mut self.specials, p, &mut self.cr, 0);
    }
    fn tick(&mut self, p: &Player) {
        self.cr.tick(&self.world, p.pos_obj(), p.pos_obj());
        self.step(p);
        self.cr.hurts.clear();
        if self.script.cutscene.is_some() {
            self.script.cutscene = None;
        }
    }
}

/// Zurg's lobbed shot (0x6C, 0x42B3A0) heads at facing − 0x80 in both components: x from the
/// sine table at facing − 0x80, z from 0x4FEE88, the table + 0x380 (the cosine of the same
/// angle). The port took z from facing + 0x400, sending it off sideways.
#[test]
fn zurg_lob_heads_at_facing_minus_0x80() {
    let Some(mut l) = load(12, true) else { return };
    let mut p = Player::new(Vec3::new(-3000.0, -2400.0, 1800.0));
    let mut lob = None;
    for _ in 0..1000 {
        l.tick(&p);
        p.pos = Vec3::new(-3000.0, -2400.0, 1800.0);
        if let Some(r) = l.script.particles.drain(..).find(|r| r.typ == 0x6C) {
            lob = Some((r, l.cr.slots[0].angles[1]));
            break;
        }
    }
    let (r, yaw) = lob.expect("Zurg lobs a shot");
    assert_eq!(r.vel, [sin_table(yaw - 0x80) >> 2, 0x400, sin_table(yaw + 0x380) >> 2], "facing {yaw:#x}");
}

/// The slime's blob (0x3F, 0x41AA10) is a flag-8 shot homing on Buzz (vy −2): 0x40FAE0's
/// seventh argument, facing << 2, is its starting heading (+0x18), and its angle is 0. The
/// port swapped them, so every blob set off due north.
#[test]
fn slime_blob_homes_from_its_facing() {
    let Some(mut l) = load(3, false) else { return };
    let s = l.cr.slots[0].pos;
    let p = player_at([s[0], s[1], s[2] + 0x10000 - 0x8000]);
    let mut fired = None;
    for _ in 0..2000 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.cr.hurts.clear();
        // Fighting (the scene over): its script's timer reads 1.
        let fighting = l.script.boss_hud && l.script.look.is_none();
        if fighting {
            l.cr.slots[0].timer = 1;
        }
        l.cr.particles.clear();
        l.step(&p);
        if fighting {
            fired = l.cr.particles.iter().find(|r| r.typ == 0x3F).copied().map(|r| (r, l.cr.slots[0].angles[1]));
            break;
        }
    }
    let (r, facing) = fired.expect("a blob");
    assert_eq!((r.vel, r.gravity, r.angle, r.spin), ([0, -2, 0], facing << 2, 0, 0x80));
}

/// Beaten, the slime's lid (special 0) drops from where 0x41AA10 put it: 0x52F6F4 is set to
/// the slime's height − 1000 while that height is lifted by 11000, so 12000 above it. The
/// port started it 1000 above, below the bounce floor (2000 above).
#[test]
fn slime_lid_drops_from_its_head() {
    let Some(mut l) = load(3, false) else { return };
    let s0 = l.cr.slots[0].pos;
    let p = player_at([s0[0], s0[1], s0[2] + 0x10000 - 0x8000]);
    let mut first = None;
    for tick in 0..30000 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        l.step(&p);
        l.cr.hurts.clear();
        if l.cr.events.drain(..).any(|e| matches!(e, Event::Died { slot: 0, .. })) {
            let s = l.cr.slots[0].pos;
            first = Some((l.specials.pos_obj(0).unwrap()[1], s[1]));
            break;
        }
        if l.script.look.is_none() && tick % 5 == 0 && l.cr.slots[0].rec.state != 4 && l.cr.slots[0].health > 0 {
            l.cr.hit(0, 0, 3);
        }
    }
    let (lid, s) = first.expect("the slime is beaten");
    // From s − 12000, one tick at −0xC00 + 0x60.
    let want = s - 12000 - 0xC00 + 0x60;
    assert!((lid - want).abs() < 64, "lid at {lid}, want about {want}");
}

/// The level-3 boss (script 6, 0x420060) flashes on alternate ticks while it can't be hurt
/// after a hit, and spins (lean + 0x40 a tick) instead of banking. The port did neither.
#[test]
fn hovering_boss_flashes_and_spins_after_a_hit() {
    let Some(mut l) = load(6, false) else { return };
    let s0 = l.cr.slots[0].pos;
    let p = player_at([0x30000, s0[1], s0[2]]);
    let mut fighting = false;
    for _ in 0..2000 {
        l.tick(&p);
        if l.script.boss_hud && l.script.look.is_none() && l.cr.slots[0].rec.state == 7 {
            fighting = true;
            break;
        }
    }
    assert!(fighting, "the fight starts");
    l.cr.hit(0, 0, 3);
    l.tick(&p);
    let mut lit = Vec::new();
    let mut lean = Vec::new();
    for _ in 0..6 {
        l.tick(&p);
        lit.push(l.cr.slots[0].lit);
        lean.push(l.cr.slots[0].angles[2]);
    }
    assert!(lit.windows(2).all(|w| w[0] != w[1]), "it blinks: {lit:?}");
    assert!(lean.windows(2).all(|w| (w[1] - w[0]) & 0xFFF == 0x40), "it spins: {lean:x?}");
}

/// The level-9 boss's minions (0x424490), once a pair has been called, are kept within
/// 0x15E0 (× 32) of the arena's middle; the port let them wander out.
#[test]
fn level9_minions_are_kept_in_the_arena() {
    let Some(mut l) = load(9, false) else { return };
    let p = player_at([0, -0x8000, 0]);
    l.tick(&p);
    // A hit picks the first pair (slots 8 and 8).
    let h = l.cr.slots[0].health;
    l.cr.slots[0].health = h - 1;
    l.step(&p);
    l.cr.slots[8].pos[0] = 0x100000;
    l.cr.slots[8].pos[2] = 0;
    l.step(&p);
    let c = l.cr.slots[8].pos;
    assert_eq!([c[0], c[2]], [sin_table(0x400) * 0x15E0 >> 9, sin_table(0x800) * 0x15E0 >> 9]);
}

/// The level-9 boss's beam heading wraps its turn by 0xFFF (0x424490), so just left of ahead
/// it aims one unit further right than a 0x1000 wrap would.
#[test]
fn level9_beam_heading_wraps_by_0xfff() {
    use ts2::level9::beam_heading;
    assert_eq!(beam_heading(0x100, 0x0F0), 0x0F1);
    assert_eq!(beam_heading(0x100, 0x110), 0x110);
    assert_eq!(beam_heading(0x100, 0x900), 0x300);
    assert_eq!(beam_heading(0x100, 0xD00), 0xF00);
}

/// Level 7's dino (0x420AF0) only breathes fire with Buzz in its room (0x54DEA0 == 4); its
/// timer still runs down. The port breathed fire anywhere.
#[test]
fn dino_breathes_fire_only_in_its_room() {
    let Some(mut l) = load(7, false) else { return };
    assert_eq!(l.cr.slots[0].id, 0x1A);
    let at = l.cr.slots[0].pos;
    l.cr.boss.state = 2;
    for (room, want) in [(false, 0), (true, 1)] {
        l.cr.boss.hud = room;
        l.cr.slots[0].timer = 10;
        l.cr.particles.clear();
        l.cr.tick(&l.world, at, at);
        let fire = l.cr.particles.iter().filter(|r| r.typ == 0x55).count();
        assert_eq!(fire, want, "in its room: {room}");
        assert_eq!(l.cr.slots[0].timer, 9);
    }
}

/// Level 8's boss missile (0x72, 0x422660) homes on Buzz from the boss's facing (0x40FAE0's
/// seventh argument, facing × 4) with its pitch (+0x20) at 0x400: it climbs straight up first.
/// The port passed the facing as the sprite angle and started it flat, heading north.
#[test]
fn level8_missile_launches_upwards_from_its_facing() {
    let Some(mut l) = load(8, false) else { return };
    assert_eq!(l.cr.slots[40].id, 0x2F);
    let at = l.cr.slots[40].pos;
    l.cr.boss.state = 2;
    l.cr.boss.spin = 600;
    l.cr.slots[40].timer = 0;
    l.cr.tick(&l.world, at, at);
    let r = *l.cr.particles.iter().find(|r| r.typ == 0x72).expect("a missile");
    let f = l.cr.slots[40].angles[1];
    assert_eq!((r.vel, r.gravity, r.angle, r.spin), ([0, -2, 0], f * 4, 0, 0x80));
    let game = PathBuf::from(env::var_os("TS2_GAME_DIR").unwrap());
    let exe = Exe::parse(fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let mut ps = Particles::new(Some(&exe), fs::read(game.join("data/rand.dat")).unwrap());
    ps.camera = r.pos;
    let i = ps.create(r.pos, r.vel, r.gravity, r.angle, r.spin, r.typ).unwrap();
    let far = [r.pos[0] + 0x40000, r.pos[1] + 0x40000, r.pos[2]];
    ps.tick(&l.world, (far, 0));
    let m = ps.slots[i];
    assert!(m.pos[1] < r.pos[1] - 0x100, "it climbs: {:?} from {:?}", m.pos, r.pos);
    let side = (m.pos[0] - r.pos[0]).abs() + (m.pos[2] - r.pos[2]).abs();
    assert!(side < (r.pos[1] - m.pos[1]) / 2, "mostly upwards at first: {:?} from {:?}", m.pos, r.pos);
}

/// In its room, level 8's boss turns the camera to itself and taunts (0xD4) when its timer
/// (0x52FB7C, 0x3C at the level's start) runs out (0x422660). The port did neither.
#[test]
fn level8_boss_taunts_in_its_room() {
    let Some(mut l) = load(8, false) else { return };
    let at = l.cr.slots[40].pos;
    let p = player_at(at);
    assert_eq!(l.world.room_at(p.pos), Some(5));
    let mut taunt = None;
    for tick in 0..400 {
        let fighting = l.cr.boss.state == 2;
        l.tick(&p);
        if fighting {
            assert!(l.cr.boss.focus.is_some(), "the camera turns to it");
        }
        if l.script.voices.contains(&0xD4) {
            taunt = Some(tick);
            break;
        }
    }
    assert!(taunt.is_some(), "it taunts");
}

fn final_arena_fight(l: &mut Level) -> Player {
    let p = Player::new(Vec3::new(-4000.0, -1000.0, 0.0));
    for _ in 0..600 {
        l.step(&p);
        l.script.tick_look();
    }
    assert!(l.script.boss_hud, "the fight is on");
    p
}

/// The final arena's wave 4 (0x42FC50 at 0x1AF) throws the bosses in at −0x700, −0x600 and
/// −0x800 (+0x48 of slots 0–2), and wave 3 sets the suitcases' target height (+0x60), not
/// their home.
#[test]
fn final_arena_bosses_jump_in_as_the_original() {
    let Some(mut l) = load(15, true) else { return };
    let p = Player::new(Vec3::new(-4000.0, -1000.0, 0.0));
    let homes = [l.cr.slots[3].home[1], l.cr.slots[4].home[1]];
    let mut seen = None;
    let mut wave3 = false;
    for _ in 0..600 {
        let before = l.cr.slots[3].vel[0];
        l.step(&p);
        l.script.tick_look();
        if before == 0 && l.cr.slots[3].vel[0] == -0x400 {
            wave3 = true;
            assert_eq!([l.cr.slots[3].target[1], l.cr.slots[4].target[1]], [0, 0]);
            assert_eq!([l.cr.slots[3].home[1], l.cr.slots[4].home[1]], homes);
        }
        if l.cr.slots[0].health == 29 {
            seen = Some([0, 1, 2].map(|k| l.cr.slots[k].vel[1]));
            break;
        }
    }
    assert!(wave3);
    assert_eq!(seen, Some([-0x700, -0x600, -0x800]));
}

/// The final arena's first boss (0x42F310) swings with 0x405CF0 alone: its animation rates
/// stay as they were (only the third boss sets 0xD0 first).
#[test]
fn final_arena_first_boss_swing_keeps_its_rates() {
    let Some(mut l) = load(15, true) else { return };
    let _ = final_arena_fight(&mut l);
    let at = l.cr.slots[0].pos;
    let p = Player::new(Vec3::from(at.map(|v| v as f32 / 32.0)));
    l.cr.slots[0].anim = 1;
    l.cr.slots[0].step.chase = 1;
    let rates = l.cr.slot_rates(0).unwrap();
    l.step(&p);
    assert_eq!(l.cr.slots[0].anim, 3, "it swings");
    assert_eq!(l.cr.slot_rates(0).unwrap(), rates);
    assert_ne!(rates, [-0x40; 2], "the old port's rates");
}

/// Level 13's Prospector (0x42BE60) and level 14's Blacksmith (0x42D3E0) only swing on a tick
/// their shared update found Buzz in their zone. The Blacksmith keeps its animation rates, and
/// its shockwave (0x66) is passed vy −2 (halved to −1: home on Buzz); the port passed −1,
/// which halves to 0 and left it a plain particle falling at its facing.
#[test]
fn prospector_and_blacksmith_swing_only_in_their_zone() {
    for (campaign, slot, ready) in [(13, 32, 4), (14, 46, 1)] {
        let Some(mut l) = load(campaign, true) else { return };
        let p = player_at(l.cr.slots[slot].pos);
        l.step(&p);
        assert!(l.script.cutscene.is_some());
        l.script.cutscene = None;
        l.step(&p);
        assert!(l.script.boss_hud);
        l.cr.slots[slot].anim = ready;
        l.cr.slots[slot].step.chase = 0;
        l.step(&p);
        assert_eq!(l.cr.slots[slot].anim, ready, "level {campaign}: no swing outside its zone");
        let rates = l.cr.slot_rates(slot).unwrap();
        l.cr.slots[slot].step.chase = 1;
        l.step(&p);
        assert_eq!(l.cr.slots[slot].anim, 3, "level {campaign}: it swings");
        if campaign == 14 {
            assert_eq!(l.cr.slot_rates(slot).unwrap(), rates, "its rates stay");
            let mut wave = None;
            for _ in 0..80 {
                l.step(&p);
                if let Some(r) = l.script.particles.drain(..).find(|r| r.typ == 0x66) {
                    wave = Some(r);
                    break;
                }
            }
            let r = wave.expect("a shockwave");
            assert_eq!(r.vel, [0, -2, 0]);
        }
    }
}

/// 0x420060 reads the scene's ticks (0x50A1F4) live, after 0x4020F0 has just set them: the
/// level-3 boss flies in for the whole 300-tick scene before fighting, and once beaten it
/// dies 180 ticks into its 300-tick scene, which then ends the level. The port read the
/// count from before the scene began (0), so it skipped the fly-in and ended the level on
/// the killing hit.
#[test]
fn hovering_boss_flies_in_and_dies_over_its_scenes() {
    let Some(mut l) = load(6, false) else { return };
    let s0 = l.cr.slots[0].pos;
    let p = player_at([0x30000, s0[1], s0[2]]);
    let (mut arrived, mut fought) = (None, None);
    let (mut beaten, mut died, mut done) = (None, None, None);
    for tick in 0..20000 {
        l.tick(&p);
        if arrived.is_none() && l.script.look.is_some() {
            arrived = Some(tick);
        }
        if fought.is_none() && l.script.boss_hud {
            fought = Some(tick);
        }
        if l.cr.events.drain(..).any(|e| matches!(e, Event::Died { slot: 0, .. })) && died.is_none() {
            died = Some(tick);
        }
        if l.script.done {
            done = Some(tick);
            break;
        }
        if fought.is_some() && beaten.is_none() && l.script.look.is_none() && tick % 10 == 0 && l.cr.slots[0].rec.state == 7 {
            l.cr.hit(0, 0, 3);
            if l.cr.slots[0].health < 0xB {
                beaten = Some(tick);
            }
        }
    }
    let (arrived, fought) = (arrived.expect("it arrives"), fought.expect("it fights"));
    assert!(fought - arrived >= 299, "the fly-in lasts its scene: {arrived} to {fought}");
    let (beaten, died, done) = (beaten.expect("beaten"), died.expect("killed"), done.expect("the level ends"));
    assert!((179..=182).contains(&(died - beaten)), "it dies 180 ticks in: {beaten} to {died}");
    assert!(done - beaten >= 299, "the death scene plays out: {beaten} to {done}");
}

/// 0x424490 likewise: a hit in the fight starts a 0x168-tick scene in which this round's flail
/// (slot 1 in the first round) calls up the pair and bursts; the round only ends once both are
/// beaten. The port read the old count (0) on the hit's tick, ended the round at once and
/// burst the next round's flail instead.
#[test]
fn level9_first_hit_bursts_the_first_flail() {
    let Some(mut l) = load(9, false) else { return };
    let p = player_at([0, -0x8000, 0]);
    let mut rising = 0;
    for _ in 0..1000 {
        l.tick(&p);
        if l.script.boss_hud {
            break;
        }
        rising += 1;
    }
    assert!(rising >= 0x167, "it rises for its whole scene: {rising}");
    let h = l.cr.slots[0].health;
    l.cr.slots[0].health = h - 1;
    l.cr.events.clear();
    let mut burst = None;
    for _ in 0..0x168 {
        l.tick(&p);
        if let Some(slot) = l.cr.events.drain(..).find_map(|e| match e {
            Event::Died { slot, .. } if (1..=6).contains(&slot) => Some(slot),
            _ => None,
        }) {
            burst = Some(slot);
            break;
        }
    }
    assert_eq!(burst, Some(1), "the first round's flail bursts");
    assert!(l.cr.slots[8].health > 0 && l.cr.slots[8].id != 0, "its minions are up");
}

/// When a hit ends one of the slime's rounds, 0x41AA10 starts the regrowing scene and, reading
/// its ticks live in the same frame (above 0x78), holds the slime's script (wait 5). The port
/// read the old count (0) and let it start regrowing at once.
#[test]
fn slime_waits_through_its_regrowing_scene() {
    let Some(mut l) = load(3, false) else { return };
    let s0 = l.cr.slots[0].pos;
    let p = player_at([s0[0], s0[1], s0[2] + 0x10000 - 0x8000]);
    let mut round_over = None;
    for tick in 0..20000 {
        l.cr.tick(&l.world, p.pos_obj(), p.pos_obj());
        let hit = l.script.boss_hud && l.script.look.is_none() && tick % 5 == 0 && l.cr.slots[0].rec.state != 4 && l.cr.slots[0].health > 0;
        if hit {
            l.cr.hit(0, 0, 3);
        }
        l.step(&p);
        l.cr.hurts.clear();
        if hit && l.script.look.is_some() {
            round_over = Some(l.cr.slots[0].wait);
            break;
        }
    }
    assert_eq!(round_over, Some(5));
}
