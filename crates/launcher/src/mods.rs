//! Mods: one folder each under the per-user mods folder ([`crate::paths::mods_dir`]), never
//! inside the game. The launcher lists them, keeps their load order and passes the enabled
//! ones to the engine (`OPENTS2_MODS`). The engine loads `[play_as]` (playing as another
//! character, `src/modding.rs`); file overrides are planned. The layout
//! (docs/opents2-design.md):
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
    /// The engine's `[play_as]` table; the launcher only shows the character. Its other keys
    /// are the engine's business and ignored here.
    #[serde(default)]
    pub play_as: Option<PlayAs>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlayAs {
    pub creature: String,
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

/// Copies each example mod in `bundled` into `user` once: skipped when its id is in
/// `offered` (copied before, maybe deleted since) or a folder of that name is there already.
/// An example whose manifest has a newer `version` than the player's copy replaces it; the
/// old manifest is kept as `mod.toml.old`. Returns the ids copied for the first time, which
/// the caller adds to `offered`.
pub fn install_examples(bundled: &Path, user: &Path, offered: &[String]) -> Vec<String> {
    let mut copied = Vec::new();
    for m in scan(bundled) {
        let id = m.id();
        let Some(name) = m.dir.file_name() else { continue };
        let to = user.join(name);
        if to.exists() {
            let theirs = scan_one(&to).and_then(|t| t.manifest.ok()).map(|t| t.version).unwrap_or_default();
            let ours = m.manifest.as_ref().map(|o| o.version.clone()).unwrap_or_default();
            if newer(&ours, &theirs) {
                let _ = std::fs::copy(to.join(MANIFEST), to.join("mod.toml.old"));
                let _ = copy_dir(&m.dir, &to);
            }
            continue;
        }
        if offered.contains(&id) {
            continue;
        }
        if copy_dir(&m.dir, &to).is_ok() {
            copied.push(id);
        }
    }
    copied
}

fn scan_one(dir: &Path) -> Option<Mod> {
    let text = std::fs::read_to_string(dir.join(MANIFEST)).ok()?;
    Some(Mod { dir: dir.to_owned(), manifest: toml::from_str(&text).map_err(|e: toml::de::Error| e.message().to_string()) })
}

/// `a` is a later dotted version than `b` ("0.10.0" > "0.9.1"; a missing part counts as 0).
fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.trim().parse().unwrap_or(0)).collect() };
    let (a, b) = (parts(a), parts(b));
    (0..a.len().max(b.len())).map(|i| (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0))).find(|(x, y)| x != y).is_some_and(|(x, y)| x > y)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()))?;
        } else {
            std::fs::copy(e.path(), to.join(e.file_name()))?;
        }
    }
    Ok(())
}

/// The enabled mods that change who the player is, in load order (the engine uses the last).
pub fn play_as_mods<'a>(order: &'a [ModEntry], found: &'a [Mod]) -> Vec<(&'a str, &'a str)> {
    order
        .iter()
        .filter(|e| e.enabled)
        .filter_map(|e| match &found.iter().find(|m| m.id() == e.id)?.manifest {
            Ok(Manifest { play_as: Some(p), .. }) => Some((e.id.as_str(), p.creature.as_str())),
            _ => None,
        })
        .collect()
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

    #[test]
    fn manifest_tolerates_play_as_and_other_engine_keys() {
        let text = "id = \"play-as-woody\"
name = \"Play as Woody\"
engine = \"0.1\"

[play_as]
creature = \"WOODY\"
laser = false
map = [[0, 13]]

[[play_as.rest]]
nodes = [5]
pivot = [0.0, 0.0, 0.0]
angles = [0.0, 0.0, 80.0]
";
        let m: Manifest = toml::from_str(text).unwrap();
        assert_eq!((m.id.as_str(), m.name.as_str()), ("play-as-woody", "Play as Woody"));
        assert_eq!(m.play_as.map(|p| p.creature).as_deref(), Some("WOODY"));
        let root = std::env::temp_dir().join(format!("opents2-test-playas-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_mod(&root, "woody", text);
        let found = scan(&root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id(), "play-as-woody");
        assert!(found[0].manifest.is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn examples_are_copied_once_and_play_as_is_reported() {
        let root = std::env::temp_dir().join(format!("opents2-test-examples-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (bundled, user) = (root.join("bundled"), root.join("user"));
        let woody = "id = \"play-as-woody\"\nname = \"W\"\n[play_as]\ncreature = \"WOODY\"\n";
        write_mod(&bundled, "play-as-woody", woody);
        write_mod(&bundled, "play-as-zurg-robot", "id = \"play-as-zurg-robot\"\nname = \"Z\"\n[play_as]\ncreature = \"ZURG1\"\nhop_height = 4.0\n");
        std::fs::write(bundled.join("README.md"), "not a mod").unwrap();
        let copied = install_examples(&bundled, &user, &[]);
        assert_eq!(copied, ["play-as-woody", "play-as-zurg-robot"]);
        assert!(user.join("play-as-woody").join(MANIFEST).is_file());
        // Deleted by the player: not put back.
        std::fs::remove_dir_all(user.join("play-as-woody")).unwrap();
        assert!(install_examples(&bundled, &user, &copied).is_empty());
        assert!(!user.join("play-as-woody").exists());
        // Both enabled: both reported, in load order (the engine uses the last).
        write_mod(&user, "play-as-woody", woody);
        let found = scan(&user);
        let saved = [ModEntry { id: "play-as-woody".into(), enabled: true }, ModEntry { id: "play-as-zurg-robot".into(), enabled: true }];
        let order = reconcile(&saved, &found);
        assert_eq!(play_as_mods(&order, &found), [("play-as-woody", "WOODY"), ("play-as-zurg-robot", "ZURG1")]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_newer_example_replaces_the_players_copy() {
        let root = std::env::temp_dir().join(format!("opents2-test-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (bundled, user) = (root.join("bundled"), root.join("user"));
        write_mod(&bundled, "t", "id = \"t\"\nname = \"T\"\nversion = \"0.1.0\"\n");
        assert_eq!(install_examples(&bundled, &user, &[]), ["t"]);
        write_mod(&user, "t", "id = \"t\"\nname = \"Mine\"\nversion = \"0.1.0\"\n");
        install_examples(&bundled, &user, &["t".into()]);
        assert!(std::fs::read_to_string(user.join("t").join(MANIFEST)).unwrap().contains("Mine"), "same version: the player's edits stay");
        write_mod(&bundled, "t", "id = \"t\"\nname = \"T2\"\nversion = \"0.10.0\"\n");
        assert!(install_examples(&bundled, &user, &["t".into()]).is_empty());
        assert!(std::fs::read_to_string(user.join("t").join(MANIFEST)).unwrap().contains("T2"));
        assert!(std::fs::read_to_string(user.join("t").join("mod.toml.old")).unwrap().contains("Mine"));
        assert!(newer("0.10.0", "0.9.1") && !newer("1.0", "1.0.0") && newer("1.0.1", ""));
        let _ = std::fs::remove_dir_all(&root);
    }
}
