//! Diagnostic: a level's moving collision objects (TERRAIN.ALL type 8) with their link ids,
//! flags and bounds. Run: cargo run --release --example movers -- <levelNN dir> [TERRAIN.ALL]
use ts2::formats::terrain;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let file = std::env::args().nth(2).unwrap_or_else(|| "TERRAIN.ALL".into());
    let t = terrain::parse(&std::fs::read(dir.join(file)).unwrap()).unwrap();
    let mut kinds = std::collections::BTreeMap::<(i32, u16), usize>::new();
    for o in &t.objects {
        *kinds.entry((o.kind, o.flags)).or_default() += 1;
    }
    println!("(kind, flags) counts: {kinds:?}");
    for (i, o) in t.objects.iter().enumerate().filter(|(_, o)| o.kind == 8 || o.linked != 0 || o.flags & 0x400 != 0) {
        let tris: Vec<_> = t.shapes[o.shape].iter().flat_map(|p| p.triangles(o.pos.map(|v| v as f32))).collect();
        let lo = tris.iter().flatten().fold([f32::MAX; 3], |a, p| [0, 1, 2].map(|k| a[k].min(p[k])));
        let hi = tris.iter().flatten().fold([f32::MIN; 3], |a, p| [0, 1, 2].map(|k| a[k].max(p[k])));
        println!("obj {i:3} kind {} link {:3} flags {:#06x} pos {:?} polys {:3} box {:?}..{:?}", o.kind, o.linked, o.flags, o.pos, t.shapes[o.shape].len(), lo.map(|v| v as i32), hi.map(|v| v as i32));
        if o.flags & 0x400 != 0 {
            let ny: Vec<i16> = t.shapes[o.shape].iter().take(8).map(|p| p.normal[1]).collect();
            println!("      normals y (first 8): {ny:?}");
        }
    }
}
