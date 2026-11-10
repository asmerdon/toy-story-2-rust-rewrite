//! Starts the real engine through the launcher's command lines, offscreen (`--shot`), for
//! each route: the original game, the modded game and the level tester. Needs
//! `TS2_GAME_DIR` and a built `ts2-viewer` beside the test (`cargo build --release` first).
use std::path::{Path, PathBuf};

use opents2_launcher::launch::{plan, Mode};

fn engine() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "ts2-viewer.exe" } else { "ts2-viewer" };
    exe.ancestors().skip(1).take(2).map(|d| d.join(name)).find(|p| p.is_file())
}

fn shoot(mode: Mode, frames: &str) -> Option<PathBuf> {
    let game = PathBuf::from(std::env::var_os("TS2_GAME_DIR")?);
    let Some(engine) = engine() else {
        eprintln!("ts2-viewer not built; skipping");
        return None;
    };
    let home = std::env::temp_dir().join(format!("opents2-test-launch-{}", std::process::id()));
    let name = match &mode {
        Mode::Original => "original".to_string(),
        Mode::Modded(_) => "modded".to_string(),
        Mode::LevelTester(n, _) => format!("level{n}"),
    };
    let shot = home.join(format!("{name}.png"));
    let _ = std::fs::remove_file(&shot);
    let mut p = plan(&engine, &game, None, &home, &mode);
    p.args.extend(["--shot".into(), shot.clone().into(), frames.into()]);
    let status = p.spawn(&home.join(format!("{name}.log"))).expect("start the engine").wait().unwrap();
    assert!(status.success(), "{name}: engine exited with {status}; see {}", home.display());
    assert!(is_png(&shot), "{name}: no screenshot at {}", shot.display());
    Some(shot)
}

fn is_png(p: &Path) -> bool {
    std::fs::read(p).is_ok_and(|b| b.starts_with(b"\x89PNG") && b.len() > 1000)
}

#[test]
fn original_game_starts() {
    shoot(Mode::Original, "120");
}

#[test]
fn modded_game_starts() {
    shoot(Mode::Modded(vec![PathBuf::from("no-such-mod")]), "120");
}

#[test]
fn level_tester_starts() {
    shoot(Mode::LevelTester(4, vec![]), "30");
}
