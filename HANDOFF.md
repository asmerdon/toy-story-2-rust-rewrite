# Handover: Toy Story 2 Rust rewrite

Last updated 2026-10-31. This file is the resume point: current state first, then the open
issues and next direction, then reference sections (§1–§10). Dated history lives in
[MODLOG.md](MODLOG.md) and git. Per-interaction test coverage lives in
[docs/coverage.md](docs/coverage.md); keep it there, not here.

Read [AGENTS.md](AGENTS.md) (delegation and evidence rules) and [PROJECT.md](PROJECT.md)
(ground rules, architecture, roadmap, machine setup). Then `docs/re/functions.md` (identified
exe functions) and `docs/formats/*.md`.

**Where things are documented** (one place each):

| Doc | Holds |
|---|---|
| [README.md](README.md) | For players and newcomers: installing OpenTS2, playing, controls, what's in, deviations, building |
| [PROJECT.md](PROJECT.md) | Ground rules, source material, architecture, roadmap, machine setup |
| HANDOFF.md (this file) | Current state, open issues, working knowledge for agents |
| [MODLOG.md](MODLOG.md) | Dated change history |
| [AGENTS.md](AGENTS.md), [CLAUDE.md](CLAUDE.md) | Agent working rules |
| [docs/coverage.md](docs/coverage.md) | Per-level interaction checklist (sim / shot / user) |
| [docs/opents2-design.md](docs/opents2-design.md) | OpenTS2: OpenMW research, launcher, installer, mod and editor data model |
| [docs/formats/](docs/formats/) | File formats |
| [docs/re/functions.md](docs/re/functions.md), [docs/re/asset-audit.md](docs/re/asset-audit.md) | Identified exe functions; the install and disc audit |

---

## Current state (2026-10-31)

- **Campaign:** all 15 levels and the front end are ported and playable in `ts2-viewer`.
- **Level 1:** signed off by the user.
- **Levels 2–15:** the user played through them in two rounds of reports; every reproduced report has been fixed (see the list below).
- **Not done:** no level other than 1 has a full sign-off.
- **Tests:** 222 pass with `TS2_GAME_DIR` and `TS2_DISC_IMAGE` set (211 engine + 11 launcher), 2026-11-03. `cargo build --release` builds the viewer, tool and launcher without warnings.
- **Git:** work is on `dev` (local `master` tracks `origin/dev`). The user asks before each push.

### Fixed from the user's playtests
Every fix below comes from the original's code, has a regression test, and was mostly checked with offscreen shots. The user has only re-played the first round's fixes.

**Collision and movement**
- Boundary walls: TERRAIN kind 0x101 lists (0x484380 / 0x48C860). These close the final arena's back and the airport's conveyor ends. See docs/formats/terrain.md.
- Belt pushes are swept (0x48B660).
- Fall plane under the lowest collision point (0x436220).
- Floor types 4-7 (level 13's oil) have little grip (0x436220): side slowing 0x10, friction 8, acceleration 0x14. Type 0xD slides (0x53C818; levels 11 and 15), state 0x20; the slide ends on landing on another floor (the exe asks its coyote counter, here a tick of contact).
- i64 maths in creature contact and the laser ray. Large levels overflowed i32, which caused Level 14's constant damage.
- Start-floor snap (0x4863A0): Level 13 starts on the X-ray conveyor.

**Drawing**
- Specials are drawn by their own layer and distance rules (0x4CDDD0): power-ups and the helicopter no longer show through walls, and the two layer copies no longer z-fight.
- Level 4: room/portal visibility (0x4BC460), applied to Level 4 only, and the paint-room hatch.
- The sky fill matches the background picture (sRGB).

**Bosses**
- Boss levels keep their own music.
- The slime wears its lid (node 10).
- Attack audit of all 15 bosses and minibosses, with tests in `tests/boss_attacks.rs`. It fixed:
  - the stale cut-scene timer (0x50A1F4), which skipped intros and death scenes;
  - projectile headings;
  - zone checks;
  - the final arena's waves.
- Zurg's entrance height.

**Levels**
- Level 2's troop holes.
- Level 7's skateboards. Ported by a helper the user stopped; the tests pass, but no shot was checked.
- Level 14's start-up no longer marks every creature touched, harmful and chasing (0x708 is the +0x42 draw distance). That had started Hamm's talk and the race at once.
- Challenge countdowns: HUD element 6 from 100 up (0x49FD40) shows seconds and the collected count.
- The slime level's laser power-up (special 0x32, `Kind::Timer`) appears from the third round, as in the original.

**Front end and viewer**
- The level select sits inside the Etch A Sketch (0x494130, map variant 1), and the map cars' heading is fixed.
- The cursor is held while playing.
- A look-at scene ends first person (0x4020F0).

### Open issues (exact next steps)

**Reported but not reproduced**
- Level 1: fell out at the living-room windowsill. Needs the user's spot.
- Level 14: helicopter buttons "not working". Physics, the script and a `--shot` all take the press (`stomping_a_helicopter_button_presses_it`). Needs the user's steps.

**Collision**
- If the body starts slightly inside an edge cylinder, it ignores that edge (`sweep_edge` / `sweep_point`). A naive fix broke the boundary walls.
- The mover carry is not swept, but a mover whose floor comes up to 64 units past Buzz's feet in a tick now picks him up (`MOVER_PICKUP`); motion is cleared after the carry, so movers moved by `step_creatures` carry too, and `place_mover` records motion.
- Creature rays don't test boundary walls.
- The camera against boundary walls is unverified.
- Open edges in L5/7/8/10, found by `examples/clipsweep.rs`, are unverified against the original.

**Drawing**
- Room culling only applies to Level 4. Levels 1, 2, 5 and 10 have portals, and 2 and 10 have their own 0x440F70 overrides.
- Creatures and pickups are not room-gated (0x447BD0, creature room byte +0x6C).
- The far set's near clip of 10000 is not applied.
- Creatures are drawn within 10176 rather than from the original's active list.
- The Level 13 follow camera can end up inside Buzz's head.

**Bosses**
- Shared kill (0x405D20) death sounds per type.
- The drill's state 5 / weapon check.
- Level 9: lights, and the camera's every-0x1E-tick pick.
- Level 15: swing outside phase 2, and boss 3's hurt voice.
- The dino fires from a fixed offset, not node 1.
- Campaign 3's dust uses the boss's height, not the floor height.

**Missing features**
- The hook's icon on the HUD's weapon meter (0x493DC0 draws a model there).
- Level 10's lights (list 13, 0x44F200) and its lifts' group re-alignment (0x52FCA0 offsets from the first group mover Buzz isn't touching).
- Level 4's camera shake when a press lands near Buzz (0x50A510 in 0x41BC20).
- Lightning on level 14 (0x42E790, 0x52FFA8).
- The countdown tick sound (0x5E in 0x49FD40).
- Pause menu.
- Options (0x4371B0).
- Movie viewer (0x453FA0).
- Credits.
- Loading pictures.
- Attract demo.
- Level 8 electric arcs and cannon.
- Drips, dynamic lights and floor shadows.
- Shield slowing Buzz.

**Unconfirmed**
- Level 14's "things not animated": the user didn't say which. Rain and lightning aren't ported.

### OpenTS2 (phase 1 done: launcher and installer)
The project is distributed as **OpenTS2**, modelled on OpenMW. Design, OpenMW research and the data model for phases 2–3: [docs/opents2-design.md](docs/opents2-design.md). (The original brief is in git history: `docs/opents2-brief.md` at commit 3d75407.) The owner is going back to polishing the port; OpenTS2's next phases wait until levels 2–15 settle.

- **Workspace:** the root `Cargo.toml` is now a workspace (`.` + `crates/launcher`, both default members). The engine crate `ts2` is unchanged in place; the split into `crates/engine` etc. is planned, not done.
- **Launcher:** `crates/launcher`, binary `opents2` (egui/eframe 0.31 on the engine's wgpu 24 / winit 0.30; rfd; serde + toml). Library modules `paths`, `install`, `iso`, `config`, `mods`, `launch` hold all logic and tests; `main.rs` is the window (tabs: Play, Game Files, Mods, Level Tester, About; green ▶ Play buttons; the token in the header and large on the Play tab).
  - Settings: `%APPDATA%\OpenTS2\opents2.toml` (`OPENTS2_HOME` overrides). Mods: `%APPDATA%\OpenTS2\mods\<id>\mod.toml`. Engine output: `last-run.log`.
  - Starts `ts2-viewer` from its own folder with `TS2_GAME_DIR`/`TS2_CD_DIR` and the game folder argument; modded adds `OPENTS2_MODS` (ignored by the engine for now). No console window.
- **Engine changes (cosmetic only):** window title "OpenTS2" and the embedded icon as the window icon (`ts2-viewer.rs`, `IconExtWindows::from_resource(1)`); `build.rs` embeds `assets/icon/opents2.rc`.
- **Static CRT:** `.cargo/config.toml` adds `+crt-static` for MSVC, so no VC++ redistributable is needed. This forced one full rebuild.
- **Disc images:** the launcher reads a `.iso` (ISO 9660 + Joliet, `crates/launcher/src/iso.rs`) and copies `audio\` + `rtlibs\` (48 files, 372 MB on the user's image) into `%APPDATA%\OpenTS2\disc` once, then uses that as the disc folder. Checked byte-identical to a 7-Zip extraction. The user's image is `C:\Users\alexs\Downloads\Disney-Pixar-Toy-Story-2-…\ToyStory2.iso` (`TS2_DISC_IMAGE` for the tests).
- **Icon:** the game's own Pizza Planet token as a flat 2D picture (face-on, unshaded), rotated 48° anticlockwise so the ring runs lower-left to upper-right as in game. The owner wants the 2D picture, not a 3D view. Owner-approved exception to the no-game-data rule. The token is Level 1 special 0x39 (first id in the exe's token list 0x4F0EDC) = layer 0 mesh 395 `Shape395`, 140 vertices, texture `tex16`. To regenerate: `cargo run --release --example token_model -- <game> out/token`, then `python tools/icon/make_icon.py out/token` → `assets/icon/opents2.ico` (16–256), `opents2.png`, `opents2-1024.png`, and the installer's `wizard-*.bmp`. Used for all exes (`opents2.rc`), the window icons, the launcher UI, shortcuts and the installer.
- **Credits:** the user is credited as **lewiloon2k** (About tab, installer publisher); repository URLs keep the GitHub path.
- **Installer:** Inno Setup 6 (installed per-user on this machine via winget). `tools/installer/build.ps1` → `target/installer/OpenTS2-0.1.0-setup.exe`. Per-user, licence/disclaimer page (`tools/installer/disclaimer.txt`), Start menu + optional desktop shortcut, uninstaller. **Open decisions for the user:** the licence for OpenTS2's own code (placeholder in disclaimer.txt; the repo has no LICENSE), whether to sign the installer.
- **Tests:** `crates/launcher` has 8 unit tests (+1 `--ignored` full ISO copy) and 3 end-to-end tests (`tests/launch.rs`) that start the built `ts2-viewer` through the launcher's command lines with `--shot` for each route (need `TS2_GAME_DIR` and a prior release build).
- **Not verified by the agent:** the launcher window itself (the user tests interactive windows), and taskbar icon appearance.
- **Next:** phase 2/3 per the design doc: the VFS (route engine file reads through layers), then the editor as `opents2-editor` saving `mods/<id>/scenes/levelNN.toml`.

### Facts and pitfalls worth knowing
- **Creature matrices turn Z first** (roll in the model's frame), then Y, then X (`scene::Place::turned`, 0x4CD880's helpers multiply on the left). The old X-Y-Z order turned a banking plane into a pitching one.
- **Script requests for particles:** `spawns` (type and mode), `particles` (fully specified), `lasting` (fully specified with a life), `drops` (type, mode, spin and a chosen floor); `rain` names the level's rain sprite.
- **The viewer draws between ticks** (`Interp` in ts2-viewer.rs): Buzz, creatures and the follow/scene camera are blended from the previous tick by `Player::tick_fraction`. Anything new that moves every tick and is drawn should be blended the same way, or it will judder on fast displays (the user's is 144 Hz).
- **Far-set specials are in level units here.** The exe passes positions `>> 7` for layer-1 (low-detail) specials and 0x4CCE30 multiplies them by 4; `Specials::set_pos` takes level units for both layers, so pass `>> 5`.
- **Stomp buttons on movers** use 0x48B640: the mover is the floor under Buzz (0x4863A0 sets its flags 0x12) or his sweep touched it (0x482A00 sets 2). `Player::touching_mover`. Surface switches use the floor's surface type on the impact tick (stomp == -0x28).
- **Offscreen screenshots:** `ts2-viewer [--level N] --shot out.png "<script>"`. Script words:
  - numbers: frames to run
  - `play`: start as Buzz
  - `+Key` / `-Key`: press and release a key
  - `cam:x,y,z,yaw,pitch`: hold the view (level / 256, y and z negated)
  - `tp:x,y,z[,facing]`: teleport, in level units
  - `?`: print Buzz's state

  In play mode, rooms come from the follow camera, so use free-camera shots to check culling.
- **Front-end scenes:**
  - Scene 0 is `level00\level.ngn`.
  - The map is scene 16, variant 1: `level06\level1t1.ngn`. Scene ids above 10 load variant 1 (0x452FC0). Variants use their base scene's `.dat` / `.raw` (`scene_file`).
  - Picture n is texture slot n (`bgr40` = 0x28).
  - Menus are drawn on a 320-wide screen, the level select on a 512-wide one.
- **Creature slot fields:** +0x40 flags, +0x42 draw distance, +0x50 home, +0x5C target (+0x60 is the hover/pin height), +0x68 target facing. Several bugs came from writing home or flags instead of target or +0x42.
- **Vertex colours:** the shader reads RGB from bytes 1–3. Anything that tints vertex colours must offset by one.
- **Culling follows the materials:** mode bit 8 means cull-none (0x4B6760).
- **Negative sound ids from scripts are jingles.**
- **Level 4's paint puzzle** starts empty (0x41C190).
- **Footsteps** come from frame-list markers 0x81 / 0x82.
- **Shell trap:** `python -` or `cat >` with no input hangs the command chain. Use heredocs or scratchpad scripts.
- **PS1 recompilation:** github.com/PeriBluGaming/ToyStory2Recomp can be used as a playable reference only. The PC exe is the source of truth.
- **Music:** `TS2_CD_DIR` or the ignored `ts2-media.txt` points at the folder containing `audio/`.

---

## 1. What this project is

A clean-room reimplementation of the 1999 PC game *Toy Story 2: Buzz Lightyear to the Rescue*,
in Rust. The original exe (`toy2.exe`, 1999-10-29 build) is decompiled with Ghidra and read
with a disassembler. Its behaviour is understood, then rewritten as original Rust. Game data
(levels, models, textures, sounds, tables inside the exe) is read **at runtime** from the
user's own install.

`ts2-viewer` is the playable front end: a free camera, with F to play as Buzz. It runs Buzz,
creatures and the level script at the original's 60 Hz with the original's integer maths.

GitHub: <https://github.com/asmerdon/toy-story-2-rust-rewrite>. Work goes to the **`dev`**
branch (local `master` tracks `origin/dev`). There is no `main` yet; the user may ask for one.

## 2. Ground rules (the user's and the project's)

- **No game data, exe bytes or decompiled code in the repo** (PROJECT.md), except the token
  icon in `assets/icon/`, which the owner approved. `re/` (the Ghidra project and output) is
  git-ignored and must never be committed. Game file types (`*.all`, `*.anm`, `*.ngn`, `*.raw`,
  `*.obj`, `*.wav`, `*.mpg`) are ignored. The owner's recordings, screenshots and old chat
  logs live in `local/` (git-ignored); leave them there.
- Write original Rust from understood behaviour; don't paste decompiler output. Name things by
  what they do and cite the exe address in comments (`0x417680`, `0x52F300`).
- Record every identified function in `docs/re/functions.md`.
- After each piece of work, build and run the tests. Commit and push to `dev` only when the user asks; they like a push before a session runs out.
- **The user tests in the viewer themselves.** Don't launch an interactive viewer; use offscreen `--shot` renders to check things, and tell them what to try.
  If the viewer is running, `cargo build` can't replace `ts2-viewer.exe` ("Access is denied"):
  build with `--lib --tests --examples` instead and tell them to close it.
- The user likes long autonomous runs and plain reports: what changed, what to test, what is
  a guess. Ask for screenshots and repro steps when something can't be reproduced.
- **Deviations are allowed when the user asks** (some are listed in §7). Mark each one in a
  code comment ("Deviation: …") and in the README.

## 3. Setup on this machine

Paths (install, disc image, Ghidra, Inno Setup) and the Ghidra re-run command are in
PROJECT.md "Machine setup". In addition, for agents:

| What | Where |
|---|---|
| Disc media | The disc image's `Setup\Toy2\CD\audio\*.wav` (music) and `rtlibs\*.dll` (MPEG-1 movies). The ignored `ts2-media.txt` in the project folder points the engine at an extracted copy; the launcher copies its own to `%APPDATA%\OpenTS2\disc`. |
| Decompiled output | `re/out/c/<address>.c` (2,811 functions), `re/out/functions.tsv`, `re/out/strings.tsv` |
| Disassembler | a Python venv with `capstone` + `pefile` in the session scratchpad (`dis.py`: `dis <va> <len>` / `hex <va> <len>`; `xref.py <hex>`: every instruction mentioning an address; `floats.py <va> <n>`). Recreate it in a new session if the scratchpad is gone. |

Commands:

```bash
cargo run --release --bin ts2-viewer -- "C:\Program Files\Disney Interactive\Toy Story 2\data\level01\level.ngn"
```
(change `level01` for other levels; F plays as Buzz)

```bash
set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2
cargo test --release
```
(196 tests; those needing game data skip without `TS2_GAME_DIR`, which doesn't count as coverage. `TS2_DISC_IMAGE` enables the launcher's disc-image tests.)

Some functions aren't in the Ghidra dump (level 10's start-up 0x425B60 wasn't): disassemble those.

### Viewer keys
WASD move, Space jump (×2 double jump), E stomp (Circle), mouse 1 / Square laser (hold to
charge), Q first person (mouse look), X target lock, F play / free camera. Debug keys: T
toggles the colour-key transparency (coins get green backgrounds; the user hit it by accident
once), C cycles vertex-colour modes, Z shows zone-0 meshes, N creatures at their spawns, P
places a creature.

## 4. How the code is laid out

| File | What |
|---|---|
| `src/game.rs` | `World`: swept sphere against `TERRAIN.ALL`, with movers that can be switched, moved and turned, a room map, surfaces. `Player`: Buzz's 60 Hz controller (move, jump, double jump, stomp, spin, super spin, laser, poles, ledges, mantle, zip lines, water, updrafts, hurt, death). |
| `src/buzz.rs` | Move-table animation player (0x401000), state numbers |
| `src/camera.rs` | The follow camera (0x4045E0) |
| `src/look.rs` | First person (0x4038E0): targets, aim dot, helmet and arms (specials 0x2D–0x2F) |
| `src/creatures.rs` | Creature slots (0x52C840 + 0x9C × slot), the behaviour-script interpreter (0x4076F0), movement, animation, contacts, hits (0x408A60), death, and per-type routines (the jump table at 0x407058) |
| `src/script.rs` | `Script` trait and `Common` (shared level state: conversations, info scenes, look-at scenes, HUD bits, token reveals, sounds, particles). Shared routines: `hamm`, `hint_character`, `call`, `finder`, `potato`, `Tilt` (0x49EC00), `PathRunner` (0x48ACC0). `for_level` builds the script for a level by internal id (1–15). |
| `src/level1.rs` … `src/level15.rs` | Each level's start-up and frame script (by internal id; campaign 3 is `level6.rs`, 6 is `level3.rs`), with a module doc listing what's ported and what isn't |
| `src/campaign.rs` | Campaign order vs internal ids, scene paths (0x452FC0) |
| `src/frontend.rs`, `src/save.rs` | Title, menus, level select, results and boss-defeated screens; `Toy2NN.sav` saves |
| `src/music.rs`, `src/fmv.rs` | Disc music (0x413150) and MPEG-1 movies |
| `src/pickups.rs` | Coins, batteries, tokens (with the appear animation), lives, shield, power-ups, info signposts, ability cages, dropped coins |
| `src/boxes.rs` | Push boxes along `level.dat` paths (0x433700 / 0x4346C0) |
| `src/rewards.rs` | Reward sparkles (list 58, 0x49FB40) |
| `src/particles.rs` | Particles from the exe's spawn and type tables |
| `src/laser.rs` | Laser beams, auto-aim, ribbons |
| `src/hud.rs`, `src/dialog.rs`, `src/cutscene.rs` | HUD (0x49FD40), speech box (0x401C30), talk scenes (0x4027F0 / 0x402A10) |
| `src/audio.rs` | Sound ids → samples, positional volume and pan (0x4A3810), jingles |
| `src/scene.rs`, `src/specials.rs` | Draw batches (levels, creatures, billboards, sprites, specials); run-time special-object transforms (0x4CCxxx) |
| `src/formats/` | `ngn`, `terrain`, `leveldat`, `packets` (incl. the backdrop picture), `anim`, `exe`, `all`, `bmp` |
| `src/bin/ts2-viewer.rs` | The viewer: rendering (sky pass, low-detail pass, detailed pass, HUD), input, audio output, token sequence and menu, glue between everything |
| `crates/launcher/` | The OpenTS2 launcher (`opents2`); see "OpenTS2" above and docs/opents2-design.md |
| `assets/icon/`, `tools/icon/`, `tools/installer/` | The icon and its generator; the Inno Setup installer and its build script |
| `examples/` | About 50 diagnostics (§9) |
| `tests/` | Integration tests, one file per level plus systems |

### Units and conventions
- Object units = level units × 32. `Player::pos` is in level units (f32); `pos_obj()` gives object units.
- +Y is **down**. Floors face −Y.
- Angles are 12-bit (0x1000 a turn) in game code, and 16-bit in `.ngn` instance rotations. The mover table uses 0x4000 a turn (>> 2 for 12-bit).
- 60 ticks a second (0x490860 counts elapsed ms × 60 / 1000).
- Scripts address movers by terrain link id − 1. A room is the low byte of a room-map object's flags.
- `level.dat` lists: table 0x559C70 + 4 × type. 58 is rewards / box spots, 60 ledges (bar pairs), 61 poles (bottom / top pairs), 62 zip lines, 63 pickups, 65 room portals.
- Buzz's object is at 0x52F300 (pos +0, vel +0x68 / +0x6C / +0x70, facing +0xE, state +0x90).

## 5. What has been done

### Systems (all levels)
- **Buzz:** the full controller (§4), checked against the original again and again with the
  user. Pad mask 0x309 while flag 4 is set (dizzy, getting up, launched, token sequence). No
  jump while pushing.
- **Collision:** the original `TERRAIN.ALL`; movers move, turn and switch, and carry Buzz. A
  rising tilting plank lifts an airborne Buzz back on top. Harmful surfaces hurt on any
  contact.
- **Creatures:**
  - The script interpreter, chase and steering, hits, knock-back, death, coin drops.
  - Routines for:
    - zurg3, the Tin Man, sheep and the RC car;
    - the lawnmower, army man, Zurg car and kite;
    - the friend types, pod, drill, plane, box, dino, boat, clown and car boss;
    - level 9's flails and minions, and the spider.
  - All stay in play everywhere; only those in the original's range of the camera make sound or call out.
- **Conversations and scenes:** the talk camera (position from track 2, look-at from the second
  track), typed speech box, info signposts, look-at scenes (counted down for every level).
- **Tokens:** the appear animation, the token sequence and "keep playing / exit level" menu
  (0x830CC4), the victory jump with Buzz turning to the camera and the camera following him.
- **HUD:** lives, batteries, coins, charge meters, challenge counters, boss bars, power-up bar.
  Batches keep draw order.
- **Pickups:** everything in level 1, the shield ball riding with Buzz, the laser power-up
  (1200, −10 a shot), Mr Potato Head's missing part hidden on his model until handed over.
- **Rendering:**
  - The sky pass: the PS1 backdrop picture plus sky and ground colours.
  - Then the whole low-detail set, then (depth cleared) the detailed set.
  - Back-face culling on opaque level geometry.
  - Reversed-Z depth.
  - Data fixes: billboards, black faces lit, keyed glass.
- **Audio:** positional sound with the camera's pitch, loops that follow their owner, jingles.
  Characters only call when nearby.

### Level 1, Andy's house: complete
Crib escape, Bo Peep's sheep, Hamm's coins, Mr Potato Head and his ear (cosmic shield cage),
the RC race, the Tin Man boss, attic see-saw, room 2 lift, springs (room 3 stomp spring, room 4
trampoline box), the fountain, steam and hot spot, push boxes (including the basement door and
the spring box), the swing bars (ledges), zip lines, the plank, the fan, the pop-up
platforms, the garage drill bits, info signposts, hint Hamm, reward sparkles, the token menu
and exit.

### Levels 2–15: ported, playtested in two rounds by the user
Each `src/levelN.rs` module doc lists what it covers and what isn't ported. docs/coverage.md
tracks each interaction: simulated, shot-checked, or user-played. The "Open issues" section
above lists what is left. Systems that used to be listed missing here (rocket and hover boots,
the grappling hook, homing particles, music, FMVs, front end, level select, saves) are all
ported now.

## 6. Problems met, and what they taught

These cost real time. Most are worth checking first when something similar turns up.

**Reading the exe**
- **Flag bytes vs flag words.** A hit clears `byte +0x41 & 0xFB`, which is flag word bit 0x400.
  It was ported as flag 4 (walking), so bumped walkers such as the RC car stopped for good.
  When the decompiler shows a byte write at an odd offset, convert it to the word bit.
- **Uninitialised "last health".** Bosses compare their health with a remembered value. The
  Tin Man's wasn't set to his starting health, so his first opening looked like a hit and he
  flinched shut. Set those values at start-up, as the original does.
- **Swapped roles.** The talk camera's two tracks were the wrong way round: position and
  look-at. `0x402030(target, position)` sorted it out. Check argument order from a second
  call site.
- **State set at the event.** The ledge grab sets Buzz's state itself (0x435100), and the
  animation override then leaves it alone. Without that, a grab from a double jump stayed in
  the jump pose. When "sometimes it animates" comes up, look for a state written directly at
  the event.
- **Special-object ids sharing an instance.** In level 1, ids 0x1A and 0x2D both name
  instance 0, the first-person helmet, which then showed in the living room. The later id now
  keeps it.
- **Which objects are which.** The "drills" were first taken for specials 1–3, which are
  wooden push boxes. The real bits are specials 9 / 0x11 / 0x12, which the level script also
  spins (0x417510). Use `examples/under` and `zones` and compare with the user's screenshots
  before changing anything.
- **Behaviour missing from the exe.** The garage drills moving and hurting couldn't be found
  anywhere: no script, creature, path or surface. They were added as a deviation, timed with
  the user. Some behaviour may live in data that isn't decoded yet.

**Engine and rendering**
- **Creature activity.** The original freezes creatures beyond about 3200 units of the camera,
  which looked broken at our draw distance. They now always run, but the original range still
  gates sounds and calls. Otherwise far-off enemies and characters are heard through floors.
- **Black faces.** The level data has faces baked pure black: back faces, and a few front
  faces such as the attic shaft wall. Back-face culling handles most; objects that are partly
  lit fill in the rest. "Decal" depth nudges were tried first and were wrong; flag 0x80000 is
  ordinary geometry.
- **Low-detail set.** Clipping it per pixel from 10000 out left holes. It is now drawn whole
  under the detailed set.
- **Colour-key glass.** The kitchen jars are blended faces mapped onto pure key green. They're
  drawn green only for non-flat objects; windows must stay see-through.
- **HUD order.** Grouping HUD quads by texture drew the speech text under its box whenever the
  power-up bar was showing. Keep draw order.
- **See-saw.** Standing feet sit a few units below the floor surface (by design), so any "did
  the floor rise past his feet" check needs a margin and an airborne condition.

**Tools and environment**
- A rebase with `core.autocrlf=true` checked everything out as CRLF, and multi-line
  perl / sed patches then silently didn't match. The repo is now `core.autocrlf=input`.
- `perl -pi -e 's|…|…|'` with `|` as the delimiter mangled patterns containing `||` and put
  code at the top of a file twice. Use the Edit tool for anything non-trivial, or a different
  delimiter.
- `python -` (reading a script from stdin) hangs in this shell. Write scripts to the
  scratchpad and run them with the venv's python, using `-I`.
- Bash heredocs drop backslashes; use `chr(92)` in Python, or write files with the Write tool.

## 7. Deviations from the original (keep the README's list in sync)
- Every creature stays in play (the original freezes distant ones); only nearby ones make
  sound or call out.
- First-person mouse look (0.8 units of 4096 a count).
- The PS1 backdrop picture is drawn behind windows and the sky (the PC only used two colours
  from it).
- Pure-black faces in otherwise lit objects take the object's average light. Fully keyed glass
  on non-flat objects is drawn dark green at half alpha.
- Coins are alpha-tested and write depth (no distance fade), so cut-out billboards hide them.
- Level 1's drill bits rise and fall (700 units over 3 s, out of step) and hurt within 200
  across.
- Pushing can start just after leaving the floor (coyote ticks), for the sloped kitchen shelf.
- The push animation only plays while the box moves (the grunt still plays on grab, as the
  original).
- The pickup jingle plays its first note only (the user hears it once in the original).

## 8. How to finish a level (the method for levels 2–15)

Every level's script is ported, so what's left is playtesting with the user, fixing what they
find, and porting the gadgets each module doc lists as "not ported". The method that worked for
level 1:

1. **Read the level's module doc** (`src/levelN.rs`) and its frame script in
   `re/out/c/<frame address>.c`. Start-ups and frames are at tables 0x4A1AB8 / 0x4A1B68 by
   internal id − 1. Frames for ids 1–10: 0x417680, 0x4190C0, 0x41AA10, 0x41C640, 0x41E880,
   0x420060, 0x421340, 0x423200, 0x424490, 0x425F60; for 11–15 see `docs/re/functions.md`.
2. **Map the level's objects** before touching behaviour:
   - `examples/creatures levelNN` (slots, types, records, behaviour scripts; `WORDS=140` for
     long scripts);
   - `examples/lists <dir>` (`ALL=1` for every point);
   - `examples/pickups <dir>` (specials and pickups);
   - `examples/boxes`;
   - `examples/movers <dir>`;
   - `examples/under <ngn> x0 y0 z0 x1 y1 z1` (what's in a region, with special ids);
   - `examples/zones`;
   - `examples/surfaces <dir> x y z r`.
3. **Ask the user to play it** in the viewer and send screenshots of anything that looks or
   behaves wrong. Most level 1 bugs were found this way, and each needed the user's description
   of the original.
4. **For each bug, find the original code first.** Use Grep over `re/out/c` for globals
   (`52f3`, `53c6`, etc.) and `xref.py` for every reference to an address. Disassemble where
   the decompiler drops arguments or values passed on the FPU stack (0x4A3810). Compare line
   by line before deciding the port is wrong. Several "bugs" turned out to be the original's
   behaviour, such as the power-up's 120 shots and the Tin Man's chase zone.
5. **Write a diagnostic before a fix** when the cause isn't obvious: a small `examples/` that
   loads the level and simulates the case (`pushsim`, `bars`, `rope`, `seesaw`, `tinman`,
   `potato`, `dialog`). These found most of the subtle issues without the viewer.
6. **Fix, add or extend a test** in `tests/levelN.rs`, update docs/coverage.md, run the full
   suite. Commit and push only when the user asks.
7. **Report** to the user: what was wrong, what changed, what to try, and which parts are
   guesses or deviations.

What's left across levels is the "Open issues" list at the top; lights (0x44F200) and floor
shadows are the biggest unported systems.

### Facts worth knowing for the next levels
- Mr Potato Head's item and gift per level: table 0x503A22 + 2 × level (level 1: 8 / cosmic
  shield, 4: 9 / disk launcher, 7: 1 / rocket boots, 10: 4 / grappling hook, 13: 5).
- Common routines take (slot, camera list, texts…). Camera lists for conversations are
  `level.dat` lists; text pointers are exe addresses (`load_story` loads them).
- Token ids: the start-up's 0x4A0C80(table, reward start); `pickups::level_setup`.
- The box list per level: `boxes::box_list` (level 1: 0x4F0F90).
- Backdrop colours for each level: `examples/sky` (level 2 is dusk purple, 3 near black, 7
  blue / brown).
- Level 9 and 10 boss arenas use `always_active` semantics in the original only for level 6.

## 9. Diagnostics (`cargo run --release --example <name> -- …`, most need `TS2_GAME_DIR`)

| Example | What it does |
|---|---|
| `creatures [levelNN]` | Creature slots, records and behaviour scripts (`WORDS=n`) |
| `exedump <i32\|i16\|u8\|str\|strs> <hex va> [n]` | Reads exe data tables by address |
| `textures <ngn> [index out.bmp]` | An ngn's textures and sizes; writes one out (front-end pictures are `bgrNN`) |
| `vcols <ngn> layer index` | An instance's batches as drawn: blend, texture, vertex colours (byte 0 alpha) |
| `lists <dir>` | `level.dat` point lists (`ALL=1`) |
| `pickups <dir>`, `kinds` | Pickups, specials, level.dat objects |
| `boxes`, `pushsim <dir>` | Push boxes, and a simulated push of each |
| `movers <dir>`, `surfaces <dir> x y z r` | Movers; collision objects and surface types near a point |
| `under <ngn> box`, `zones <ngn> [zone]`, `flagged <ngn>` | Instances in a region, by zone, by upper flags |
| `dark`, `blackfaces`, `winding`, `keyed`, `blended`, `materials`, `texsample` | Rendering data: vertex colours, black faces, winding, keyed texels, blends, material modes |
| `overlap <ngn>` | Coplanar overlapping faces (z-fighting) |
| `backdrop <dir> out.png`, `sky` | The level's background picture and its colours |
| `poles`, `rope`, `bars`, `seesaw`, `tinman <dist>`, `arena` | Simulations of poles, rope exits, ledge bars, the see-saw, the Tin Man fight, a creature's zone |
| `potato`, `dialog` | Conversations through the level script; speech-box typing |
| `shared_specials`, `floorline`, `room`, `wander`, `stress`, `gaps` | Misc: shared specials, floor heights, room lookups, fall-through stress tests |
| `token_model <game> <out>` | Exports the Pizza Planet token (level 1 special 0x39) for the icon generator |
| `floor_at <dir> x z [file]`, `object_tris <dir> index [file]` | Every collision triangle under a point; one object's triangles and normals |
| `zurgintro [-v\|-f]` | Level 12's entrance tick by tick (`-v`: every tick, with the scene camera; `-f`: the fight, Zurg's distance from Buzz and the middle) |
| `bossanim <campaign level> slot x y z [ticks]` | A creature slot tick by tick with Buzz held at a point (animation, state, angles, the scene) |
| `minions9`, `surface_kind <dir> type [file]` | Level 9's first minion call-up tick by tick; every collision object of one surface type |

## 10. Where to find more
- `docs/re/functions.md`: every exe function and global identified, with behaviour notes.
- `docs/formats/`: the file formats.
- Each `src/levelN.rs` module doc: that level's script, objects, and what's not ported.
- `git log`: one commit per fix, with the reason in the message.
