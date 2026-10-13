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
| 0x401000 | `StepBuzzSeq` ✔ | One entry per 0x10000 of rate. 0x81/0x82 footsteps (on ground), 0x83–0x86 sounds 0x30/0x10/0x17/0x43, 0xFE = done → state 0, 0xFF n = back n (n = 1 holds, fraction cleared) |
| 0x434090 | `StartJump(vy, kind)` ✔ | Clears on-ground (`0x52F39C`), state 2, vy = arg; normal callers pass −0xC00 |
| 0x4340D0 | `BuzzAirborne(obj, jumpVy, force)` ✔ | Jump start, variable height (release while rising: +0x180/k², then halve − 0x100/k²), double jump (−0x900/k after release+press, height-limited, clamp 0x22A–0x74C), gravity 0x100/k² per tick, terminal 0x800 (k = 2; k = 4 with flag 0x52F340&0x100). Jump phase at +0x8C: 1 held, 2 released, 5/6 double |
| | (0x4340D0 details) ✔ | Double-jump speed above the start: `sqrt(2·(0x100/k²)·(0x6A80 − h))` (peak 0x6A80 above the first jump's start, `0x53C674`; valid flag `0x53C61C`), truncated, clamped 0x22A–0x74C. Writes state 8 (`+0x90`) and plays sound 0x10. `+0x8E` = floor contact from collision (0x484380); `+0x9C` = 6-tick coyote counter (set while on floor, 0x436220), so jumps start while it's non-zero and the double jump needs it at 0. A press that can't double jump moves phase 2→3 (in practice only after special jumps: the release cut keeps normal jumps below 0x400 upward). Phase resets to 0 only on release while on the floor |
| `0x53C838` (data) | `g_fallTimer` ✔ | Counts while falling (vy > 0x80); > 60 ticks → 0x50 (panic, state 0xC, sound 0x16; a press with phase 0 gets a rescue double jump); landing at 0x50 → −0x46 (70-tick recovery, state 0xD, sound 0x2F, velocity zeroed) |
| 0x433ED0 | `ClearBuzzSpecials` ✔ | Zeroes fall/stomp/skid/etc. timers |
| 0x49E660 | `PlaySound(id, obj)` ✔ | Sound table 0x502950 (16 bytes per id) |
| 0x433F40 | `StickTarget` ✔ | Target facing `+0x48` = d-pad table 0x4F5AC8 + camera angle; returns magnitude (0x4000 = full) |
| 0x43BA80 | `PoseBuzzAnim` ✔ | Buzz's own pose builder (buffer 0x53EEE0, 0x6C/node). Base first, then overlay; the base's root node (6) moves point (0,−200,0) and that offset is added to every overlay node's translation (upper body follows hips, up to ~100 units). −3 advances the output node, −1/−2 don't (no shipped animation has −1/−2 before an animated node) |
| 0x43B0C0 / 0x43ACA0 | `LoadHierChars` ~ | Loads `chars\*.all` per creature into the pose system |
| 0x4A4BB0 | `BuzzSwim` ✔ | Drifts towards a target depth; Cross raises it; vy clamped ±0x180 |
| 0x490860 | `FrameTimer` ✔ | Ticks = elapsed ms × 60 / 1000 → **60 ticks per second**; alternative fixed mode is 2 ticks/frame |

### Buzz's controller (cluster 0x433xxx–0x436xxx; earlier mislabelled as creature AI)
| Address | Name | Notes |
|---|---|---|
| 0x436220 | `BuzzControl` ✔ | Fills a movement profile, runs the helpers below, scales velocity by ticks, calls move-and-collide |
| 0x4346C0 | `BuzzTurn` ✔ | Per tick facing moves min(diff, 0x300)/8 towards `+0x48`; diff ≥ 0x5DD snaps and (on ground) starts a 26-tick skid (`0x53C5D4`, state 0x12, sound 0x12). Stick taps < 16 ticks from standing only turn tap/16 of the way (`0x53C678`/`0x53C5D0`) |
| 0x4343D0 | `BuzzGroundAccel` ✔ | Velocity → forward/side (sin table 0x4FE788 = round(16384·sin), used >> 2). Side decays 0x80/tick, forward by friction (0x30; 0x10 in the air once coyote = 0; 4 during a skid), +accel+friction while pushing and below max 0x380·mag; clamp side ±0x380, forward ±0x400. Stores forward/side at +0x74/+0x78 |
| 0x434D20 | `BuzzStomp` ✔ | Circle edge in the air with phase 1/2 or state 8: timer `0x53C64C` = 1, sound 0x17; 14 ticks frozen, then vy = 0x800 with no horizontal motion; on floor → −40 (dust, sound 0xF), first 26 ticks frozen (state 0x18), no jumping until 0. State 0x17 while > 0 |
| 0x434550 | `BuzzMoveAndCollide` ~ | Applies velocity and resolves collision |
| 0x4354E0 | `BuzzClimb` ✔ (ported) | Poles = `level.dat` list 61, prepared by 0x414550: point pairs (bottom, top) → bottom × 32, kind (from a marker point with \|x\|=\|y\|=\|z\|, value / 50), length. Grab within (d >> 8)² ≤ 0x1FF horizontally, from 0x1E00 below the bottom to 0x3600 below the top (state 0x53C668 = 8, 10-tick jump lock 0x53C658). Up: vy −0x100; down: slide, +0x20/tick to 0x400, spinning facing by vy/16; nothing: stop; left/right turn 0x20/tick. Kind 2 slides only (+0x10/tick to 0x800). Jump: vy −0x400, push sin/cos × 8192 along the stick; at the top of a kind-1 pole: hop off with vy −0x600. Off the bottom: fall (phase 5, state 3). Released (−1) until 182 units away. States 0xF hang / 0xE up / 0x10 slide (sound 0x23) |
| 0x414550 | `PreparePoints` ~ | Converts level.dat lists 58–62 for runtime (poles above) |
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

Buzz states (`0x52F390`): 0 run (speed-scaled), 1 stand, 2 jump (rising), 3 fall, 4 land, 5 (from timer `+0x94` > 0x44, not the double jump), 8 double jump (wings), 0xC long-fall panic, 0xD fall recovery, 0x12 skid, 0x17/0x18 stomp, 10/11 teeter on a ledge (front/behind probe), 0x1C/0x1D idle fidgets, 0x1F swim; others are set by flags from other systems (hurt, specials) and not yet labelled. `0x52F39C` = on ground. Velocity `0x52F368/36C/370`.

`level.dat` list table: 0x559C70 + 4 × type (e.g. 0x559D58 = 58 rewards, 0x559D64 = 61 poles, 0x559D6C = 63). Reader: `src/formats/leveldat.rs`.

Ported in `src/game.rs` (`Player::tick`, 60 Hz, integer units) and `src/buzz.rs` (`BuzzAnimator`, the 0x401000 stepper): profile, stomp, airborne, turn, accelerate, coyote counter, fall timer, and the 0x4011D0 states 0–4, 8, 0xC, 0xD, 0x12, 0x17, 0x18 and the idle fidgets 0x1C/0x1D (rand.dat: 2,048 bytes loaded at 0x84C3E0, read pointer 0x52ADAC reset per level, wraps after 0x5DC; one byte per tick on idle entry 14, 1 in 4 fidgets). 0xFE also clears the standing sub-state 0x50A098. Collision data is the original (`TERRAIN.ALL`); the resolve is still ours.

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
| 0x489C30 | `InitCollision` ✔ | Loads `levelNN	errain|terr1|terr2|terr3.all` (mode bits), bounds per object, 5×5 grid. docs/formats/terrain.md |
| 0x489980 | `LoadTerrain` ✔ | Directory of 76-byte object records → object table 0x7295A8 (300 × 0x34); instances share template data |
| 0x485940 | `GatherCollision` ✔ | Polygons within ±0x500 of a point into one of 8 caches |
| 0x486520 | `FloorHeight` ~ | Height under a point from the gathered polygons (rotates type-8 movers by their level object's angles) |
| 0x486280 / 0x4863A0 | `FloorBelow` / `PlayerFloor` ✔ | Wrappers: gather + 0x486520; the player version probes from y − 0x400 and handles standing on movers |
| 0x484380 | `MoveAndCollideCore` ~ | Swept resolve: 0x482A00 (polygons) + 0x483EF0 (floor), 3–4 passes; writes floor contact to `+0x8E` |
| 0x49FAB0 | `SpawnChallengeReward` ~ | Matches type-58 positions to objects 0x71/0x73 |

## Rendering primitives
| Address | Name |
|---|---|
| 0x4A9400 | `MatIdentity` ~ |
| 0x4A9AA0 / 0x4A9CB0 / 0x4A9EC0 | `MatRotX` / `MatRotY` / `MatRotZ` ✔ (row vectors, 65536 = 360°) |
| 0x4A98D0 | `MatScale` ~ |
| 0x4A9750 | `MatTranslate` ~ |
| 0x4A9F60 | `MatMul` ~ |

## Sound (ported in `src/audio.rs`)
| Address | Name | Notes |
|---|---|---|
| 0x47EC20 | `LoadLevelSounds` ✔ | Slots from name lists: global (0x4FD140[0], from slot 1), level effects (0x4FD140[level], from 87), voices (0x4FD5E0[level], from 67); file `data\sfx\<name>.wav` |
| 0x47E5B0 / 0x47E420 | `LoadWav` / `CreateBuffer` ✔ | DirectSound buffer flags 0x80E0 (frequency, pan, volume); the sample's own rate is kept per slot (0x726234) |
| 0x49E660 | `PlaySound(id, obj)` ✔ | 16-byte entries at 0x502950: slot (0x4000 = per-level remap at 0x5028E8, pairs of level/slot), PS1 pitch (unused on PC), volume, second volume, pitch-variation shift. obj < 0x100 → fixed left/right (0x4A37E0), else positional (0x4A3810) |
| 0x49EA60 / 0x49EA90 | `PlayVoice` ✔ | Same, one voice per object (0x4A3E60 checks) |
| 0x4A3810 | `PositionalVolume` ✔ | Offset to the listener in camera space (x × 4, z × 2 per level unit); ears at x = ±0x400; per-ear volume = (0xC00 − distance) / 24 × vol / 128, 0–128 |
| 0x47DE50 | `StartSample(slot, left, right, …)` ✔ | Volume = dB table 0x4FD668[max(l, r)] (hundredths of a dB); pan = (r − l) × 10000 / 128 |
| 0x401000 markers | | 0x83–0x86 → sounds 0x30/0x10/0x17/0x43 |

Idle voice lines: 0x4011D0 decrements 0x50A0BC every tick; when a fidget starts and it's 0, it's reset to 0x708 and 0x49EA90 plays 0xCE / 0xCF alternately (last one in 0x50A0B0).
0x4A2D80 (`BuzzFootEffects`): frame-list markers 0x81/0x82 set 0x53C634 bits 1/2; this spawns dust or splash particles at Buzz's feet by floor surface (0x52B81C), via the particle spawner 0x40FDF0 (no sound).

Buzz's sounds: 0x19 jump (voice, BUZJMP1), 0x10 double jump (BUZWING), 0x12 skid, 0x17 stomp start, 0xF stomp impact (BUZSTOMP), 0x16 long-fall scream, 0x2F long-fall landing.

## Misc
| Address | Name | Notes |
|---|---|---|
| 0x4A6390 | `CheckCD` ✔ | validate.tta |
| 0x412D70 | `ConfigCDAudio` ✔ | "Base CD track is %d" |
