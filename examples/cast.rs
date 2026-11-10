//! Diagnostic: which creatures each level's `.ngn` carries, with node and animation counts.
//! cargo run --release --example cast   (TS2_GAME_DIR set)
use ts2::formats::ngn;

fn main() {
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").expect("TS2_GAME_DIR"));
    for n in 0..20 {
        for stem in ["level", "level1"] {
            let p = game.join(format!("data/level{n:02}/{stem}.ngn"));
            let Ok(b) = std::fs::read(&p) else { continue };
            match ngn::parse(&b) {
                Ok(g) => {
                    let list: Vec<String> = g.creatures.iter().map(|c| format!("{}:{}({}n,{}a)", c.id, c.name, c.nodes.len(), c.animations.len())).collect();
                    println!("level{n:02}/{stem}: {}", list.join(" "));
                }
                Err(e) => println!("level{n:02}/{stem}: error {e:?}"),
            }
        }
    }
}
