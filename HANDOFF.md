# Handover: Toy Story 2 Rust rewrite

Last updated 2026-10-19. **Level 1 is complete and signed off by the user.** Levels 2–10 have
their scripts ported but haven't been playtested. This file explains how the project works,
what has been done, what went wrong along the way, and how to carry on with the next levels.

Read this first. Then read `docs/re/functions.md` (every identified exe function, with what we
know about it) and `docs/formats/*.md` (`ngn`, `terrain`, `anim`, `packets`, `all`).

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

- **No game data, exe bytes or decompiled code in the repo.** `re/` (the Ghidra project and
  output) is git-ignored and must never be committed. Game file types (`*.all`, `*.anm`,
  `*.ngn`, `*.raw`, `*.obj`) are ignored. Screenshots in the project folder (`level1.png`,
  `screenshot1.png`) are game imagery: leave them uncommitted.
- Write original Rust from understood behaviour; don't paste decompiler output. Name things by
  what they do and cite the exe address in comments (`0x417680`, `0x52F300`).
- Record every identified function in `docs/re/functions.md`.
- After each piece of work: build, run the tests, commit, push to `dev`.
- **The user tests in the viewer themselves.** Don't launch the viewer; tell them what to try.
  If the viewer is running, `cargo build` can't replace `ts2-viewer.exe` ("Access is denied"):
  build with `--lib --tests --examples` instead and tell them to close it.
- The user likes long autonomous runs and plain reports: what changed, what to test, what is
  a guess. Ask for screenshots and repro steps when something can't be reproduced.
- **Deviations are allowed when the user asks** (some are listed in §7). Mark each one in a
  code comment ("Deviation: …") and in the README.

## 3. Setup on this machine

| What | Where |
|---|---|
| Project | `C:\Users\alexs\Documents\Toy Story 2 - Rust Rewrite` |
| Game install | `C:\Program Files\Disney Interactive\Toy Story 2` (levels 1–10 only) |
| CD image | `C:\Users\alexs\Downloads\Disney-Pixar-Toy-Story-2-…\ToyStory2.iso`: music as WAVs (`Setup\Toy2\CD\audio\*.wav`, `tracks.txt`) and FMVs (`Setup\Toy2\CD\rtlibs\*.dll`). Levels 11–15 are empty here too. 7-Zip (`C:\Program Files\7-Zip\7z.exe`) can list or extract it. |
| Rust | stable via rustup, MSVC build tools; audio through `rodio` 0.19 |
| Ghidra | 12.1.4 at `C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC`, JDK 21 at `C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot` |
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
(about 100 tests; those needing game data skip without `TS2_GAME_DIR`)

To re-run the Ghidra dump on the existing project (about 10 minutes):
```bash
set JAVA_HOME=C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot
C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat re\proj TS2 -process toy2.exe -noanalysis -scriptPath tools\ghidra -postScript DumpAround.java re\out
```
Some functions aren't in the dump (level 10's start-up 0x425B60 wasn't): disassemble those.

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
| `src/script.rs` | `Script` trait and `Common` (shared level state: conversations, info scenes, look-at scenes, HUD bits, token reveals, sounds, particles). Shared routines: `hamm`, `hint_character`, `call`, `finder`, `potato`, `Tilt` (0x49EC00), `PathRunner` (0x48ACC0). `for_level` builds the script for levels 1–10. |
| `src/level1.rs` … `src/level10.rs` | Each level's start-up and frame script, with a module doc listing what's ported and what isn't |
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
| `examples/` | About 45 diagnostics (§9) |
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

### Levels 2–10: scripts ported, not playtested
Each `src/levelN.rs` module doc lists what it covers. In short:

| Level | Ported | Not ported |
|---|---|---|
| 2 | see-saws, water, Hamm, hint Hamm, the sarge's troops, stomp switches → door → spring, rope lift, the Zurg kite boss | drawbridge, tree, ball, squash pad, boxes list, kite string effects |
| 3 | the slime boss (rounds, regrowth, look-at scenes) | lights, fog, drips, camera shake |
| 4 | hanging platforms and rigs, shuttles, presses, drawbridge, button lift, water, characters, timed challenge, the drill boss | the crane combination puzzle (token 3), drips, lights |
| 5 | gliding platforms, see-saws, trampoline, water, the clown, characters, challenge | the bubble machine, the falling gate, drips, fog |
| 6 | the boss arena | music change, camera drift |
| 7 | path movers and locks, spring, trampoline, egg challenge and gate, characters, the dino | bobbing platform and button race, rockets, chicks pushed by boxes, drips, sparks |
| 8 | spinners, characters, flying race, water, the car boss | the claw machine, 0x422D20, the room 2 cannon, swaying specials, lights |
| 9 | the boss, flails, minion rounds, chase, beam | flail call-up animation, sparks |
| 10 | spinners, air shafts, fans, wind, lifts, vents, stomp-switch puzzle, sparks, characters, runaway token, the spider | lifts pausing while Buzz mantles onto them (0x559E80), lights, electric beams |

Also not ported anywhere: rocket boots (0x4A4D60 etc.), the grappling hook (0x4A5370, list 59),
lights (0x44F200), floor shadows, homing particles, footstep dust, music and FMVs, a front end
or level select. Levels 11–15 have no data.

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

## 8. How to port and finish a level (the plan for levels 2–10)

Every level's script is already ported, so what's left is mostly playtesting with the user and
fixing what they find, plus porting the gadgets listed as "not ported". The method that worked
for level 1:

1. **Read the level's module doc** (`src/levelN.rs`) and its frame script in
   `re/out/c/<frame address>.c`. Start-ups and frames are at tables 0x4A1AB8 / 0x4A1B68 by
   level − 1. Frames: 0x417680, 0x4190C0, 0x41AA10, 0x41C640, 0x41E880, 0x420060, 0x421340,
   0x423200, 0x424490, 0x425F60.
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
6. **Fix, add or extend a test** in `tests/levelN.rs`, run the full suite, commit, push.
7. **Report** to the user: what was wrong, what changed, what to try, and which parts are
   guesses or deviations.

Suggested order: **level 2** (the next in play order, with lots of gadgets still to port:
drawbridge, tree, ball, squash pad, boxes), then levels 3–5, the level 4 crane puzzle, level 6,
then 7–10. Bigger items after that:
- a front end and level select (load the next level after "exit level";
  `exe::level_number`, `data\levelNN`);
- the rocket boots and grappling hook (abilities from Mr Potato Head in later levels);
- music from the CD WAVs (`tracks.txt` maps them);
- lights (0x44F200) and floor shadows.

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

## 10. Where to find more
- `docs/re/functions.md`: every exe function and global identified, with behaviour notes.
- `docs/formats/`: the file formats.
- Each `src/levelN.rs` module doc: that level's script, objects, and what's not ported.
- `git log`: one commit per fix, with the reason in the message.
