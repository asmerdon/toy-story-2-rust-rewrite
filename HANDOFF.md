# Handoff: Toy Story 2 Rust rewrite

Read this first, then `PROJECT.md`, `docs/re/functions.md` and `docs/formats/*.md` (`terrain.md`, `ngn.md`, `anim.md`, `packets.md`, `all.md`).

## Setup on this machine
- Project: `C:\Users\alexs\Documents\Toy Story 2 - Rust Rewrite` (local git repo, no remote)
- Game install: `C:\Program Files\Disney Interactive\Toy Story 2` (PC, 1999-10-29 build)
- Rust: stable via rustup, MSVC build tools. Audio output uses `rodio` 0.19 (no default features).
- Ghidra 12.1.4: `C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC`; JDK 21 in `C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot`
- Decompiled output: `re/out/c/<address>.c` (**2,811 functions**), `re/out/functions.tsv`, `re/out/strings.tsv`. **Git-ignored; never commit.** Re-run on the existing project (about 10 min):
  ```
  set JAVA_HOME=C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot
  C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat re\proj TS2 -process toy2.exe -noanalysis -scriptPath tools\ghidra -postScript DumpAround.java re\out
  ```
- Disassembly: `re/out` hides call arguments, jump tables and some offsets. A scratch Python venv with `capstone` + `pefile` (scripts: disassemble a range, hex-dump exe data, scan the whole `.text` for operands mentioning an address) fills the gap. Recreate in the session scratchpad.
- Tests: `set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2`, then `cargo test --release` (15 test binaries, all passing). `cargo clippy --all-targets` fails only on an old `6.283` in `examples/wander.rs`.
- Viewer: `cargo run --release --bin ts2-viewer -- "C:\Program Files\Disney Interactive\Toy Story 2\data\level01\level.ngn"`
- Diagnostics (`cargo run --release --example <name> -- <args>`): `rotfit`, `spikes`, `wander` (fall-through stress test), `pickups <levelNN dir>`, `overlap <level.ngn>` (coplanar overlaps by mesh set/zone), `sprites <level.ngn>` (billboard groups), `movers <levelNN dir>` (type-8 and room-map objects), `boxes` and `creatures [levelNN]` (need TS2_GAME_DIR).
- The user mentioned github.com/morluto/rea (an MCP wrapper over Ghidra/IDA). Not installed: our Ghidra dump + capstone cover it. If the user installs it, its tools appear in a new session.

## Rules
- No game data, exe bytes or decompiled code in the repo. Everything is read from the user's install at runtime (exe tables via `src/formats/exe.rs`).
- Write original Rust from understood behaviour. Don't paste decompiler output.
- The user tests the viewer themselves (don't launch it). Build, run the tests, commit, then report what to try.
- Record every identified function in `docs/re/functions.md`.
- The user prefers long autonomous runs: do as much as possible before reporting; don't ask permission for harmless commands.
- Write files with LF endings. Bash heredocs break on quotes and backslashes in this shell: write patch scripts to a scratch file with the Write tool and run them with the venv's python.

## Code map
| File | What |
|---|---|
| `src/game.rs` | `World` (swept sphere vs TERRAIN.ALL; movers switchable/placeable through `Cell`s; room map at ¼ scale; surfaces) and `Player` (Buzz's 60 Hz integer controller; pre-move hook; spin attack; hurt/death/respawn; health and lives) |
| `src/buzz.rs` | Move-table animation player (0x401000 stepper), state numbers |
| `src/camera.rs` | Original follow camera (0x4045E0 main path) |
| `src/audio.rs` | Sound ids → samples, positional volume/pan, jingles (negative ids) |
| `src/scene.rs` | Level/creature batches; instance matrices; billboards (codes 5/6); sprites; special objects |
| `src/specials.rs` | Run-time special-object transforms (0x4CCxxx) |
| `src/pickups.rs` | Coins, batteries, tokens, lives, power-ups; dropped coins |
| `src/boxes.rs` | Pushable objects along level.dat paths (0x433700) |
| `src/level1.rs` | Level 1's setup/script, the parts without creatures |
| `src/creatures.rs` | Creature slots, behaviour script interpreter, movement, animation, contacts, hits, death |
| `src/formats/` | `ngn`, `terrain`, `leveldat` (lists, pickups, special-object records), `packets`, `anim`, `exe`, `all` |
| `src/bin/ts2-viewer.rs` | Viewer and play mode (two-pass level draw, per-frame billboards/specials/creatures) |

## Status (what the user has confirmed)
- Play mode (F) in level 1: movement, jumps, stomp, climbing, camera, sounds: feel matches the original.
- Pickups: collections work, coins etc. look correct.
- Starting lives: 5 (user).

## Done in the last run, not yet tested by the user
- **Flicker**: the two mesh sets are a detailed and a low-detail copy; now drawn in the original's two passes (far set beyond 10000, depth cleared, near set within 12000).
- **Banisters**: code 5/6 groups are billboards (code 6 upright, code 5 facing the camera), rebuilt each frame.
- **Coin sound**: unchanged on purpose: the exe really plays PICKUP5 six times (0, 50, 100, 150, 330, 520 ms, fading). Ask the user to compare with the original if it still sounds wrong.
- **Skid**: unchanged on purpose: 0x4346C0 snaps and re-skids (with sound) on every ≥ 0x5DD reversal on the ground; the port matches it call for call. Ask the user to compare with the original game.
- **Level objects**: specials drawn from live transforms; boxes; level 1's basement door (push it over: it tips open and swaps collision), basement plank, room 2 tops and pop-up platforms, room 4 trampoline, fan.
- **Creatures**: all spawns run their behaviour scripts (wander, hop, chase, animate) near the camera.
- **Combat**: spin attack (E on the ground; hold to charge, release for the super spin), stomp hits, robots hurt Buzz on contact, batteries go down, death → respawn at the start with a life fewer, beaten robots drop a coin and come back after 30 s.

## Next steps (in priority order)
1. **Level 1 creatures' own routines** (+0x94): ZURG3 shooting (needs particles: laser 0x26 shots that hurt Buzz via 0x4100F0), Tin Man boss 0x416AB0, sheep 0x416A60 (Bo Peep's challenge), RC race 0x406A60, Hamm (0x4A1CE0 coins → token), Mr Potato Head (0x4A2480), Rex; then the creature parts of 0x417680 (room 1 falling toys, Bo Peep, RC race, lift, spring, room 5 tilt 0x49EC00, shield gift).
2. **Particles** (0x40FAE0/0x40FDF0/0x410F40, type table 0x4EC160, 64 slots): shots, hit sparks, dust, sparkles; 0x4100F0 checks them against Buzz (coins kind 0x10, hurt kinds 1/2).
3. **Token sequence** and HUD (0x49FD40, sprites 0x493F40), hover boots (0x4A4BB0), the laser (0x434990).
4. Creature contact ellipsoids (+0x84 per-animation table) instead of model-bounds cylinders.
5. Remaining polish: slippery surface 13, riding movers (0x488E10), particles for footsteps, camera extras.

## Useful facts
- Units: object units = level units × 32; 12-bit angles in game code, 16-bit in `.ngn` instance rotations; +Y is down.
- Buzz's object is at 0x52F300 (`docs/re/functions.md`). Creatures: 0x52C840 + 0x9C × slot.
- `level.dat` list table: 0x559C70 + 4 × type (58 rewards/box spots, 61 poles, 63 pickups); special-object records after section 2.
- Scripts address movers by link id − 1. Room = low byte of the room-map object's flags.
- Sound table 0x502950; jingles 0x503828.
- Music is CD audio, FMVs are on the CD too: neither is in the install (ask the user for a BIN/CUE rip or the CD's tracks).
