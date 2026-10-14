//! Diagnostic: level.dat lists 60 (ledges Buzz can grab, 0x435100) and 62 (zip lines,
//! 0x4359D0): point pairs.
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("level dir"));
    let lists = ts2::formats::leveldat::parse(&std::fs::read(dir.join("level.dat")).unwrap()).unwrap();
    for kind in [60, 62] {
        match lists.iter().find(|l| l.kind == kind) {
            Some(l) => {
                println!("list {kind}: {} points", l.points.len());
                for p in l.points.chunks(2) {
                    println!("  {:?}", p);
                }
            }
            None => println!("list {kind}: none"),
        }
    }
}
