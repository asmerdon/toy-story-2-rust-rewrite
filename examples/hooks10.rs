//! Diagnostic: a level's hook points (`level.dat` list 59, level units), the floor 0x447DB0
//! drops each onto, and the nearest other point: `hooks10 <levelNN dir>`.
use ts2::formats::{leveldat, terrain};
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let t = terrain::parse(&std::fs::read(dir.join("TERRAIN.ALL")).unwrap()).unwrap();
    let lists = leveldat::parse(&std::fs::read(dir.join("level.dat")).unwrap()).unwrap();
    let world = ts2::game::World::from_terrain(&t);
    let Some(l) = lists.iter().find(|l| l.kind == 59) else { return println!("no list 59") };
    for (i, h) in l.points.iter().enumerate() {
        let f = world.floor_below(h[0] as f32, h[2] as f32, (h[1] - 10) as f32);
        let near = l.points.iter().enumerate().filter(|&(j, _)| j != i).map(|(j, o)| {
            let d = [0, 1, 2].map(|k| (o[k] - h[k]) as f64);
            (j, (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() as i32)
        }).min_by_key(|x| x.1);
        println!("{i:2} {h:?} floor {f:?} nearest {near:?}");
    }
}
