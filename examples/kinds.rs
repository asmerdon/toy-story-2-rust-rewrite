//! Diagnostic: a level's pickups by kind. cargo run --example kinds -- <levelNN dir> <level number>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(&a[1]);
    let lv: usize = a[2].parse().unwrap();
    let game = dir.parent().unwrap().parent().unwrap();
    let e = ts2::formats::exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let dat = std::fs::read(dir.join("level.dat")).unwrap();
    let p = ts2::pickups::Pickups::new(lv, &dat, Some(&e)).unwrap();
    let mut by: std::collections::BTreeMap<String, Vec<[i32; 3]>> = Default::default();
    for i in &p.items {
        by.entry(format!("{:?}", i.kind)).or_default().push(i.pos);
    }
    for (k, v) in by {
        println!("{k}: {} {:?}", v.len(), if k == "Coin" { &v[..0] } else { &v[..] });
    }
}
