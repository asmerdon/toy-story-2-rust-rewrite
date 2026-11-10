//! The per-user settings file, `opents2.toml` (see [`crate::paths`]).
//!
//! ```toml
//! version = 1
//! game_dir = 'C:\Program Files\Disney Interactive\Toy Story 2'
//! cd_dir = 'D:\Setup\Toy2\CD'     # optional: music and movies
//! level = 4                       # the level tester's last pick
//!
//! # The modded game's load order, first to last: later mods override earlier ones,
//! # as OpenMW's data= lines do. The engine gets the enabled ones in OPENTS2_MODS.
//! [[mods]]
//! id = "hd-buzz"
//! enabled = true
//! ```
//!
//! Like OpenMW's user openmw.cfg, this holds only what the player chose; defaults live in
//! the code. Unknown keys are ignored so newer launchers can add settings.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub game_dir: Option<PathBuf>,
    pub cd_dir: Option<PathBuf>,
    pub level: usize,
    pub mods: Vec<ModEntry>,
    /// Ids of the example mods already copied into the mods folder once, so one the player
    /// deletes isn't put back.
    pub examples: Vec<String>,
}

/// One mod in the load order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModEntry {
    pub id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self { version: CONFIG_VERSION, game_dir: None, cd_dir: None, level: 1, mods: Vec::new(), examples: Vec::new() }
    }
}

impl Config {
    /// Reads the settings, or the defaults when there are none or they can't be read.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, format!("# OpenTS2 settings, written by the launcher.\n{body}"))
    }

    /// The ids of the enabled mods, in load order.
    pub fn enabled_mods(&self) -> impl Iterator<Item = &str> {
        self.mods.iter().filter(|m| m.enabled).map(|m| m.id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let dir = std::env::temp_dir().join(format!("opents2-test-config-{}", std::process::id()));
        let path = dir.join("opents2.toml");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Config::load(&path), Config::default());
        let c = Config {
            game_dir: Some(r"C:\Games\Toy Story 2".into()),
            cd_dir: None,
            level: 12,
            mods: vec![ModEntry { id: "a".into(), enabled: true }, ModEntry { id: "b".into(), enabled: false }],
            ..Config::default()
        };
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path), c);
        assert_eq!(c.enabled_mods().collect::<Vec<_>>(), ["a"]);
        // Missing keys take defaults; unknown keys are ignored.
        std::fs::write(&path, "level = 3\nfuture = true\n[[mods]]\nid = \"x\"\n").unwrap();
        let c = Config::load(&path);
        assert_eq!((c.level, c.game_dir, c.mods[0].enabled), (3, None, true));
    }
}
