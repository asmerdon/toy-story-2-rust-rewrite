# Toy Story 2 — Rust Rewrite

A clean-room reimplementation of *Toy Story 2: Buzz Lightyear to the Rescue!* (PC, Traveller's Tales / Activision, 1999) in Rust. It runs natively on modern Windows and other platforms, and **loads all assets from the player's own install**.

## Ground rules

- **No original data in the repo.** No `.all`/`.ngn`/`.wav`/exe bytes and no decompiled C. The engine reads the user's install at runtime (the same model as OpenMW and OpenRCT2). The game is still Disney/Activision copyright; "abandonware" is not a legal status.
- **Behaviour comes from reading the original code in Ghidra, then rewriting it in idiomatic Rust**, not from a line-by-line translation of decompiler output.
- **Every format is documented** in `docs/formats/` with evidence, and every parser has a test against a real install (`TS2_GAME_DIR`).

## What we have to work with

Install: `C:\Program Files\Disney Interactive\Toy Story 2` (180 MB).

| Source | Notes |
|---|---|
| `toy2.exe` | MSVC, 32-bit, **unpacked**, no copy protection seen. About 880 KB of `.text`. Built 1999-10-29. D3D6 / DirectDraw / DirectInput / DirectSound / DirectShow (`amstream`). |
| Source-file names in strings | `toy2.cpp`, `Win95.cpp`, `direct6.cpp`, `modesel.cpp`, `nu3d\world.c`, `objload.c`, `hobjload.c`, `objcore.c`, `fmv.c` (TT's "nu3d" engine) |
| **PS1 executables** in `data\` | `SLUS_008.93` (US), `SLES_020.67` (EU), `psx.exe` (dev build): the same game logic in MIPS. Cross-reference for decompilation. |
| Asset pipeline leftovers | `mkdat.btm`, `mkgfx.btm`, `Pconv.cfg`: PC `.ngn` files were generated from PS1 `.dat` + `.raw` by TT's `pconv.exe`. Both forms ship. |
| `creatures.cfg` | Creature ID → name → folder table (ids 0–37+) |
| `toy2.ini` | Debug key bindings, PS1 pad mapping |
| `sfx\*.wav` | ~250 standard WAVs (SFX + voice) |
| **Not in the install (on CD)** | Music (CD audio tracks; exe logs "Base CD track is %d"), FMV cutscenes, `validate.tta` (CD check) |

**Action for the owner:** make a BIN/CUE image of the game CD at some point, so music and FMV can be supported.

## Architecture (target)

```
ts2/                    one crate for now; split into a workspace once it grows
  src/formats/          pure parsers: .all .anm .ngn .raw .bin(PAD) .dat ...
  src/export.rs         debug exporters (OBJ/glTF) for checking formats in Blender
  src/bin/ts2-tool.rs   asset CLI: scan, export
  (later)
  src/render/           wgpu renderer: PS1-style vertex colour + textured polys
  src/game/             entities, player controller, creature AI, levels, menus
  src/audio/            kira/rodio: SFX, voice, CD-track music from user's CD image
  src/platform/         winit window, gilrs gamepad, config, save files (Toy2xx.sav)
```

## Phases

### Phase 0: Recon ✅ (done)
- Inventoried the install, found the PS1 executables, the build scripts and the CD dependency.

### Phase 1: Asset formats (in progress)
**Key finding:** the PC game doesn't need most of the PS1-era files. Each level's `.ngn` bundles that level's textures (BMP), geometry, mesh placements, and every creature it uses (meshes per skeleton node, smooth-joint vertices, animations). The `.ngn` format is the runtime target. `.all`/`.raw`/`.dat` are pipeline sources, useful for cross-checking.

| Format | Status |
|---|---|
| `.ngn` textures (0x104) | ✅ Named BMP files; exported as-is. |
| `.ngn` level geometry (0x100/0x101) | ✅ All 20 level files, ~400k triangles. Mesh tags, materials → textures, tri/quad/strip/fan groups, placements (translation only, verified by seam matching). Exports to OBJ+MTL+BMP. |
| `.ngn` creatures (0x106) | ✅ structure for all 146 creature records: node names, per-node meshes with textures and normals, shared smooth-joint buffers. Exports to OBJ. ❌ Shared-joint attachment (0x208). |
| Animations (`.ngn` 0x204 = `.anm`) | ✅ Decoded from the exe's sampler (Ghidra). All 845 animations parse. Model-space per-node poses, no hierarchy. `export-pose` writes posed OBJs. Visual check pending. See `docs/formats/anim.md`. |
| `.ngn` 0x102/0x103/0x105 | ❌ Small tables (paths/triggers/visibility?). |
| `.all` PS1 models | ✅ geometry for all 68 (9,556 polys), incl. smooth-shaded (per-vertex normal indices). Trailer not decoded. Now reference-only. |
| `.raw` PS1 textures | Compressed; not needed on PC. |
| `.bin` / `PAD\PATH*.BIN` | Not started. Level objects, AI paths. |
| `fmv.bin`, `level.vis`, `TERRAIN.ALL` | Not started. |
| `.sav` | Not started. 407 bytes, easy win. |

Details: `docs/formats/all.md`, `docs/formats/ngn.md`.

Exit criterion: a viewer that renders any level with its characters standing in it. Everything except animation is now in place for this.

### Phase 2: Code reverse-engineering (Ghidra), started
Setup: Ghidra 12.1.4 at `C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC`, Temurin JDK 21. The headless script `tools/ghidra/DumpAround.java` writes a function list, strings with their users, and decompiled C for all 1,317 functions to `re/out/`, which is git-ignored and must never be committed. Re-run with:
```
set JAVA_HOME=C:\Program Files\Eclipse Adoptium\<jdk-21 folder>
C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat re\proj TS2 -import re\toy2.exe -overwrite -scriptPath tools\ghidra -postScript DumpAround.java re\out
```
Known functions so far: creature loader 0x4CA420 (`hobjload.c`), animation store 0x4CA040, object draw/animate 0x4CD880, node sampler 0x4CD170/0x4CD1B0, rotate X/Y/Z 0x4A9AA0/0x4A9CB0/0x4A9EC0, mesh loaders 0x4CB320/0x4CB4E0/0x4CB970/0x4CC970 (`objload.c`), CD check 0x4A6390 (`validate.tta`), CD-audio config 0x412D70.

Remaining steps:
1. Import `toy2.exe`, apply MSVC6 runtime signatures, label functions via the source-file assert strings (`Win95.cpp Line 227` etc.).
2. Find the loaders (`objload.c`, `hobjload.c`) first; they confirm the Phase 1 formats.
3. Then: main loop and timing, player state machine, camera, collision, creature behaviours (by `creatures.cfg` id), level scripting, menus, save/load.
4. Import `SLUS_008.93` into a second Ghidra project (MIPS R3000) for comparison where x86 is unclear.
5. Write findings to `docs/re/` (function address → name → purpose).

### Phase 3: Engine
- `winit` + `wgpu` renderer reproducing PS1 look (vertex colour × texture, optional affine warp/no-filter toggles), widescreen and high resolution.
- Fixed-timestep simulation matching the original tick rate (the original's frame-timing bugs are what break it on modern PCs).
- Input: keyboard (from `toy2.ini` defaults) + gamepad.

### Phase 4: Gameplay
Player controller (Buzz move set) → one level end-to-end (Level 1, "Andy's House") → creatures by id → bosses → remaining levels → menus, saves, cutscenes, music.

### Phase 5: Polish
Settings, rebinding, glTF/HD texture override hooks, Linux/macOS builds.

## Rough effort
Phase 1: weeks. Phase 2–4: the bulk, about 6–18 months part-time for one person. The PS1 build and the shipped asset pipeline make this one of the more tractable late-90s 3D games.

## Running the tools

```
set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2
cargo test --release
cargo run --release -- scan-models "%TS2_GAME_DIR%"
cargo run --release -- export-all "%TS2_GAME_DIR%" out\models
cargo run --release -- scan-ngn "%TS2_GAME_DIR%"
cargo run --release -- export-level "%TS2_GAME_DIR%\data\level01\level.ngn" out\level01
```
`export-level` writes the level OBJ + MTL, every texture as BMP, and one `creature_<id>_<name>.obj` per creature in that level. All of it opens directly in Blender.
OBJ output (`out/`, git-ignored) imports into Blender with vertex colours (Color Attributes).
