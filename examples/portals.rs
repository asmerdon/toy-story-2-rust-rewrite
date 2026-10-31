//! Diagnostic: a level's room portals (ngn chunks 0x102 polygons, 0x103 links, read by
//! 0x4C3BE0 / 0x4C3DF0) and its instances per room (instance flags & 0xFF, 0x4C3740).
//! Run: cargo run --release --example portals -- <levelNN dir>
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("level dir"));
    let bytes = std::fs::read(dir.join("level.ngn")).unwrap();
    let n = ts2::formats::ngn::parse(&bytes).unwrap();
    let p = ts2::scene::Portals::parse(&bytes);
    for (i, l) in p.links.iter().enumerate() {
        let poly = &p.polys[l.poly];
        let c = poly.iter().fold([0.0f32; 3], |a, v| [a[0] + v[0], a[1] + v[1], a[2] + v[2]]).map(|s| s / poly.len() as f32);
        println!("portal {i}: poly {} ({} pts, centre {:?}) room {} -> {}  {:?}", l.poly, poly.len(), c.map(|v| v.round()), l.from, l.to, poly);
    }
    for (li, layer) in n.layers.iter().enumerate() {
        let mut rooms = std::collections::BTreeMap::<u32, (usize, std::collections::BTreeSet<u32>)>::new();
        for i in &layer.instances {
            let e = rooms.entry(i.flags & 0xFF).or_default();
            e.0 += 1;
            e.1.insert(i.flags >> 16 & 0xF);
        }
        println!("layer {li}: {:?}", rooms);
    }
}
