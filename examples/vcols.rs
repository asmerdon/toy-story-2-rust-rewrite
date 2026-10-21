//! Diagnostic: an instance's batches as drawn: blend, texture, vertex count and the distinct
//! vertex colours (byte 0 is alpha): `vcols <level.ngn> layer index`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n = ts2::formats::ngn::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let (l, i): (usize, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let (batches, _) = ts2::scene::level_parts_where(&n, |x, y, _| x == l && y == i);
    for b in &batches {
        let mut cols: Vec<[u8; 4]> = b.vertices.iter().map(|v| v.color).collect();
        cols.sort();
        cols.dedup();
        println!("{:?} texture {:?} {} vertices, colours {:?}", b.blend, b.texture.map(|t| &n.textures[t].name), b.vertices.len(), &cols[..cols.len().min(8)]);
    }
}
