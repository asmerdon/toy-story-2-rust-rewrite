# Handoff: Toy Story 2 Rust rewrite

Read this first, then `PROJECT.md`, `docs/re/functions.md` and `docs/formats/*.md` (`terrain.md`, `ngn.md`, `anim.md`, `packets.md`, `all.md`).

## Setup on this machine
- Project: `C:\Users\alexs\Documents\Toy Story 2 - Rust Rewrite` (local git repo, no remote)
- Game install: `C:\Program Files\Disney Interactive\Toy Story 2` (PC, 1999-10-29 build)
- Rust: stable via rustup, MSVC build tools. Audio output uses `rodio` 0.19 (no default features).
- Ghidra 12.1.4: `C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC`; JDK 21 in `C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot`
- Decompiled output: `re/out/c/<address>.c` (**2,811 functions**), `re/out/functions.tsv`, `re/out/strings.tsv`. **Git-ignored; never commit.** `tools/ghidra/DumpAround.java` now creates functions in code that auto-analysis missed (code reached through jump tables; it had skipped about half the exe). Re-run on the existing project (about 10 min):
  ```
  set JAVA_HOME=C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot
  C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat re\proj TS2 -process toy2.exe -noanalysis -scriptPath tools\ghidra -postScript DumpAround.java re\out
  ```
- Disassembly helper (scratch, not in the repo): a Python venv with `capstone` + `pefile` was used to read code the decompiler hides (`__ftol` float maths, call arguments) and exe data tables. Recreate it in a scratch folder if needed.
- Tests: `set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2`, then `cargo test --release` (13 test binaries, all passing). `cargo clippy --all-targets` fails only on an old `6.283` in `examples/wander.rs`.
- Viewer: `cargo run --release --bin ts2-viewer -- "C:\Program Files\Disney Interactive\Toy Story 2\data\level01\level.ngn"`
- Diagnostics (`cargo run --release --example <name> -- <args>`): `rotfit` (instance rotation vs collision), `spikes` (long thin triangles per primitive type), `wander` (random-walk fall-through stress test from every spawn), `pickups <levelNN dir>` (list 63, sprite table, special objects with their level.dat records; set TS2_GAME_DIR for the exe parts).
- Disassembly: `re/out` hides call arguments and jump tables; a scratch Python venv with `capstone` + `pefile` (disassemble a range, hex-dump exe data, scan for references to an address) fills the gap.

## Rules
- No game data, exe bytes or decompiled code in the repo. Everything is read from the user's install at runtime (exe tables via `src/formats/exe.rs`).
- Write original Rust from understood behaviour. Don't paste decompiler output.
- The user tests the viewer themselves (don't launch it). Build, run the tests, commit, then report what to try.
- Record every identified function in `docs/re/functions.md`.
- The user prefers long autonomous runs: do as much as possible before reporting; don't ask permission for harmless commands.
- Write files with LF endings. In bash heredocs, Python strings with backslashes get mangled; write patch scripts to a scratch file with the Write tool instead.

## Code map
| File | What |
|---|---|
| `src/game.rs` | `World` (swept-sphere collision against TERRAIN.ALL triangles) and `Player` (Buzz's controller as a 60 Hz integer port: profile → stomp → climb → airborne → turn → accelerate → move/collide → coyote/fall timer → anim state) |
| `src/buzz.rs` | Move-table animation player (0x401000 stepper), state numbers |
| `src/camera.rs` | Original follow camera (0x4045E0 main path) |
| `src/audio.rs` | Sound ids → samples per level, positional volume/pan (0x49E660 / 0x4A3810 / 0x47DE50) |
| `src/scene.rs` | Level/creature batches; instance rotation + scale; overlay root offset; sprites |
| `src/pickups.rs` | Coins, batteries, tokens, lives, power-ups: setup 0x447DB0, draw/spin 0x440F70, collect 0x4A0F80, token setup 0x4A0C80/0x4A0DB0 (level 1 only) |
| `src/formats/` | `ngn` (incl. 0x105 specials, sprite quads), `terrain` (TERRAIN.ALL), `leveldat` (level.dat lists, poles), `packets` (.raw), `anim`, `exe` (exe tables), `all` |
| `src/bin/ts2-viewer.rs` | Viewer and play mode |

## Status
- **Play mode (F)**, level 1 tested by the user: movement, jumps, double jump with wings, stomp (E), skid, long fall, teeter, fidgets and idle voice lines, climbing poles/ropes (W/S/A/D, Space), original camera (V toggles the old free one), sound effects, collision from the original data (K shows it). The user says the feel matches the original.
- **Rendering**: instances rotated and scaled; sprite quads (codes 5/6: pivot + 4 corner offsets) for rails, spindles, cobwebs.

## Not yet confirmed by the user
From the run before last:
- Sprite corners are offsets from their pivot (bed rails and banister spindles were floating/hanging).
- Positional audio scale (the camera matrix is 4.12): sounds panned and faded wildly before.
- Collision: faces catch starts up to 8 units inside (fall-through). The stress test never reproduced a fall-through, so if the user still falls through, **ask where** (a position or screenshot).
- Camera: a final line-of-sight check keeps it in front of walls in tight spaces.

From the last run (pickups, see `docs/re/functions.md` → Pickups):
- 70 spinning coin sprites with shadows, 6 batteries, 3 extra lives, power-ups and tokens, collected with the original's radius and the PICKUP5 jingle; counts in the window title and console.
- Level 1 starts with only token id 60 collectable (as the original: the others come from challenges, which aren't ported). The 50th coin plays 50COINS.
- Things to ask about: coin size (200 units across, from the sprite routine's ±size corners), sprite orientation (texture upright?), shadow look, spin speed.
- Not ported: the token menu ("keep on playing / exit level"), hover boots behaviour, cutscene triggers (6-poly objects stay and spin), damage (health only goes up), token sparkles/grow animation, starting lives (3 is a guess).

## Next steps (in priority order)
1. **Level 1 polish** (the user wants level 1 right first, to learn the process for the rest):
   - Any remaining visual issues the user reports (ask for screenshots).
   - A sheep spawn (17670, 3125, −18803) floats 250 units above any floor: creatures probably fall or stand on objects in the original; check the sheep's code when doing creatures.
   - Glass pots render translucent (material mode 2); probably correct, confirm with the user.
2. **Pickups follow-ups**: hover boots (0x4A4BB0, timer 0x882920, Buzz state 0x1F, sound 0x50), the token sequence (0x4A0F80 tail, states in `0x830CC4`; menu in 0x49FD40), token setup for other levels (their 0x4A1A50 targets call 0x4A0C80 with a list address and reward start), a HUD (sprites 0x493F40 draw the counters; digits are sprite 0x1A+).
3. **Enemies**: behaviour per creature type (spawn records have 19 undecoded parameter bytes). Start with the level 1 cast (ZURG1/ZURG3 robots, sheep, Bo Peep, Hamm, Rex, Mr Potato Head, RC, Tin Man, COTBIT).
4. **Doors and moving platforms (level scripts)**: type-8 movers and their instances sit in their start state. Per-level scripts drive them (level 1: 0x417680, via the per-level switch 0x4A0F80). Known pieces:
   - Current room `0x54DEA0` = low byte of the collision flags under Buzz (0x4402B0 via 0x4885C0); room adjacency from `level.dat` lists ≥ 0x41 (0x43E6E0).
   - Special objects (`.ngn` 0x105, parsed in `Ngn::specials`): position 0x4CCE30, rotation 0x4CCC70, scale 0x4CCB20.
   - Movers: collision on/off 0x4878A0 / 0x4878D0, velocity 0x487900, spin 0x487970, applied each frame by 0x488E10 (which also carries Buzz).
   - Also: the resolver's slope push and riding movers.
5. **Particles** (spawner 0x40FDF0): footstep puffs/splashes by surface (0x4A2D80, from frame-list markers 0x81/0x82; there are no footstep sounds), stomp dust. Camera extras: wall whiskers, manual mode (L2/R2/Triangle), stomp shake 0x50A510.
6. **Cosmetic**: `.ngn` 0x102/0x103 (0x103 links pairs to 0x102 entries; texture or UV animation?), the 0x41 26-byte material tail, sprite offsets in the OBJ exporter. Music is CD audio (not in the install).

## Useful facts
- Units: object units = level units × 32; 12-bit angles (4096 = 360°) in game code, 16-bit in `.ngn` instance rotations; +Y is down.
- Buzz's object is at 0x52F300 (`docs/re/functions.md` lists its fields and globals).
- `level.dat` list table: 0x559C70 + 4 × type (58 rewards, 61 poles, 63 four-value lists).
- Sound table 0x502950 (16 bytes per id); sample slots from name lists at 0x4FD140 / 0x4FD5E0.
- Buzz's animation nodes: 0–6 legs (base anims, root 6), 7–19 upper body incl. wings 11/12 and 17/18, 20–22 never animated. `ts2-tool anim-info <file.ngn> <creature>` prints node tables.
