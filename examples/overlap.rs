//! Diagnostic: z-fighting candidates in a level. Finds pairs of triangles from different
//! groups that lie in the same plane (within 1 unit) and overlap, and reports where they are
//! and what they are made of. Run: cargo run --release --example overlap -- <level.ngn>
use glam::Vec3;
use std::collections::HashMap;
use ts2::formats::ngn;
use ts2::scene;

struct Tri {
    p: [Vec3; 3],
    n: Vec3,
    d: f32,
    lo: Vec3,
    hi: Vec3,
    /// (batch texture, blend) and the instance it came from
    tag: (Option<usize>, scene::Blend, usize, usize),
}

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let n = ngn::parse(&std::fs::read(&path).unwrap()).unwrap();
    let mut tris = Vec::new();
    for (li, layer) in n.layers.iter().enumerate() {
        for ii in 0..layer.instances.len() {
            let b = scene::level_batches_where(&n, |a, b, i| a == li && b == ii && i.flags & 0xFFFF != 0);
            for batch in &b {
                for t in batch.vertices.chunks_exact(3) {
                    let p = [Vec3::from(t[0].pos), Vec3::from(t[1].pos), Vec3::from(t[2].pos)];
                    let c = (p[1] - p[0]).cross(p[2] - p[0]);
                    if c.length() < 1.0 {
                        continue;
                    }
                    let nrm = c.normalize();
                    tris.push(Tri { p, n: nrm, d: nrm.dot(p[0]), lo: p[0].min(p[1]).min(p[2]), hi: p[0].max(p[1]).max(p[2]), tag: (batch.texture, batch.blend, li, ii) });
                }
            }
        }
    }
    let cell = 256.0;
    let mut grid: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        let c = ((t.p[0] + t.p[1] + t.p[2]) / 3.0 / cell).floor();
        grid.entry((c.x as i32, c.y as i32, c.z as i32)).or_default().push(i);
    }
    let inside = |t: &Tri, q: Vec3| {
        let s = |a: Vec3, b: Vec3| (b - a).cross(q - a).dot(t.n);
        let (a, b, c) = (s(t.p[0], t.p[1]), s(t.p[1], t.p[2]), s(t.p[2], t.p[0]));
        (a > 0.5 && b > 0.5 && c > 0.5) || (a < -0.5 && b < -0.5 && c < -0.5)
    };
    let mut kinds: HashMap<String, usize> = HashMap::new();
    let mut spots: HashMap<(i32, i32), usize> = HashMap::new();
    let mut zones: HashMap<(u32, u32), usize> = HashMap::new();
    let mut same_inst = 0;
    let mut opposite = 0;
    let mut inst_pairs: HashMap<((usize, usize), (usize, usize)), usize> = HashMap::new();
    let mut count = 0;
    for (&(x, y, z), list) in &grid {
        let mut near = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(l) = grid.get(&(x + dx, y + dy, z + dz)) {
                        near.extend_from_slice(l);
                    }
                }
            }
        }
        for &i in list {
            let a = &tris[i];
            for &j in &near {
                if j <= i {
                    continue;
                }
                let b = &tris[j];
                if a.n.dot(b.n).abs() < 0.999 || (a.d - b.d * a.n.dot(b.n).signum()).abs() > 1.0 {
                    continue;
                }
                if a.lo.cmpgt(b.hi + 1.0).any() || b.lo.cmpgt(a.hi + 1.0).any() {
                    continue;
                }
                let cb = (b.p[0] + b.p[1] + b.p[2]) / 3.0;
                let ca = (a.p[0] + a.p[1] + a.p[2]) / 3.0;
                if !(inside(a, cb) || inside(b, ca)) {
                    continue;
                }
                count += 1;
                if a.n.dot(b.n) < 0.0 {
                    opposite += 1;
                }
                if (a.tag.2, a.tag.3) == (b.tag.2, b.tag.3) {
                    same_inst += 1;
                }
                let tn = |t: Option<usize>| t.map_or("none".to_string(), |t| n.textures[t].name.clone());
                *kinds.entry(format!("{} {:?} / {} {:?}", tn(a.tag.0), a.tag.1, tn(b.tag.0), b.tag.1)).or_default() += 1;
                *spots.entry(((ca.x / 1000.0) as i32, (ca.z / 1000.0) as i32)).or_default() += 1;
                let f = |t: &Tri| n.layers[t.tag.2].instances[t.tag.3].flags;
                let (fa, fb) = (f(a).min(f(b)), f(a).max(f(b)));
                *zones.entry((fa, fb)).or_default() += 1;
                let (ka, kb) = ((a.tag.2, a.tag.3), (b.tag.2, b.tag.3));
                *inst_pairs.entry((ka.min(kb), ka.max(kb))).or_default() += 1;
            }
        }
    }
    println!("{} triangles, {count} coplanar overlapping pairs ({same_inst} within one instance, {opposite} facing opposite ways)", tris.len());
    let mut k: Vec<_> = kinds.into_iter().collect();
    k.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (s, c) in k.iter().take(15) {
        println!("{c:6}  {s}");
    }
    let mut z: Vec<_> = zones.into_iter().collect();
    z.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    let same_zone: usize = z.iter().filter(|((a, b), _)| a & 0xFFFF == b & 0xFFFF).map(|(_, c)| c).sum();
    println!("same zone (flags & 0xFFFF): {same_zone}");
    for ((a, b), c) in z.iter().take(15) {
        println!("{c:6}  flags {a:#x} / {b:#x}");
    }
    let mut ip: Vec<_> = inst_pairs.into_iter().collect();
    ip.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    let tri_count = |l: usize, i: usize| tris.iter().filter(|t| (t.tag.2, t.tag.3) == (l, i)).count();
    for (((l0, i0), (l1, i1)), c) in ip.iter().take(12) {
        let (a, b) = (&n.layers[*l0].instances[*i0], &n.layers[*l1].instances[*i1]);
        println!(
            "{c:5}  L{l0}#{i0} ({} tris, {:?}, rot {:x?}, scale {:?})  L{l1}#{i1} ({} tris, {:?}, rot {:x?}, scale {:?})",
            tri_count(*l0, *i0), a.pos, a.rot, a.scale, tri_count(*l1, *i1), b.pos, b.rot, b.scale
        );
    }
    // Pairs within the detailed set (both layer 0): these can fight in one depth pass.
    for (((l0, i0), (l1, i1)), c) in ip.iter().filter(|(((a, _), (b, _)), _)| *a == 0 && *b == 0).take(20) {
        let (a, b) = (&n.layers[*l0].instances[*i0], &n.layers[*l1].instances[*i1]);
        println!("near-set pair {c:4}  L0#{i0} zone {:#x} {:?}  L0#{i1} zone {:#x} {:?}", a.flags, a.pos, b.flags, b.pos);
    }
    let mut s: Vec<_> = spots.into_iter().collect();
    s.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for ((x, z), c) in s.iter().take(10) {
        println!("{c:6}  near x {}k z {}k", x, z);
    }
}
