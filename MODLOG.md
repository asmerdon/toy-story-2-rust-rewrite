# Change log

Dated history of the work, newest at the bottom. Current state is in HANDOFF.md; per-commit detail is in `git log`.

## 2026-10-19: recovered levels 11–15, campaign order, Zurg

- Read README, HANDOFF and PROJECT; retained existing implementation and untracked screenshots.
- Route: continue the existing original Rust reimplementation, reading user-owned data.
- Corrected missing-level diagnosis: later assets are level01..05/level1.*, per original
  loader 0x452FC0 and shipped conversion configurations. All 15 core sets parse.
- Campaign/internal ID separation fixes requested 3/6 order without breaking tables.
- Added campaign CLI, repeatable asset audit, real-data campaign tests.
- Ported Zurg's level-12 setup and frame behavior, animated node projectile attachment,
  bounce/trails and Buzz homing. Real-data fight simulation reaches completion.
- Test corrections: empty point lists are legitimate in boss arenas; hit kind 1 deals
  two damage with this executable (ten hits take Zurg from 29 to 9), not one damage.
- Verification: 100 tests passed with TS2_GAME_DIR enabled; release binaries build;
  git diff --check clean. No viewer launched (user handles visual playtesting).
- Media: 23 WAV clean decodes; 22 MPEG videos named DLL, 21 with recoverable MP2 packet
  errors. Level-1 sampled error is at the end. See docs/re/asset-audit.md.
- No original data or decompiler output added to Git; no Program Files changes or save changes.
- Next: user visual validation of Zurg; port 11/13/14/15 from recovered data and original
  frame routines; finish outstanding gadgets in 2–10; music/FMV playback, menu and saves.


## 2026-10-19: level 2 and original music

- Removed blanket zone-0 culling: neighbourhood houses/fences/tree are zone 0.
  Special instances still have their separate draw path; first-person models stay hidden
  outside that mode. Regression confirms level 1's former zone-0 set is all specials.
- Music names from 0x413150 table 0x4ECCD8, level-to-campaign inverse from 0x4986D0
  table 0x50268C. Stereo PCM retained; dedicated looping rodio sink with M mute.
  Local ignored media config references the private disc extract, no assets committed.
- Ported 0x4190C0 pump/ball, drawbridge and tree behavior, including mover changes,
  sounds, splash requests and release of pushing state; 0x418E50 box list 0x4F1518.
- Integrated tests exercise inflation through repeated pad contact, buoyancy/collision,
  bridge fall, tree height gate and final angles. Existing level-1 box tests pass.
- Visual/audio playtesting left to the user. Kite string/sky, music theme transitions
  and FMV remain outstanding; no claim of full level-2 visual sign-off.
- Final validation: 107 tests passed with installed data and CD tracks; release binaries
  build successfully; diff whitespace check clean. Kept local for the batch push.

## 2026-10-31: recovery documentation for interrupted October 27 work

- Reviewed current Git state and commit `7c34cc9`; confirmed saved agent policy, mover rotation fix, background draw order, creature landing PC correction, final-level suitcase coordinates/attack rate, and new diagnostics/regressions.
- Updated README.md with a concise progress summary and HANDOFF.md with exact observed checks, failed approaches, verification gaps and prioritized resume steps.
- Baseline suite passed before edits; focused rotation and suitcase tests passed after their fixes. The coupled boss test failed before the last animation-rate correction; final test/build completion could not be recovered. No new tests were run for this documentation-only update.
- Background packet availability was audited across all 15 campaign levels; no post-fix screenshot validation or exhaustive levels 02–15 interaction pass was completed. Level 04 visibility and final-arena falls remain open.
- Preserved all user recordings/screenshots, private original data and existing implementation. No new commit or push performed for this update.
## 2026-10-31 (later): verification and playtest fixes

- Verified 7c34cc9: the targeted tests and the full suite pass, and the release viewer builds. Post-fix shots inspected (backgrounds, final-level suitcases).
- Sky fills match the background picture: the fills sample its corner texels (sRGB path), and are linearised without one.
- Boss levels keep their own music: only normal levels' scripts call 0x49EAC0.
- Slime boss wears its lid: special 0 follows node 10 each tick (0x41AA10 / 0x43C1C0). The node-point helper uses the rest pose for unanimated nodes.
- Level 2: ported the troop-in-the-holes stomp game (0x4190C0). Flag-0x20 creatures pin to target y (+0x60).
- Level 14 constant damage: creature contact and laser-ray maths in i64 (i32 wrapped on large levels).
- Level select: map variant 1 (`level1t1.ngn`), Etch A Sketch frame (0x494130, tables read from the exe), car heading (0x438910). Variant scenes read their base scene's .dat/.raw.
- Final level: the fall plane under the lowest collision point kills Buzz (0x436220), on all levels. Boss swings need the chase bit, plus particle and speed details. A new coupled fight test covers real attacks, hits, defeat and rescue.
- Spawns: ResetPlayer's floor snap (0x4863A0). Level 13 starts on the X-ray conveyor.
- Shot scripts: `tp:` and `?` steps. New docs/coverage.md checklist. Nothing committed.
- Level 4: the paint-room hatch folds open (0x41C640). Room/portal visibility (0x4BC460) and the 12000 special cut are applied to level 4 only. Level 1 shots are pixel-identical.
- Particles (0x410F40): flag-8 homing for every kind, using the original's 0xC4 constants (this changes Zurg's shots), plus the 0x2B / 0x2C floor rules. Final check: 160 tests pass in 36 suites; the release viewer builds.

## 2026-10-31 (third pass): second playtest round

- Collision: boundary walls (TERRAIN kind 0x101) loaded and swept for Buzz, the camera and rays. Belt pushes are swept (0x48B660 / 0x488E10). This closes the final arena's back and the airport's conveyor ends. Zurg's entrance height goes to target y.
- Level 14: removed the start-up write of 0x708 into every creature's flags (it belongs to the +0x42 draw distance). That stops Hamm and the race starting by themselves and every creature chasing Buzz.
- Viewer: cursor hidden and confined while playing. A look-at scene ends first person (0x4020F0).
- Rendering: specials drawn by their own layer's rules on every level (layer 1 beyond 1000 in the far pass; layer 0 within 12000 after the clear, 0x4CDDD0 / 0x4BC460). This fixes things showing through walls and z-fighting. Level 1 is identical indoors. 169 tests pass.
- Boss attack audit across the 5 boss levels and 10 minibosses. Divergences fixed with 15 regressions (`tests/boss_attacks.rs`); open items are in HANDOFF. 184 tests pass in 37 suites.

## 2026-10-31 (fourth pass): countdowns, docs consolidation

- HUD element 6 from 100 up draws the challenge countdown (two digits) and the collected count with icon 0x33 (0x49FD40). Before this, timed challenges and races showed no timer. The tick sound 0x5E is not ported.
- Confirmed the slime level's laser power-up (19-polygon `Kind::Timer`, 0x53C824 = 1200) appears from the slime's third round; renamed its doc comment.
- HANDOFF consolidated into one current-state section with open issues and next direction; dated history is now here and in git. README status refreshed. Added docs/opents2-brief.md (launcher/installer brief).

## 2026-10-31 (fifth pass): OpenTS2 phase 1, launcher and installer

- Researched OpenMW (config layering, data=/content=, wizard, launcher Data Files tab, packaging); summary and the phase 2–3 data model (layered VFS, `mod.toml`, editor scene files) in docs/opents2-design.md.
- Workspace: root `Cargo.toml` gains `[workspace]` with `crates/launcher`. Engine crate untouched apart from the viewer's window title ("OpenTS2") and window icon, and a `build.rs` that embeds the icon. No gameplay change.
- `crates/launcher` (`opents2`, egui/eframe 0.31 sharing the engine's wgpu/winit): game-folder detection and validation, optional disc folder (accepts the ISO's `Setup\Toy2\CD` layout), Play original / Play modded / Level tester, a Mods tab (scan, enable, order, dependency warnings; not loaded by the engine yet), About with disclaimer and credits. Settings in `%APPDATA%\OpenTS2\opents2.toml`.
- Icon: original Pizza Planet-style token drawn by `tools/icon/make_icon.py`; embedded in all executables and used as the window icon.
- `.cargo/config.toml`: static C runtime on MSVC.
- Installer: Inno Setup 6 script and `tools/installer/build.ps1` (per-user, licence/disclaimer page, shortcuts, uninstaller, no game files).
- Checks: `cargo build --release` clean; `cargo test --release` 194 pass with TS2_GAME_DIR (3 new end-to-end tests start the engine via the launcher's command lines with `--shot`; shots of the startup screen and level 4 inspected). Installer built (7.4 MB); silent per-user install to a temp folder contained only opents2.exe, ts2-viewer.exe, README.txt and the uninstaller; Start-menu and desktop shortcuts created; silent uninstall removed the folder, shortcuts and registry entry and kept %APPDATA%\OpenTS2 and saves. Shipped exes import no vcruntime. Launcher window not run by the agent (the user tests windows). Nothing committed.

## 2026-10-31 (sixth pass): launcher feedback

- Disc images: `crates/launcher/src/iso.rs` reads ISO 9660/Joliet; the Game Files tab finds a `.iso` in the disc folder (or via "Disc image…") and copies its music and movies to `%APPDATA%\OpenTS2\disc` with a progress bar. Tests: a synthetic Joliet image; the user's image (`TS2_DISC_IMAGE`, list + an `--ignored` full copy, 48 files byte-identical to 7-Zip's).
- Title-case buttons and tabs (Play Original, Play Modded, Level Tester, Game Files); green Play buttons, including a "▶ Play N. Level" button at the top of the Level Tester.
- Credits name the user as lewiloon2k (About, installer publisher).
- `examples/token_model.rs`: exports the Pizza Planet token (Level 1 special 0x39, `Shape395`, tex16) to out/ for the user's icon work. Not used in the build.
- Checks: release build clean (old `zurgintro` example warning only); launcher tests 9 + 3 end-to-end pass; installer rebuilt.

## 2026-10-31 (seventh pass): token icon, docs clean-up

- Icon: by the owner's decision, the app icon is now the game's Pizza Planet token, rendered tilted as in game by `tools/icon/make_icon.py` from the `token_model` export (replacing the drawn placeholder). Used for every exe, the window icons, the launcher header and Play tab, shortcuts, and the installer (setup icon plus wizard pictures `wizard-*.bmp`). PROJECT.md records this as the one exception to the no-game-data rule.
- Docs: PROJECT.md rewritten as the stable overview (ground rules, source material, architecture, history, roadmap, machine setup); HANDOFF gains a doc map, drops setup duplicated in PROJECT, and its stale level-2–10 plan; README gains a doc table; format docs corrected where later work resolved their open questions (animations, patches 0x208, vertex colour order, player start, mover carry); functions.md's dated sections turned into tables; asset audit notes music and movies are played.
- Removed `MODDING_PLAN.md` (stale) and `docs/opents2-brief.md` (done; superseded by the design doc; in git history). The owner's recordings, screenshots and the old ChatGPT log moved to `local/` (git-ignored).
- Checks: release build clean; 196 tests pass with TS2_GAME_DIR and TS2_DISC_IMAGE; installer rebuilt with the token icon and wizard pictures. The launcher window and installer pages were not opened by the agent.
- Icon redone at the owner's request: the flat face-on picture of the token rotated 48° in 2D (`make_icon.py` `TILT`), instead of a 3D view.

## 2026-11-01: playtest round 3 (stomps, level 9 boss, Zurg intro)

- Level 7: "Ride the HayBale" ported from 0x421340 (it was listed as not ported, so the stomp switch right of the start did nothing): the button tips, a 45-second challenge countdown, the bale bobs and rocks, and standing on it throws Buzz towards the shelf. The egg challenge's clock pauses during a ride, as in the original.
- Stomp buttons on movers use 0x48B640 (`Player::touching_mover`: floor or any contact) on levels 4, 7 and 10. Level 4's lift buttons now tilt their collision -0x180 like their specials (was a quarter).
- New `tests/stomps.rs`: all 33 stomp switches landed on with Buzz's physics. New level 14 test: a real jump-and-stomp on a helicopter button takes a press. The user's level 14 report is not reproduced.
- Levels 7 and 11: far-set special copies were placed at a quarter of their position (the exe's `>> 7` assumes 0x4CCE30's x4); now level units.
- Level 9: flail tips on animated node 1 (0x43C1C0) in model units; the minions were called up inside the boss. Flails' animations half a cycle apart; the calling flail plays animation 1 (sequence 0x17) and throws sparks (node 4).
- Viewer: frame times within 12% of a whole (or half) tick count as exactly that (`game::snap_frame_time`), so vsync jitter no longer runs 0 then 2 ticks; aimed at the jittery Zurg entrance, whose simulation is smooth tick to tick.
- Token exit: the exe's victory (0x4A0F80) is an ordinary jump (state 2). Deviation at the owner's request: `Player::fly_off` spreads the wings (state 8) and climbs 0x400 a tick until the level ends (test `fly_off_spreads_the_wings_and_keeps_climbing`; not seen in a shot, the viewer was open).
- Diagnostics: `floor_at`, `object_tris`; `zurgintro -v`.

## 2026-11-01 (second pass): playtest round 3 follow-ups

- Token exit: Deviation at the owner's request, Buzz spreads his wings and flies up until the level ends (`Player::fly_off`); the user confirmed it looks better.
- Viewer: draws between ticks (Buzz, creatures, follow and scene cameras), so steady motion no longer judders on the user's 144 Hz display; the scene camera is refreshed after the scripts run. Aimed at the jittery boss entrances (levels 9 and 12).
- Level 12: Zurg's chase case in 0x4076F0 (type 0x39) ported: he keeps 2000 from Buzz, higher while retaliating; he used to fly straight onto Buzz.
- Level 9: the beam comes from the eye (node 0, (0, 0, -400)), not the base.
- Level 13: floor types 4-7 (the oil, type 5) have little grip (0x436220).
- Not reproduced: level 9's "clips inside the boss for a second" after a call-up.
- Checks: workspace release build clean; 204 tests pass (193 engine + 11 launcher).

## 2026-11-01 (third pass): playtest round 4

- Level 9: the scene camera kept following the burst flail's tip after its slot was cleared, so it jumped into the boss; the tips are kept as last seen alive.
- Level 3 (Bombs Away): creature matrices now turn Z first (0x4CD880 / 0x450C70), so the plane banks round its length in its intro instead of pitching like a hit.
- Gadgets: the animation states for the rocket boots (10), hover boots (0x1F) and grapple pull (0x1E); the rocket boots' flames (particles 0x2F, kind 0x15) ride on Buzz's heels with exhaust for the whole boost.
- Sliding floors (type 0xD, levels 11 and 15) ported: slide downhill, slow turning, state 0x20. Buzz's vertical floor probe now reports the floor's surface type and normal too.
- Level 2: leaves fall from the tree (list 0, particle 0x37).
- Level 5: sparks run along the zip wires (list 62, particle 0x4C every 0x14 ticks), hurting Buzz unless he spins.
- Levels 5 and 14: rain (0x44ED90 / 0x44F010) and its splashes (`src/rain.rs`).
- Not done: the disc launcher and the HUD ammo count (ammo is collected but discs can't be fired); level 14's lightning.
- Checks: workspace release build clean; 210 tests pass (199 engine + 11 launcher).

## 2026-11-01 (fourth pass): disc launcher

- The disc launcher ported (0x434990, 0x4A4910, 0x4A4960, particle kinds 0x1D / 0x1E): with ammo B the fire button throws discs, homing on the nearest creature (or the first-person target) or flying straight, at most 6 in flight; homing discs hit (HitCreature kind 4) or bounce off creatures that can't be hurt. The HUD's weapon meter shows the disc or hook count (0x49FD40).
- A `cargo fmt` run reformatted the whole workspace by mistake; it was undone (untouched files restored from git, edited files re-merged onto their original layout; a few edited spots keep rustfmt's layout).
- Checks: workspace release build clean; 213 tests pass (202 engine + 11 launcher).

## 2026-11-03 (evening): playtest round 7

- Level 2 leaves and bushes: their faces are blended (mode 0x02) but wholly opaque, drawn unsorted without depth writes, so a bush's back or a far bush showed over the near one. Such faces are now drawn alpha-tested with depth writes (deviation, README).
- Movers, from the mover's frame: Buzz's start-of-tick position, carried with each mover that moved, is swept to where he ends against that mover alone (sliding as the main sweep does, then clamped by everything else); taken when it moves him more than 2 units. With the floor probe from round 6.
- Mantle onto a mover (0x435F30 sets 0x559E80; 0x488E10): that mover stands still while he climbs, and on level 10 every mover does (`World::set_hold`, `hold_all`).
- Level 10's lifts run only once the three switches are solved (0x425F60 tests 0x52FC14 & 4).
- Level 4's scoop (drawbridge, mover 0x1A): Buzz standing on it turns with it (0x4896A0).
- Viewer: special objects are drawn between ticks (`Interp::special`), so lifts no longer judder against the interpolated camera.
- Deviation (owner's request): a double jump on the rocket boots keeps its wings.
- Checked, not changed: power-ups come back 400 ticks (6.7 s) after being taken, one at a time (0x4A0F80); the pool float's collision is the data's flat slab under its back (its top within 10 of the drawn back; the head has none).
- Checks: workspace release build clean; 222 tests pass.

## 2026-11-03 (later): playtest round 6

- Movers: each mover remembers where it was before its first move or turn in a tick (`World::motions`). After the sweep, Buzz's start-of-tick feet are carried with each mover that moved; if they were over its floor and now his feet are under it, he's put on top. A platform rising into a jumping or bouncing Buzz (level 2's pool float, level 4's platforms, every mover in levels 2, 4, 7 and 10) used to pass straight through him.
- Level 10's hook points: drawn as sprite 0x1E (300 × 500, standing on the point, within 13312 of the camera; 0x440F70); dropped onto the floor under them at load (0x447DB0); targetable within 8192 and only on screen (0x4BA1F0, which also rules out the one under Buzz's feet); the marker sits 0xD0 above the point (0x4038E0). Test: every climb on level 10.
- Level 10's electricity in rooms 7 and 12 (0x425F60): arcs along list 12's pairs, sparks rising from list 11, and list 60's sparks (at the list's raw values, as the original spawns them).
- Level 4: the far-set copies follow what they copy: the presses (+0x12, 0x41BC20), the hanging platforms (+4, with their angles, 0x41BEE0) and special 0xD (0xC). They stayed at their start when the camera was more than 1000 away, so a press showed where it wasn't.
- Pickups: a power-up coming back after its 400 ticks sparkles if the camera is near (0x4A0F80). The 400-tick timer is the same for hooks, discs and hover boots; rocket boots come back when they run out.
- Viewer: `--shot` teleports keep the facing given.
- Checked, not changed: the rocket pose overrides the double jump's wings in the original too (0x4011D0 sets state 10 while 0x53C5E0 runs, after 0x4340D0 writes 8).
- Not reproduced: level 2's missing leaf section (every leaf platform with a coin on it is drawn where its collision is; the fern near the far fence has open fronds over solid collision, as in the data).
- Checks: workspace release build clean; 220 tests pass.

## 2026-11-03: playtest round 5

- Particles: the spawn-mode table has 29 entries (0..0x1C), not 18; everything spawned with modes 0x12–0x1C (level 2's conkers, mode 0x13) had no velocity or gravity. Kind 0x17 lands on its floor; the death actions of 0x410B80 ported (splats, dust, drops). Fixed the disc's bounce (it switches to kind 0x1E) and hit (death action 8).
- Moving platforms: a mover whose floor comes up past Buzz's feet (up to 64 a tick) picks him up; motion is cleared after the carry, so movers moved after the tick carry too; `place_mover` records motion. Test: every flat-topped mover in levels 4, 7 and 10.
- Grappling hook: in first person the target lock no longer breaks on any mouse movement (40 pixels of movement does), so the hook can be thrown with a hand on the mouse.
- Level tester: levels opened on their own start with every ability, so gadget cages are open.
- Checks: workspace release build clean; 216 tests pass.
