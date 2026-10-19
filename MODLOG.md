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
