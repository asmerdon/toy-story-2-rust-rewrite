# Toy Story 2 – Rust Rewrite

A clean-room reimplementation of the 1999 PC game *Toy Story 2: Buzz Lightyear to the Rescue*, written in Rust.

The original game's behaviour was worked out by reverse-engineering the exe with Ghidra and a disassembler, then rewritten from that understanding. **This repository contains no game data or original code.** Everything is loaded at runtime from your own copy of the game.

- Ported functions are documented in [`docs/re/functions.md`](docs/re/functions.md).
- File formats are documented in [`docs/formats/`](docs/formats/).

## What it does

`ts2-viewer` is a viewer you can play in. It loads the original levels, models, textures, animations and sounds. It runs Buzz and the level logic at the original 60 Hz, using the original's integer maths.

```bash
cargo run --release --bin ts2-viewer -- "C:\Program Files\Disney Interactive\Toy Story 2\data\level01\level.ngn"
```

Prefer campaign numbers (including the correct order for Bombs Away and Slime Time):

```powershell
cargo run --release --bin ts2-viewer -- --level 3
cargo run --release --bin ts2-viewer -- --level 12
```

`--level <1..15> [game_dir]` uses `TS2_GAME_DIR`, or the default install above.
Direct file paths remain supported and refer to internal asset IDs. Press F to play as Buzz.
Campaign 3 loads `level06/level.ngn`; campaign 6 loads `level03/level.ngn`.
Levels 11–15 load `level01`–`level05` respectively, using **`level1.ngn`**.
Their companion data is present; folders named `level11`–`level15` contain conversion configs.

Validate all 15 asset sets with:

```powershell
cargo run --release --bin ts2-tool -- audit-levels "C:\Program Files\Disney Interactive\Toy Story 2"
```

This parses geometry/textures/models, point lists, spawn packets and collision for every
campaign level. It reports script availability separately from asset availability.

## Ported so far

- **Buzz:**
  - Running, jumping, double jump, stomp, spin attack and charged super spin.
  - The laser, including the charged shot.
  - First-person aiming with target lock.
  - Poles, ledge grabs, mantling, zip lines, pushing boxes and swimming.
  - Being thrown by springs.
  - Getting hurt, dying, and lives.
- **World:**
  - The original collision data.
  - Movers that switch on and off, slide and tilt, and carry Buzz.
  - Platforms that follow scripted paths.
  - Room detection.
- **Camera:** the original follow camera, the cutscene camera and the look-at scenes.
- **Creatures:**
  - The shared behaviour-script interpreter.
  - Hit shapes, combat and coin drops.
  - Type-specific routines for most enemies and characters in levels 1–10.
- **HUD:** lives, batteries, coins, charge meters, challenge counters and boss bars.
- **Particles:** driven by the exe's own particle tables.
- **Conversations:** typed speech boxes and the talk camera.
- **Sound:** positional sound with jingles.
- **Pickups:** tokens, power-ups, the info signposts and ability-locked items.
- **Level scripts:**
  - **Level 1 (Andy's House): complete and playable start to finish.** This includes the crib escape, Bo Peep's sheep, Hamm's coins, Mr Potato Head and his missing ear, the RC race, the Tin Man boss, the attic see-saw, the lift, springs, push boxes, swing bars, the garage drills and the tutorial signposts. Collecting a token brings up the "keep playing / exit level" menu and the exit animation.
  - **Levels 2–10:** each level's own script is ported, including its characters, gadgets, challenges and bosses (the Zurg kite, the slime, the drill, the clown, the dino, the car, the level 9 boss with its minions and the spider). These still need playtesting.
- **Rendering:** the original two-pass level draw, back-face culling, the PS1 background picture behind the windows and sky, and fixes for the level data's black faces and glass.
- **Tests:** 100 tests, many driving the real game data, covering movement, combat, bosses, races and gadgets.

## Not done yet

- Levels 2–10 are ported but not yet playtested, so expect bugs there.
- A few puzzles and gadgets are skipped, the biggest being level 4's crane puzzle and level 8's claw machine.
- There is no graphical front end yet. Command-line campaign selection supports levels 1–15.
- Music and FMV playback are not integrated. The supplied disc contains 23 music WAVs and 22 MPEG videos named `.dll`; see `docs/re/asset-audit.md` for decode results.
- The rocket boots and the grappling hook aren't ported.
- Levels 11, 13, 14 and 15 have their complete core asset sets available, but their level scripts are not ported yet.
- Level 12 (Zurg): entrance, attacks, hit recovery, arena boundaries, death and completion are ported and simulation-tested with real data. Visual playtesting remains; lighting and the nearest-light marker are omitted.

## Deviations from the original

A few things are deliberately different, mostly to fix what looked wrong when drawn further or sharper than the original:

- Every creature keeps moving wherever the camera is (the original freezes distant ones); only nearby ones make sound.
- First-person mode has mouse look.
- The PS1 background picture is drawn behind windows and the sky (the PC version only used two colours from it).
- Faces stored pure black in otherwise lit objects take the object's light, and fully transparent-textured glass (the kitchen jars) is drawn as green glass.
- Level 1's garage drill bits move and hurt, matched to the original by eye.

## Requirements

- An installed copy of the PC game, by default at `C:\Program Files\Disney Interactive\Toy Story 2`.
- Stable Rust with the MSVC build tools.

## Running the tests

The tests that use game data are skipped unless `TS2_GAME_DIR` is set:

```bash
set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2
cargo test --release
```

## Status

See [`HANDOFF.md`](HANDOFF.md) for where work stopped and what comes next.
