//! Pickups from level.dat (list 63 + the special-object table). Skipped unless TS2_GAME_DIR
//! is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::{exe::Exe, leveldat, ngn};
use ts2::pickups::{Kind, Pickups};

/// Every level's object table parses, and each special object sits where the `.ngn` places
/// the instance with the same id (the PS1 records and the PC instances are the same objects).
#[test]
fn special_objects_match_ngn_instances() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let (mut checked, mut matched) = (0, 0);
    for nn in 1..=10 {
        for stem in ["level", "level1"] {
            let Ok(dat) = fs::read(dir.join(format!("data/level{nn:02}/{stem}.dat"))) else { continue };
            let objects = leveldat::special_objects(&dat).unwrap_or_else(|e| panic!("level{nn:02}/{stem}: {e}"));
            let Ok(b) = fs::read(dir.join(format!("data/level{nn:02}/{stem}.ngn"))) else { continue };
            let n = ngn::parse(&b).unwrap();
            for s in &n.specials {
                let Some(Some(o)) = objects.get(s.id as usize) else { continue };
                let p = n.layers[s.layer].instances[s.instance].pos;
                checked += 1;
                if (0..3).all(|k| (p[k] - o.pos[k] as f32).abs() < 2.0) {
                    matched += 1;
                }
            }
        }
    }
    eprintln!("{matched}/{checked} special objects at their instance's position");
    // Movers and multi-part objects start elsewhere (88% match exactly in the shipped data).
    assert!(checked > 300 && matched * 10 > checked * 8, "{matched}/{checked}");
}

#[test]
fn level_1_pickups() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let exe = Exe::parse(fs::read(dir.join("toy2.exe")).unwrap()).unwrap();
    let dat = fs::read(dir.join("data/level01/level.dat")).unwrap();
    let p = Pickups::new(1, &dat, Some(&exe)).unwrap();
    let count = |k: Kind| p.items.iter().filter(|i| i.kind == k).count();
    assert_eq!(p.items.iter().filter(|i| i.is_sprite()).count(), 70);
    assert_eq!(count(Kind::Battery), 6);
    assert_eq!(count(Kind::Life), 3);
    assert_eq!(count(Kind::Token), 10);
    // The level's tokens (0x4F0EDC) start hidden except index 3; the five reward spots too.
    assert_eq!(p.token_ids, [57, 58, 59, 60, 48]);
    let open: Vec<u8> = p.items.iter().filter(|i| i.kind == Kind::Token && !i.removed).map(|i| i.id).collect();
    assert_eq!(open, [60]);
    // The coin sprite: texture 31, 31-pixel cells.
    let coin = p.sprites[16].as_ref().unwrap();
    assert_eq!((coin.texture, coin.size, coin.cells[0]), (31, [31, 31], [0, 0xC0]));
    // Special 49 waits for an unlocked move (0x503A44): shown but not collectable.
    assert_eq!(p.items.iter().find(|i| i.id == 49).map(|i| (i.kind, i.radius)), Some((Kind::Shield, 0)));
}

/// The HUD's sprites (0x49FD40) all resolve to textures in level 1, and a fresh HUD draws
/// lives, batteries and coins.
#[test]
fn hud_sprites_resolve() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let e = Exe::parse(fs::read(dir.join("toy2.exe")).unwrap()).unwrap();
    let n = ngn::parse(&fs::read(dir.join("data/level01/level.ngn")).unwrap()).unwrap();
    let sprites = ts2::hud::sprite_table(&e, 1);
    for k in [6usize, 10, 11, 12, 13, 14, 15, 16, 20] {
        let d = sprites[k].as_ref().unwrap_or_else(|| panic!("sprite {k}"));
        let name = format!("tex{:02}", d.texture);
        assert!(n.textures.iter().any(|t| t.name.eq_ignore_ascii_case(&name)), "sprite {k}: {name} missing");
    }
    let mut hud = ts2::hud::Hud::default();
    hud.level_start();
    let status = ts2::hud::Status { lives: 5, health: 14, coins: 12, ..Default::default() };
    for _ in 0..40 {
        hud.tick(&status);
    }
    let q = hud.quads(&status, &sprites);
    let used: Vec<usize> = q.iter().map(|q| q.sprite).collect();
    // Drawn last-queued first (0x4B8DD0), so the battery column lands on top of its frame.
    assert_eq!(used, [0x14, 0x14, 0x10, 0xC, 6, 0xB, 0x14, 10]);
    // Digits: 5 lives, 1 and 2 coins.
    assert_eq!(q[6].cell, 0x1A + 5);
    assert_eq!((q[1].cell, q[0].cell), (0x1B, 0x1C));
    assert_eq!(ts2::hud::batches(&q, &sprites, &|slot| {
        let name = format!("tex{slot:02}");
        let i = n.textures.iter().position(|t| t.name.eq_ignore_ascii_case(&name))?;
        Some((i, [256.0, 256.0]))
    }).iter().map(|b| b.vertices.len()).sum::<usize>(), 8 * 6);
}

/// A token revealed in full (0x4A0DB0): hidden at first, a burst at tick 32, popping in
/// and settling at full size after 0x84 ticks.
#[test]
fn revealed_token_animates_in() {
    let Some(game) = std::env::var_os("TS2_GAME_DIR").map(std::path::PathBuf::from) else { return };
    let e = ts2::formats::exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let dat = std::fs::read(game.join("data/level01/level.dat")).unwrap();
    let mut pk = ts2::pickups::Pickups::new(1, &dat, Some(&e)).unwrap();
    assert_eq!(pk.burst.len(), 16);
    let at = pk.reveal_token_animated(0).expect("token 0 appears");
    let i = pk.items.iter().position(|p| p.id == pk.token_ids[0]).unwrap();
    let far = [at[0] as f32 + 100000.0, at[1] as f32, at[2] as f32];
    let (mut burst_at, mut first_seen) = (None, None);
    for t in 0..0x84 {
        let evs = pk.tick(far, [at[0] as f32, at[1] as f32, at[2] as f32]);
        if evs.iter().any(|e| matches!(e, ts2::pickups::Event::TokenBurst(_))) {
            burst_at = Some(t);
        }
        if first_seen.is_none() && pk.items[i].scale > 0.0 {
            first_seen = Some(t);
        }
    }
    eprintln!("burst at {burst_at:?}, visible from {first_seen:?}, scale now {}", pk.items[i].scale);
    assert_eq!(burst_at, Some(31));
    assert!(first_seen.is_some_and(|t| t > 30));
    assert_eq!(pk.items[i].scale, 1.0);
}
