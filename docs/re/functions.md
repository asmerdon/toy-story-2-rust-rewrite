# Function map: `toy2.exe` (PC, 1999-10-29 build)

Addresses are virtual addresses in the original exe (image base 0x400000). Names are ours.
Confidence: **✔** confirmed by behaviour or data round-trip, **~** strong inference, **?** guess.
Decompiler output lives in git-ignored `re/out/` (see PROJECT.md). Never commit it.

## Game loop
| Address | Name | Notes |
|---|---|---|
| 0x49D910 | `MainLoop` ~ | 12-case game-state switch; calls the frame update |
| 0x49DFE0 | `GameFrame` ✔ | Per-frame update; calls ~30 subsystems in order (below) |
| 0x452FC0 | `InitLevelPlay` ✔ | Debug strings "InitLevelPlay : START/END"; loads rand.dat, level.dat, level.raw |
| 0x47EC20 | `LoadLevelSounds` ✔ | Per-level WAV lists from tables 0x4FD140 / 0x4FD5E0 |

`GameFrame` calls, in order, with what we know:
| Address | Name | Notes |
|---|---|---|
| 0x49EBA0 | `ReadGamePad` ~ | Small; touches the in-game pad `0x52AD88` and the menu pad `0x88279C` |
| 0x402A10 | ? | |
| 0x436220 | `UpdateCreatures` ~ | Dispatches to the 0x434xxx/0x435xxx per-enemy helpers; sets animation slots |
| 0x405860 | `UpdatePlayer` ~ | Calls 0x4038E0 and 0x4045E0 (pad-driven movement) |
| 0x4CE050 | ? | engine-side |
| 0x4A0F80 | ? | 13-case switch |
| 0x4011D0 | ? | |
| 0x4A2D80 | ? | uses the player pointer heavily |
| 0x49FB40 | `ChallengeRewards` ~ | Uses the type-58 positions from level.dat |
| 0x410F40 | `UpdateLevelObjects` ~ | 53-case switch on object byte +0x2F (pickups, grapple points: "GRAPHOOK") |
| 0x49FD40 | `TokenMessages` ~ | "you have collected a token" |
| 0x4A3EF0 | `FlushSoundVoices` ✔ | debug string |

## Player
| Address | Name | Notes |
|---|---|---|
| `0x52F300` (data) | `g_buzz` ✔ | Buzz's object: +0/+4/+8 position (level units × 32), +0xE facing (12-bit), +0x12/+0x16 animations |
| `0x52F34C` (data) | `g_checkpoint` ~ | Respawn position, updated by 0x4A28F0, cleared to 0xFFFE2584 by 0x42B2D0 |
| `0x882940` (data) | `g_fallRecovery` ~ | Position restored by 0x4A5540 (y + 0x1600) |
| `0x4F59A4` (data) | `PlayerStartTable` ✔ | 16 bytes per game level 1..15: i32 x, y, z (×32), i16 facing. Verified against every level's floor (tests/player_start.rs) |
| 0x414720 | `InitPlayerForLevel` ✔ | Calls ResetPlayer, then sets the camera from Buzz (0x403450) |
| 0x433D50 | `ResetPlayer(obj, level)` ✔ | Clears the struct, loads the start table entry, snaps to floor (0x4863A0) − 0x100, sets facing, animations = 0xFFFF |
| 0x440260 | `SetPlayerPos(x, y, z)` ✔ | Used by creature code to move Buzz |
| `0x52ADAC` (data) | `g_playerPtr` ~ | Pointer used by many systems (camera/targets?) |
| `0x52AD88` (data) | `g_pad` ✔ | In-game pad bits, PS1 layout (below) |
| 0x4038E0 | `PlayerLockOn` ✔ | Collects targets in range; R1 (0x800) cycles the lock-on target |
| 0x4045E0 | `PlayerMove` ~ | 601 lines; facing angle × sine table 0x4FE788, recursive |

| 0x4011D0 | `UpdateBuzzAnimState` ✔ | Picks Buzz's state (`0x52F390`) and plays its row of the move table into slots +0x12 (overlay) / +0x16 (base) |
| `0x4DF3F0` (data) | `BuzzMoveTable` ✔ | 33 × 20 bytes: frame-list ptr, overlay anim, base anim, rate (16.16 per tick; negative = scaled by horizontal speed), flag. Uses all 35 animations (tests/player_start.rs) |
| 0x401000 | `StepBuzzSeq` ~ | Frame-list step with event markers ≥ 0x80 |
| 0x434090 | `StartJump(vy, kind)` ✔ | Clears on-ground (`0x52F39C`), state 2, vy = arg; normal callers pass −0xC00 |
| 0x4340D0 | `BuzzAirborne(obj, jumpVy, force)` ✔ | Jump start, variable height (release while rising: +0x180/k², then halve − 0x100/k²), double jump (−0x900/k after release+press, height-limited, clamp 0x22A–0x74C), gravity 0x100/k² per tick, terminal 0x800 (k = 2; k = 4 with flag 0x52F340&0x100). Jump phase at +0x8C: 1 held, 2 released, 5/6 double |
| 0x4A4BB0 | `BuzzSwim` ✔ | Drifts towards a target depth; Cross raises it; vy clamped ±0x180 |
| 0x490860 | `FrameTimer` ✔ | Ticks = elapsed ms × 60 / 1000 → **60 ticks per second**; alternative fixed mode is 2 ticks/frame |

### Buzz's controller (cluster 0x433xxx–0x436xxx; earlier mislabelled as creature AI)
| Address | Name | Notes |
|---|---|---|
| 0x436220 | `BuzzControl` ✔ | Fills a movement profile, runs the helpers below, scales velocity by ticks, calls move-and-collide |
| 0x4346C0 | `BuzzTurn` ✔ | Turns facing towards the stick at the profile's turn rate |
| 0x4343D0 | `BuzzGroundAccel` ✔ | World velocity → forward/sideways; accelerate/brake/clamp by profile; back to world |
| 0x434D20 | `BuzzStomp` ✔ | Circle in the air: hang, then slam at 0x800 (states 0x17/0x18), dust + 40-tick recovery on impact |
| 0x434550 | `BuzzMoveAndCollide` ~ | Applies velocity and resolves collision |
| 0x4354E0 | `BuzzClimb` ✔ | Pole/ladder: up/down moves (cap 0x400, slide 0x800), jump off −0x400 + facing push, or −0x600 |
| 0x4359D0 / 0x435100 / 0x435F30 | other movement modes ? | |

Default ground profile (object units per tick; ÷32 × 60 → level units per second):

| Field | Value | Level units |
|---|---|---|
| Max forward speed | 0x380 | 1,680 /s |
| Acceleration while pushing (accel 0x28 + friction 0x30) | 0x58 /tick | 9,900 /s² |
| Friction | 0x30 /tick | 5,400 /s² |
| Normal jump | −0x600 | 2,880 /s up (full height only while held) |
| Double jump | −0x480 | 2,160 /s up |
| Turn-rate field | 0x400 | |
| Gravity (airborne, 0x4340D0) | 0x100/k² = 0x40 /tick | 7,200 /s² |
| Terminal fall | 0x800 | 3,840 /s |

Alternate profiles in 0x436220 use accel 0x10/0x08 and jumps of −0x640 / −0x3C0 / −0x300 (air control, special surfaces; not yet labelled). `StartJump(−0xC00)` is a special, bigger jump.

Buzz states (`0x52F390`): 0 run (speed-scaled), 1 stand, 2 jump (rising), 3 fall, 4 land, 10/11 teeter on a ledge (front/behind probe), 0x1C/0x1D idle fidgets, 0x1F swim; others are set by flags from other systems (hurt, specials) and not yet labelled. `0x52F39C` = on ground. Velocity `0x52F368/36C/370`.

Game level numbers: `levelNN\level.*` = level NN, `levelNN\level1.*` = level NN+10 (InitLevelPlay subtracts 10 and switches to the `level1` files).

## Cutscene scripts
| Address | Name | Notes |
|---|---|---|
| 0x402610 / 0x4027F0 | `StartCutscene(id)` ~ | Saves Buzz state, looks up id in the table at 0x50A150 (16 bytes: id, level.dat table, sub-script), runs the script at 0x4DF69C / 0x4DF6CC |
| 0x402A10 | `RunCutsceneScript` ✔ | Opcode 0 = select level.dat point table; 1 = place Buzz at point k (×32, snap to floor); … |

### Pad bits (PS1 layout, confirmed by "press jump to exit" testing 0x4000)
| Bit | Button | | Bit | Button |
|---|---|---|---|---|
| 0x0001 | Select | | 0x0100 | L2 |
| 0x0008 | Start | | 0x0200 | R2 |
| 0x0010 | Up | | 0x0400 | L1 |
| 0x0020 | Right | | 0x0800 | R1 |
| 0x0040 | Down | | 0x1000 | Triangle |
| 0x0080 | Left | | 0x2000 | Circle |
| | | | 0x4000 | Cross (jump) |
| | | | 0x8000 | Square |

## Objects and animation
| Address | Name | Notes |
|---|---|---|
| 0x405CF0 | `SetAnim(obj, anim, seq)` ✔ | anim → +0x12, sequence pointer → +0x74, time from first byte |
| 0x405C80 | `StepAnimSeq(obj)` ✔ | One frame-list step per tick; 0xFF codes hold/stop/loop |
| 0x4E0588 (data) | shared sequence table ✔ | 25 entries (1-based) |
| 0x4CD880 | `DrawAnimatedObjects` ✔ | Builds root matrix (X, Y+0x800, Z), plays slots +0x12/+0x16 |
| 0x4CD170 / 0x4CD1B0 | animation header / node sampler ✔ | see docs/formats/anim.md |

Object fields seen by the renderer: `+0x00..0x08` position (int, ×1/32 when drawn), `+0x0C/+0x0E/+0x10` rotation, `+0x14` creature slot, `+0x12` animation, `+0x16` second animation, `+0x18` time (16.16), `+0x1C` second time, `+0x74` sequence pointer.

## Loading
| Address | Name | Notes |
|---|---|---|
| 0x4CA420 | `LoadCreature` ✔ | `.ngn` chunk 0x106 tags 0x200–0x208 (hobjload.c) |
| 0x4CA040 | `StoreAnimation` ✔ | Raw 0x204 blobs, up to 100 per creature |
| 0x4CA1D0 | `LoadPatches` ✔ | Tag 0x208 |
| 0x4CB320 / 0x4CB4E0 / 0x4CB970 / 0x4CC970 | mesh tag readers ✔ | objload.c |
| 0x452310 | `LoadPackets` ✔ | "Loading packet data %s"; type 35 creature list, 36 map |
| 0x47B170 | `Unpack` ✔ | LZ decompressor (docs/formats/packets.md) |
| 0x43E6E0 | `ParseLevelDat` ~ | level.dat sections 1–3 |
| 0x49FAB0 | `SpawnChallengeReward` ~ | Matches type-58 positions to objects 0x71/0x73 |

## Rendering primitives
| Address | Name |
|---|---|
| 0x4A9400 | `MatIdentity` ~ |
| 0x4A9AA0 / 0x4A9CB0 / 0x4A9EC0 | `MatRotX` / `MatRotY` / `MatRotZ` ✔ (row vectors, 65536 = 360°) |
| 0x4A98D0 | `MatScale` ~ |
| 0x4A9750 | `MatTranslate` ~ |
| 0x4A9F60 | `MatMul` ~ |

## Misc
| Address | Name | Notes |
|---|---|---|
| 0x4A6390 | `CheckCD` ✔ | validate.tta |
| 0x412D70 | `ConfigCDAudio` ✔ | "Base CD track is %d" |
