# Handover: Toy Story 2 Rust rewrite

Last updated 2026-10-31 (verification and user-report fixes; see the first entry). **Level 1 is complete and signed off by the user.** Every level
(2–15) now has its script ported; none of 2–15 has been fully playtested. This file explains
how the project works, what has been done, what went wrong along the way, and how to carry on.

Read this first. Then read `docs/re/functions.md` (every identified exe function, with what we
know about it) and `docs/formats/*.md` (`ngn`, `terrain`, `anim`, `packets`, `all`).

---

## 2026-10-31 (third pass): second round of playtest reports — newest, read first

The user retested. Everything reported before is fixed, except what's listed below. Their new reports, with status:

| # | Report | Status |
|---|---|---|
| 1 | Level 5 bones challenge: one bone missing | **Not reproduced**: all five (0x61–0x65, y 827 over the water) exist, can be collected (`bones_all_collectable`) and show once the challenge starts (`l5-bones-0.png`). Ask the user which bone and when |
| 2 | Clipping: L1 windowsill, L13 conveyor ends | Fixed L13 (Opus helper): the **boundary walls** (TERRAIN kind 0x101, read by 0x484380 / 0x48C860; see docs/formats/terrain.md) were never loaded, and belt pushes weren't swept. L1 windowsill **not reproduced**; ask the user for the spot. Sweep `examples/clipsweep.rs`: edge falls in L5/7/8/10 look like open pits, not verified against the original |
| 3 | L14 enemies moving fast | Likely fixed with 4: the setup wrote 0x708 into every creature's flags (chase 8, harmful 0x100, touched 0x200); the original writes the draw distance at +0x42 |
| 4 | L14 starts in a race and talking to Hamm | Fixed (same cause); `standing_at_the_start_starts_no_talk_or_race` |
| 5 | Final level: should be walled at the back | Fixed: a 29-segment boundary loop closes it; `final_arena_back_is_closed_by_its_boundary_wall` (replaces the fall-plane test; 0x436220 still applies to real falls) |
| 6 | Draw order: helicopter over the flagpole, power-ups over walls | Fixed (Opus helper):
- Specials now follow 0x4CDDD0's per-layer rules on every level: layer 1 drawn beyond 1000 (0x5088B4) in the low-detail pass, before the depth clear; layer 0 within 12000 (0x5088B0), clipped like the detailed set.
- Level 1 is pixel-identical indoors.
- Test: `specials_of_each_layer_have_their_own_distance`. Shots: `out/rv/cmp/*`, sweep sheets `out/rv/sweep/sheet-lN.png`.
- Open:
  - L13 follow camera ends inside Buzz's head after 300 ticks.
  - The far-set near clip of 10000 is not applied (it showed holes).
  - Creatures draw within 10176 rather than the original's active list. |
| 7 | L7 skateboards (stomp to ride) | Ported by a Sonnet helper (the user stopped it before it reported): `Level7::skateboards` from 0x421340; tests `skateboard_*`. No shot checked |
| 8 | Hide the cursor in play | Done: `sync_cursor` (hidden and confined while playing, released on focus loss) |
| 9 | First-person model visible in cut-scenes | Done: 0x4020F0 ends first person; `Look::cancel` called when a script look-at begins |
| 10 | L12 Zurg intro sinks into the floor | Fixed: the height goes to slot +0x60 (target), not home; `zurg_entrance_stays_above_the_floor` |
| — | All bosses: attacks exactly as the original | Not started; helper after rendering |

Open collision details:
- The edge-cylinder slip: a body starting slightly inside an edge ignores it, and a naive fix broke the boundaries.
- The camera is pulled in by boundary walls. The original's camera reads them too, but this is unverified.
- Creature rays against boundaries are not done.
- The mover carry is still added to position unswept.

Pushed as `6653ae3` to dev, then the draw-order fix (169 tests).

## 2026-10-31 (later): verification and the user's playtest reports

Objective: verify commit `7c34cc9`, then fix what the user found in a playtest. Nothing
committed. Per-level coverage now lives in [docs/coverage.md](docs/coverage.md); keep it there.

Helpers used under AGENTS.md:
- Level 2 holes: Sonnet (standard).
- Particle homing: Sonnet (standard).
- Final-level fall and fight: Opus (advanced).
- Level 4 visibility: Opus (advanced).
All helpers ran one at a time, and the main agent reviewed each one's diff.

### Verified first
- `tilted_mover_matches_rendered_special_rotation` and `later_levels final_arena` pass.
- The full suite passes and the release viewer builds. At the end of this session: 160 tests in 36 suites, all passing.
- Post-fix shots inspected:
  - Level 2 background picture (`out/verify/level2-*`).
  - Level 15 suitcases, which sit in place.
  - See-saws: not re-shot; covered by `continuation_audit` simulation only.

### Fixed (cause → fix, evidence)
| Report | Cause | Fix | Evidence |
|---|---|---|---|
| Background colour ≠ picture | Flat fills got raw sRGB bytes as linear vertex colour; textures are sRGB-decoded (fill drew (99,66,143) against the picture's (32,14,70)) | `sky_batch` samples the picture's corner texels for the fills; without a picture the colour is linearised | `level2-sky-fixed.png`: seamless |
| Boss music switches to miniboss theme | 0x49EAC0 is called only by normal levels' scripts; boss scripts 0x41AA10/0x420060/0x424490/0x42B3A0/0x42FC50 restart their own track (0x52AD8A) at fight start | `music::level_switches_theme`, viewer `theme_events` | unit test |
| Level 6 slime has no trash-can lid | Port placed special 0 once; 0x41AA10 sets it each tick on node 10 (−700, then −0x20) via 0x43C1C0 with a wobble (0x52F6F8); it drops from 0x3E9 | `level3.rs`; `scene::creature_node_point` now uses rest pose for nodes an animation doesn't move (as the renderer does) | `slime_shrinks_over_four_rounds` lid check fails before (1250/1295 ticks off), passes after; `level6-slime-0.png` |
| Level 2 hole-stomping troop | 0x4190C0's mole game wasn't ported; also `think` pinned flag-0x20 creatures to home y, not target y (+0x60) | `Level2::troop_holes`; `creatures.rs` target[1] | two tests in `tests/level2.rs` |
| Level 14 constant damage | `Creatures::contact` (0x407440) squared/multiplied object units in i32; level 14 spans >10⁶ units, so far creatures (slot 46, id 0x3A, ~50,000 units away) wrapped to "touching" | i64 maths in `contact` and `laser_ray` | `tests/level14.rs` fails before, passes after; teleport grid lost 14→4 health before, 14→10 after (left: propellers, a real hazard) |
| Main-menu (level select) cars backwards | 0x438910 calls 0x4520D0 with (Δz, −Δx) | `Select::move_pieces` heading | decompile only; not visually confirmed |
| Level select not inside the Etch A Sketch | Scenes > 10 load variant 1 (0x452FC0), so the map is `level1t1.ngn` with `etch_lh/rh` (tex32/33); 0x494130 draws 14 screen quads (tables 0x500B20 / 0x500AB0, read at runtime) zooming in over 55 ticks | `Screen::Select.variant() = 1`, `Frame::panels`, `front_batches`; viewer `scene_file` uses the base scene's .dat/.raw for variants (levelt1.cfg) | `select-etch.png`; frontend test |
| Final level: falling off the back | Not a missing wall: the floor ends at x +6500, as in the original. 0x436220 kills Buzz 0x2000 object units under the lowest collision point (0x489C30, 0x729124); the port silently reset him at +4000 | `World::lowest`, death via `hurt(0, 4)` (all levels) | `final_arena_running_off_the_back_kills_buzz_at_the_fall_plane` |
| Final fight (attacks) | Blacksmith/prospector swing needed 0x4076F0's chase bit (`Step.chase & 1`), not checked; 0x66 particle vy −2; gunslinger keeps its speed on defeat | `level15.rs`, `Creature::step` | `final_arena_bosses_swing_only_while_chasing`, `final_arena_fight_hurts_buzz_and_all_three_bosses_fall_to_hits` (real attacks, hits via `hit`, defeat, rescue) |
| Spawn points (11–15) | ResetPlayer snaps with 0x4863A0 → 0x486520 (nearest floor above within 2048, from 32 above), +8; port took the floor below. Only level 13 differs: it starts on the X-ray conveyor | `World::start_floor` | `level_13_starts_on_the_conveyor_above_its_table_point`; `l13-start-orig.png` |

Tools added: the `--shot` script takes `tp:x,y,z[,facing]` (level units) and `?` (prints
Buzz's position, health, floor). Use them to reproduce reports at real coordinates.

### Still open (exact next steps)
1. **Level 4 (done this session, Opus helper):**
   - The hatch is two specials (0x40 outside, 0x3F inside). 0x41C640 folds their y scale (0x52F7D0) to 0 by 0x200 a tick while Buzz is within 0xFA × 256 of (0x51B97, −0x104FD, 0x5E58E), and back to 0x1000 otherwise. Ported in `level4.rs`.
   - Interior props showed through walls because the port lacked the original's room/portal walk (0x4BC460: chunks 0x102 / 0x103, camera room via 0x4885C0, facing and view tests 0x4BACF0 / 0x4BA2F0, room 0 always drawn) and the 12000 cut for layer-0 specials. Now in `scene::Portals` and the viewer's `RoomCull`, **gated to level 4 only** (`ROOM_CULLED_LEVELS`).
   - Level 1 shots are pixel-identical before and after. Tests: `paint_room_hatch_opens_for_buzz`, `paint_room_seen_only_through_its_portals`, plus viewer unit tests. Shots: `out/verify/cmp_*.png` and `l4f_*` (free camera; the play+cam shots are not valid evidence).
   - Open:
     - Creatures and pickups are not yet room-gated (0x447BD0 checks the creature room byte +0x6C; its writer is not found).
     - Levels 1, 2, 5 and 10 have portals but still draw everything; 2 and 10 have their own 0x440F70 overrides.
     - The hatch room lighting (0x54F3BC / 0x54F3DC) is not ported.
2. **Particles (0x410F40), done this session (Sonnet helper):**
   - Flag-8 homing now applies to every kind (vy −1 = Buzz, gravity = yaw, +0x20 pitch, −1 = flat). It uses the original's constants: frame period 0xC4 → speed /8 and steering (life/16)² + 4; otherwise /16 and 8.
   - Kind 0x2B hugs floor − 0x800; kind 0x2C dies above floor − 0x1000.
   - **This changes Zurg's shots (level 12):** they now steer weakly at first. The Zurg particle test was updated to match.
   - Still open: creature-targeted homing, trails 0x6B / 0x2C / 0x69 / 0x6A, and whether the blacksmith shockwave actually lands more (the fight test's counts are unchanged).
3. **Level 14 "things not animated":** the user didn't say which. Rain and lightning aren't
   ported; ask them which objects.
4. Mode-4 death countdown: original 0x2E ticks, port `die()` 0x78. Unchanged.
5. The 0x101 record in TERR1.ALL (a closed loop near x 6410–6450) has no reader outside the
   loader; it's treated as unused.
6. Car heading and the Etch A Sketch zoom need the user's eye. Levels 2–15: work through
   docs/coverage.md.

## 2026-10-31: recovered continuation status

The user requested a documentation update after the session hit its usage limit. This entry records the interrupted work; no new gameplay changes or test runs were made on October 31. Tracked implementation and agent guidance are already saved in commit `7c34cc9`. The pre-existing untracked recordings, screenshots and `ChatGPT Log.md` remain untouched. Do not infer push status from the local commit.

### Scope and working rules

Continue the Rust rewrite within PROJECT.md's clean-room boundaries. AGENTS.md now defines provider-neutral economical/standard/advanced capability tiers, direct work for small tasks, one helper at a time, bounded briefs, escalation, and evidence requirements. CLAUDE.md links to it. The previous session used one inherited-model helper for sequential read-only boss/rendering investigations; no cheaper model was selected. Helpers consulted relevant level scripts, creature interpreter, world/scene/specials code, viewer integration, tests, and private original routines. Main-agent checks reproduced the consequential bugs.

### Saved implementation

| Area | Change / evidence | Status |
| --- | --- | --- |
| See-saws | `src/game.rs` now applies Z/Y/X rotation, matching `scene::instance_matrix`, rather than X/Y/Z. `tilted_mover_matches_rendered_special_rotation` failed before and passed after the fix. | Focused regression verified; final broad regression and visual playtest pending. |
| Backgrounds | `examples/continuation_audit.rs` found decodable type-36 pictures in campaign levels 01–07, 09, 11, 13, 14. None in 08, 10, 12, 15. `sky_batch` draws the opaque picture before flat fills: the prior strict Greater depth test rejected the picture at equal depth. | Asset evidence verified; post-fix rendering not yet inspected. |
| Floating suitcase | `src/level15.rs` places far specials 1/4 in full level units (`>>5`), matching near specials 0/3. Original setter 0x4CCE30 scales second-layer coordinates by four; Rust's setter does not. | New alignment test failed before and passed after. |
| Creature jump continuation | `src/creatures.rs` landing advances `pc` by one i16 word, not two. Original 0x4076F0 advances two bytes. The old port could resume on an operand. | Saved; broad jumping-creature regression pending. |
| Final boss attack | Blacksmith attack rates changed from positive 64 to negative 64. Positive rates depend on movement and freeze after the script sets speed to zero; original behavior uses negative rates. | Saved; result of the last coupled test/build command was not recovered. |
| Test coverage | `tests/later_levels.rs` adds LOD suitcase alignment and a coupled test that calls creature ticking as well as level scripting. | The coupled test reproduced a frozen boss before the last rate fix. Do not claim it passes until rerun. |

`examples/continuation_audit.rs` simulates the five see-saws in levels 01, 02 and 05. After the rotation fix it observed sustained contact and tipping on all five. An initial Level 05 probe was outside the short plank at offset -700; using -350 reached its collision surface. That initial miss was a diagnostic placement error, not evidence of absent collision. Level 01's attic remains the signed-off reference; its single-axis rotation is unchanged.

### Verification actually observed

- Before implementation, `cargo test --release --offline` completed successfully with `TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2`.
- The rotation regression failed before the fix and passed afterward.
- The suitcase alignment regression failed before the position correction and passed afterward.
- `cargo test --release --offline --test later_levels final_arena -- --nocapture` still failed on a stationary boss after the landing and suitcase fixes, before the final negative-rate correction.
- The final command reran that test and requested `cargo build --release --offline --bin ts2-viewer`; its completion result was lost at interruption. Source is saved, but neither final success nor binary freshness is established.
- Inspected pre-fix screenshots `out/continuation/level02-before.png` and `level04-before.png`, plus extracted `background02.png` and `background04.png`. These show the pictures exist (neighborhood sunset and construction skyline). No post-fix image has been inspected. All are local ignored outputs, not distributable assets.

### Unresolved findings and next actions

1. Run the focused final-arena and rotation regressions with game data, then the full release suite and release viewer build. Extend the boss test to verify attacks, real hit calls, defeat and rescue; the older test directly assigns health and never ticks creatures, and the new coupled test only checks movement/HUD so far.
2. Render and inspect matching post-fix background views, final-level suitcase/intro/fight views, and rotated see-saws. Preserve Level 01 behavior. Use offscreen `--shot` to avoid taking over user input.
3. Level 04: detailed static geometry clips at 12,000 view-depth units, while dynamic specials are unlimited; depth clears between far and near passes. Interior props can therefore appear over far walls. Original 0x4CDDD0 also uses room/portal selection (0x4BC460), absent here. No fix was made. Investigate near-layer-special range matching and room visibility without blindly clipping far-layer specials. Paint-room reference: level coordinates [9500,-4000,19050], zone 2; exterior walls zone 1.
4. Final-level falling: unresolved. `level05/TERR1.ALL` contains four static collision objects and no movers; do not invent a missing mover link or barriers. The displaced suitcase had no matching collision and may explain some falls, but that is an inference. Reproduce actual fall coordinates against collision after the visual correction.
5. Secondary boss discrepancy remains: original blacksmith/prospector proximity attacks require shared movement Step.flags bit 1; the port does not check it. Investigate before changing behavior.
6. Build an explicit coverage checklist and test scriptable interactions in campaign levels 02–15. This exhaustive pass has NOT been completed or claimed. Existing tests and script inventories are a starting point, not full playthrough coverage. Older front-end/gadget tasks below remain outstanding unless superseded by later verified entries.

## Progress and what's left (2026-10-21)

### Where it stands

| Area | State |
|---|---|
| Level 1 | Complete, signed off by the user |
| Levels 2–10 | Scripts ported; not playtested (the user is working through them in order) |
| Levels 11–15 | Scripts ported and simulation-tested; not playtested |
| Level 4 paint puzzle (token 3) | **User confirmed it works**, jingles included; they repeated (same-sample notes), now played once per run of one sample |
| Level 8 claw machine (token 3) | Ported and simulation-tested; not playtested |
| Rocket boots, hover boots, grappling hook | Ported and unit-tested; not playtested |
| Footsteps, footprint trails, wading slowdown | Ported; **user confirmed all three** |
| Stomp swirl (particles 0x12 / 0x13) | Ported; not checked in the viewer |
| Two-sided materials (fences, boards, cut-outs) | From the material bit (mode 8) in both mesh sets; level 1 confirmed. User still sees level 2 pool water drawing over everything; not reproduced in shots (needs their spot) |
| Front end | Title, menu and map confirmed by the user. New, needing a playtest: movies (logos, trailer, level intros, boss movies), results after normal levels, boss-defeated after boss levels, game over when lives run out (lives and abilities now carry across levels), "SAVE GAME?" (Slinky) and the load / save screens |
| Tests | 142 pass with `TS2_GAME_DIR` set; release build clean |

### Still to do, in order

1. **User re-tests** the paint jingles, then the claw machine, boots and hook (paint, fence
   and level 1 confirmed 2026-10-21). Fix what they report.
2. **Front end, rest of it:** done: title, menu, level select, results, boss defeated, game
   over, save prompt and the load / save screens (0x49D910's flow is in functions.md;
   scenes and variants in `campaign::variant_path`). Left: the pause menu (Escape quits to
   the menu for now), options (0x4371B0, scene 0 variant 2), the movie viewer (0x453FA0),
   credits (0x43A380, variant 3), loading pictures (`gfx\levelNx`), the
   attract demo, and the level select's per-level sound (0x4A3B90). The viewer switches
   scenes by rebuilding itself with `load()` and `attach()` on the same window.
3. **Saves:** done (`src/save.rs`, the original's format). Not kept yet: health between
   levels, the movies-seen and boss flags (+0x157 / +0x158, kept as loaded), volumes.
4. **Music changes:** done: miniboss and race themes (0x49EAC0, `music::theme`), and the
   front end's tracks (title, menu, results, game over, boss defeated). Needs a listen.
5. **FMV playback:** done (`src/fmv.rs`: own demux, `oxideav-mpeg12video` for the video in
   background runs of 4 GOPs, Symphonia for MP2). Needs a watch. The movie viewer menu (0x453FA0)
   isn't ported.
6. **Playtest-and-fix pass over levels 2–15** in campaign order.
7. **Smaller gadgets still skipped:** level 8's electric arcs (0x422D20) and room 2 cannon,
   level 11's lights / room 5 overlay / slot 31 constraint, drips, dynamic lights (0x44F200),
   floor shadows, the stomp's camera shake, the shield slowing Buzz (0x436220 reads 0x52F398:
   weaker jump −0x3C0, gentler acceleration; not ported).
8. **Side projects** the user wants afterwards: a tool to replace characters and textures
   (assets already load at runtime, so an override folder checked first is the natural
   route), and a web build (wgpu and winit run in the browser; audio via rodio and file
   loading need web versions).

### Recent findings worth knowing

- **Front-end scenes:** scene 0 is `level00\level.ngn` (pictures `bgr40`–`bgr44`, font `tex31`),
  scene 16 the level-select map `level06\level1.ngn`. The original's texture table is global
  and each scene only partly fills it, so the map uses scene 0's `tex31`: `load()` borrows it.
  Menus draw in a 320-wide sprite screen, the level select in 512.
- **Negative sound ids from level scripts** are jingles (pushed as `0xFFFx`); the viewer now
  routes them, which fixed level 4's paint sounds.

- **Vertex colour bytes:** the shader reads red, green, blue from bytes 1–3 (`color.yzw`), not
  0–2. Anything tinting vertex colours must offset by one (fixed for lit creatures).
- **Layer 1 is not just low detail.** The original draws the second mesh set only from 10000
  out, then clears depth and draws the first set. We draw layer 1 whole under layer 0, so
  anything missing from layer 0 shows layer 1's copy with props drawn over it.
- **Culling follows the materials** (`Material::two_sided`): mode bit 8 → cull-none
  (0x4B6760 sets render state 0x40), in both mesh sets; everything else is back-face
  culled, except pure-black faces of two-sided materials, which stay culled. An earlier
  thickness guess ("flat sheets") dropped billboards in thin meshes (level 1's carved
  headboards) and culled the far set's fences from behind; it's gone.
- **Screenshots without a window:** `ts2-viewer [--level N] --shot out.png "<script>"`
  renders offscreen, silent: numbers run frames, `+Space` / `-Space` press and release,
  `play` starts Buzz, `cam:x,y,z,yaw,pitch` holds the view. Use it to check rendering
  and menus yourself before asking the user.
- **Paint puzzle state:** the paint starts empty and colourless (height 0, light 0), as
  0x41C190 sets it. Specials 0x33–0x35 are the lit green signs, 0x30–0x32 the grey ones.
- **Footsteps** come from frame-list markers 0x81 / 0x82, now on `BuzzAnimator::feet`;
  `Player::foot_effects` ports 0x4A2D80's sounds and prints.
- **Shell traps:** a bare `cat >` or `python -` with no input hangs the whole command chain.
  Write scripts to the scratchpad and use the Edit tool for source changes.

## 2026-10-20: levels 11, 13–15 (Claude session)

- **Context:** a PS1 static recompilation exists (github.com/PeriBluGaming/ToyStory2Recomp).
  It's machine-generated C with almost no symbols, so it isn't ported from; it serves as a
  playable reference for how levels behave. The PC exe stays the source of truth.
- Levels 13–15 (written by the ChatGPT session, which hit its limit mid-level-14) were
  checked, formatted, given level 14 tests and committed. Two bugs fixed in them:
  `set_script` takes a **word** index (pass `0x1C / 2`, as levels 2–10 do; the drafts passed
  byte offsets), and `story.hints` must be set **before** `load_story` or the hint texts
  aren't loaded.
- Level 11 (Al's Penthouse) ported in `src/level11.rs`: water levels, valves, plug, pipes
  and the boat's routes, floats, six turrets and their buttons, doors, spring, horse
  challenge, critters, gunslinger. New creature routines 0x28 / 0x29 / 0x2E.
- `Boxes::pushed()` exposes 0x53C648. `campaign::script_available` is true for all 15.
- Verification: **117 passed** with `TS2_GAME_DIR`; release build clean.
- Then ported: level 4's "crane" (really a paint-mixing puzzle: the crane is a push box,
  slot 5 is the PAINT creature lit in the mixed colour; token 3), level 8's claw machine
  (token 3 is the prize), rocket boots, hover boots and the grappling hook (hook points =
  list 59, targeted in first person as 1000 + index; hook ammo = pickups `ammo_a`).
  Creatures gained `light` (+0x24–0x28) and `scale_y` (+0x2E); the viewer tints lit
  creatures by `light` and scales by `scale_y`. Level 4's `given[3]` is now false (the
  puzzle reveals it). 123 tests pass.
- Remaining (in order): front end and level select, saves, boss/race music changes, FMV
  playback; then the playtest-and-fix pass over levels 2–15. Small gadgets still skipped:
  level 8's electric arcs (0x422D20) and cannon, drips, lights, floor shadows.
- Lesson: `python -` (stdin) still hangs this shell. A command that starts with it blocks the
  whole chain; write scripts to the scratchpad. The capstone venv lives in the session
  scratchpad (`re/dis.py`: `dis|hex|i32 <va> <n>`).

## Latest continuation: neighbourhood scenery, music and gadgets

- User confirmed levels 3/6 ordering and signed off level 1. Focus is now level 2;
  Zurg polish is deferred until their ordered playthrough.
- Fixed missing neighbourhood house/fences/tree: zone 0 is not a camera-only zone.
  The static pass excludes special-object instances, not zone-0 scenery. Camera
  models remain under first-person control. Removed the old Z visibility toggle.
- Original stereo level themes loop on a separate rodio sink. **M** toggles music
  mute; SFX remain independent. `src/music.rs` reads names from exe table 0x4ECCD8
  and campaign ordering from 0x50268C. PCM8/16 mono/stereo decoder validates chunks.
- `TS2_CD_DIR` overrides the media root; local ignored `ts2-media.txt` is already
  configured to the private extracted CD folder under the Codex chat's work/disc.
  If that folder moves, update the config to the folder containing `audio/`.
- Level 2: loaded the two pushable objects from exe table 0x4F1518; ported the
  falling drawbridge and tree collision swaps, pump inflation, ball launch,
  buoyancy, stomp interaction and splashes. Box push/progress cells allow scripts
  to release a pushed object or reset tree progress through the existing interface.
- Remaining level-2 visual effects: kite string and sky effects. Music currently
  stays on the level theme; miniboss/race/completion theme changes remain to port.
- Headless regressions cover scenery inclusion and level-1 camera exclusion,
  all 15 music tracks, malformed/stereo PCM, pump/ball and bridge/tree transitions.
  No viewer launched: user continues visual and listening checks themselves.
- Verification: **107 passed** with `TS2_GAME_DIR` set and local CD music configured;
  `cargo build --release --bins` and `git diff --check` passed.
- Changes kept local for the user's preferred batch push. No game assets added.

## 2026-10-19 continuation: read before the historical notes below

**The missing-level diagnosis below was wrong. All 15 core asset sets are installed.**
Levels 11–15 are `data/level01..05/level1.ngn`, with `level1.dat`, `level1.raw`
and `TERR1.ALL` beside them. The original loader (0x452FC0) and Pconv configs confirm it.
All 15 sets passed parser checks with `TS2_GAME_DIR` set.

- `src/campaign.rs` separates campaign numbers from executable IDs. Campaign 3 is
  internal 6 (Bombs Away); campaign 6 is internal 3 (Slime Time). Do not swap only
  the scripts: sounds, starts, HUD and pickups must retain internal IDs.
- Viewer: `cargo run --release --bin ts2-viewer -- --level N [game_dir]`.
- Audit: `cargo run --release --bin ts2-tool -- audit-levels <game_dir>`.
- `src/level12.rs`: Zurg setup/frame logic, intro, attack timing, recovery, death,
  completion, HUD and camera. Animated node-1 projectile origin uses the renderer's
  matrices via `Creatures::load_animations` / `node_point`.
- `src/particles.rs`: Zurg bounce/trail routines 0x42B090 / 0x42B250, and the
  flag-8 Buzz homing path in 0x410F40. Creature-targeted homing remains unported.
- Level 12 is simulation-tested, **not visually signed off**. Lighting and the
  nearest-light marker are not ported. Playtest the intro, spin hits, retaliation,
  death fall, projectile muzzle position and camera.
- Levels 11, 13, 14, 15 still need their scripts. Existing level 2–10 omissions remain.
- Disc audit: 23 PCM WAVs decode cleanly; 22 MPEG video files are present. 21 report
  an MP2 packet error during decode (at the end of the sampled level-1 intro).
  `tracks.txt` is an old directory listing, NOT an authoritative track map.
- Verification: full `cargo test --release` with game data: **100 passed**;
  `cargo build --release --bins` passed. User preference remains to playtest themselves.
- Current user prefers batching pushes at the end; this supersedes the historical
  instruction below to push after every piece of work.

See `docs/re/asset-audit.md` and `MODLOG.md` for details. No original assets were
added to the repository or installed/modified in Program Files.

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
| Game install | `C:\Program Files\Disney Interactive\Toy Story 2` (all 15; see the correction above) |
| CD image | `C:\Users\alexs\Downloads\Disney-Pixar-Toy-Story-2-…\ToyStory2.iso`: music as WAVs (`Setup\Toy2\CD\audio\*.wav`, `tracks.txt`) and FMVs (`Setup\Toy2\CD\rtlibs\*.dll`). Later levels use level1.* under folders 01–05; see correction above. 7-Zip (`C:\Program Files\7-Zip\7z.exe`) can list or extract it. |
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
or level select. Levels 11–15 data was subsequently located; see correction above.

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
- music from the CD WAVs (`tracks.txt` is not a reliable mapping; verify against executable tables);
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

## 10. Where to find more
- `docs/re/functions.md`: every exe function and global identified, with behaviour notes.
- `docs/formats/`: the file formats.
- Each `src/levelN.rs` module doc: that level's script, objects, and what's not ported.
- `git log`: one commit per fix, with the reason in the message.
