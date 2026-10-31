//! OpenTS2 launcher logic, kept apart from the window so it can be tested.
//!
//! - [`paths`]: where OpenTS2 keeps its settings and mods, and where the engine is.
//! - [`install`]: finding and checking the player's game folder and disc folder.
//! - [`iso`]: copying the music and movies out of a disc image.
//! - [`config`]: the per-user settings file, `opents2.toml`.
//! - [`mods`]: the mod folder and its manifests (phase 1: listed, not yet loaded).
//! - [`launch`]: the command lines that start `ts2-viewer`.
//!
//! See docs/opents2-design.md for the model these follow (OpenMW's launcher and config).

pub mod config;
pub mod install;
pub mod iso;
pub mod launch;
pub mod mods;
pub mod paths;

/// The launcher's version, shown in the About tab.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The disclaimer shown in the launcher and the installer.
pub const DISCLAIMER: &str = "OpenTS2 is an unofficial fan project. It is not affiliated with, \
endorsed by or connected to Disney, Pixar, Traveller's Tales or Activision. Toy Story 2 is \
their property. OpenTS2 contains no game data: you need your own copy of the original PC game \
(Toy Story 2: Buzz Lightyear to the Rescue, 1999) to play.";
