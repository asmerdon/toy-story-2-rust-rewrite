//! Diagnostic: level instances with blended (non-opaque) groups, their blend, texture, colour
//! and centre; `blended <level.ngn>`.
fn main() {
    let n = ts2::formats::ngn::parse(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    for (li, layer) in n.layers.iter().enumerate().take(1) {
        for ii in 0..layer.instances.len() {
            for b in ts2::scene::level_batches_where(&n, |x, y, _| x == li && y == ii) {
                if b.blend == ts2::scene::Blend::Opaque || b.vertices.is_empty() {
                    continue;
                }
                let k = b.vertices.len() as f32;
                let c = b.vertices.iter().fold([0.0f32; 3], |a, v| [a[0] + v.pos[0] / k, a[1] + v.pos[1] / k, a[2] + v.pos[2] / k]);
                let col = b.vertices.iter().fold([0u32; 4], |a, v| [0, 1, 2, 3].map(|j| a[j] + v.color[j] as u32)).map(|s| s / b.vertices.len() as u32);
                println!("L{li}#{ii} {:?} tex {:?} {} tris colour {col:?} at {:?}", b.blend, b.texture.map(|t| n.textures[t].name.clone()), b.vertices.len() / 3, c.map(|v| v.round()));
            }
        }
    }
}
