//! Prints a level's level.dat point lists: `lists <levelNN dir>`.

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("level dir"));
    let lists = ts2::formats::leveldat::parse(&std::fs::read(dir.join("level.dat")).unwrap()).unwrap();
    for l in &lists {
        let pts: Vec<_> = l.points.iter().take(std::env::var("ALL").map_or(12, |_| 999)).collect();
        println!("list {:>3}: {} points{} {:?}", l.kind, l.points.len().max(l.quads.len()), if l.points.len() > 12 { " (first 12)" } else { "" }, pts);
    }
}
