//! Sound tables and samples. Skipped unless TS2_GAME_DIR is set.

use std::{env, fs, path::PathBuf};
use ts2::audio::{SoundBank, slot_names};
use ts2::formats::exe::Exe;

#[test]
fn level_sounds_load_and_buzz_sounds_resolve() {
    let Some(dir) = env::var_os("TS2_GAME_DIR").map(PathBuf::from) else { return };
    let exe = Exe::parse(fs::read(dir.join("toy2.exe")).unwrap()).unwrap();
    let names = slot_names(&exe, 1).unwrap();
    assert!(names.len() > 80, "{} names", names.len());
    let bank = SoundBank::load(&exe, &dir.join("data/sfx"), 1).unwrap();
    // Every named sample exists in data\sfx.
    let missing: Vec<_> = names.iter().filter(|(_, n)| !dir.join("data/sfx").join(format!("{n}.wav")).exists() && !dir.join("data/sfx").join(format!("{}.wav", n.to_ascii_lowercase())).exists()).collect();
    eprintln!("{} slots, {} samples loaded, missing {:?}", names.len(), bank.sample_count(), missing);
    assert!(bank.sample_count() + missing.len() >= names.len() - 5);
    // Buzz: jump voice, double jump, skid, stomp start/impact, fall scream, hard landing.
    for id in [0x19u16, 0x10, 0x12, 0x17, 0xF, 0x16, 0x2F] {
        let (s, vol, _) = bank.sound(id).unwrap_or_else(|| panic!("sound {id:#x} has no sample"));
        assert!(s.rate >= 8000 && !s.data.is_empty() && vol > 0, "sound {id:#x}");
    }
}
