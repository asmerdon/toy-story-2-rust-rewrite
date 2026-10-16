//! Diagnostic: a level's instances by zone (placement flags & 0xFFFF), with bounds and
//! textures; `zones <level.ngn> [zone]`.
use ts2::formats::ngn;
use ts2::scene;
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let only: Option<u32> = std::env::args().nth(2).and_then(|a| a.parse().ok());
    let n = ngn::parse(&std::fs::read(&path).unwrap()).unwrap();
    for (li, layer) in n.layers.iter().enumerate() {
        for (ii, inst) in layer.instances.iter().enumerate() {
            let zone = inst.flags & 0xFFFF;
            if only.is_some_and(|z| z != zone) {
                continue;
            }
            let b = scene::level_batches_where(&n, |a, b, _| a == li && b == ii);
            let (mut lo, mut hi, mut tris) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN), 0);
            let mut tex = std::collections::BTreeSet::new();
            for batch in &b {
                tris += batch.vertices.len() / 3;
                if let Some(t) = batch.texture {
                    tex.insert(n.textures[t].name.clone());
                }
                for v in &batch.vertices {
                    lo = lo.min(glam::Vec3::from(v.pos));
                    hi = hi.max(glam::Vec3::from(v.pos));
                }
            }
            if tris == 0 {
                continue;
            }
            println!("L{li}#{ii} zone {zone} flags {:#x}: {tris} tris, {:?}..{:?}, {:?}", inst.flags, lo.round(), hi.round(), tex);
        }
    }
}
