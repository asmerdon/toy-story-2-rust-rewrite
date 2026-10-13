//! Decodes every creature animation in a real install. Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::formats::{anim, ngn};

fn level_files() -> Option<Vec<PathBuf>> {
    let dir = PathBuf::from(env::var_os("TS2_GAME_DIR")?).join("data");
    let mut out = Vec::new();
    for d in fs::read_dir(dir).unwrap() {
        let d = d.unwrap().path();
        if d.is_dir() && d.file_name().unwrap().to_string_lossy().starts_with("level") {
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
fn every_animation_decodes() {
    let Some(files) = level_files() else {
        eprintln!("TS2_GAME_DIR not set; skipping");
        return;
    };
    let (mut anims, mut poses) = (0usize, 0usize);
    for f in files {
        let n = ngn::parse(&fs::read(&f).unwrap()).unwrap();
        for c in &n.creatures {
            for (i, raw) in c.animations.iter().enumerate() {
                let a = anim::parse(raw).unwrap_or_else(|e| panic!("{} {} anim {i}: {e}", f.display(), c.name));
                assert!(a.frame_count >= 1, "{} {} anim {i}: no frames", f.display(), c.name);
                assert!(a.node_offsets.len() <= c.nodes.len() + 1, "{} {} anim {i}: {} table entries for {} nodes", f.display(), c.name, a.node_offsets.len(), c.nodes.len());
                for node in 0..a.node_offsets.len() {
                    for frame in 0..a.frame_count as usize {
                        if let Some(p) = a.pose(node, frame) {
                            assert!(p.translation.iter().all(|t| t.abs() < 8192.0), "{} {} anim {i}: {p:?}", f.display(), c.name);
                            assert!(p.scale.iter().all(|s| *s > -8.0 && *s < 8.0), "{} {} anim {i}: {p:?}", f.display(), c.name);
                            poses += 1;
                        } else {
                            assert!(a.node_offsets[node].is_none(), "{} {} anim {i}: record for node {node} out of range", f.display(), c.name);
                        }
                    }
                }
                anims += 1;
            }
        }
    }
    eprintln!("{anims} animations, {poses} node poses");
    assert!(anims > 500);
}
