//! Diagnostic: special objects that name the same instance as another (per level).
use std::collections::HashMap;
fn main() {
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").unwrap());
    for lv in 1..=10 {
        let Ok(b) = std::fs::read(game.join(format!("data/level{lv:02}/level.ngn"))) else { continue };
        let n = ts2::formats::ngn::parse(&b).unwrap();
        let mut by: HashMap<(usize, usize), Vec<u16>> = HashMap::new();
        for s in &n.specials {
            by.entry((s.layer, s.instance)).or_default().push(s.id);
        }
        let mut shared: Vec<_> = by.into_iter().filter(|(_, v)| v.len() > 1).collect();
        shared.sort();
        println!("level {lv}: {shared:?}");
    }
}
