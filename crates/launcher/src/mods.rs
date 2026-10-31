//! Mods: one folder each under the per-user mods folder ([`crate::paths::mods_dir`]), never
//! inside the game. Phase 1 lists them and keeps their load order; the engine doesn't load
//! them yet. The planned layout (docs/opents2-design.md):
//!
//! ```text
//! mods\hd-buzz\
//!   mod.toml           the manifest below
//!   data\...           files that override the game's data\ at the same relative path
//!   sfx\...            the same for the game's sfx\
//!   scene\*.toml       changes made in the level editor (later)
//! ```
//!
//! ```toml
//! id = "hd-buzz"
//! name = "HD Buzz"
//! version = "1.0.0"
//! authors = ["someone"]
//! description = "Sharper textures for Buzz."
//! requires = []          # ids of mods that must load earlier
//! engine = "0.1"         # the OpenTS2 version it was made for
//! ```
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::ModEntry;

pub const MANIFEST: &str = "mod.toml";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub engine: String,
}

/// A mod found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mod {
    pub dir: PathBuf,
    pub manifest: Result<Manifest, String>,
}

impl Mod {
    /// The manifest's id, else the folder name.
    pub fn id(&self) -> String {
        match &self.manifest {
            Ok(m) => m.id.clone(),
            Err(_) => self.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        }
    }
}

/// Every folder in `dir` that has a manifest (or tries to), sorted by folder name.
pub fn scan(dir: &Path) -> Vec<Mod> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut mods: Vec<Mod> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join(MANIFEST).is_file())
        .map(|dir| {
            let manifest = std::fs::read_to_string(dir.join(MANIFEST))
                .map_err(|e| e.to_string())
                .and_then(|s| toml::from_str(&s).map_err(|e| e.message().to_string()));
            Mod { dir, manifest }
        })
        .collect();
    mods.sort_by(|a, b| a.dir.cmp(&b.dir));
    mods
}

/// The load order for the mods on disk: the saved order first (dropping mods that are
/// gone), then newly found mods at the end, disabled.
pub fn reconcile(saved: &[ModEntry], found: &[Mod]) -> Vec<ModEntry> {
    let ids: Vec<String> = found.iter().map(Mod::id).collect();
    let mut order: Vec<ModEntry> = saved.iter().filter(|e| ids.contains(&e.id)).cloned().collect();
    for id in ids {
        if !order.iter().any(|e| e.id == id) {
            order.push(ModEntry { id, enabled: false });
        }
    }
    order
}

/// Problems with an enabled load order: a required mod that is missing, disabled or loads later.
pub fn order_problems(order: &[ModEntry], found: &[Mod]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, e) in order.iter().enumerate().filter(|(_, e)| e.enabled) {
        let Some(Ok(m)) = found.iter().find(|m| m.id() == e.id).map(|m| &m.manifest) else { continue };
        for r in &m.requires {
            match order.iter().position(|o| &o.id == r) {
                Some(j) if order[j].enabled && j < i => {}
                Some(j) if order[j].enabled => out.push(format!("{} needs {r} to load before it", e.id)),
                _ => out.push(format!("{} needs {r}, which isn't enabled", e.id)),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_mod(root: &Path, folder: &str, body: &str) {
        let d = root.join(folder);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(MANIFEST), body).unwrap();
    }

    #[test]
    fn scan_reconcile_and_check_order() {
        let root = std::env::temp_dir().join(format!("opents2-test-mods-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(scan(&root).is_empty());
        write_mod(&root, "a", "id = \"base\"\nname = \"Base\"\n");
        write_mod(&root, "b", "id = \"addon\"\nname = \"Addon\"\nrequires = [\"base\"]\n");
        write_mod(&root, "c", "not toml");
        std::fs::create_dir_all(root.join("no-manifest")).unwrap();
        let found = scan(&root);
        assert_eq!(found.len(), 3);
        assert!(found[2].manifest.is_err());
        assert_eq!(found[2].id(), "c");

        let saved = [ModEntry { id: "addon".into(), enabled: true }, ModEntry { id: "gone".into(), enabled: true }];
        let mut order = reconcile(&saved, &found);
        let ids: Vec<_> = order.iter().map(|e| (e.id.as_str(), e.enabled)).collect();
        assert_eq!(ids, [("addon", true), ("base", false), ("c", false)]);
        assert_eq!(order_problems(&order, &found), ["addon needs base, which isn't enabled"]);
        order[1].enabled = true;
        assert_eq!(order_problems(&order, &found), ["addon needs base to load before it"]);
        order.swap(0, 1);
        assert!(order_problems(&order, &found).is_empty());
    }
}
