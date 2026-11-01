//! Diagnostic: every collision object of one surface type (flags low byte), with its bounds and
//! the slope of its faces: `surface_kind <levelNN dir> type [TERRAIN.ALL]`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let kind = u8::from_str_radix(a[2].trim_start_matches("0x"), 16).unwrap();
    let file = a.get(3).cloned().unwrap_or_else(|| "TERRAIN.ALL".into());
    let t = ts2::formats::terrain::parse(&std::fs::read(dir.join(file)).unwrap()).unwrap();
    for (i, o) in t
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| o.flags & 0x400 == 0 && (o.flags & 0xFF) as u8 == kind)
    {
        let tris = t.object_triangles(i);
        let lo = tris
            .iter()
            .flat_map(|(p, _)| p.iter())
            .fold([f32::MAX; 3], |m, v| [0, 1, 2].map(|k| m[k].min(v[k])));
        let hi = tris
            .iter()
            .flat_map(|(p, _)| p.iter())
            .fold([f32::MIN; 3], |m, v| [0, 1, 2].map(|k| m[k].max(v[k])));
        let ups: Vec<[f32; 3]> = tris
            .iter()
            .map(|(_, n)| *n)
            .filter(|n| n[1] < -0.5)
            .collect();
        println!(
            "object {i}: kind {} {} faces, {} up, box {:?}..{:?}, up normals {:?}",
            o.kind,
            tris.len(),
            ups.len(),
            lo.map(|v| v as i32),
            hi.map(|v| v as i32),
            ups.iter()
                .take(4)
                .map(|n| n.map(|c| (c * 100.0).round() / 100.0))
                .collect::<Vec<_>>()
        );
    }
}
