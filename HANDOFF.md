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
- Tests: `set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2`, then `cargo test --release` (all passing). `cargo clippy --all-targets` fails only on an old `6.283` in `examples/wander.rs`.
- Viewer: `cargo run --release --bin ts2-viewer -- "C:\Program Files\Disney Interactive\Toy Story 2\data\level01\level.ngn"`
- Diagnostics (`cargo run --release --example <name> -- <args>`): `rotfit`, `spikes`, `wander` (fall-through stress test), `pickups <levelNN dir>`, `overlap <level.ngn>` (coplanar overlaps by mesh set/zone), `sprites <level.ngn>` (billboard groups), `movers <levelNN dir>` (type-8 and room-map objects), `boxes` and `creatures [levelNN]` (need TS2_GAME_DIR), `stress [runs] [ticks]` (level 1 played by random input with everything on; reports fall-throughs), `gaps <levelNN dir>` (visible floor without collision; noisy: counts ceilings too).
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
| `src/creatures.rs` | Creature slots, behaviour script interpreter, movement, animation, contacts (ellipsoids from .all), hits, death, laser ray test |
| `src/laser.rs` | Laser beams: auto-aim, rays, bounce, ribbons |
| `src/hud.rs` | HUD elements 0-4, sprite table |
| `src/formats/` | `ngn`, `terrain`, `leveldat` (lists, pickups, special-object records), `packets`, `anim`, `exe`, `all` |
| `src/bin/ts2-viewer.rs` | Viewer and play mode (two-pass level draw, per-frame billboards/specials/creatures) |

## Status (what the user has confirmed)
- Play mode (F) in level 1: movement, jumps, stomp, climbing, camera, sounds: feel matches the original.
- Pickups: collections work, coins etc. look correct. Starting lives: 5.
- Earlier round's flicker/banister fixes, level objects, creatures, combat: not reported as broken.

## User feedback on the last round (2026-10-13) and what was done
1. **Push audio wrong** -> the box scrape plays every tick; the original's DirectSound has six buffers per sample (0x47DE50: a free one plays, else the one furthest through restarts). The viewer now emulates that pool for all non-voice sounds. Ask the user whether pushing sounds right now.
2. **Shooting is the main attack** -> the wrist laser is ported (left mouse button = Square): 0x434990 (arm windup 11 ticks, refire, charge whine, charged shot after 64 ticks held), 0x4A5D30 (auto-aim, 4 beam slots), 0x4A5870 (ray vs creature ellipsoids; bounces off laser-proof ones), 0x4A5C40 / 0x44E1A0 (ribbon of sprite 9). Muzzle point is approximated (90 units ahead of his chest); the original uses node 15 of the posed model. Impact particles are not drawn yet (particle system still to do).
3. **Spin should charge** -> 0x434EB0 re-ported exactly: whine 0x27 every tick from charge 13 while held, super spin (state 0x13, 181 ticks, sound 0x26 each tick) then dizzy (state 0x14, 119 ticks). The super spin was probably invisible before (no state).
4. **No HUD** -> `src/hud.rs` ports 0x49FD40 elements 0-4 (lives, batteries, coins, spin meter, laser meter) with their slide timers, drawn in a screen-space pass from the sprite table (0x557500). Not yet: token menu, pause menu, ammo counters, elements 5-9.
5. **Collision gaps** -> no repro found (examples `wander`, `stress` and `gaps` all clean or inconclusive). Added a vertical floor probe after each move (Buzz can't end a tick below a floor his body passed through). If it still happens, ask the user exactly where (room, what Buzz was doing). The real fix is porting the original resolver (0x484380 / 0x482A00 / 0x483EF0 / 0x486520, ~2000 decompiled lines) and mover riding 0x488E10.
- Creature contacts now use the real hit shapes: each character's `.all` file's type-9 directory entry (sphere + per-animation ellipsoid table), see docs/formats/all.md.

## Next steps (in priority order)
1. **Particles** (0x40FAE0 / 0x40FDF0 / 0x410F40, type table 0x4EC160, 64 slots): laser impacts and sparks, spin trails (0x14-0x17), dust; then 0x4100F0 (particles vs Buzz: coins kind 0x10, hurt kinds 1/2) for ZURG3 shots.
2. **Level 1 creatures' own routines** (+0x94): ZURG3 shooting, Tin Man boss 0x416AB0, sheep 0x416A60 (Bo Peep), RC race 0x406A60, Hamm (0x4A1CE0), Mr Potato Head (0x4A2480); then the creature parts of 0x417680.
3. Token sequence / menu (0x49FD40 with 0x830CC4 = 9), hover boots (0x4A4BB0), first-person look (L1, 0x50A13C).
4. Buzz's hurt (state 5) and death (state 7) animations (0x4011D0), laser muzzle from node 15.
5. Original collision resolver port if gaps persist; slippery surface 13; riding movers.

## Useful facts
- Units: object units = level units × 32; 12-bit angles in game code, 16-bit in `.ngn` instance rotations; +Y is down.
- Buzz's object is at 0x52F300 (`docs/re/functions.md`). Creatures: 0x52C840 + 0x9C × slot.
- `level.dat` list table: 0x559C70 + 4 × type (58 rewards/box spots, 61 poles, 63 pickups); special-object records after section 2.
- Scripts address movers by link id − 1. Room = low byte of the room-map object's flags.
- Sound table 0x502950; jingles 0x503828.
- Music is CD audio, FMVs are on the CD too: neither is in the install (ask the user for a BIN/CUE rip or the CD's tracks).
