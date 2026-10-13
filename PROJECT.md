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
| Format | Status |
|---|---|
| `.all` character models | **Geometry parsed for all 68 models (8,435 polygons).** 34/68 parse cleanly to the geometry-end marker. Exports to OBJ with vertex colours. Open: trailer section, smooth-skinned mesh block (Buzz, Bo Peep, Emperor Zurg), texture tags. See `docs/formats/all.md`. |
| `.anm` animations | Not started. Probably per-part transforms keyed to `.all` parts. |
| `.raw` textures | Not started. PS1 VRAM-style texture pages + CLUTs (4/8-bit). |
| `.ngn` levels (PC) | Not started. Largest task. Compare with `.dat` + `.raw` (PS1 source) for the same level. |
| `.bin` / `PAD\PATH*.BIN` | Not started. Level objects, AI paths. |
| `fmv.bin`, `level.vis`, `TERRAIN.ALL` | Not started. |
| `.sav` | Not started. 407 bytes, easy win. |

Exit criterion: a viewer that renders any level with its characters standing in it.

### Phase 2: Code reverse-engineering (Ghidra)
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
```
OBJ output (`out/`, git-ignored) imports into Blender with vertex colours (Color Attributes).
