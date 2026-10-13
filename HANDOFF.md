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
1. **Jump feel is off.** The user says the double jump is too weak relative to the first. The likely cause is in `src/game.rs`: when Buzz is above the first jump's start height (almost always true at the apex), we use the *minimum* clamp 0x22A instead of the original's computed value. Decode the float expression in 0x4340D0, in the `__ftol` branch near the double jump. Also check how the jump phase interacts with the hold, release and second press.
2. **Wings don't animate.** In the original, the double jump (and maybe falling) deploys Buzz's wings. Find how: probably a separate node, overlay animation or flag, set by an event (`FUN_0049e660(0x10, …)` fires on the double jump). Check move-table state 5 (anim 6) and which nodes it covers.
3. **Stomp** (0x434D20: Circle in the air, hang, slam at 0x800, states 0x17/0x18) is decoded but not implemented.
4. **Turning:** the original turns at a limited rate (0x4346C0, using the profile's turn-rate field). We currently snap to face the stick.
5. **Real collision:** port the original (move-and-collide 0x434550, floor query 0x486280/0x4863A0, `level.dat` sections 2–3, parser 0x43E6E0) instead of the render-triangle collision in `src/game.rs`.
6. **Remaining Buzz states:** teeter (10/11), idle fidgets (0x1C/0x1D), climb (0x4354E0), swim (0x4A4BB0), and the animation event markers (≥ 0x80 in frame lists).
7. **Enemies:** behaviour per creature type (spawn records have 19 undecoded parameter bytes; per-creature frame-list tables set the speeds).
8. **Deferred cosmetic items:** level instance rotation/scale fields (plant leaves, a floating lamp, car wheels look wrong), the `0x41` 26-byte material tail, and the `.ngn` chunks 0x102/0x103/0x105.
