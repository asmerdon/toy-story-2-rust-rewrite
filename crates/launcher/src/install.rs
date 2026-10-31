//! Finding and checking the player's own copy of the game. Like OpenMW's wizard, OpenTS2
//! never copies or ships the data: it records where the install is and reads it from there.
use std::path::{Path, PathBuf};

/// Where the retail installer puts the game.
pub const DEFAULT_GAME_DIR: &str = r"C:\Program Files\Disney Interactive\Toy Story 2";

/// Files a usable install must have, relative to the game folder: the exe (tables the engine
/// reads), the front end, and the first and last campaign levels (levels 11–15 are
/// `level1.ngn` in folders 01–05; see `ts2::campaign`).
const REQUIRED: [&str; 4] = [
    "toy2.exe",
    r"data\level00\level.ngn",
    r"data\level01\level.ngn",
    r"data\level05\level1.ngn",
];

/// Why a folder isn't a usable game install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameDirError {
    NotAFolder,
    /// The `data` folder (or a level folder) was picked: the install is this parent.
    PickedSubfolder(PathBuf),
    Missing(Vec<String>),
}

impl std::fmt::Display for GameDirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAFolder => write!(f, "This folder doesn't exist."),
            Self::PickedSubfolder(p) => write!(f, "This is inside the game. Pick the folder that contains toy2.exe: {}", p.display()),
            Self::Missing(m) => write!(f, "This isn't a Toy Story 2 install: missing {}.", m.join(", ")),
        }
    }
}

/// Checks that `dir` is a Toy Story 2 PC install.
pub fn check_game_dir(dir: &Path) -> Result<(), GameDirError> {
    if !dir.is_dir() {
        return Err(GameDirError::NotAFolder);
    }
    let missing: Vec<String> = REQUIRED.iter().filter(|f| !dir.join(native(f)).is_file()).map(|f| f.to_string()).collect();
    if missing.is_empty() {
        return Ok(());
    }
    if let Some(root) = dir.ancestors().skip(1).take(2).find(|a| check_game_dir_files(a)) {
        return Err(GameDirError::PickedSubfolder(root.to_owned()));
    }
    Err(GameDirError::Missing(missing))
}

fn check_game_dir_files(dir: &Path) -> bool {
    REQUIRED.iter().all(|f| dir.join(native(f)).is_file())
}

/// `data\level01` written with this platform's separator.
fn native(rel: &str) -> PathBuf {
    rel.split('\\').collect()
}

/// Places to look for an install, most likely first.
pub fn game_dir_candidates() -> Vec<PathBuf> {
    let mut c = Vec::new();
    if let Some(p) = std::env::var_os("TS2_GAME_DIR") {
        c.push(PathBuf::from(p));
    }
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(p) = std::env::var_os(var) {
            c.push(PathBuf::from(p).join("Disney Interactive").join("Toy Story 2"));
        }
    }
    c.push(PathBuf::from(DEFAULT_GAME_DIR));
    c.dedup();
    c
}

/// The first candidate that is a valid install.
pub fn detect_game_dir() -> Option<PathBuf> {
    game_dir_candidates().into_iter().find(|p| check_game_dir(p).is_ok())
}

/// What a disc folder provides: music (`audio\`) and movies (`rtlibs\`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscMedia {
    /// The folder holding `audio\` (and `rtlibs\`): what the engine's `TS2_CD_DIR` wants.
    pub root: PathBuf,
    pub music: bool,
    pub movies: bool,
}

/// Finds the media inside a picked disc folder. Accepts the folder itself, the disc's root
/// (`Setup\Toy2\CD`) or a folder with a `CD` subfolder, as the engine does for the install.
pub fn find_disc_media(dir: &Path) -> Option<DiscMedia> {
    let tries = [dir.to_owned(), dir.join("CD"), dir.join("Setup").join("Toy2").join("CD")];
    tries.into_iter().find_map(|root| {
        let music = root.join("audio").is_dir();
        let movies = root.join("rtlibs").is_dir();
        (music || movies).then_some(DiscMedia { root, music, movies })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_install(root: &Path) {
        for f in REQUIRED {
            let p = root.join(native(f));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("opents2-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn accepts_install_and_rejects_others() {
        let d = temp("install");
        fake_install(&d);
        assert_eq!(check_game_dir(&d), Ok(()));
        assert_eq!(check_game_dir(&d.join("data")), Err(GameDirError::PickedSubfolder(d.clone())));
        assert_eq!(check_game_dir(&d.join("nope")), Err(GameDirError::NotAFolder));
        let empty = temp("empty");
        assert!(matches!(check_game_dir(&empty), Err(GameDirError::Missing(m)) if m.len() == REQUIRED.len()));
        std::fs::remove_file(d.join("toy2.exe")).unwrap();
        assert_eq!(check_game_dir(&d), Err(GameDirError::Missing(vec!["toy2.exe".into()])));
    }

    #[test]
    fn disc_media_found_in_disc_layouts() {
        let d = temp("disc");
        assert_eq!(find_disc_media(&d), None);
        let cd = d.join("Setup").join("Toy2").join("CD");
        std::fs::create_dir_all(cd.join("audio")).unwrap();
        assert_eq!(find_disc_media(&d), Some(DiscMedia { root: cd.clone(), music: true, movies: false }));
        std::fs::create_dir_all(cd.join("rtlibs")).unwrap();
        assert_eq!(find_disc_media(&cd), Some(DiscMedia { root: cd, music: true, movies: true }));
    }

    /// The real install, when the tests are given one.
    #[test]
    fn real_install_is_valid() {
        let Some(game) = std::env::var_os("TS2_GAME_DIR") else { return };
        assert_eq!(check_game_dir(Path::new(&game)), Ok(()));
    }
}
