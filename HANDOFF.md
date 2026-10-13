# Handoff: Toy Story 2 Rust rewrite

Read this first, then `PROJECT.md`, `docs/re/functions.md` and `docs/formats/*.md`.

## Setup on this machine
- Project: `C:\Users\alexs\Documents\Toy Story 2 - Rust Rewrite` (local git repo, no remote)
- Game install: `C:\Program Files\Disney Interactive\Toy Story 2` (PC, 1999-10-29 build)
- Rust: stable via rustup (`%USERPROFILE%\.cargo\bin`), MSVC build tools installed
- Ghidra 12.1.4: `C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC`; JDK 21 (Temurin) in `C:\Program Files\Eclipse Adoptium\`
- Decompiled output: `re/out/c/<address>.c` (1,317 functions), plus `re/out/strings.tsv` and `functions.tsv`. **Git-ignored and must never be committed.** Re-generate with the command in PROJECT.md (Phase 2).
- Tests: `set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2`, then `cargo test --release` (21 tests)
- Viewer: `cargo run --release --bin ts2-viewer -- "C:\Program Files\Disney Interactive\Toy Story 2\data\level01\level.ngn"`

## Rules
- No game data, exe bytes or decompiled code in the repo. Everything is read from the user's install at runtime (exe tables via `src/formats/exe.rs`).
- Write original Rust from understood behaviour. Don't paste decompiler output.
- The user tests the viewer themselves. Build, run the tests, commit, then report what to try.
- Record every identified function in `docs/re/functions.md`.

## Status
See the README table in PROJECT.md. In short:
- **Formats done:** `.ngn` (textures, level meshes, instances, creatures, patches), `.anm`/0x204 animations, `.raw` packets (LZ) and creature spawns, the `.all` PS1 models, the exe player-start table and Buzz's move table.
- **Viewer:**
  - renders levels with textures, vertex colour and the three blend modes
  - every creature appears at its spawn point and animates
  - play mode (F) starts Buzz at the original start, with the original run, jump, double-jump and gravity values and a move-table-driven animation
  - Z shows the zone-0 meshes (the first-person helmet view), which are hidden by default

## Known issues / next steps (in priority order)
Done this session: the double-jump formula (item 1), the wings (item 2: state 8, overlay anim 22), and the move-table 0xFE end code (landing played junk frames). Stomp, turning, coyote time, long fall and skid are fully decoded in `docs/re/functions.md` but **not yet implemented**.
1. **Port Buzz's controller as a fixed 60 Hz integer tick** (object units, 12-bit facing): profile → stomp → airborne (exact phase machine, coyote counter, fall timer) → turn → accel → move → coyote/fall bookkeeping, then the 0x4011D0 state selection with the 0x401000 stepper (FE → state 0). This covers the old items 3 (stomp, key E since F is play mode) and 4 (turning).
2. **Root-node offset in `scene::creature_batches_layered`**: add the base anim's root-node displacement of (0, −200, 0) to the overlay nodes (0x43BA80). The torso currently ignores the hips (up to ~100 units in the double jump).
3. **Real collision:** port the original (move-and-collide 0x434550 → 0x4855F0/0x484380, floor query 0x486280/0x4863A0, `level.dat` sections 2–3, parser 0x43E6E0) instead of the render-triangle collision in `src/game.rs`.
4. **Remaining Buzz states:** teeter (10/11), idle fidgets (0x1C/0x1D), climb (0x4354E0), swim (0x4A4BB0), sound/footstep markers.
5. **Enemies:** behaviour per creature type (spawn records have 19 undecoded parameter bytes; per-creature frame-list tables set the speeds).
6. **Deferred cosmetic items:** level instance rotation/scale fields, the `0x41` 26-byte material tail, and the `.ngn` chunks 0x102/0x103/0x105.

Tools: `ts2-tool anim-info <file.ngn> <creature>` prints every animation's node table. Buzz: nodes 0–6 legs (base anims, root 6), 7–19 upper body incl. wings 11/12 (right) and 17/18 (left), 20–22 never animated.
