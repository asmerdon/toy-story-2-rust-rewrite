# OpenTS2 design

OpenTS2 distributes the Rust rewrite as an open engine for Toy Story 2 that runs on the
player's own copy of the game, following OpenMW (Open Morrowind). This document records what
we take from OpenMW, what we simplify, and the data model the later phases (editor, mods)
build on. Phase 1 (launcher and installer) is built; the rest is the plan.

## 1. What OpenMW does, and what we take

Sources: [file paths and config layering](https://openmw.readthedocs.io/en/latest/reference/modding/paths.html),
[installing mods](https://openmw.readthedocs.io/en/latest/reference/modding/mod-install.html),
[installing the game files / the wizard](https://openmw.readthedocs.io/en/latest/manuals/installation/install-game-files.html),
[downloads and packages](https://openmw.org/downloads/),
[source](https://gitlab.com/OpenMW/openmw).

| OpenMW | What it does | OpenTS2 |
|---|---|---|
| Separate programs: `openmw` (engine), `openmw-launcher`, `openmw-cs` (editor), `openmw-wizard` | Each tool does one job; the launcher just writes config and starts the engine | **Copy.** `ts2-viewer` (engine), `opents2` (launcher), later `opents2-editor`. The wizard's job is small for us (find a folder), so it is the launcher's *Game files* tab rather than a separate program. |
| The wizard finds an existing Morrowind install (looks for `Morrowind.esm`) or installs from the retail CD, then imports `Morrowind.ini` | Never redistributes data; records where it is | **Copy, simplified.** Auto-detect the default install paths, browse, validate (`toy2.exe` and key `.ngn` files), and accept the disc for music and movies: a drive, a copied folder, or a `.iso`, whose `audio\` and `rtlibs\` the launcher copies once into `%APPDATA%\OpenTS2\disc` (the engine reads loose files; the VFS could later read the image in place). No install-from-CD (the retail installer does that) and nothing to import: the engine reads the exe's own tables. |
| `openmw.cfg`: `data=` lines are search paths; later directories override earlier ones (a virtual file system); `content=` lines are plugins in load order | Mods are layered, never written into the game | **Copy the model.** A layered file system over the game folder with mod folders on top, in the order the player picks (§3). One list instead of two: our mods are folders, not separate plugin files inside folders. |
| Config layering: local / global / user `openmw.cfg`, command line highest; multi-value settings merge, single values take the highest priority; the launcher writes only the user file | Defaults are never edited by the player | **Simplify.** Defaults live in code; one per-user `opents2.toml` holds only what the player chose; the launcher passes paths through the engine's command line and environment, which win. No global/local layers until something needs them. |
| `data-local`: where the editor writes, always loaded last | The editor never writes into the game or other mods | **Copy.** The editor saves into a mod folder of its own (§4). |
| Launcher *Data Files* tab: content lists, check and drag to order, dependency warnings | Load order and conflicts are visible | **Copy, simplified.** *Mods* tab: check to enable, ▲/▼ to order, warnings when a required mod is missing or later. Named content lists (profiles) can come later. |
| Packages: Windows installer, macOS DMG, Linux Flatpak/distro packages; the VC++ redistributable is a prerequisite on Windows | | **Copy the spread** later (§6). We link the C runtime statically, so no prerequisite. |

What we leave out for now: the INI importer, the global config layer, archives (`fallback-archive=`/BSA; our mods are loose files), and profiles.

## 2. Pieces and repository layout

```
Cargo.toml              workspace root; also the engine crate `ts2` (unchanged for now)
src/                    the engine: lib + bins ts2-viewer, ts2-tool
crates/launcher/        opents2 (egui), lib: paths, install, config, mods, launch
assets/icon/            the icon: the game's Pizza Planet token (flat, rotated) made by tools/icon/make_icon.py
                        from examples/token_model.rs's export (the one owner-approved game asset)
tools/installer/        Inno Setup script + build.ps1
```

Planned split, done when each piece needs it (not now; the engine crate is large and its
tests depend on its layout):

```
crates/engine     today's src/ (formats, game, rendering, the viewer)   — move when the editor needs it as a library
crates/vfs        the layered file system (§3)                          — the first modding step
crates/modding    manifests, load order, resolution (from launcher/src/mods.rs)
crates/editor     opents2-editor: free camera + placement, saves mod files
crates/launcher   opents2
```

The launcher depends on `ts2` only for `campaign::LEVELS` (level names). It shares wgpu and
winit versions with the engine, so the two don't compile duplicate graphics stacks.

### Why egui/eframe

eframe 0.31 runs on wgpu 24 and winit 0.30, the versions the engine already uses, so it adds
egui itself and little else (checked with `cargo tree -d`). It is pure Rust, builds with
the same toolchain, and the same widgets suit the future editor's panels. The alternatives
were Slint (heavier, its own language), Iced (another renderer stack) or Win32 dialogs (not
portable). Folder pickers use `rfd` (native dialogs); settings use `serde` + `toml`.

### How the launcher starts the engine

It doesn't change how the engine finds data. It runs `ts2-viewer` (installed beside it) with:

| Route | Command line | Environment |
|---|---|---|
| Play original | `ts2-viewer <game>` | `TS2_GAME_DIR=<game>`, `TS2_CD_DIR=<disc media>` if set |
| Play modded | `ts2-viewer <game>` | as above, plus `OPENTS2_MODS=<mod dirs in load order>` (`;`-separated), which the engine ignores until the VFS exists |
| Level tester | `ts2-viewer --level N <game>` | as above |

The working directory is the OpenTS2 folder, so a stray `ts2-media.txt` elsewhere has no
effect. The engine's output goes to `%APPDATA%\OpenTS2\last-run.log`; the launcher shows when
the game stops with an error.

## 3. Data model: the layered file system

Every engine file read goes through one function, `vfs.open("data/level04/level.ngn")`,
that looks in the layers from the top down:

```
1. editor output       mods/<editor mod>/...     (when the editor is running on it)
2. mod N               mods/<id N>/...           (last in the load order wins)
   ...
3. mod 1               mods/<id 1>/...
4. the game            <game dir>/...            (read only, never written)
5. the disc            <disc dir>/audio, rtlibs  (music and movies)
```

- Paths are the game's own relative paths (`data\level04\level.ngn`, `sfx\buzz01.wav`),
  matched case-insensitively, as on the original's Windows file system.
- A mod overrides a whole file. Finer-grained changes (one texture inside an `.ngn`, one
  creature placement) come from typed override files (below), which the engine merges after
  loading the base file. Whole `.ngn` replacements stay possible.
- `toy2.exe` is never overridden: its tables are part of the game logic we port. Mods that
  change behaviour do it through scripts (phase 3), not by patching tables.
- Conflicts are reported, not errors: the launcher can list, per file, which mods provide it
  and which one wins.

Engine change needed (phase 3, small): route the existing `fs::read(game.join(...))` calls
through the VFS. They start from `campaign` paths, `music::media_root`, the viewer's `load`
and the sound loading; inventory them with a search for `fs::read` before starting.

### Typed overrides inside a mod

```
mods/<id>/
  mod.toml                         manifest
  data/levelNN/...                 whole-file overrides (same path as the game)
  textures/levelNN/<name>.png      replaces the .ngn texture of that name (any size; scaled UVs)
  models/<creature>.gltf           replaces a creature's meshes (same node names)
  sounds/<id>.wav                  replaces a sound effect or voice
  scenes/levelNN.toml              editor changes for that level (§4)
  scripts/*.rhai                   behaviour changes (phase 3; language to decide)
```

### The manifest, `mod.toml`

```toml
id = "hd-buzz"                  # unique, folder-name safe
name = "HD Buzz"
version = "1.0.0"
authors = ["someone"]
description = "Sharper textures for Buzz."
requires = ["base-fixes"]       # must be enabled and load earlier
engine = "0.1"                  # OpenTS2 version it was made for
# Phase 3 additions: conflicts = [...], optional `play_as = "WOODY"` etc.
```

The launcher reads this today (`crates/launcher/src/mods.rs`) and warns about missing or
mis-ordered requirements. Which files a mod overrides is not declared: it is whatever the mod
folder contains, which the launcher can list.

### Load order in the settings

`%APPDATA%\OpenTS2\opents2.toml` keeps the order and the enabled flags:

```toml
[[mods]]
id = "base-fixes"
enabled = true
[[mods]]
id = "hd-buzz"
enabled = true
```

New mods found on disk join the end, disabled; mods removed from disk drop out.

## 4. Where the editor saves

The free-camera editor (phase 2) never writes into the game folder or into someone else's
mod. It edits one mod at a time (creating `mods/<id>/` if needed) and saves per-level scene
files:

```toml
# mods/my-level-tweaks/scenes/level04.toml
[[creature]]                    # an added creature
type = "REX"                    # creatures.cfg name
pos = [12852, 0, 959]           # level units
facing = 277                    # degrees
[[move]]                        # an existing spawn moved
spawn = 17                      # index in level.raw's spawn list
pos = [13000, 0, 1000]
[[remove]]
pickup = 42                     # index in the level's pickup list
[[path]]                        # a replaced point list (level.dat type)
kind = 12
points = [[0, 0, 0], [100, 0, 0]]
```

The engine applies these after loading the level's original lists, so they survive game
updates and combine with other mods in load order. The editor runs as its own program
(`opents2-editor`, launched from the launcher) built on the engine library, like OpenMW-CS.

## 5. Phase 3 modding notes

- **Asset overrides:** the VFS plus the typed overrides above.
- **Playing as other creatures:** the player controller is tied to Buzz's move table
  (0x401000); another creature needs a mapping from Buzz's states to that creature's
  animations, given in the mod (`play_as` + an animation map).
- **Animations from other Rust rewrites:** an importer that retargets their clips to our
  model-space per-node poses (docs/formats/anim.md), saved as mod files, never bundled.
- **Scripted behaviour:** an embedded script language over a narrow API (spawn, move, speak,
  give); candidates Rhai or Lua (mlua). Decide when the first real mod needs it.

## 6. Packaging

**Windows (built):** Inno Setup 6 (`tools/installer/opents2.iss`, built by
`tools/installer/build.ps1`). Chosen over WiX/cargo-wix and NSIS because a per-user install
with no admin rights is one setting (`PrivilegesRequired=lowest`, with an all-users option),
and the licence page, Start-menu and optional desktop shortcuts and the uninstaller come
without custom actions. A per-user MSI with shortcuts needs registry key paths and per-user
workarounds in WiX. Inno produces a single `.exe`, which can be code-signed later.

- Installs `opents2.exe`, `ts2-viewer.exe` and a README (the disclaimer) to
  `%LOCALAPPDATA%\Programs\OpenTS2`.
- Uninstall removes those and the shortcuts, plus `last-run.log`; settings, mods and saves stay.
- The executables link the C runtime statically (`.cargo/config.toml`), so there is no
  VC++ redistributable to install.
- Not signed yet: SmartScreen will warn on first run until it is.

**Later:**
- **Linux:** a Flatpak (as OpenMW is on Flathub) and an AppImage or tarball. The launcher
  already uses XDG paths. Needs the engine's default game path to stop assuming `C:\`.
- **macOS:** a `.app` bundle in a DMG (`cargo-bundle`), icon from `opents2-1024.png` as `.icns`;
  needs signing and notarisation to open without warnings.
- On both, the game's data is copied off the disc or a Windows install by the player; the
  wizard step would need to accept a folder copied from elsewhere (it already does).
