# Toy Story 2 — Rust Rewrite / OpenTS2

A clean-room reimplementation of *Toy Story 2: Buzz Lightyear to the Rescue!* (PC, Traveller's Tales / Activision, 1999) in Rust, distributed to players as **OpenTS2**: an engine, launcher and installer that run on the player's own copy of the game, in the manner of OpenMW (Open Morrowind).

This file holds what doesn't change much: the ground rules, the architecture and the roadmap. Current status and next steps are in [HANDOFF.md](HANDOFF.md); players' instructions are in [README.md](README.md); agent rules are in [AGENTS.md](AGENTS.md).

## Ground rules

- **No original data in the repository.** No `.all`/`.ngn`/`.raw`/`.wav`/`.mpg`, no exe bytes and no decompiled C. The engine reads the player's install at runtime (the OpenMW / OpenRCT2 model). The game is still Disney/Activision copyright.
  - **One exception, by the owner's decision (2026-10-31):** the app icon is a render of the game's Pizza Planet token (`assets/icon/`, made by `tools/icon/make_icon.py` from the player's own install). It ships in the executables and the installer. Nothing else from the game does.
- **Behaviour comes from reading the original code** (Ghidra, a disassembler), then rewriting it as idiomatic Rust. No line-by-line translation of decompiler output. Name things by what they do and cite the exe address in comments (`0x417680`).
- **Every format is documented** in `docs/formats/` with evidence, every identified exe function in `docs/re/functions.md`, and every parser has a test against a real install (`TS2_GAME_DIR`).
- **Deviations from the original** only when the owner asks; each is marked "Deviation:" in the code and listed in the README.
- `re/` (the Ghidra project and decompiler output) is git-ignored and private. `local/` holds the owner's recordings, screenshots and notes, also git-ignored.

## Source material

| Source | Notes |
|---|---|
| `toy2.exe` | MSVC, 32-bit, unpacked, no copy protection. Built 1999-10-29. D3D6 / DirectDraw / DirectInput / DirectSound / DirectShow. Source-file names in its strings (`toy2.cpp`, `nu3d\world.c`, `objload.c`, `hobjload.c`, `fmv.c`) are TT's "nu3d" engine. The source of truth for behaviour. |
| `data\` | Levels as `.ngn` (the PC runtime format, written by TT's `pconv.exe` from the PS1 data), `.raw` packets, `.dat` point lists, `TERRAIN.ALL` collision, `sfx\*.wav`, `creatures.cfg`. Levels 11–15 are `level1.*` in folders 01–05 (`campaign.rs`). |
| PS1 executables in `data\` | `SLUS_008.93`, `SLES_020.67`, `psx.exe`: the same logic in MIPS, for cross-reference. |
| The game disc | Music (23 WAVs in `audio\`) and movies (22 MPEG-1 files named `.dll` in `rtlibs\`). See `docs/re/asset-audit.md`. |

## Architecture

```
Cargo.toml            workspace; also the engine crate `ts2`
src/                  the engine
  formats/            pure parsers: ngn, anim, terrain, leveldat, packets, exe, all, bmp
  game.rs, buzz.rs …  simulation at the original's 60 Hz with its integer maths
  level1.rs …15.rs    each level's start-up and frame script
  frontend.rs, save.rs, music.rs, fmv.rs, audio.rs, scene.rs …
  bin/ts2-viewer.rs   the game: wgpu renderer, winit window, rodio audio, input
  bin/ts2-tool.rs     asset audit and exporters
examples/             diagnostics (HANDOFF §9)
tests/                integration tests against the real install
crates/launcher/      OpenTS2 launcher (`opents2`, egui)
assets/icon/          the Pizza Planet token icon
tools/icon/           icon generator; tools/installer/: Inno Setup installer
tools/ghidra/         the headless Ghidra dump script
docs/                 formats, exe functions, coverage, OpenTS2 design
```

Rendering is wgpu + winit, sound rodio, movies `oxideav-mpeg12video` + Symphonia, the launcher egui (eframe on the same wgpu/winit).

## How we got here

1. **Recon:** inventoried the install, found the PS1 executables, the pipeline leftovers and the disc dependency.
2. **Formats:** `.ngn` (textures, geometry, instances, specials, creatures, animations, patches), `.raw` packets, `level.dat`, `TERRAIN.ALL`, `.all` hit shapes, saves.
3. **Reverse engineering:** Ghidra 12 headless dump (`tools/ghidra/DumpAround.java`) of all functions to `re/out/`; findings in `docs/re/functions.md`.
4. **Engine and gameplay:** the viewer grew into the game: Buzz's controller, collision, creatures, every level's script, bosses, HUD, front end, saves, music and movies.
5. **Playtesting:** Level 1 signed off; levels 2–15 through two rounds of the owner's reports (`docs/coverage.md`).
6. **OpenTS2 phase 1:** launcher and installer (`docs/opents2-design.md`).

## Roadmap

1. **Finish the port:** sign off levels 2–15; the missing features in HANDOFF (pause menu, options, movie viewer, credits, loading pictures, attract demo, a few gadgets).
2. **OpenTS2 0.1 release:** the installer as built now, once the port is signed off.
3. **Modding (OpenTS2 phase 3, after the port settles):** a layered virtual file system (mods over the game's data), asset overrides, playing as other creatures, scripted behaviour. Design in `docs/opents2-design.md`.
4. **Level editor (phase 2):** `opents2-editor`, a free camera for placing objects, creatures, pickups and paths, saved as mod files.
5. **Other platforms:** Linux (Flatpak) and macOS packages; the engine's default game path must stop assuming `C:\`.

## Machine setup (the owner's PC)

| What | Where |
|---|---|
| Project | `C:\Users\alexs\Documents\Toy Story 2 - Rust Rewrite` |
| Game install | `C:\Program Files\Disney Interactive\Toy Story 2` |
| Disc image | `C:\Users\alexs\Downloads\Disney-Pixar-Toy-Story-2-Buzz-Lightyear-to-the-Rescue_Win_EN_ISO-Version\ToyStory2.iso` |
| Rust | stable via rustup, MSVC build tools |
| Ghidra | 12.1.4 at `C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC`, JDK 21 at `C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot` |
| Inno Setup | 6, per-user (`winget install JRSoftware.InnoSetup`) |
| 7-Zip | `C:\Program Files\7-Zip\7z.exe` (lists or extracts the ISO) |

Re-run the Ghidra dump on the existing project (about 10 minutes):

```bash
set JAVA_HOME=C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot
C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat re\proj TS2 -process toy2.exe -noanalysis -scriptPath tools\ghidra -postScript DumpAround.java re\out
```

For a fresh import, replace `-process toy2.exe -noanalysis` with `-import re\toy2.exe -overwrite`.
