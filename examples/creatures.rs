//! Diagnostic: a level's creature spawns with their parameters, and the behaviour scripts
//! (exe table 0x4E02C4) they use. Run with TS2_GAME_DIR set:
//! cargo run --release --example creatures -- [levelNN] [level|level1]
use ts2::formats::{exe, packets};

fn main() {
    let game = std::path::PathBuf::from(std::env::var("TS2_GAME_DIR").unwrap());
    let lvl = std::env::args().nth(1).unwrap_or_else(|| "level01".into());
    let stem = std::env::args().nth(2).unwrap_or_else(|| "level".into());
    let e = exe::Exe::parse(std::fs::read(game.join("toy2.exe")).unwrap()).unwrap();
    let pk = packets::parse(&std::fs::read(game.join(format!("data/{lvl}/{stem}.raw"))).unwrap()).unwrap();
    let mut behaviours = std::collections::BTreeSet::new();
    for (i, s) in pk.iter().flat_map(packets::spawns).enumerate() {
        let p = &s.params;
        let w = |k: usize| i16::from_le_bytes([p[k], p[k + 1]]);
        println!(
            "slot {i:2} id {:2} at {:?} beh {:3} turn {:3} face {:3} hp {:3} respawn {:3} flags {:#06x} zone {}x{} ang {} b1a {} b1b {} acc {} side {} b1e {} speed {}",
            s.creature, s.pos, p[0], p[1], p[2], p[3], p[4], w(5) as u16, w(7), w(9), w(11), p[13], p[14], p[15], p[16], p[17], p[18]
        );
        behaviours.insert(p[0]);
    }
    for b in behaviours {
        let ptr = u32::from_le_bytes(e.at(0x4E02C4 + 4 * b as u32, 4).unwrap().try_into().unwrap());
        let words: Vec<i16> = (0..48).map(|k| i16::from_le_bytes(e.at(ptr + 2 * k, 2).unwrap().try_into().unwrap())).collect();
        println!("behaviour {b} @ {ptr:#x}: {words:?}");
    }
}
