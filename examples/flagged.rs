//! Diagnostic: instances by upper flag bits, with their average vertex colour.
use std::collections::BTreeMap;
fn main() {
    let n = ts2::formats::ngn::parse(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let mut by: BTreeMap<(usize, u32), (usize, usize, [u64; 4], usize)> = BTreeMap::new();
    for (li, layer) in n.layers.iter().enumerate() {
        for (ii, inst) in layer.instances.iter().enumerate() {
            let b = ts2::scene::level_batches_where(&n, |x, y, _| x == li && y == ii);
            let e = by.entry((li, inst.flags >> 16)).or_insert((0, 0, [0; 4], 0));
            e.0 += 1;
            for batch in &b {
                for v in &batch.vertices {
                    e.1 += 1;
                    for k in 0..4 {
                        e.2[k] += v.color[k] as u64;
                    }
                    if v.color[1] < 8 && v.color[2] < 8 && v.color[3] < 8 {
                        e.3 += 1;
                    }
                }
            }
        }
    }
    for ((l, f), (cnt, verts, col, black)) in by {
        let avg = col.map(|c| c / verts.max(1) as u64);
        println!("layer {l} flags {f:#x}xxxx: {cnt} instances, {verts} verts, colour {avg:?}, {black} near-black verts");
    }
}
