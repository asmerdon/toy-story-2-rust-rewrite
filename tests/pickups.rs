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
