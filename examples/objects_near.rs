//! Diagnostic: terrain objects whose bounds come near a box, with kind, flags and link.
//! cargo run --release --example objects_near -- <level> x0 y0 z0 x1 y1 z1 [-v]
use glam::Vec3;
use std::{env, fs, path::PathBuf};
use ts2::formats::terrain;
fn main() {
    let a: Vec<String> = env::args().collect();
    let n: usize = a[1].parse().unwrap();
    let v: Vec<f32> = a[2..8].iter().map(|s| s.parse().unwrap()).collect();
    let (lo, hi) = (Vec3::new(v[0], v[1], v[2]), Vec3::new(v[3], v[4], v[5]));
    let game = PathBuf::from(env::var("TS2_GAME_DIR").unwrap());
    let path = ts2::campaign::level(n).unwrap().path(&game);
    let t = terrain::parse(&fs::read(terrain::path_for(&path).unwrap()).unwrap()).unwrap();
    for (i, o) in t.objects.iter().enumerate() {
        let tris = t.object_triangles(i);
        let (mut l, mut h) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (p, _) in &tris {
            for q in p {
                l = l.min(Vec3::from(*q));
                h = h.max(Vec3::from(*q));
            }
        }
        if h.cmplt(lo).any() || l.cmpgt(hi).any() {
            continue;
        }
        println!("object {i}: kind {} flags {:#x} link {} pos {:?} {} tris {l:?}..{h:?}", o.kind, o.flags, o.linked, o.pos, tris.len());
        if a.iter().any(|s| s == "-v") {
            for (p, nrm) in &tris {
                let (pl, ph) = (p.iter().fold(Vec3::splat(f32::MAX), |m, q| m.min(Vec3::from(*q))), p.iter().fold(Vec3::splat(f32::MIN), |m, q| m.max(Vec3::from(*q))));
                if ph.cmplt(lo).any() || pl.cmpgt(hi).any() {
                    continue;
                }
                println!("   {:?} n {:?}", p.map(|v| v.map(|c| c as i32)), nrm.map(|c| (c * 100.0).round() / 100.0));
            }
        }
    }
}
