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

## User feedback, round 3 (2026-10-14), and what was done
1. **Pickup sound plays several times; push scrape awful.** Scrape: sounds with bit 15 in their slot word are owned loops (0x4A3980): now one looping buffer per (slot, owner), stopped 16 ticks after the owner last played it. Pickup: the exe's jingle −1 plays PICKUP5 six times (PS1 pitches unused on PC) yet the user hears it once in the running game; now only its first note plays (deviation, noted in the viewer). Ask again.
2. **Stuck pushing the attic box (box 6, path with a 90° corner).** The original lets go when the stick points more than 90° from the push (tail of 0x4346C0); ported, plus the grab compares the facing, and facing is held to the push angle. Test `attic_box_goes_round_its_corner`.
3. **Charging gun/spin doesn't work.** Most likely the meters were invisible (item 5) and the whine/whirr were broken by the buffer pool; both fixed. Logic is unit-tested. Ask the user to check again.
4. **W/S spam snaps with the skid sound.** The original's turn code snaps and skids on every reversal too, but the keyboard goes through a d-pad table (0x433F40): W+S held together = forward, not "no input"; ported (`game::dpad_heading`, camera-to-Buzz angle). Skid now also needs no laser. If still different, ask what the original does exactly (does Buzz turn smoothly? no sound?).
5. **HUD bars missing.** Screen sprites draw last-queued first (0x4B8DD0); the dark meter frames were covering the bars. Reversed.
6. **Laser thickness wrong.** Ribbon UVs were transposed: the sprite's 31-px axis runs along the beam, the 8-px axis across (0x4B9100). Fixed, with cell 2 at the muzzle end. Position was confirmed good.

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
