//! Where OpenTS2 keeps its own files. Everything the player owns lives in one per-user
//! folder, apart from the install (which an upgrade or uninstall replaces) and the game:
//!
//! ```text
//! %APPDATA%\OpenTS2\            (Linux: $XDG_CONFIG_HOME/opents2, macOS: ~/Library/Application Support/OpenTS2)
//!   opents2.toml                settings written by the launcher
//!   mods\<id>\mod.toml          one folder per mod, with its manifest
//!   disc\audio, disc\rtlibs     music and movies copied from a disc image
//!   last-run.log                the engine's output from the last launch
//! ```
//!
//! `OPENTS2_HOME` overrides the folder (tests, portable installs). Saves stay where the
//! engine keeps them (`TS2_SAVE_DIR`, else `%APPDATA%\ts2-rust`).
use std::path::PathBuf;

/// The per-user OpenTS2 folder.
pub fn home() -> PathBuf {
    if let Some(p) = std::env::var_os("OPENTS2_HOME") {
        return p.into();
    }
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from).map(|p| p.join("OpenTS2"));
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/OpenTS2"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|p| p.join("opents2"));
    base.unwrap_or_else(|| PathBuf::from("OpenTS2"))
}

pub fn config_file() -> PathBuf {
    home().join("opents2.toml")
}

pub fn mods_dir() -> PathBuf {
    home().join("mods")
}

/// Music and movies copied from a disc image (`audio\`, `rtlibs\`).
pub fn disc_copy_dir() -> PathBuf {
    home().join("disc")
}

pub fn log_file() -> PathBuf {
    home().join("last-run.log")
}

/// The engine executable, installed beside the launcher.
pub fn engine_exe() -> PathBuf {
    let name = if cfg!(windows) { "ts2-viewer.exe" } else { "ts2-viewer" };
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}
