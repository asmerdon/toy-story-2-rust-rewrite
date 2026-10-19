# Asset audit — 2026-10-19

Tested against the local 1999-10-29 PC executable and the user-supplied ToyStory2.iso
(655,325,184 bytes). No alternate source is currently required for core level data.

## Level layout

The `level11`–`level15` directories contain converter configurations, not runtime assets.
Their `OUTPUT_FILE` values point to earlier directories. `InitLevelPlay` at 0x452FC0
subtracts 10 and selects the `level1` file variant for later levels.

| Campaign | Internal ID | NGN path below data | Collision |
|---|---|---|---|
| 1, 2 | 1, 2 | level01, level02 / level.ngn | TERRAIN.ALL |
| 3 Bombs Away | 6 | level06/level.ngn | TERRAIN.ALL |
| 4, 5 | 4, 5 | level04, level05 / level.ngn | TERRAIN.ALL |
| 6 Slime Time | 3 | level03/level.ngn | TERRAIN.ALL |
| 7–10 | 7–10 | level07..10/level.ngn | TERRAIN.ALL |
| 11–15 | 11–15 | level01..05/level1.ngn | TERR1.ALL |

The DAT and RAW companions use the same stem as the NGN. All 15 sets successfully
parsed NGN textures/models, DAT point lists, RAW spawns and terrain collision. Buzz
is present in every model set. Boss-only levels can legitimately have zero point lists.
This establishes core asset availability, not complete gameplay or every auxiliary format.

## Disc media

- `Setup/Toy2/CD/audio`: 23 WAV files, all fully decoded by FFmpeg without errors.
- `Setup/Toy2/CD/rtlibs`: 22 video files with `.dll` names; these were parsed as media,
  never executed. They include level-intro/boss clips through level 15, an ending,
  logos and a trailer. The sampled intro has MPEG-1 video (320x208) and MP2 audio.
- All 22 decoded with normal FFmpeg error recovery. 21 reported one invalid MP2
  packet each. With `-xerror`, the level-1 intro stopped at 46.41 seconds of a
  reported 46.45 seconds. This suggests a terminal-packet issue for that sample;
  it does not prove the same cause for every clip or pristine source integrity.
- `tracks.txt` is a dated directory listing (some file sizes differ from the actual
  WAVs). Do not use it as a proven per-level track mapping.
- Playback integration is still pending. Original media stays outside Git.

## Verification

`ts2-tool audit-levels <game_dir>` is a repeatable parser audit. Real-data integration
tests validate all 15 paths and identify the swapped bosses by creature IDs 48 and 16.
Full regression suite after this work: 100 tests passed with TS2_GAME_DIR enabled;
release viewer and tool both build. No viewer launched, per project handoff preference.

## Research references

Existing format research: https://github.com/mouksx/Toy-Story-2-Modding
and editor: https://github.com/EpicMinecartz/ToyTwoToolbox .
The findings above were verified from this user's files and existing local RE notes,
not assumed from those repositories.
