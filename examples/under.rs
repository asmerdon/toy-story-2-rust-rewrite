//! Diagnostic: level instances whose bounds lie in a box: `under <level.ngn> x0 y0 z0 x1 y1 z1`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n = ts2::formats::ngn::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let f = |i: usize| a[i].parse::<f32>().unwrap();
    let (lo, hi) = (glam::Vec3::new(f(2), f(3), f(4)), glam::Vec3::new(f(5), f(6), f(7)));
    let specials: std::collections::HashMap<(usize, usize), u16> = n.specials.iter().map(|s| ((s.layer, s.instance), s.id)).collect();
    for (li, layer) in n.layers.iter().enumerate().take(1) {
        for (ii, inst) in layer.instances.iter().enumerate() {
            let b = ts2::scene::level_batches_where(&n, |x, y, _| x == li && y == ii);
            let (mut l, mut h) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
            let mut tex = std::collections::BTreeSet::new();
            for batch in &b {
                if let Some(t) = batch.texture {
                    tex.insert(n.textures[t].name.clone());
                }
                for v in &batch.vertices {
                    l = l.min(glam::Vec3::from(v.pos));
                    h = h.max(glam::Vec3::from(v.pos));
                }
            }
            if l.cmpge(lo).all() && h.cmple(hi).all() {
                println!("L{li}#{ii} special {:?} flags {:#x} pos {:?} bounds {:?}..{:?} {:?}", specials.get(&(li, ii)), inst.flags, inst.pos, l.round(), h.round(), tex);
            }
        }
    }
}
