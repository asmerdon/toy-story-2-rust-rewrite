//! The front end with the real exe tables and the level-select map. Skipped unless
//! TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::exe::Exe;
use ts2::frontend::{FrontEnd, Pad, Screen, Tables};

fn setup() -> Option<(PathBuf, Exe)> {
    let dir = PathBuf::from(env::var_os("TS2_GAME_DIR")?);
    let exe = Exe::parse(fs::read(dir.join("toy2.exe")).ok()?).ok()?;
    Some((dir, exe))
}

#[test]
fn tables_match_the_campaign() {
    let Some((_, exe)) = setup() else { return };
    let t = Tables::read(&exe);
    assert_eq!(t.order, ts2::campaign::LEVELS.iter().map(|l| l.internal_id).collect::<Vec<_>>());
    assert_eq!(t.names[1], "andy's house");
    assert_eq!(t.names[3], "bombs away");
    assert_eq!(t.names[15], "final showdown");
    assert_eq!(&t.needed[..15], &[0, 0, 3, 0, 0, 10, 0, 0, 18, 0, 0, 28, 0, 0, 40]);
}

fn select(dir: &std::path::Path, f: &mut FrontEnd) {
    let hub = ts2::campaign::scene_path(dir, ts2::campaign::HUB);
    let lists = ts2::formats::leveldat::parse(&fs::read(hub.with_extension("dat")).unwrap()).unwrap();
    let random = fs::read(dir.join("data/rand.dat")).unwrap_or_default();
    f.begin_select(&lists, random);
}

#[test]
fn level_select_flies_to_a_level_and_gates_on_tokens() {
    let Some((dir, exe)) = setup() else { return };
    let mut f = FrontEnd::new(Tables::read(&exe));
    // Andy's House and the Neighbourhood done with one token: Bombs Away (3 needed) is locked.
    f.progress.tokens[1] = 1;
    f.screen = Screen::Select;
    select(&dir, &mut f);
    let sel = f.select.as_ref().unwrap();
    assert_eq!((sel.level, sel.last), (1, 2));
    // The camera arrives at level 1's views.
    for _ in 0..600 {
        f.tick(Pad(0));
    }
    let frame = f.frame(&[]);
    let (cam, _, _) = frame.camera.unwrap();
    let hub = ts2::campaign::scene_path(&dir, ts2::campaign::HUB);
    let lists = ts2::formats::leveldat::parse(&fs::read(hub.with_extension("dat")).unwrap()).unwrap();
    let spots = &lists.iter().find(|l| l.kind == 2).unwrap().points;
    let near = (3..6).any(|k| (0..3).all(|i| (cam[i] / 32 - spots[k][i]).abs() < 1500));
    eprintln!("camera at {:?}", cam.map(|v| v / 32));
    assert!(near, "camera {:?} not near level 1's views {:?}", cam.map(|v| v / 32), &spots[3..6]);
    assert!(frame.pieces.len() == 13, "{} map pieces", frame.pieces.len());
    assert!(!frame.glyphs.is_empty(), "no level name");
    // Right to level 2 (the last that can be chosen), right again does nothing.
    for b in [Pad::RIGHT, 0, Pad::RIGHT, 0] {
        f.tick(Pad(b));
    }
    assert_eq!(f.select.as_ref().unwrap().level, 2);
    // Level 2 has none needed: jump picks it.
    f.tick(Pad(Pad::JUMP));
    for _ in 0..0x20 {
        f.tick(Pad(0));
    }
    assert_eq!(f.screen, Screen::Play(2));
    assert_eq!(f.screen.scene(&f.tables), 2);

    // With level 2 done (1 token in all), level 3 is the newest and needs 3: refused.
    f.progress.tokens[2] = 0;
    f.progress.tokens[1] = 1;
    f.progress.tokens[2] = 0x80;
    f.first_clear = true;
    f.level_done();
    select(&dir, &mut f);
    assert_eq!(f.select.as_ref().unwrap().level, 3, "the select moves on after a first clear");
    for _ in 0..0x40 {
        f.tick(Pad(0));
    }
    f.tick(Pad(Pad::JUMP));
    for _ in 0..0x40 {
        f.tick(Pad(0));
    }
    assert_eq!(f.screen, Screen::Select, "a locked level was entered");
    // Cancel goes back to the menu.
    for _ in 0..0x20 {
        f.tick(Pad(Pad::CANCEL));
    }
    assert_eq!(f.screen, Screen::Menu);
}
