# Agent brief: OpenTS2 launcher and installer (phase 1)

Copy everything below the line into a new agent session started in
`C:\Users\alexs\Documents\Toy Story 2 - Rust Rewrite`.

---

You are continuing a clean-room Rust reimplementation of the 1999 PC game *Toy Story 2: Buzz
Lightyear to the Rescue*. The project is at `C:\Users\alexs\Documents\Toy Story 2 - Rust Rewrite`.

## Before you start

1. Read these files: `AGENTS.md`, `CLAUDE.md`, the "Current state" section of `HANDOFF.md`, the ground rules in `PROJECT.md`, and `README.md`.
2. Check `git status` and `git log`.
3. Follow AGENTS.md's working mode:
   - work directly on small or tightly coupled changes;
   - use the cheapest capable helper for bounded tasks, one helper at a time;
   - verify what helpers report;
   - keep HANDOFF.md and MODLOG.md current.

Don't commit or push unless the user asks.

## The goal

The user wants to distribute the project as **OpenTS2**, an open engine for Toy Story 2 that runs on the player's own copy of the game, in the spirit of OpenMW (Open Morrowind).

The long-term vision, in order:

1. **An installer and launcher (this brief).** A normal Windows installer that installs OpenTS2 and creates shortcuts. A launcher window lets the player:
   - point OpenTS2 at their Toy Story 2 installation (and optionally the game disc, for music and movies);
   - **play the original game** (the faithful port);
   - **play a modded game** (the original plus a chosen set of mods);
   - **open the level tester**: the existing free-camera viewer on any level, with play-as-Buzz toggling.
2. **A level editor:** a free-camera editor for placing and changing objects, creatures, pickups and paths, saved as mod files, never by changing the original data.
3. **Modding:**
   - asset overrides (textures, models, sounds);
   - playing as other creatures;
   - porting animations from other Rust game rewrites;
   - scripted behaviour changes.

   Mods live outside the game folder and load in an order the player picks.

The app is named **OpenTS2**, and its icon is a **Pizza Planet token**.

**Keep phase 1 simple.** Deliver an installer and a launcher that run the existing binaries. Design them so the later phases (an editor, a mod list with load order, and an override folder searched before the game's data) can be added without a rewrite.

## Research first: OpenMW as the model

Spend a bounded amount of time researching OpenMW (openmw.org, its GitLab repository and its documentation).

Things to learn from it:
- **Separate pieces:** the engine (`openmw`), the launcher (`openmw-launcher`), the editor (`openmw-cs`) and the first-run wizard, and how each is used.
- **First run and data:** how the wizard finds or imports the original game's data without redistributing it.
- **Configuration:** the `openmw.cfg` model. `data=` lines are search paths (a virtual file system where later paths override earlier ones), and `content=` lines are the plugins in load order. User settings are kept apart from the defaults.
- **Mod management in the launcher:** Data Files and Content tabs, enabling and ordering, and how conflicts show up.
- **Installers and packaging per platform,** and how they handle licences and disclaimers.

Write a short summary of what to copy and what to simplify in `docs/opents2-design.md`, with source links. Then build.

## What exists now

- **Crate layout:** one crate, `ts2` (`Cargo.toml`), with binaries `ts2-viewer` (the game and level viewer) and `ts2-tool` (asset audit).
- **Rendering stack:** wgpu and winit; audio uses rodio. Rust 2024 edition, stable toolchain, MSVC.
- **`ts2-viewer` command lines:**
  - `ts2-viewer`: the full game from the startup screens.
  - `ts2-viewer [game_dir]`: the same, with an explicit game folder.
  - `ts2-viewer --level N [game_dir]`: one campaign level (1–15) in free camera; F toggles playing as Buzz, Escape closes.
  - `ts2-viewer <path\to\levelNN\level.ngn>`: a level file by path.
  - `--shot out.png "<script>"`: offscreen renders for testing.
- **Game location:** `TS2_GAME_DIR`, else `C:\Program Files\Disney Interactive\Toy Story 2`. It must contain `toy2.exe` and `data\`.
- **Disc media (music and FMV):** `TS2_CD_DIR`, else a git-ignored `ts2-media.txt` in the working directory, else `<game>\audio` or `<game>\CD\audio`.
- **Saves:** `TS2_SAVE_DIR`, else `%APPDATA%\ts2-rust`, in the original's `Toy2NN.sav` format.
- **Assets** are read at runtime from the user's installation. The repository must never contain game data, exe bytes or decompiled code (`re/` is private and git-ignored).
- **Tests:** `cargo test --release` (185 tests; set `TS2_GAME_DIR` for the data-driven ones).

## Phase 1 deliverables

1. **Launcher application** (`opents2` or `opents2-launcher` binary), a small native window:
   - **Game folder:**
     - auto-detect the default install;
     - browse for another;
     - validate it (look for `toy2.exe`, `data\level01\level.ngn` and the like);
     - show a clear message when it's invalid.
   - **Optional disc folder** for music and movies, validated (`audio\`, `rtlibs\`).
   - **Three actions:**
     - **Play original:** runs `ts2-viewer` with the configured paths.
     - **Play modded:** for now, shows a mod list (empty or "coming soon") and runs the same game. Define the mod folder location and a placeholder config format now.
     - **Level tester:** a level picker (1–15, with names from `campaign::LEVELS`) that runs `ts2-viewer --level N`.
   - **Settings** saved in a per-user config file (for example `%APPDATA%\OpenTS2\opents2.toml`), separate from saves and mods. Pass the paths to the game through the existing environment variables or arguments, rather than changing how the game finds data.
   - **Version and credits,** plus a clear disclaimer: an unofficial fan project, not affiliated with Disney, Pixar, Traveller's Tales or Activision; the original game is required.
   - **Your choice:** pick the UI toolkit and justify it in the design doc. egui/eframe fits well, since the project already uses wgpu and winit. Keep its dependencies modest.
2. **App identity:**
   - The window title and executables are named OpenTS2.
   - The icon is a Pizza Planet token. **Make it as new artwork** (for example an SVG converted to `.ico` at 16–256 px). Do not extract it from the game's textures or copy studio artwork.
   - Embed the icon in the Windows executables (for example with `winres` or `embed-resource`) and use it as the window icon.
3. **Windows installer:**
   - Builds from the repository with one documented command. A good choice is WiX through `cargo-wix` (MSI), or Inno Setup or NSIS; justify the choice.
   - Installs the launcher and `ts2-viewer`, adds Start-menu and optional desktop shortcuts, includes an uninstaller, shows a licence/disclaimer page, and ships **no game files**.
   - Per-user install by default, so no admin rights are needed if possible.
   - Note how macOS and Linux packages could follow, but don't build them yet.
4. **Structure for later phases:**
   - Propose the workspace split you want, for example `crates/engine`, `crates/launcher`, `crates/editor`, `crates/modding`. Make only the minimal moves that phase 1 needs, and keep all tests passing. Don't do a large refactor of the engine now.
   - Document the planned data model in `docs/opents2-design.md`:
     - a layered virtual file system (user mods over game data), as OpenMW does;
     - a mod manifest (name, version, author, load order, dependencies, which files it overrides);
     - where a free-camera editor would save its changes (as mod files, never into the game folder).
5. **Docs:**
   - README: a section on installing and using OpenTS2, plus how to build the installer.
   - HANDOFF.md: the current state, decisions and next steps.
   - MODLOG.md: the changes.

## Acceptance checks

- `cargo build --release` builds the launcher and viewer without warnings. The full `cargo test --release` still passes with `TS2_GAME_DIR` set.
- The launcher detects the default install, rejects a wrong folder, and starts:
  - the full game;
  - a chosen level in the level tester;
  - the "modded" route.

  Verify with offscreen screenshots where possible. The user tests interactive windows themselves, so describe what they should try.
- The installer builds from a clean checkout with the documented command. Installing puts working shortcuts in place, and uninstalling removes everything except the user's config and saves.
- No game data is in the repository or the installer. Check the packaged file list.
- The icon shows in the taskbar, the window, the shortcuts and the installer.

## Constraints

- Keep the clean-room rules (PROJECT.md): no original assets, exe bytes or decompiled code in the repository or the installer.
- Don't change gameplay behaviour in this work. If anything in the engine has to change (for example, accepting paths from the launcher), keep the change small, and test it.
- Ask the user only about real decisions: the final icon look, the licence text, and whether to sign the installer.
