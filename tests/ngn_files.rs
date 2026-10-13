//! Parses every `.ngn` in a real install. Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::ngn::{self, Prim};

fn ngn_files() -> Option<Vec<PathBuf>> {
    let dir = PathBuf::from(env::var_os("TS2_GAME_DIR")?).join("data");
    let mut out = Vec::new();
    for d in fs::read_dir(dir).unwrap() {
        let d = d.unwrap().path();
        if d.is_dir() {
            for f in fs::read_dir(&d).unwrap() {
                let f = f.unwrap().path();
                if f.extension().is_some_and(|e| e.eq_ignore_ascii_case("ngn")) {
                    out.push(f);
                }
            }
        }
    }
    Some(out)
}

#[test]
fn every_ngn_parses() {
    let Some(files) = ngn_files() else {
        eprintln!("TS2_GAME_DIR not set; skipping");
        return;
    };
    assert!(files.len() > 40, "only {} .ngn files", files.len());
    for f in files {
        let n = ngn::parse(&fs::read(&f).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert!(!n.textures.is_empty(), "{} has no textures", f.display());
        for t in &n.textures {
            let (w, h, bpp) = t.dimensions().unwrap_or_else(|| panic!("{} {} is not a BMP", f.display(), t.name));
            assert!(w > 0 && h != 0 && matches!(bpp, 8 | 24), "{} {}: {w}x{h} {bpp}bpp", f.display(), t.name);
        }
        for layer in &n.layers {
            assert_eq!(layer.instances.len(), layer.meshes.len(), "{}", f.display());
            for (i, inst) in layer.instances.iter().enumerate() {
                assert_eq!(inst.mesh as usize, i, "{}: instance {i} points at mesh {}", f.display(), inst.mesh);
            }
            for m in &layer.meshes {
                for g in &m.groups {
                    assert!(!matches!(g.prim, Prim::Other(_)), "{}: {:?}", f.display(), g.prim);
                    assert!((g.material as usize) < m.materials.len(), "{}: material {} of {}", f.display(), g.material, m.materials.len());
                }
                for mat in &m.materials {
                    if let Some(t) = mat.texture {
                        assert!((t as usize) < m.textures.len(), "{}: texture {t} of {:?}", f.display(), m.textures);
                    }
                }
                // levelNtK.ngn are reduced-texture variants of the same geometry; they reference
                // textures they don't ship, presumably substituted at load time.
                let reduced = f.file_stem().unwrap().to_string_lossy().to_ascii_lowercase().contains('t');
                for name in &m.textures {
                    assert!(reduced || n.textures.iter().any(|t| &t.name == name), "{}: mesh uses missing texture {name}", f.display());
                }
            }
        }
    }
}

#[test]
fn level01_shape() {
    let Some(_) = ngn_files() else { return };
    let dir = PathBuf::from(env::var_os("TS2_GAME_DIR").unwrap());
    let n = ngn::parse(&fs::read(dir.join("data/level01/level.ngn")).unwrap()).unwrap();
    assert_eq!(n.textures.len(), 18);
    assert_eq!(n.layers.len(), 2);
    assert_eq!(n.layers[0].meshes.len(), 828);
    assert_eq!(n.layers[1].meshes.len(), 298);
    let names: Vec<&str> = n.creatures.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["BUZZ", "ZURG1", "ZURG3", "TINMAN", "SHEEP", "BOPEEP", "RC", "MRPOT", "HAMM", "COTBIT", "REX"]);
    let buzz = &n.creatures[0];
    assert_eq!(buzz.id, 0);
    assert_eq!(buzz.nodes.len(), 23);
    assert_eq!(buzz.animations.len(), 35);
    assert!(!buzz.shared_vertices.is_empty());
}

#[test]
fn every_creature_has_one_slot_per_node() {
    let Some(files) = ngn_files() else { return };
    let mut total = 0;
    for f in files {
        let n = ngn::parse(&fs::read(&f).unwrap()).unwrap();
        for c in &n.creatures {
            assert!(!c.nodes.is_empty(), "{} {}", f.display(), c.name);
            for node in &c.nodes {
                assert_eq!(node.mesh.is_some(), node.flags & 1 != 0, "{} {} {}", f.display(), c.name, node.name);
            }
            for p in &c.patches {
                for (vi, ni) in p.vertices() {
                    assert!(vi < c.shared_vertices.len(), "{} {}: patch vertex {vi} of {}", f.display(), c.name, c.shared_vertices.len());
                    assert!(ni < c.nodes.len(), "{} {}: patch node {ni} of {}", f.display(), c.name, c.nodes.len());
                }
                assert!((p.material as usize) < c.shared_materials.len(), "{} {}: patch material {}", f.display(), c.name, p.material);
            }
            total += 1;
        }
    }
    assert_eq!(total, 146);
}

/// Chunk 0x105: special objects point at real instances.
#[test]
fn special_objects_point_at_instances() {
    let Some(dir) = std::env::var_os("TS2_GAME_DIR").map(std::path::PathBuf::from) else { return };
    let n = ts2::formats::ngn::parse(&std::fs::read(dir.join("data/level01/level.ngn")).unwrap()).unwrap();
    assert!(!n.specials.is_empty());
    for s in &n.specials {
        assert!(s.instance < n.layers[s.layer].instances.len(), "{s:?}");
    }
    eprintln!("{} specials, e.g. {:?}", n.specials.len(), &n.specials[..n.specials.len().min(8)]);
}
