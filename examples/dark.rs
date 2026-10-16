//! Diagnostic: level triangles near a point, by instance and texture, with their vertex
//! colours: `dark <level.ngn> x y z radius`.
use std::collections::BTreeMap;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n = ts2::formats::ngn::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let c = glam::Vec3::new(a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap());
    let r: f32 = a[5].parse().unwrap();
    for (li, layer) in n.layers.iter().enumerate() {
        for ii in 0..layer.instances.len() {
            let b = ts2::scene::level_batches_where(&n, |x, y, _| x == li && y == ii);
            let mut rows: BTreeMap<String, (usize, [u32; 4], f32)> = BTreeMap::new();
            for batch in &b {
                for t in batch.vertices.chunks_exact(3) {
                    let m = (glam::Vec3::from(t[0].pos) + glam::Vec3::from(t[1].pos) + glam::Vec3::from(t[2].pos)) / 3.0;
                    if m.distance(c) > r {
                        continue;
                    }
                    let name = format!("{} {:?}", batch.texture.map_or("none".into(), |k| n.textures[k].name.clone()), batch.blend);
                    let e = rows.entry(name).or_insert((0, [0; 4], f32::MAX));
                    e.0 += 1;
                    for v in t {
                        for k in 0..4 {
                            e.1[k] += v.color[k] as u32;
                        }
                    }
                    e.2 = e.2.min(m.distance(c));
                }
            }
            for (k, (cnt, col, d)) in rows {
                let avg = col.map(|v| v / (cnt as u32 * 3));
                println!("L{li}#{ii} flags {:#x}: {k}: {cnt} tris, colour {avg:?}, nearest {d:.0}", layer.instances[ii].flags);
            }
        }
    }
}
