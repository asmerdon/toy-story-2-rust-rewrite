//! Diagnostic: types a level's conversation texts through the speech box and reports how many
//! character quads show after a while: `dialog` (TS2_GAME_DIR set).
fn main() {
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").unwrap());
    let e = ts2::formats::exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    for va in [0x4F0ADCu32, 0x4F0B60, 0x4F0C10, 0x4F036C, 0x4F03E0, 0x502834, 0x5027E0] {
        let mut s = Vec::new();
        while let Ok(b) = e.at(va + s.len() as u32, 1) {
            if b[0] == 0 || s.len() > 400 {
                break;
            }
            s.push(b[0]);
        }
        let text = String::from_utf8_lossy(&s).into_owned();
        let mut d = ts2::dialog::Dialog::new(&text);
        let mut counts = Vec::new();
        for t in 0..400 {
            d.tick(false, false);
            if t % 50 == 0 {
                counts.push(d.quads().len());
            }
        }
        println!("{va:#x} ({} chars): quads over time {counts:?}", text.len());
    }
}
