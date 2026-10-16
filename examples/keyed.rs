//! Diagnostic: level groups whose texture area is mostly the colour key (transparent):
//! `keyed <level.ngn>`.
fn main() {
    let n = ts2::formats::ngn::parse(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let imgs: Vec<_> = n.textures.iter().map(|t| ts2::bmp::decode(&t.bmp, Some(ts2::bmp::PC_KEY))).collect();
    for (li, layer) in n.layers.iter().enumerate().take(1) {
        for ii in 0..layer.instances.len() {
            for b in ts2::scene::level_batches_where(&n, |x, y, _| x == li && y == ii) {
                let Some(t) = b.texture else { continue };
                let Some(img) = &imgs[t] else { continue };
                for tri in b.vertices.chunks_exact(3) {
                    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                    for v in tri {
                        for k in 0..2 {
                            lo[k] = lo[k].min(v.uv[k]);
                            hi[k] = hi[k].max(v.uv[k]);
                        }
                    }
                    let (mut keyed, mut all) = (0, 0);
                    for y in (lo[1] * img.height as f32) as u32..=((hi[1] * img.height as f32) as u32).min(img.height - 1) {
                        for x in (lo[0] * img.width as f32) as u32..=((hi[0] * img.width as f32) as u32).min(img.width - 1) {
                            all += 1;
                            if img.rgba[((y * img.width + x) * 4 + 3) as usize] == 0 {
                                keyed += 1;
                            }
                        }
                    }
                    if all > 0 && keyed * 10 >= all * 9 {
                        let c = tri.iter().fold([0.0f32; 3], |a, v| [0, 1, 2].map(|k| a[k] + v.pos[k] / 3.0));
                        println!("L{li}#{ii} {:?} {} keyed {keyed}/{all} at {:?}", b.blend, n.textures[t].name, c.map(|v| v.round()));
                    }
                }
            }
        }
    }
}
