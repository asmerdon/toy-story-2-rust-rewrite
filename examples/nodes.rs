//! Diagnostic: a creature's nodes with their rest-pose bounds (model units, +Y down).
//! cargo run --release --example nodes -- <level.ngn relative to data/> <NAME>
use ts2::formats::ngn;

fn main() {
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").expect("TS2_GAME_DIR"));
    let a: Vec<String> = std::env::args().collect();
    let g = ngn::parse(&std::fs::read(game.join("data").join(&a[1])).unwrap()).unwrap();
    let c = g.creatures.iter().find(|c| c.name.eq_ignore_ascii_case(&a[2])).expect("creature");
    for (i, n) in c.nodes.iter().enumerate() {
        let Some(m) = &n.mesh else { println!("{i:2} {} (no mesh)", n.name); continue };
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in &m.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v.pos[k]);
                hi[k] = hi[k].max(v.pos[k]);
            }
        }
        println!("{i:2} {} lo {:?} hi {:?}", n.name, lo.map(|x| x.round()), hi.map(|x| x.round()));
    }
}
