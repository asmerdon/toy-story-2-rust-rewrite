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

/// Smooth-shaded models: normal blocks + per-vertex normal indices.
#[test]
fn smooth_models_reach_geometry_end() {
    let Some(dir) = game_dir() else { return };
    for name in ["chars/buzz", "chars/bopeep", "chars5/buzz", "chars5/zemp", "chars5/bub"] {
        let m = all::parse(&fs::read(dir.join(format!("data/{name}.all"))).unwrap()).unwrap();
        assert_eq!(m.stop, Stop::GeometryEnd, "{name}");
        for p in &m.parts {
            if !p.normal_indices.is_empty() {
                assert_eq!(p.normal_indices.len(), p.vertex_count(), "{name}");
            }
        }
        assert!(m.parts.iter().any(|p| !p.normals.is_empty()), "{name} has no normals");
    }
}

/// ZURG1's hit ellipsoid and sphere (the `.all` directory's type-9 entry).
#[test]
fn zurg1_hit_shapes() {
    let Some(game) = std::env::var_os("TS2_GAME_DIR").map(std::path::PathBuf::from) else { return };
    let b = std::fs::read(game.join("data/chars/zurg1.all")).unwrap();
    let h = ts2::formats::all::hit_shapes(&b).unwrap();
    assert_eq!(h.sphere, [0, -6560, 0, 298]);
    assert_eq!(h.ellipsoids.len(), 3);
    assert_eq!(h.ellipsoid(7), ts2::formats::all::Ellipsoid { offset: [0, -6944, 0], scale: [256, 269, 561], radius: 248 });
}
