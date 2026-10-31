# Continuation log — 2026-10-19

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
