# Handoff: Toy Story 2 Rust rewrite

Read this first, then `PROJECT.md`, `docs/re/functions.md` and `docs/formats/*.md`.

## Setup on this machine
- Project: `C:\Users\alexs\Documents\Toy Story 2 - Rust Rewrite` (local git repo, no remote)
- Game install: `C:\Program Files\Disney Interactive\Toy Story 2` (PC, 1999-10-29 build)
- Rust: stable via rustup (`%USERPROFILE%\.cargo\bin`), MSVC build tools installed
- Ghidra 12.1.4: `C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC`; JDK 21 (Temurin) in `C:\Program Files\Eclipse Adoptium\`
- Decompiled output: `re/out/c/<address>.c` (1,317 functions), plus `re/out/strings.tsv` and `functions.tsv`. **Git-ignored and must never be committed.** Re-generate with the command in PROJECT.md (Phase 2).
- Tests: `set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2`, then `cargo test --release` (27 tests)
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
Done: collision from the original `TERRAIN.ALL` (K shows it); double-jump formula and wings; Buzz's controller as a 60 Hz integer port (stomp on E, turning and skid, coyote time, long fall); the 0x401000 frame-list stepper (0xFE ends a list); the base root-node offset for overlay animations (0x43BA80).
1. **Collision resolve:** the original collision data (`TERRAIN.ALL`, docs/formats/terrain.md) is now loaded and used; the resolve is still ours (floor ray + wall sphere). Port 0x484380 → 0x482A00 / 0x483EF0 next, then the type-8 movers (platforms/doors driven by level objects, `level.dat` section 3 parsed by 0x43E6E0).
2. **Remaining Buzz states:** teeter (10/11, needs the floor probe 0x486280 and a resolve that lets Buzz stand over an edge), climb (0x4354E0), swim (0x4A4BB0), sounds (0x49E660 ids are noted in comments; `sfx\*.wav` mapping via the table at 0x502950), stomp dust and camera shake. The original's camera (0x403450 and friends) isn't ported either.
3. **Enemies:** behaviour per creature type (spawn records have 19 undecoded parameter bytes; per-creature frame-list tables set the speeds).
4. **Deferred cosmetic items:** level instance rotation/scale fields, the `0x41` 26-byte material tail, and the `.ngn` chunks 0x102/0x103/0x105.

Tools: `ts2-tool anim-info <file.ngn> <creature>` prints every animation's node table. Buzz: nodes 0–6 legs (base anims, root 6), 7–19 upper body incl. wings 11/12 (right) and 17/18 (left), 20–22 never animated.
