//! Diagnostic: an instance's blended groups: UV range and the average texel colour under it.
//! `texsample <level.ngn> layer index`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n = ts2::formats::ngn::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let (l, i): (usize, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    for b in ts2::scene::level_batches_where(&n, |x, y, _| x == l && y == i) {
        let Some(t) = b.texture else { continue };
        let img = ts2::bmp::decode(&n.textures[t].bmp, Some(ts2::bmp::PC_KEY)).unwrap();
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in &b.vertices {
            for k in 0..2 {
                lo[k] = lo[k].min(v.uv[k]);
                hi[k] = hi[k].max(v.uv[k]);
            }
        }
        let (mut sum, mut cnt) = ([0u64; 4], 0u64);
        for y in (lo[1] * img.height as f32) as u32..((hi[1] * img.height as f32) as u32).min(img.height) {
            for x in (lo[0] * img.width as f32) as u32..((hi[0] * img.width as f32) as u32).min(img.width) {
                let p = ((y * img.width + x) * 4) as usize;
                for k in 0..4 {
                    sum[k] += img.rgba[p + k] as u64;
                }
                cnt += 1;
            }
        }
        println!("{:?} {}: uv {lo:?}..{hi:?}, avg texel {:?}", b.blend, n.textures[t].name, sum.map(|s| s / cnt.max(1)));
    }
}
