//! Decompresses every `.raw` packet file in a real install. Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::{ngn, packets};
use ts2::scene;

fn data_dir() -> Option<PathBuf> {
    Some(PathBuf::from(env::var_os("TS2_GAME_DIR")?).join("data"))
}

fn raw_files(dir: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in fs::read_dir(dir).unwrap() {
        let d = d.unwrap().path();
        if d.is_dir() {
            for f in fs::read_dir(&d).unwrap() {
                let f = f.unwrap().path();
                if f.extension().is_some_and(|e| e.eq_ignore_ascii_case("raw")) {
                    out.push(f);
                }
            }
        }
    }
    out
}

#[test]
fn every_packet_decompresses_to_its_stated_size() {
    let Some(dir) = data_dir() else {
        eprintln!("TS2_GAME_DIR not set; skipping");
        return;
    };
    let mut n = 0;
    for f in raw_files(&dir) {
        let b = fs::read(&f).unwrap();
        let p = packets::parse(&b).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        n += p.len();
    }
    eprintln!("{n} packets");
    assert!(n > 500);
}

#[test]
fn level01_spawns_are_inside_the_level_and_use_its_creatures() {
    let Some(dir) = data_dir() else { return };
    let raw = packets::parse(&fs::read(dir.join("level01/level.raw")).unwrap()).unwrap();
    let spawns: Vec<_> = raw.iter().flat_map(packets::spawns).collect();
    assert!(spawns.len() > 30, "{} spawns", spawns.len());

    let ngn = ngn::parse(&fs::read(dir.join("level01/level.ngn")).unwrap()).unwrap();
    let ids: Vec<usize> = ngn.creatures.iter().map(|c| c.id).collect();
    let (lo, hi) = scene::bounds(&scene::level_batches(&ngn)).unwrap();
    for s in &spawns {
        assert!(ids.contains(&(s.creature as usize)), "spawn uses creature {} not in level {ids:?}", s.creature);
        for k in 0..3 {
            let v = s.pos[k] as f32;
            assert!(v >= lo[k] - 1.0 && v <= hi[k] + 1.0, "spawn {s:?} outside level bounds {lo:?}..{hi:?}");
        }
    }
}
