# Handoff: Toy Story 2 Rust rewrite

Read this first, then `PROJECT.md`, `docs/re/functions.md` and `docs/formats/*.md` (`terrain.md`, `ngn.md`, `anim.md`, `packets.md`, `all.md`).

## Setup on this machine
- Project: `C:\Users\alexs\Documents\Toy Story 2 - Rust Rewrite` (local git repo, no remote)
- Game install: `C:\Program Files\Disney Interactive\Toy Story 2` (PC, 1999-10-29 build)
- Rust: stable via rustup, MSVC build tools. Audio output uses `rodio` 0.19 (no default features).
- Ghidra 12.1.4: `C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC`; JDK 21 in `C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot`
- Decompiled output: `re/out/c/<address>.c` (**2,811 functions**), `re/out/functions.tsv`, `re/out/strings.tsv`. **Git-ignored; never commit.** Re-run on the existing project (about 10 min):
  ```
  set JAVA_HOME=C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot
  C:\Users\alexs\Tools\ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat re\proj TS2 -process toy2.exe -noanalysis -scriptPath tools\ghidra -postScript DumpAround.java re\out
  ```
- Disassembly: `re/out` hides call arguments, jump tables and some offsets. A scratch Python venv with `capstone` + `pefile` (scripts: disassemble a range, hex-dump exe data, scan the whole `.text` for operands mentioning an address) fills the gap. Recreate in the session scratchpad.
- Tests: `set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2`, then `cargo test --release` (all passing). `cargo clippy --all-targets` fails only on an old `6.283` in `examples/wander.rs`.
- Viewer: `cargo run --release --bin ts2-viewer -- "C:\Program Files\Disney Interactive\Toy Story 2\data\level01\level.ngn"`
- Diagnostics (`cargo run --release --example <name> -- <args>`): `rotfit`, `spikes`, `wander` (fall-through stress test), `pickups <levelNN dir>`, `overlap <level.ngn>` (coplanar overlaps by mesh set/zone), `sprites <level.ngn>` (billboard groups), `movers <levelNN dir>` (type-8 and room-map objects), `boxes` and `creatures [levelNN]` (need TS2_GAME_DIR), `stress [runs] [ticks]` (level 1 played by random input with everything on; reports fall-throughs), `gaps <levelNN dir>` (visible floor without collision; noisy: counts ceilings too).
- The user mentioned github.com/morluto/rea (an MCP wrapper over Ghidra/IDA). Not installed: our Ghidra dump + capstone cover it. If the user installs it, its tools appear in a new session.

## Rules
- No game data, exe bytes or decompiled code in the repo. Everything is read from the user's install at runtime (exe tables via `src/formats/exe.rs`).
- Write original Rust from understood behaviour. Don't paste decompiler output.
- The user tests the viewer themselves (don't launch it). Build, run the tests, commit, then report what to try.
- Record every identified function in `docs/re/functions.md`.
- The user prefers long autonomous runs: do as much as possible before reporting; don't ask permission for harmless commands.
- Write files with LF endings. Bash heredocs break on quotes and backslashes in this shell: write patch scripts to a scratch file with the Write tool and run them with the venv's python.

## Code map
| File | What |
|---|---|
| `src/game.rs` | `World` (swept sphere vs TERRAIN.ALL; movers switchable/placeable through `Cell`s; room map at ¼ scale; surfaces) and `Player` (Buzz's 60 Hz integer controller; pre-move hook; spin attack; hurt/death/respawn; health and lives) |
| `src/buzz.rs` | Move-table animation player (0x401000 stepper), state numbers |
| `src/camera.rs` | Original follow camera (0x4045E0 main path) |
| `src/audio.rs` | Sound ids → samples, positional volume/pan, jingles (negative ids) |
| `src/scene.rs` | Level/creature batches; instance matrices; billboards (codes 5/6); sprites; special objects |
| `src/specials.rs` | Run-time special-object transforms (0x4CCxxx) |
| `src/pickups.rs` | Coins, batteries, tokens, lives, power-ups; dropped coins |
| `src/boxes.rs` | Pushable objects along level.dat paths (0x433700) |
| `src/level1.rs` | Level 1's setup/script (0x417680): doors, fan, plank, tops; crib bars; Bo Peep, Hamm, Mr Potato Head conversations; RC race |
| `src/creatures.rs` | Creature slots, behaviour script interpreter, movement, animation, contacts (ellipsoids from .all), hits, death, laser ray test |
| `src/laser.rs` | Laser beams: auto-aim, rays, bounce, ribbons |
| `src/hud.rs` | HUD elements 0-6 (lives, health, coins, spin, laser, sheep, laps), sprite table |
| `src/particles.rs` | Particle slots, spawn modes and types from the exe's tables, update, death actions, drawing, touches |
| `src/look.rs` | First-person look mode (0x4038E0): targets, marker/aim dot, helmet and arms |
| `src/dialog.rs` | Speech box (layout, typing, box) and the HUD font |
| `src/cutscene.rs` | Talk cutscenes (0x4027F0 / 0x402A10): placing, camera glide, token at the end |
| `src/formats/` | `ngn`, `terrain`, `leveldat` (lists, pickups, special-object records), `packets`, `anim`, `exe`, `all` |
| `src/bin/ts2-viewer.rs` | Viewer and play mode (two-pass level draw, per-frame billboards/specials/creatures) |

## Status (what the user has confirmed)
- Play mode (F) in level 1: movement, jumps, stomp, climbing, camera, sounds: feel matches the original.
- Pickups: collections work, coins etc. look correct. Starting lives: 5.
- Earlier round's flicker/banister fixes, level objects, creatures, combat: not reported as broken.

## Round 5 (2026-10-14): done, not yet tested by the user
The user asked for first-person mode (with the crib in level 1) and to carry on with the plan. Done and committed:
- **First person** (`src/look.rs`): Q = L1 toggles, WASD looks, X = R1 cycles targets (hold to lock), mouse 1 fires. Helmet/arms model (specials 0x2D-0x2F) shown at the eyes, hidden otherwise (it used to sit on the living-room floor).
- **Crib** (room 1): two COTBIT bars shootable only from inside the crib; both down -> the side goes and the crib parts drop. Test `tests/crib.rs`.
- **Conversations**: speech box + talk cutscene camera; Buzz frozen, jump continues, Square skips. Bo Peep (find 5 sheep -> token 1, HUD sheep count), Hamm (50 coins -> token 0), Mr Potato Head (find his ear, the "Other" pickup -> cosmic shield ability bit; the shield's use isn't ported).
- **RC race** (room 2): touch the car, talk, then race 3 laps round the room; win -> token 2, HUD laps counter; leaving the room resets it. Test `rc_race_runs_and_counts_laps` in `tests/bopeep.rs`.
- `World::room_at` now asks 24 units above the point (feet sit below the floor surface; it returned None at floor height in room 2).
- New examples: `kinds <levelNN dir> <level>` (pickups by kind), `room <levelNN dir> x y z` (room-map lookups).

- **Tin Man boss** (room 5, after the round-5 handoff): walk up to his platform -> challenge conversation -> fight. Hit him with the laser while he's open (animations 3 / 5); he flashes, flinches; below 10 health he dies and token 4 appears. Boss health bar (HUD element 7), camera turns towards him, engine hum. Test `tin_man_fight_ends_with_a_token`. - **Room 5 see-saw** (0x49EC00): movers can now turn (`World::turn_mover`, collision rotated about the mover's position; `ride_turn` carries Buzz standing on it). It rests tipped one way (±39°) and tips towards where Buzz stands, harder when he lands. Kind-8 movers now ignore terrain flag 0x100 (the original's mover loop does). Test `tests/seesaw.rs`.

- **Room gadgets**: room 2 lift (climb pole 8: special 6 and the pole sink 1472 units, speeding up), room 3 spring (stomp onto surface 8 next to Hamm: thrown ~5400 up along heading 0x11E onto a shelf; new `Player::launched` flight mode), room 4 fountain squirts, steam vents and a hot spot that costs a battery. Tests `tests/rooms.rs`.

- **Hint Hamm** (slot 0x23, list 0x27, 0x4A1E60): calls now and then (slot 70); touched, gives the hint for the next token not yet collected (texts via pointers at 0x4F0F7C), or "you have all the tokens". `Level1::tokens` is set by the viewer from `Pickups::tokens`. Bo Peep calls out (sound 0xB2, 0x4A26F0) until she has her sheep. No dedicated test yet (builds, suite passes).

- **Info pickups** (the 9 tutorial signposts, 0x402610): touching one plays its scene with the tutorial text. **Cosmic shield cage**: the shield pickup (id 0x31) is caged and can't be taken until Mr Potato Head gets his ear; then the cage (special 0x19, mover 10) goes and it shows at double size. Tests in `tests/rooms.rs`.

Ask the user to try (none of round 5 is user-tested yet):
- Tin Man: climb the plant and rope to the attic (room 5), get challenged, shoot him while he's open; health bar at the top, token 4 when he dies.
- Room 5 see-saw: stand / land on either end.
- Room 2 lift: climb pole 8 (by the RC car's room door, about (1271, 6746, -3030)).
- Room 3 spring: jump and stomp onto it next to Hamm (about (17601, 6695, -8278)): thrown up onto a high shelf.
- Room 4: fountain squirts / steam; the hot spot near (23462, 4601, -13082) costs a battery.
- Talk to the hint Hamm.
- Earlier round-5 items: Q in the bedroom crib and shoot both bars; talk to Bo Peep / Hamm / Mr Potato Head; collect the sheep and the ear; race the RC car in room 2.

## User feedback, round 4 (2026-10-14), and what was done
- Audio, HUD, laser look: user happy. Attic box works.
- **No push animation** -> state 0x19 while pushing (Player.pushing, set by Boxes).
- **Charging cut by the idle fidget** -> fidgets give way to idle while laser/spin/charge run (0x4011D0); the whole-body-state rule then no longer cancels them.
- **W/S snap freezes** -> each snap restarts the skid's frame list (+0x92 = 0xFFFF), so the 26-tick skid plays in full each time. Keyboard headings are now relative to the camera's orbit yaw (0x52F3AC is its ideal position).
- **Mantle missing** -> 0x435F30 ported (jump at a ledge whose top passes your head height: put on top, 82-tick climb animation, state 9). Also the ledge grab 0x435100 (level.dat list 60, 7 in level 1, on the basement walls 2750 up) and zip lines 0x4359D0 (list 62, 2 in level 1). Tests: `mantle_climbs_onto_a_high_ledge`, `level_1_ledge_is_grabbed_and_climbed`, `level_1_zip_line_is_ridden`.
- Hurt (state 5) and dying (7) animations now play.
- Then (not yet tested by the user): particles (src/particles.rs: laser impacts and sparks, spin rings, death bursts), ZURG3 shooting (0x406220) with shots that hurt Buzz or bounce off his spin (0x4100F0), dropped coins as the original bouncing coin particle (type 0x3D) picked up by touch.

## User feedback, round 3 (2026-10-14), and what was done
1. **Pickup sound plays several times; push scrape awful.** Scrape: sounds with bit 15 in their slot word are owned loops (0x4A3980): now one looping buffer per (slot, owner), stopped 16 ticks after the owner last played it. Pickup: the exe's jingle −1 plays PICKUP5 six times (PS1 pitches unused on PC) yet the user hears it once in the running game; now only its first note plays (deviation, noted in the viewer). Ask again.
2. **Stuck pushing the attic box (box 6, path with a 90° corner).** The original lets go when the stick points more than 90° from the push (tail of 0x4346C0); ported, plus the grab compares the facing, and facing is held to the push angle. Test `attic_box_goes_round_its_corner`.
3. **Charging gun/spin doesn't work.** Most likely the meters were invisible (item 5) and the whine/whirr were broken by the buffer pool; both fixed. Logic is unit-tested. Ask the user to check again.
4. **W/S spam snaps with the skid sound.** The original's turn code snaps and skids on every reversal too, but the keyboard goes through a d-pad table (0x433F40): W+S held together = forward, not "no input"; ported (`game::dpad_heading`, camera-to-Buzz angle). Skid now also needs no laser. If still different, ask what the original does exactly (does Buzz turn smoothly? no sound?).
5. **HUD bars missing.** Screen sprites draw last-queued first (0x4B8DD0); the dark meter frames were covering the bars. Reversed.
6. **Laser thickness wrong.** Ribbon UVs were transposed: the sprite's 31-px axis runs along the beam, the 8-px axis across (0x4B9100). Fixed, with cell 2 at the muzzle end. Position was confirmed good.

## Next steps (in priority order)
1. **Where I stopped (2026-10-14):** level 1 script 0x417680 is ported except the items below. Remaining small level 1 items: a test for the hint Hamm, the camera-height limits while on pole 10 (0x53C5E4 = 0x3C), reward sparkles 0x49FB40, the spring's camera lag. Then move on to the other levels (user: "once you have finished with level 1, proceed with rest"). ("Room 1 falling toys" from earlier notes isn't in 0x417680.)
2. Particles still to do: homing (flag 8, kinds 0x1D/0x26), 0x410850 explosions (death actions 1/2/4), lights 0x44F200, floor shadows (0x54F098), Buzz's footstep dust 0x4A2D80.
3. Token sequence / menu (0x49FD40 with 0x830CC4 = 9), rocket/hover boots (0x4A4D60 / 0x4A4F80 / 0x4A4EC0, 0x4A4BB0; level 1 has no boots pickup), grappling hook in first person (0x4A5370, list 59).
4. Original collision resolver if gaps persist; slippery surface 13; riding movers 0x488E10.

## Useful facts
- Level 1 slots: crib bars 0 / 2, Bo Peep 1, RC car 0x1D, Hamm 0x1E, Mr Potato Head 0x1F. Talk lists: 0x19 Potato, 0x1A Tin Man, 0x1B RC, 0x1C Bo Peep, 0x1D Hamm; RC waypoints list 30.
- Units: object units = level units × 32; 12-bit angles in game code, 16-bit in `.ngn` instance rotations; +Y is down.
- Buzz's object is at 0x52F300 (`docs/re/functions.md`). Creatures: 0x52C840 + 0x9C × slot.
- `level.dat` list table: 0x559C70 + 4 × type (58 rewards/box spots, 61 poles, 63 pickups); special-object records after section 2.
- Scripts address movers by link id − 1. Room = low byte of the room-map object's flags.
- Sound table 0x502950; jingles 0x503828.
- Music is CD audio, FMVs are on the CD too: neither is in the install (ask the user for a BIN/CUE rip or the CD's tracks).
