//! Collision files (`TERRAIN.ALL` / `TERR1.ALL`). Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::{exe, ngn, terrain};
use ts2::{game, scene};

#[test]
fn every_terrain_file_parses_and_lies_under_the_start() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else {
        eprintln!("TS2_GAME_DIR not set; skipping");
        return;
    };
    let exe = exe::Exe::parse(fs::read(dir.join("toy2.exe")).unwrap()).unwrap();
    let mut checked = 0;
    for nn in 1..=10 {
        for stem in ["level", "level1"] {
            let level_path = dir.join(format!("data/level{nn:02}/{stem}.ngn"));
            if !level_path.exists() {
                continue;
            }
            let path = terrain::path_for(&level_path).unwrap();
            let Ok(bytes) = fs::read(&path) else { continue };
            let t = terrain::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let tris = t.static_triangles();
            assert!(!tris.is_empty(), "{}: no collision", path.display());
            let level = exe::level_number(&level_path).unwrap();
            let Ok(start) = exe.player_start(level) else { continue };
            let [x, y, z] = start.pos;
            let world = game::World::from_terrain(&t);
            let render = game::World::from_batches(&scene::level_batches(&ngn::parse(&fs::read(&level_path).unwrap()).unwrap()));
            let (cf, rf) = (world.floor_below(x, z, y - 2000.0), render.floor_below(x, z, y - 2000.0));
            eprintln!("level {level:2}: {} objects, {} triangles; floor under start: collision {cf:?}, render {rf:?}", t.objects.len(), tris.len());
            let cf = cf.unwrap_or_else(|| panic!("level {level}: no collision floor under the start"));
            if let Some(rf) = rf {
                assert!((cf - rf).abs() < 64.0, "level {level}: collision floor {cf} vs render floor {rf}");
            }
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} levels checked");
}

/// Level 1's instance 38 is a moving piece stored unrotated (rot = 270° about X); rotated, its
/// mesh fills exactly the box of the collision object that moves with it (type 8, link 2).
#[test]
fn rotated_instance_matches_its_collision_object() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let level = dir.join("data/level01");
    let n = ngn::parse(&fs::read(level.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&fs::read(level.join("TERRAIN.ALL")).unwrap()).unwrap();
    let inst = &n.layers[0].instances[38];
    let mesh = &n.layers[0].meshes[inst.mesh as usize];
    let m = scene::instance_matrix(inst);
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &mesh.vertices {
        let p = [0, 1, 2].map(|j| v.pos[0] * m[0][j] + v.pos[1] * m[1][j] + v.pos[2] * m[2][j] + inst.pos[j]);
        for j in 0..3 {
            lo[j] = lo[j].min(p[j]);
            hi[j] = hi[j].max(p[j]);
        }
    }
    let obj = t.objects.iter().find(|o| o.kind == 8 && o.linked == 2).unwrap();
    let (mut clo, mut chi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for tri in t.shapes[obj.shape].iter().flat_map(|p| p.triangles(obj.pos.map(|v| v as f32))) {
        for p in tri {
            for j in 0..3 {
                clo[j] = clo[j].min(p[j]);
                chi[j] = chi[j].max(p[j]);
            }
        }
    }
    for j in 0..3 {
        assert!((lo[j] - clo[j]).abs() < 4.0 && (hi[j] - chi[j]).abs() < 4.0, "axis {j}: mesh {lo:?}-{hi:?} vs collision {clo:?}-{chi:?}");
    }
}
