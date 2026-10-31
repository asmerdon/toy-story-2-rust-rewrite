//! Starting the engine. The launcher passes the player's choices through the engine's
//! existing command line and environment (`TS2_GAME_DIR`, `TS2_CD_DIR`), so the engine finds
//! data exactly as it does when run by hand.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// What to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// The full game from the startup screens.
    Original,
    /// The full game with these mod folders, in load order. The engine doesn't read them
    /// yet; they go in `OPENTS2_MODS` so the layered file system can pick them up later.
    Modded(Vec<PathBuf>),
    /// One campaign level (1–15) in the free camera; F plays as Buzz.
    LevelTester(usize),
}

/// A resolved launch: the program, its arguments and environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(&'static str, OsString)>,
    pub cwd: PathBuf,
}

pub fn plan(engine: &Path, game: &Path, cd: Option<&Path>, home: &Path, mode: &Mode) -> Plan {
    let mut args: Vec<OsString> = Vec::new();
    if let Mode::LevelTester(n) = mode {
        args.push("--level".into());
        args.push(n.to_string().into());
    }
    args.push(game.into());
    let mut env = vec![("TS2_GAME_DIR", game.into())];
    if let Some(cd) = cd {
        env.push(("TS2_CD_DIR", cd.into()));
    }
    if let Mode::Modded(dirs) = mode {
        env.push(("OPENTS2_MODS", std::env::join_paths(dirs).unwrap_or_default()));
    }
    // The engine also reads a `ts2-media.txt` from its working directory; start it in the
    // OpenTS2 folder so only the launcher's settings apply.
    Plan { program: engine.to_owned(), args, env, cwd: home.to_owned() }
}

impl Plan {
    /// Starts the engine without a console window, its output going to `log`.
    pub fn spawn(&self, log: &Path) -> std::io::Result<Child> {
        std::fs::create_dir_all(&self.cwd)?;
        let out = std::fs::File::create(log)?;
        let err = out.try_clone()?;
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args).envs(self.env.iter().map(|(k, v)| (k, v))).current_dir(&self.cwd);
        cmd.stdin(Stdio::null()).stdout(out).stderr(err);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.spawn()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines() {
        let (engine, game, home) = (Path::new("ts2-viewer"), Path::new("G"), Path::new("H"));
        let p = plan(engine, game, None, home, &Mode::Original);
        assert_eq!(p.args, ["G"]);
        assert_eq!(p.env, [("TS2_GAME_DIR", "G".into())]);
        assert_eq!(p.cwd, home);

        let p = plan(engine, game, Some(Path::new("CD")), home, &Mode::LevelTester(12));
        assert_eq!(p.args, ["--level", "12", "G"]);
        assert_eq!(p.env[1], ("TS2_CD_DIR", "CD".into()));

        let mods = vec![PathBuf::from("m1"), PathBuf::from("m2")];
        let p = plan(engine, game, None, home, &Mode::Modded(mods.clone()));
        assert_eq!(p.args, ["G"]);
        let (k, v) = &p.env[1];
        assert_eq!((*k, std::env::split_paths(v).collect::<Vec<_>>()), ("OPENTS2_MODS", mods));
    }
}
