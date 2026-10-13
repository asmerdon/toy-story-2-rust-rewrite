# Handoff: Toy Story 2 Rust rewrite

Read this first, then `PROJECT.md`, `docs/re/functions.md` and `docs/formats/*.md`.

## Setup on this machine
- Project: `C:\Users\alexs\Documents\Toy Story 2 - Rust Rewrite` (local git repo, no remote)
- Game install: `C:\Program Files\Disney Interactive\Toy Story 2` (PC, 1999-10-29 build)
- Rust: stable via rustup (`%USERPROFILE%\.cargo\bin`), MSVC build tools installed
- Ghidra 12.1.4: `C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC`; JDK 21 (Temurin) in `C:\Program Files\Eclipse Adoptium\`
- Decompiled output: `re/out/c/<address>.c` (1,317 functions), plus `re/out/strings.tsv` and `functions.tsv`. **Git-ignored and must never be committed.** Re-generate with the command in PROJECT.md (Phase 2).
- Tests: `set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2`, then `cargo test --release` (39 tests)
- Viewer: `cargo run --release --bin ts2-viewer -- "C:\Program Files\Disney Interactive\Toy Story 2\data\level01\level.ngn"`

## Rules
- No game data, exe bytes or decompiled code in the repo. Everything is read from the user's install at runtime (exe tables via `src/formats/exe.rs`).
- Write original Rust from understood behaviour. Don't paste decompiler output.
- The user tests the viewer themselves. Build, run the tests, commit, then report what to try.
- Record every identified function in `docs/re/functions.md`.

## Status
See the README table in PROJECT.md. In short:
- **Formats done:** `TERRAIN.ALL` collision, `.ngn` (textures, level meshes, instances, creatures, patches), `.anm`/0x204 animations, `.raw` packets (LZ) and creature spawns, the `.all` PS1 models, the exe player-start table and Buzz's move table.
- **Viewer:**
  - renders levels with textures, vertex colour and the three blend modes
  - every creature appears at its spawn point and animates
  - play mode (F) starts Buzz at the original start; his controller is a 60 Hz port of the original (run, turn, skid, jump, double jump with wings, stomp on E, long fall), animated from the move table
  - Z shows the zone-0 meshes (the first-person helmet view), which are hidden by default

## Known issues / next steps (in priority order)
Done: climbing poles and ropes (level.dat list 61; W/S climb and slide, A/D turn, Space jumps off); idle voice lines; sprite quads (prims 5/6) without their pivot (fixed the spiky rails/cobwebs); level instances now rotated and scaled (fixes the plant leaves/lamp/wheels and doors drawn lying down); sound effects (src/audio.rs, the original positional volume/pan); swept-sphere collision after the original resolver, and teeter; collision from the original `TERRAIN.ALL` (K shows it); double-jump formula and wings; Buzz's controller as a 60 Hz integer port (stomp on E, turning and skid, coyote time, long fall); the 0x401000 frame-list stepper (0xFE ends a list); the base root-node offset for overlay animations (0x43BA80).
1. **Doors and moving platforms (level scripts):** collision movers (type 8) and their instances sit in their start state, so closed doors stay closed. They're opened by per-level hand-written scripts, not data. Level 1's is 0x417680 (called through the per-level switch 0x4A0F80). The pieces are known:
   - **Rooms:** the current room `0x54DEA0` is the low byte of the collision flags under Buzz (0x4402B0 via 0x4885C0), with a room-adjacency table built from `level.dat` type ≥ 0x41 lists (0x43E6E0). Level 1 opens a pair of swinging doors (objects at 0x52C840 / 0x52C978) while Buzz is in room 1, and switches mover 9's collision to match.
   - **Special objects** (`.ngn` 0x105, parsed): scripts move instances by id (position 0x4CCE30, rotation 0x4CCC70, scale 0x4CCB20). E.g. special 4 (instance 38) tilts open by 45° under a condition on 0x53C70E/0x53C70C (not yet identified) while movers 14/1 swap.
   - **Movers:** collision on/off 0x4878A0/0x4878D0, velocity 0x487900, spin 0x487970, applied by 0x488E10 (which also carries Buzz).
   Next: find which door the user means (ask for its location), port that bit of level 1's script plus mover/special motion, then riding movers and the resolver's slope push.
2. **Remaining Buzz states and effects:** swim (0x4A4BB0), the original camera (0x403450 and friends). **Particles** (spawner 0x40FDF0): footstep dust/splashes (0x4A2D80 turns frame-list markers 0x81/0x82 into surface-dependent puffs; there are no footstep sounds), stomp dust, and the stomp camera shake. Music is CD audio (not in the install).
3. **Enemies:** behaviour per creature type (spawn records have 19 undecoded parameter bytes; per-creature frame-list tables set the speeds).
4. **Deferred cosmetic items:** the `0x41` 26-byte material tail, and the `.ngn` chunks 0x102/0x103 (0x103 links pairs to 0x102 entries; texture or UV animation?).

Tools: `ts2-tool anim-info <file.ngn> <creature>` prints every animation's node table. Buzz: nodes 0–6 legs (base anims, root 6), 7–19 upper body incl. wings 11/12 (right) and 17/18 (left), 20–22 never animated.
