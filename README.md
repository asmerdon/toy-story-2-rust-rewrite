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
  - **Level 1 (Andy's House):** complete. This includes the crib escape, Bo Peep's sheep, Hamm's coins, Mr Potato Head, the RC race, the Tin Man boss, the see-saw, the lift, the spring and the tutorial signposts.
  - **Levels 2–5:** mostly done, including their characters, gadgets and bosses (the Zurg kite, the slime, the drill and the clown).
  - **Level 6:** the boss fight is in progress.
- **Tests:** 70+ tests drive the real game data, covering movement, combat, bosses, races and gadgets.

## Not done yet

- Level 6's boss can't be beaten yet.
- Levels 7–10 have only the shared enemy and character code; none of their own level logic is ported yet.
- A few puzzles and gadgets are skipped, the biggest being level 4's crane puzzle.
- There is no token or menu screen.
- There is no music or FMVs; these are on the game CD, not in the install.
- The boots and the grappling hook aren't ported.
- Levels 11–15 can't be done from the PC install, because their data is only on the CD.

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
