//! Parses every `.all` model in a real install. Skipped unless TS2_GAME_DIR is set,
//! because the repository never contains game data.

use std::{env, fs, path::PathBuf};
use ts2::formats::all::{self, Stop};

fn game_dir() -> Option<PathBuf> {
    env::var_os("TS2_GAME_DIR").map(PathBuf::from)
}

#[test]
fn every_model_parses_some_geometry() {
    let Some(dir) = game_dir() else {
        eprintln!("TS2_GAME_DIR not set; skipping");
        return;
    };
    let mut count = 0;
    for chars in fs::read_dir(dir.join("data")).unwrap() {
        let chars = chars.unwrap().path();
        if !chars.file_name().unwrap().to_string_lossy().to_ascii_lowercase().starts_with("chars") {
            continue;
        }
        for f in fs::read_dir(&chars).unwrap() {
            let f = f.unwrap().path();
            if !f.extension().is_some_and(|e| e.eq_ignore_ascii_case("all")) {
                continue;
            }
            let m = all::parse(&fs::read(&f).unwrap()).unwrap();
            assert!(m.poly_count() > 0, "{} has no polygons", f.display());
            for p in m.parts.iter().flat_map(|p| &p.polys) {
                assert_eq!(p.verts.len(), p.colors.len());
                assert!(p.verts.iter().all(|v| v.pos.iter().all(|c| c.abs() < 8192)), "{} has out-of-range vertex", f.display());
            }
            count += 1;
        }
    }
    assert!(count > 60, "only found {count} models");
}

#[test]
fn woody_reaches_geometry_end() {
    let Some(dir) = game_dir() else { return };
    let m = all::parse(&fs::read(dir.join("data/chars/woody.all")).unwrap()).unwrap();
    assert_eq!(m.stop, Stop::GeometryEnd);
    assert_eq!(m.poly_count(), 240);
}
