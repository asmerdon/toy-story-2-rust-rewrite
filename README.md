# Toy Story 2 – Rust Rewrite

A clean-room reimplementation of the 1999 PC game *Toy Story 2: Buzz Lightyear to the Rescue*, written in Rust.

The original game's behaviour was worked out by reverse-engineering the exe with Ghidra and a disassembler, then rewritten from that understanding. **This repository contains no game data or original code.** Everything is loaded at runtime from your own copy of the game.

- Ported functions are documented in [`docs/re/functions.md`](docs/re/functions.md).
- File formats are documented in [`docs/formats/`](docs/formats/).
- Where work stopped and what comes next is in [`HANDOFF.md`](HANDOFF.md).

## Status — 2026-10-31

All 15 campaign levels, the front end, saves, music and movies are in.
- **Level 1** is signed off by the project's playtester.
- **Levels 2–15** have been through two rounds of playtest reports. Every report that could be reproduced is fixed:
  - collision walls and conveyor ends;
  - boss attacks, checked against the original's code;
  - drawing order;
  - challenge countdowns;
  - the Etch A Sketch level select;
  - level-specific scripts.
- **Not yet:** a full sign-off of levels 2–15.

[HANDOFF.md](HANDOFF.md) has the current state and open issues; [docs/coverage.md](docs/coverage.md) has what has been checked, level by level. Agents follow [AGENTS.md](AGENTS.md).

**Next:** OpenTS2, an installer and launcher (play the original or a modded game, or open the level tester), then a level editor and modding support. See [docs/opents2-brief.md](docs/opents2-brief.md).

## Requirements

- An installed copy of the PC game, by default at `C:\Program Files\Disney Interactive\Toy Story 2`. Set `TS2_GAME_DIR` if it's somewhere else.
- For music and movies, the files from the game disc: set `TS2_CD_DIR` to the folder that contains `audio\` and `rtlibs\`, or put that path in a file named `ts2-media.txt` in the repository folder (it's git-ignored). Without them the game runs silent of music and skips the movies.
- Stable Rust with the MSVC build tools.

## Playing

Start the whole game, from the startup screens and title onwards:

```bash
cargo run --release --bin ts2-viewer
```

Or jump straight into one level by its campaign number (1–15), then press F to play as Buzz:

```bash
cargo run --release --bin ts2-viewer -- --level 4
```

The numbers follow the campaign order, so 3 is Bombs Away and 6 is Slime Time. `--level <1..15> [game_dir]` uses `TS2_GAME_DIR`, or the default install above. A direct path to a `level.ngn` file also works, but it takes the exe's internal level numbering.

### Controls

| Key | Action |
|---|---|
| WASD (arrows in menus) | Move |
| Space | Jump (again in the air for a double jump); pick in menus |
| E | Spin attack on the ground (hold to charge), stomp in the air |
| Left mouse | Wrist laser (hold to charge) |
| Q | First person (mouse to look); X locks on to a target |
| Escape | Back in menus; in a level, quit to the main menu |
| Space, Enter or Escape | Skip a movie |
| M | Mute or unmute the music |
| V | Switch between the original camera and a free follow camera |

While you play, the mouse cursor is hidden and kept inside the window.

When a level is opened with `--level`, F toggles between playing and a free camera, and Escape closes the window.

## What's in

### The game

- **Buzz:** running, jumping, double jump, stomp, spin attack and charged super spin; the laser and its charged shot; first-person aiming with target lock; poles, ledge grabs, mantling, zip lines, swing bars, pushing boxes and swimming; springs; rocket boots, hover boots and the grappling hook; getting hurt, dying and lives.
- **World:** the original collision data (including the invisible boundary walls), movers that switch on and off, slide, tilt and carry Buzz, platforms on scripted paths, and room detection.
- **Camera:** the original follow camera, the cutscene camera and the look-at scenes.
- **Creatures:** the shared behaviour-script interpreter, hit shapes, combat and coin drops, and type-specific routines for the enemies and characters of every level.
- **Pickups:** coins, batteries, extra lives, tokens, power-ups, the info signposts and ability-locked items.
- **Level scripts:** all 15 campaign levels, with their characters, gadgets, challenges, races and bosses.
  - **Level 1 (Andy's House)** is complete and signed off.
  - **Levels 2–15** are ported and playtested in two rounds; a full sign-off is still to come.
- **HUD and conversations:** lives, batteries, coins, charge meters, challenge counters and boss bars; typed speech boxes and the talk camera; the "keep playing / exit level" token menu.
- **Particles**, driven by the exe's own particle tables.

### The front end

- The startup screens (trademarks and rating), the logo movies and the trailer.
- The title screen and main menu.
- The level-select map, with its flying camera, its models changing from blueprints to finished levels as they unlock, and the token gates on levels 3, 6, 9, 12 and 15.
- After a level: the results screen (tokens, coins, abilities), or the "boss defeated" screen after a boss.
- "Save game?" and eight save slots; "load game" from the main menu. Lives, abilities, tokens and movies seen carry through a game and into saves.
- Game over when the lives run out.

### Sound, music and movies

- Positional sound effects and jingles, from the game's own sound tables.
- The original stereo music, chosen as the exe chooses it: each level's theme, the miniboss and race themes, and the front end's tracks.
- The disc's movies, played when the original plays them: level intros the first time a level is started, boss movies, and the endings.

### Rendering

- The original two-pass level draw (low-detail set in the distance, detailed set close up), with special objects following each set's distance rules, and Level 4's room/portal visibility.
- Back-face culling decided per material, as the original does, so fences, boards and cut-outs are drawn from both sides.
- Fixes for the level data's black faces and glass, and the PS1 background picture behind windows and the sky.

## Not done yet

- A final sign-off pass over levels 2–15; see HANDOFF's open issues.
- The countdown's tick sound; room culling outside level 4.
- The pause menu (Escape quits to the main menu for now), options, the movie viewer, the credits, the loading pictures and the attract demo.
- A few small gadgets: level 8's electric arcs and cannon, drips, dynamic lights, floor shadows and the stomp's camera shake; level 11's lights; level 12's lighting.
- Health isn't carried between levels yet, and the sound and music volume settings aren't saved.

## Deviations from the original

A few things are deliberately different, mostly to fix what looked wrong when drawn further or sharper than the original:

- Every creature keeps moving wherever the camera is (the original freezes distant ones); only nearby ones make sound.
- First-person mode has mouse look.
- The PS1 background picture is drawn behind windows and the sky (the PC version only used two colours from it).
- Faces stored pure black in otherwise lit objects take the object's light, and fully transparent-textured glass (the kitchen jars) is drawn as green glass.
- Level 1's garage drill bits move and hurt, matched to the original by eye.
- Saves go to `%APPDATA%/ts2-rust` (or `TS2_SAVE_DIR`) instead of beside the game, which is usually read-only. They use the original's `Toy2NN.sav` format, so saves copied between the two work.

## Running the tests

The tests that use game data are skipped unless `TS2_GAME_DIR` is set; the movie test also needs the disc files.

```bash
set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2
cargo test --release
```

The suite (185 tests) covers movement, combat, bosses, races, gadgets, the front end, saves and movie decoding. A passing simulation test is not proof that a level plays correctly; see docs/coverage.md.

## Tools for development

- **Screenshots without a window:** `ts2-viewer [--level N] --shot out.png "<script>"` renders offscreen and silently. In the script, numbers run that many frames, `+Space` / `-Space` press and release a key, `play` starts Buzz, `cam:x,y,z,yaw,pitch` holds the camera, `tp:x,y,z` puts Buzz somewhere (level units) and `?` prints his position and health.
- **Asset audit:** `cargo run --release --bin ts2-tool -- audit-levels "<game dir>"` parses the geometry, textures, models, point lists, spawns and collision of every campaign level.
- **Diagnostics:** about 50 examples in `examples/` (`cargo run --release --example <name>`), listed in HANDOFF.

## Third-party crates

Rendering uses wgpu and winit, sound uses rodio, the movies' video is decoded by `oxideav-mpeg12video` and their sound by Symphonia.
