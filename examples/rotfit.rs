//! Diagnostic: for each rotated .ngn instance, how well do its vertices sit on the collision
//! surfaces with and without applying `rot`? (Not part of the build; run with --example.)
use glam::{Mat3, Vec3};
use std::collections::HashMap;
use ts2::formats::{ngn, terrain};

fn closest(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 { return a; }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 { return b; }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 { return a + ab * (d1 / (d1 - d3)); }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 { return c; }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 { return a + ac * (d2 / (d2 - d6)); }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 { return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6))); }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let n = ngn::parse(&std::fs::read(dir.join("level.ngn")).unwrap()).unwrap();
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let tris: Vec<[Vec3; 3]> = t.objects.iter().filter(|o| matches!(o.kind, 6..=8))
        .flat_map(|o| t.shapes[o.shape].iter().flat_map(move |p| p.triangles(o.pos.map(|v| v as f32))))
        .map(|tr| tr.map(Vec3::from)).collect();
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, tr) in tris.iter().enumerate() {
        let lo = tr[0].min(tr[1]).min(tr[2]);
        let hi = tr[0].max(tr[1]).max(tr[2]);
        for x in (lo.x / 512.0).floor() as i32..=(hi.x / 512.0).floor() as i32 {
            for z in (lo.z / 512.0).floor() as i32..=(hi.z / 512.0).floor() as i32 {
                grid.entry((x, z)).or_default().push(i);
            }
        }
    }
    let dist = |p: Vec3| -> f32 {
        let k = ((p.x / 512.0).floor() as i32, (p.z / 512.0).floor() as i32);
        grid.get(&k).map_or(9999.0, |v| v.iter().map(|&i| (closest(p, tris[i][0], tris[i][1], tris[i][2]) - p).length()).fold(9999.0, f32::min))
    };
    let ang = |r: u32| (r & 0xFFFF) as f32 / 65536.0 * std::f32::consts::TAU;
    // Name = order applied to the vector (first letter first).
    let orders: [(&str, fn(f32, f32, f32) -> Mat3); 7] = [
        ("none", |_, _, _| Mat3::IDENTITY),
        ("XYZ", |x, y, z| Mat3::from_rotation_z(z) * Mat3::from_rotation_y(y) * Mat3::from_rotation_x(x)),
        ("ZYX", |x, y, z| Mat3::from_rotation_x(x) * Mat3::from_rotation_y(y) * Mat3::from_rotation_z(z)),
        ("YXZ", |x, y, z| Mat3::from_rotation_z(z) * Mat3::from_rotation_x(x) * Mat3::from_rotation_y(y)),
        ("XZY", |x, y, z| Mat3::from_rotation_y(y) * Mat3::from_rotation_z(z) * Mat3::from_rotation_x(x)),
        ("YZX", |x, y, z| Mat3::from_rotation_x(x) * Mat3::from_rotation_z(z) * Mat3::from_rotation_y(y)),
        ("ZXY", |x, y, z| Mat3::from_rotation_y(y) * Mat3::from_rotation_x(x) * Mat3::from_rotation_z(z)),
    ];
    let mut wins = vec![0usize; orders.len()];
    let mut rows = Vec::new();
    for (li, l) in n.layers.iter().enumerate() {
        for (ii, inst) in l.instances.iter().enumerate() {
            let scaled = inst.scale.iter().any(|&v| (v - 1.0).abs() > 0.01);
            if std::env::var("SCALE").is_ok() {
                if !scaled { continue; }
            } else if inst.rot.iter().all(|&r| r & 0xFFFF == 0) { continue; }
            let m = &l.meshes[inst.mesh as usize];
            let (rx, ry, rz) = (ang(inst.rot[0]), ang(inst.rot[1]), ang(inst.rot[2]));
            let pos = Vec3::from(inst.pos);
            let step = (m.vertices.len() / 40).max(1);
            let sc = Mat3::from_diagonal(Vec3::from(inst.scale));
            let scores: Vec<f32> = orders.iter().enumerate().map(|(k, (_, f))| {
                // With SCALE set: order 0 = ZYX unscaled, order 1 = ZYX scaled.
                let r = if std::env::var("SCALE").is_ok() {
                    let zyx = orders[2].1(rx, ry, rz);
                    if k == 0 { zyx } else { zyx * sc }
                } else { f(rx, ry, rz) };
                let ds: Vec<f32> = m.vertices.iter().step_by(step).map(|v| dist(pos + r * Vec3::from(v.pos))).collect();
                ds.iter().filter(|&&d| d < 12.0).count() as f32 / ds.len() as f32
            }).collect();
            // Count an order only when it beats every other order clearly.
            for k in 0..scores.len() {
                if (0..scores.len()).all(|j| j == k || scores[k] > scores[j] + 0.1) {
                    wins[k] += 1;
                }
            }
            rows.push((li, ii, inst.flags, inst.rot, m.vertices.len(), scores));
        }
    }
    for (name, w) in orders.iter().map(|o| o.0).zip(&wins) { println!("{name}: {w}"); }
    let mut table: std::collections::BTreeMap<u32, (usize, usize, usize)> = Default::default();
    for r in &rows {
        let rot = r.5[1..].iter().cloned().fold(0.0f32, f32::max);
        let none = r.5[0];
        let e = table.entry(r.2 & 0xFFFF).or_default();
        if rot > none + 0.2 { e.0 += 1 } else if none > rot + 0.2 { e.1 += 1 } else { e.2 += 1 }
    }
    println!("flags-low: (rotation helps, rotation hurts, unclear)");
    for (k, v) in table { println!("  {k:#x}: {v:?}"); }
    let mut table: std::collections::BTreeMap<u32, (usize, usize, usize)> = Default::default();
    for r in &rows {
        let rot = r.5[1..].iter().cloned().fold(0.0f32, f32::max);
        let none = r.5[0];
        let e = table.entry(r.2 >> 16).or_default();
        if rot > none + 0.2 { e.0 += 1 } else if none > rot + 0.2 { e.1 += 1 } else { e.2 += 1 }
    }
    println!("flags-high:");
    for (k, v) in table { println!("  {k:#x}: {v:?}"); }
    for r in rows.iter().filter(|r| r.5[0] > r.5[1..].iter().cloned().fold(0.0f32, f32::max) + 0.2) {
        println!("hurts: layer {} inst {} flags {:#x} rot {:x?} verts {} scores {:?}", r.0, r.1, r.2, r.3, r.4, r.5.iter().map(|s| (s * 100.0) as i32).collect::<Vec<_>>());
    }
}
