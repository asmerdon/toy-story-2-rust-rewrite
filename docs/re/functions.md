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
| 0x4A0F80 | `UpdatePickups` ✔ | See Pickups below |
| 0x4011D0 | `UpdateBuzzAnimState` ✔ | |
| 0x4A2D80 | `BuzzFootEffects` ✔ | |
| 0x49FB40 | `RewardSparkles` ✔ (`src/rewards.rs`) | Spawns sparkle particles 0x71/0x73 at `level.dat` list 58 points within 600 (>> 8) of the camera, four points per frame. List 58 is split by a (0,0,0) point: group 0 (count `0x830E28`) for pushable-box spots (0x433700), group 1 for challenge rewards; 0x49FAB0 kills a point's sparkle |
| 0x410F40 | `UpdateParticles` ✔ | 64 particles × 0x3C bytes at 0x529E58 (earlier mislabelled as level objects; "GRAPHOOK" is the sine table aliasing a string). 53-case switch on the particle's update kind (+0x2F) |
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
| 0x4045E0 | `CameraUpdate` ✔ (main path ported, `src/camera.rs`) | Struct: pos +0, target +0xC, height followers +0x18/+0x1C, look pitch +0x20, look yaw +0x22, distance +0x26 (eases to 0x50A128 = 0x4B0 by 0x20/tick), yaw +0x28, pitch +0x2E (rests at 0x40), whisker state +0x32. Lag 0x52AD98 = 0x10 (position moves 1/(lag/2) per tick). Auto mode (0x52F0CC & 0x40, on by default): moving → yaw −= diff/(2·0x2A) towards Buzz's facing (0x60 while skidding; diff > 0x600 turns the other way ×3); standing still (0x50A534 timer) → swing behind him. Manual mode: L2/R2 orbit 0x10/tick, Triangle snaps behind. Line of sight: 0x48A4C0 / 0x48C860 whiskers (left, right, up) nudge yaw ±0x60 and pitch −0x200..0x300; 0x48D530 pulls the camera in. Stomp shake 0x50A510. Close to Buzz (< 400 units) it rises |
| 0x403450 | `CameraInit` ✔ | Behind Buzz at 0x4B0, 0x3200 above the floor there, pitch 0x40 |

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
| 0x4A4BB0 | `BuzzHover` ✔ | Hover boots (was mislabelled swim): while timer 0x882920 runs (600 ticks from 0x4A4B70, a pickup), Buzz drifts to a height 0x882928 above the floor (0x2000–0xC000; Cross raises it 0x400/tick, else it sinks), vy ±0x20/tick capped ±0x180, jet particles 0x51–0x53, sound 0x50; anim state 0x1F |
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
| 0x4359D0 | `BuzzZipLine` ✔ | `level.dat` list 62 (point pairs). Airborne with no other state bits and 0x53C600 = 0: within the segment's box (x/z ± 0x800, y from 0x1000 above to 0x3E00 below), within 90 units of the line (measured along its dominant axis) and 0–624 below it, over 200 from its end: 0x53C618 = 1 (held in x/z while dropping to 0x3E00 below), then 2 (riding: sound 0x23, speed 0x53C604 +1/tick to 0x30, position along the unit direction 0x53C638, facing eases along it, state 0x11). End or a fresh jump (after the 10-tick lock 0x53C658): off with vy −0x5C0, speed × facing, jump phase 1, state 3, voice 0x19 if jump held; 0x53C600 = 30 |
| 0x435100 | `BuzzLedge` ✔ | `level.dat` list 60. No state bits but ground/laser/0x400: feet 0–0x4000 below a ledge and within 64 units of it: sound 0x1B, 0x53C66C = 0x45, state 0x1A (animation restarted), hang point 0x4000 below the line, facing the perpendicular nearest his own. Above 0xF: eases 1/16 a tick to the hang point and the facing, no motion (returns 2). Crossing 0xF: vy −0x600, push sin/cos(facing) >> 1, phase 2 unless jump held, voice 0x19 (returns 1); bit 0x400 while > 0xF |
| 0x435F30 | `BuzzMantle` ✔ | Falling (vy > 0), no state bits, ground over 0x2000 below: probe 1/3 × 16384 ahead and 0x3600 up with 0x486280; a flat floor (normal y < −15000) whose top − 200 lies between the last and current head heights; sphere sweeps (radius 4000) up from the feet and across the top (from 0x1838 above it, 4/3 of the probe) must be clear. Then sound 0x17, dust (0x440260), Buzz on the top (feet = top − 200), 0x53C660 = 0x52 (state bit 4, animation state 9), facing to the wall's normal. While > 0: facing eases 1/16, returns 7 (no motion) |

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

## Pickups (ported in `src/pickups.rs`)
| Address | Name | Notes |
|---|---|---|
| 0x447DB0 | `SetupPickups` ✔ | Copies `level.dat` list 63 (16 bytes: i32 x, y, z, then id byte, radius byte, i16 floor) to 0x5546B8 with radius 0x11 and the floor height (0x486310 from y − 10; none if > 0x1800 below), then appends every special object from the first id (0x30; 0x60 in levels 4/10/11/14, 0x50 in 5, 400 in 16) with radius = object size >> 3 |
| 0x44E520 | `PickupKind(id)` ✔ | By the PS1 model's polygon count (0x43E430): 6 cutscene, 0x12 life, 0x13 timer `0x53C824`, 0x14 hover boots, 0x1E ammo B, 0x1F/0x32/0x50 challenge item, 0x20 battery, 0x24/0x48 token, 0x27 0x4A4D60, 0x3C shield (0x4A50D0: 900 ticks in `0x52F398`), 100 ammo A; else −1 |
| 0x440F70 | `DrawLevel` ✔ (pickup part) | Ids < 0x30: billboard sprites (table 0x557500 ← 0x4EC0A4, coin = 16: texture 31, 31-px cells, 10 frames, two frames apart per entry), half-size 100, alpha fades near the draw distance `0x54BEF0` = 0x680 (horizontal, >> 4); shadow: flat, texture 31 cell (96..127), half-size 80, 10 above the floor, 0x88000000. Special objects tumble by (10, ((i>>2&3)·3+7)·2, ((i&7)+4)·2) per tick (12-bit). Sets +0xD bit 0x80 on everything in range |
| 0x4A0F80 | `UpdatePickups` ✔ | Collects marked entries: ((Buzz − (0, 230, 0) − p) >> 3)² < (radius + 14)². Coins +1 (99 max; 50 → sound 0x4F), battery +4 health (14 max, `0x52F396`), life +1 (9 max, `0x52F39A`), token → bit in `0x52F0D7[level]` and the token sequence `0x830CC4`; ammo/hover boots come back after 400 ticks. Plays jingle −1 at the pickup (PICKUP5 six times, fading) |
| 0x4A0C80 | `SetupTokens(ids, rewardStart)` ✔ | The level's five token ids (level 1: i16 list at 0x4F0EDC) go in table 0x830CCC (16 bytes: id, state, entry, timer); all five and the reward ids rewardStart.. are hidden (scale 0) and removed |
| 0x4A0DB0 | `RevealToken(k, instant)` ✔ | State 1, entry back in the list; not instant: 0x84-tick grow and a camera cut. Level 1's setup (0x4171D0) reveals token 3 at once; challenges reveal the rest |
| 0x4A2080 | `ProgressGates` ✔ | Per-level items tied to unlocked moves (`0x52F2D8` bits): tables 0x503A44/54/A4/B8/C8 of {level, hide id, pickup id, mover}; locked: pickup shown with radius 0 |
| 0x4A1A50 / 0x4A1B00 | per-level setup / script switches ✔ | Jump tables: level 1 setup 0x4171D0, script 0x417680 |
| 0x49E9D0 | `StepJingles` ✔ | PlaySound with a negative id starts a jingle (table 0x503828 + 4(−id − 1)): 3 i16 rumble, then {slot, PS1 pitch, volume, delay} steps, −1 ends, −2 n loops |
| 0x49FD40 | `DrawHud` ~ | Counters; token sequence state 9 shows "you have collected a token! keep on playing / exit level" |

## Level objects (ported: `src/specials.rs`, `src/boxes.rs`, `src/level1.rs`)
| Address | Name | Notes |
|---|---|---|
| 0x4CCE30 / 0x4CCEF0 | `SetSpecialPos` / `GetSpecialPos` ✔ | Level units in; object units (× 32) out |
| 0x4CCC70 / 0x4CCCE0 / 0x4CCD40 | special angles ✔ | Own + arg × 16 / arg × 16 / read ÷ 16 (12-bit in, 16-bit stored) |
| 0x4CCB20 | `SetSpecialScale` ✔ | arg × own scale / 4096; all zero hides (flag 1 at instance +0x8C) |
| 0x4CCFF0 | `ScaleAbout(dst, src)` ✔ | dst position pulled towards src's by src's scale, per axis |
| 0x4CCBE0 | `SpecialMatrix` ✔ | Scale, rot Z, Y, X, translate |
| 0x4878A0 / 0x4878D0 | `MoverOn` / `MoverOff` ✔ | Argument = link id − 1: the link table 0x7286EC is indexed by link id and these read entry (arg + 1). Set the terrain object's type to 8 / 0 |
| 0x488510 | `PlaceMover(id, x, y, z)` ✔ | Object units; moves its bounds by the difference |
| 0x488580 / 0x4885A0 | mover contact flags / contact data ✔ | Flags bit 1: Buzz touching its side, bit 2: standing on it; data: contact normal |
| 0x4885C0 | `RoomAt(pos)` ✔ | At a quarter scale: the objects flagged 0x400 are a room map (bounds × 4 = the level), not collision; returns their flags low byte |
| 0x4335D0 | `SetupBoxes(list)` ✔ | Triples {special, mover, path} until −1 (level 1: 0x4F0F90, seven entries) |
| 0x4334D0 | `BoxSegment` ✔ | Heading, direction (normalised to 4096 by 0x451FD0), length; if the next point is straight below the segment end, +0x16 = length / 2 (slides off past halfway) |
| 0x433700 | `PushBoxes` ✔ | Called from 0x436220 just before Buzz moves. Pick: on the floor, stick pushed, touching a box's side with the stick within ±0x180 of (contact normal heading − 0x800); voice 0x2D. Push ±8 of the segment heading: +0xC per tick (−0xC backwards), segment changes at the ends; past +0x16: slide off (+0x18 per tick, sound 0x31, Buzz frozen 10 ticks via 0x53C5DC), then fall (+0x40 per tick to 0x800, sound 0x2F on landing). Buzz's x/z velocity keeps his offset to the box; sound 0x2E while it moves |
| 0x4171D0 | `Level1Setup` ✔ (objects part) | Tokens, boxes, reads specials 6/9/0x11/0x12/0xA/0x10, plank 0x15 flat, movers 8 and 14 off |
| 0x417680 | `Level1Script` ~ (part ported) | Door (box 3 over → movers 14 on / 1 off, special 4 tips to 0x200), basement plank, fan 0xE, room 2 tops (0x417510) and pop-ups (0x417380: movers 12/13 off below phase 0x400, on at 0x1400), room 4 trampoline (mover 3: −0x980, −0xC00 from a stomp, sound 0x1C). Not yet: room 1 falling toys, Bo Peep, RC race, Hamm, the lift (special 6 + pole 8), room 3 spring, room 5 tilt (0x49EC00), room 6 shield |
| 0x434090 | `StartJump(vy, phase)` ✔ | See Player |
| 0x49F400 | `Near(a, b, r)` ✔ | (Δ >> 8)² < r² |

## Creatures (ported: `src/creatures.rs`)
| Address | Name | Notes |
|---|---|---|
| 0x407150 | `SetupCreatures` ✔ | 64 slots × 0x9C at 0x52C840, one per spawn record (+0x98 points at it) |
| 0x406CD0 | `ResetCreature(c, first)` ✔ | Record bytes: +0xD behaviour (table 0x4E02C4), +0xE turn rate, +0xF facing × 16, +0x10 health (+0x7E), +0x11 respawn (+0x7A; 100 → 0x708), +0x12 flags (+0x40), +0x14/+0x16 zone half sizes (× 256), +0x18 zone angle (× 8), +0x1A state (bit 1: type-2 hits work, bit 2: Buzz's attacks work), +0x1C/+0x1D deceleration, +0x1E acceleration, +0x1F top speed. Sets the type routine +0x94 by id (4 ZURG3 0x406220, 5 Tin Man 0x416AB0, 6 sheep 0x416A60, 8 RC 0x406A60, …) |
| 0x4086F0 | `UpdateCreatures` ✔ | Dead ones count +0x7A down and restart; live ones within reach of the camera ((d >> 8)² < 400², or flag 1) run 0x4076F0; +0x70 > 0 delayed death, < 0 dying, then removed (0x405D20 mode 2) |
| 0x4076F0 | `CreatureThink` ✔ | Script ops: 1 wait, 2 random wait, 3 random target in radius, 5 random point in zone, 8 skip if, 0xC flags, 0xD animation, 0xE random test, 0xF top speed, 0x10/0x12 distance tests, 0x11 Buzz in zone, 0x14 accel, 0x16 animation rates, 0x17 sound, 0x18 jump at target, 0x19 face target / Buzz, 0x1A velocity, 0x1B target Buzz, 0x1C vy, 0x1D turn rate, 0x1E state, 0x1F/0x23 targets towards / away from Buzz, 0x21 timer, −1 back; anything else yields (6/7: jump until landing). Flags +0x40: 1 always active, 4 walk to target, 8 chase Buzz inside the zone, 0x10 hover, 0x20 fixed height, 0x80 touchable, 0x100 harmful, 0x400 airborne jump, 0x800 no coin. Movement: velocity in the creature's frame, deceleration, acceleration to top speed, turning, zone clamp (rotated rectangle), gravity 0x40/tick to 0x800 with floor following, or hovering towards the target height. Animation clock: rate × forward speed / 2 (rate < 0: fixed); then calls +0x94 with {flags, chase, side, forward} |
| 0x405C80 / 0x405CF0 | animation step / set ✔ | Frame lists in the shared table 0x4E0588 |
| 0x407440 | `CreatureContacts` ✔ | Buzz's centre (y − 0x1CC0) against each creature's sphere (slot +0x38: offset, radius) then its ellipsoid for the current animation (+0x84 table indexed by +0x12: offset rotated by the facing, axes scaled by s/256, radius), both grown by his reach: 400 while spinning (0x53C650 > 0x14 or super spin 0x53C83C < −0x77) or just after a stomp lands, else 150. Attack kind 1 spin / 5 stomp (only from above) if the record's state has bit 2. Harmful creatures (flag 0x100) hurt Buzz (mode 3) when he isn't attacking; others push him (mode 1); sheep (health 102) nothing |
| 0x408A60 | `HitCreature(c, angle, kind)` ✔ | Table 0x4E066C {type, knock-back ticks (+0x7C), damage}: 0 none, 1 spin (1, 30, 2), 2 (2, 4, 1), 3 (2, 4, 4), 4 (3, 4, 2), 5 stomp (1, 30, 2); type 2 needs state bit 1; health < 100 only; at 0: flags &= 0xFE63, health 999, death script (pointer 0x4E02CC), 0x405D20(1) |
| 0x405D20 | `KillCreature(c, mode)` ✔ (no particles) | Mode 1: coin dropped (particle 0x3D) unless flag 0x800, death animation (ids 3/0xE: anim 2, 94 ticks; 4: 62; 0x14: 70), sound 10. Mode 2: removed; never back if no respawn time |
| 0x434EB0 | `BuzzSpin` ✔ | Circle on the ground with nothing else going on (0x53C828 & 0xFFF7F = 1): 48-tick spin (sound 0x11; cancels the laser). Held with no other state bits: charge +1/tick to 60; from 13 the whine 0x27 every tick (pitch slot 3 = 0x800 + 0x50·charge, unused on PC). Released at 60: super spin, 0x53C83C = −300 (state bit 2): sound 0x26 every tick and particles 0x14/0x15 while below −0x77 (state 0x13, hits), then sound 0x18 and 119 ticks dizzy (state 0x14). Animation 9 on the upper body at (48 − t) / 2 |
| 0x4071E0 | `HurtBuzz(angle, mode)` ✔ | Mode 1 knock-back (sin >> 4), 2 a battery (skipped while 0x52F394 > 0; the shield 0x52F398 turns it into a −60 flash), then vy −0x200, phase 5, 90 ticks invulnerable, voice 0x1A; below 0 batteries: dies (voice 0x15, 0x52B7DC = 2, 120 ticks). Mode 4 kills |
| 0x434550 | `BuzzMove` ✔ | After the move: the floor's surface (terrain object flags low byte, copied by 0x482A00 / 0x483EF0): types 0–3 hurt, 16–19 kill and put him back, 13 slippery (0x53C818, not ported) |

## Laser (ported: `src/game.rs` `laser_tick`, `src/laser.rs`)

| Address | Name | Notes |
|---|---|---|
| 0x434990 | `BuzzLaser` ✔ | Square (pad 0x8000). Timer 0x53C620: a fresh press with no state bits but ground / first person / 0x4000 (0x53C828 & 0xFBEFE = 0) starts it at 1 (state bit 0x80; clears both spin values); 1–11 raise the arm, 12 fires; then +1/tick to 0x3F → 0. Held: charge 0x53C840 +1/tick to 0x40 (whine 0x27 above 12; timer loops back by 0x28 past 0x33); a new press refires below 0x35. Released: charge > 0x24 → timer 0x34; charge 0x40 → charged shot. Fire: timer 12, sound 2 (6 charged), 0x4A5D30 from y − 0x2C00 along the facing (first person: the look angles 0x52F3C0/2); a bounce adds a second beam from the far end at yaw + 0x600 + 4r, pitch 4r − 0x200 (sound 7). Bits 0xFBE3C (climbing, long falls, pushing, …) put the arm down. Powered modes 0x53C824 / 0x882964 auto-fire (not ported) |
| 0x4A5D30 | `FireBeam(kind, yaw, pitch, origin, muzzleOffset, 4, autoAim)` ✔ | Four 0x2C-byte beams at 0x882970 (free or the shortest-lived). Auto-aim over the in-play list: on screen (+0x40 bit 1), record +0x1A bit 4, not dying, within 4096 units and 768 of height: the one nearest the yaw; within ±0x80 the ray turns onto its ellipsoid centre and pitches (±0x40) to it, else Buzz turns to it (0x52F348). Ray 4 × 0x8000 long; 0x48C860 (level) then 0x4A5870 (creatures) shorten it. Colours kind 0 red width 0x20, 1 yellow 0x40, 2 green 0x20; life 0x20. Hit: impact particle (1 / 10 / 0x49), 5 sparks (type 4); a wall hit adds particle 5 and sound 8. Back end moves at 2048/tick toward the far end. Returns the beam if it bounced |
| 0x4A5870 | `LaserVsCreatures(origin, delta, kind)` ✔ | Each targetable creature's ellipsoid as a sphere in its own frame; nearest entry point shortens delta. Hit kind 2 (3 charged) of 0x4E066C at Buzz's facing; returns 2, or 4 if the record has no bit 1 (the beam bounces) |
| 0x4A5C40 | `UpdateBeams` ✔ | Queues each live beam as ribbon 9 (0x44E100) from the far end to the back end, colour × life·4/128; life and steps −1, back end += step |
| 0x44E100 / 0x44E1A0 | `QueueRibbon` / `DrawRibbon` ✔ | Up to 100 per frame, drawn by 0x440F70: a camera-facing strip of sprite `n` (table 0x557500), half width `w`, in 400-unit segments from the start (first segment cell 2, the rest cell 1), colour (r, g, b), mode 0x4840 |
| 0x4DF294 (data) | laser arm frames | 32 bytes: the upper body plays animation 0x1A at frame (table[t / 2]·2 + t % 2) / 2 for timer t (0x4011D0); spin and laser stop in states whose two layers are the same animation |

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
| 0x43B9B0 | `LoadCreatureTypes` ✔ | Clears the per-type tables: ellipsoid pointers 0x53E6C8[128] (default 0x4F6EA0: offset (0, −250, 0), scales (512, 256, 512), radius 250), spheres 0x53EAC8 (0, 500, 0, 500); then 0x43B0C0 |
| 0x43B0C0 | `LoadCreatureModels(list)` ✔ | For each creature id used by the level: `chars*\<name>` (a switch of paths) through 0x43ACA0; then copies each slot's type sphere to +0x38 and ellipsoid table to +0x84 |
| 0x43ACA0 | `LoadCharacter(path, …)` ✔ | Loads `<path>.anm` (0x4A6940) and `<path>.all` (0x43D820); a last directory entry of type 9 (stored as 12) is the hit-shape entry: its data block is the ellipsoid table, its +0x2C words the sphere |
| 0x43D820 | `LoadAllModel(path, first, buf)` ✔ (hit shapes only) | `.all`: word 0 × 2 = directory offset; u32 count, 0x4C-byte entries (u32 size in words, …, u32 type at +0x10; types ≥ 0x100 are shared blocks chained through 0x12345678 markers); data blocks follow each other from offset 4 |
| 0x4CA420 | `LoadCreature` ✔ | `.ngn` chunk 0x106 tags 0x200–0x208 (hobjload.c) |
| 0x4CA040 | `StoreAnimation` ✔ | Raw 0x204 blobs, up to 100 per creature |
| 0x4CA1D0 | `LoadPatches` ✔ | Tag 0x208 |
| 0x4CB320 / 0x4CB4E0 / 0x4CB970 / 0x4CC970 | mesh tag readers ✔ | objload.c |
| 0x452310 | `LoadPackets` ✔ | "Loading packet data %s"; type 35 creature list, 36 map |
| 0x47B170 | `Unpack` ✔ | LZ decompressor (docs/formats/packets.md) |
| 0x43E6E0 | `ParseLevelDat` ✔ | level.dat: section 1 point lists; section 2 (u32 n, n × 128-byte cells); section 3 object records (20 bytes: i32 x, y, z, i16 size, u8 flags, u8, u32 model offset) until flags = 0; then the special-object table 0x54F628 (i32 count, count + 1 record offsets; −1 = none), then a second record list. Parsed by `leveldat::special_objects`; 88% of special objects sit exactly at their `.ngn` instance |
| 0x43E430 / 0x43E2D0 | `ModelPolyCount` ✔ | Polygon count of a PS1 model (two encodings by flags & 0x6F) |
| 0x489C30 | `InitCollision` ✔ | Loads `levelNN	errain|terr1|terr2|terr3.all` (mode bits), bounds per object, 5×5 grid. docs/formats/terrain.md |
| 0x489980 | `LoadTerrain` ✔ | Directory of 76-byte object records → object table 0x7295A8 (300 × 0x34); instances share template data |
| 0x485940 | `GatherCollision` ✔ | Polygons within ±0x500 of a point into one of 8 caches |
| 0x486520 | `FloorHeight(point, range)` ✔ | From the gathered polygons (type-8 movers rotated): the nearest upward floor above the point within `range` (0x10000 from 0x486280) if any, else the nearest at or below; floor normal to 0x728698 |
| 0x486280 / 0x4863A0 | `FloorBelow` / `PlayerFloor` ✔ | Wrappers: gather + 0x486520; the player version probes from y − 0x400 and handles standing on movers |
| 0x484380 | `MoveAndCollideCore` ~ | Swept resolve: 0x482A00 (polygons) + 0x483EF0 (floor), 3–4 passes; writes floor contact to `+0x8E` |
| 0x49FAB0 | `PutOutSparkle(k, group)` ✔ | Point k of group 0 (box spots) or 1 (rewards, + 0x830E28) gets y = 0x80000000 and its live sparkle (same position, type 0x71 + 2 × group) life 1. Level 1: (1, 1) at the start and on the trampoline, (0, 1) on the spring |
| 0x414550 | `ConvertLists` ✔ | List 58: drops the (0, 0, 0) split point (index kept in 0x830E28) and scales to object units; lists 60 / 62 scaled; list 61 converted to poles {x, y, z, kind, length} |

## Level rendering
| Address | Name | Notes |
|---|---|---|
| 0x4CDDD0 | `DrawLevelSets` ✔ | Two passes: the second mesh set (low detail) with view clip from 10000 out, then depth cleared and the first set clipped at 12000. Per-instance culling 0x4BC720 by squared distance (0x5088B0/B4, set by 0x4BC410). Ranges from detail level `0x508D74` (0–2; table 0x508D24, 24 bytes per level: −, far-from, near-to, near planes, cull distances); 2 = 10000/12000, the exe's defaults. Without this split the two sets overlap coplanar and flicker (examples/overlap.rs: 2,725 pairs in level 1, all first-set vs second-set) |
| 0x4B70E0 | `DrawMeshPrims` ✔ | Per primitive type: 0 list, 2 strip, 3 quads as strips, 4/5 billboards (see ngn.md codes 5/6) |
| 0x4B6320 | `SetRenderState` ✔ | State bits → D3D: 4 ZENABLE off, 0x800 ZWRITE off, 0x10/0x20/0x40 cull CCW/CW/none, 0x400/0x4000/0x20000/0x100000… blend pairs, 0x8000/0x10000 ZBIAS 1/2 (only flat sprites use it) |

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
| 0x4A3810 | `PositionalVolume` ✔ | Offset to the listener through the camera matrix (4.12) >> 17: x in level units, z in half level units; ears at x = ±0x400; per-ear volume = (0xC00 − distance) / 24 × vol / 128, 0–128 |
| 0x47DE50 | `StartSample(slot, left, right, …, owner)` ✔ | Six DirectSound buffers per sample: with an owner it updates the buffer already playing for it; otherwise a free buffer (play cursor 0) plays, and with all six busy the one furthest through restarts. Frequency is the sample's own (pitch unused). Volume = dB table 0x4FD668[max(l, r)] (hundredths of a dB); pan = (r − l) × 10000 / 128 |
| 0x401000 markers | | 0x83–0x86 → sounds 0x30/0x10/0x17/0x43 |

Idle voice lines: 0x4011D0 decrements 0x50A0BC every tick; when a fidget starts and it's 0, it's reset to 0x708 and 0x49EA90 plays 0xCE / 0xCF alternately (last one in 0x50A0B0).
0x4A2D80 (`BuzzFootEffects`): frame-list markers 0x81/0x82 set 0x53C634 bits 1/2; this spawns dust or splash particles at Buzz's feet by floor surface (0x52B81C), via the particle spawner 0x40FDF0 (no sound).

Buzz's sounds: 0x19 jump (voice, BUZJMP1), 0x10 double jump (BUZWING), 0x12 skid, 0x17 stomp start, 0xF stomp impact (BUZSTOMP), 0x16 long-fall scream, 0x2F long-fall landing.

## Misc
| Address | Name | Notes |
|---|---|---|
| 0x4A6390 | `CheckCD` ✔ | validate.tta |
| 0x412D70 | `ConfigCDAudio` ✔ | "Base CD track is %d" |

## HUD (ported: `src/hud.rs`)

| Address | Name | Notes |
|---|---|---|
| 0x49FD40 | `DrawHud` ✔ (elements 0–4) | Per element k: show timer 0x52C824 + 2k, slide 0x52F2E0 + 2k. 0 lives: sprite 10 at (16, 16) half size (320-wide), digit sprite 0x14 cell lives + 0x1A at (60, 32) (512-wide). 1 batteries: sprite 0xB at (272, 16) half size; column sprite 6 at (476, 47 − h), 5 × h with h = 2·health + 2, colour (0x80, 6h, 0), blinking below 5; frame sprite 0xC at (296, 16). 2 coins (bottom, y = 0xE0 − offset): sprite 0x10 cell t / 2 (t mod 24) at (26, y) scaled (0xCCD, 0x800); digits at 0x33 and 0x3F. 3 spin meter (shown 0x78 while 0x53C83C ≠ 0): sprite 0xE at (272, 0xDE); bar sprite 6 at (438, 0xDA), length charge / 2 (super spin: shrinking), yellow, blinking black when full; frame sprite 0xD. 4 laser meter (shown 0x3C while 0x53C620 ≠ 0): sprite 0xF, beside the spin meter when that's up; bar width charge × 0x1644 / 2, colour (0x80, 2·charge, 0), blinking when full. Also: token menu (0x830CC4 = 9), pause menus, ammo counters, elements 5–9 (level counters) |
| 0x49FD40 (element 7) | boss bar ✔ | v = 0x52B7E0 clamped to 0..54, blinking (0x52AD61 < 8) below 11: sprite 6 at (0xE4, off + 0x14) colour (0x80, 2v, 0) scaled (v × 0x1000, 0x4000), frame sprite 0xD at (0xE0, off + 0x10) scaled 0x2000 |
| 0x49FC60 | `HudSlide(k)` ✔ | Timer running: slide += 0x20 to 0x400, timer −1 (unless ≥ 1000); else slide −= 0x20 to 0. Returns 1 at slide 0 (hidden), else sin(slide) >> 8 − 0x40 |
| 0x447D40 | `BuildSpriteTable` ✔ | 0x557500[128]: entries 0–0x31 from 0x4EC0A4, then the level's list 0x4F7284[level] (to −1); 0x557704 = 0x4F7268 for sprite −1 |
| 0x493F40 / 0x494820 | `HudSprite` / `HudSpriteTinted` ✔ | (x, y) in a 512 × 256 screen; size × (sx, sy) / 4096; mode & 0x60: 0x60 alpha 0xFF − (mode >> 8), 0x20 additive, 0x40 subtractive |
| 0x4942D0 / 0x493DC0 / 0x4946A0 | narrow-screen variants ✔ | As above with x in a 320-wide screen (0x4946A0 / 0x494820 use the HUD tint 0x54DE9C / 0x554038 / 0x54DD6C) |
| 0x4B8CC0 | `ScreenQuad(x, y, w, h, uv0, uv1, tex, argb, mode)` ✔ | 0..1 screen units; colour through the identity table 0x50851C |
| 0x4A2960 | tick cadence ✔ | 0x52F1C1 = ticks this frame. Counters 0x52AD5A… run modulo 2, 3, 4, 5, 6, 7, 8, 16, 32, 64; 0x52F1C2 / 0x52F1C3 count how many 2- / 3-tick periods passed this frame, 0x52F1C4–0x52F1CB flag that a 4-, 5-, 6-, 7-, 8-, 16-, 32-, 64-tick period passed. The HUD blinks read the counters (0x52AD60 mod 8, 0x52AD61 mod 16, 0x52AD63 mod 64) |

## Sound ownership, input, screen sprites (round 3 findings)

| Address | Name | Notes |
|---|---|---|
| 0x4A3980 / 0x4A3B90 | `PlayOwnedSound(owner, slot, …)` ✔ | Sounds whose slot word has bit 15 (e.g. 0x26 super spin, 0x27 charge whine, 0x2E box scrape) go here from 0x49E660 with the object as owner: 0x47DE50 with that owner and looping on; a table at 0x830E58 (32 × {buffer, timeout 0x10, volumes}) keeps one buffer per owner, refreshed each call |
| 0x4A3BE0 | `UpdateOwnedSounds` ✔ | Per frame: timeout −1; at 0 the buffer stops (0x47D930) |
| 0x4A3C80 | `PlayHeldSound` ~ | Owned, not looping, no timeout |
| 0x47E5B0 | `LoadSoundEffect(name, slot, …)` ✔ | One buffer from the WAV, five more by DuplicateSoundBuffer; frequency kept per slot (0x726234); tag 0x725294 = 1 |
| 0x414AF0 | `ReadInput` ✔ | Keyboard (DirectInput 256-byte state): key table 0x529B10 {key, pad bit}; direction keys also set axes ±0x8000 (0x731CD8/C). A joystick sets 0x830C34 = 1 (analog path) |
| 0x433F40 | `StickHeading(pad)` ✔ | Angle from the camera (0x52F3AC/B4) to Buzz plus: digital, table 0x4F5AC8[(pad >> 4) & 15] (up 0, up+right 0x200, right 0x400, …; up+down = 0, left+right = 0); analog, the axes' angle through the curve 0x4F5AB4 (8 segments). Writes the target facing 0x52F348; returns the magnitude (0x4000 digital) |
| 0x4346C0 (tail) | push release ✔ | While pushing (0x53C648), a target more than 90° from the push angle 0x53C624 lets go; otherwise facing = push angle. The skid on a sharp turn needs 0x53C828 & 0xFF481 = 1 (not while the laser is up) |
| 0x4B8DD0 | `QueueScreenSprite` ✔ | Mode 0xC40 sprites go to the front of the list 0x884AC0 (drawn last-queued first: meter bars over their frames); additive/subtractive ones to the back |
| 0x4B9100 | `QueueQuad(corners[4], uv0, uv1, …)` ✔ | Corner UVs (u0, v1), (u0, v0), (u1, v1), (u1, v0): for ribbons u runs along the beam (0x44E1A0 walks from the back end, first segment cell 2) |
| 0x4346C0 (snap) | skid restart ✔ | A snap with the skid also writes 0xFFFF to Buzz +0x92 (0x52F392), so the skid's frame list (state 0x12: 13 frames at 0.5/tick) plays again from its start on every reversal |
| 0x4011D0 (states) | overrides ✔ | After the ground/air choice, in order: ledge 0x1A (unless state 8), push 0x19, zip 0x11, …, super spin 0x13 / dizzy 0x14, climb 0xE–0x10, mantle 9, stomp 0x17 / 0x18, long-fall landing 0xD, hurt 5 (first 22 ticks of a hit, blinking after), dying 7. Fidgets (0x1C/0x1D) give way to idle while the laser, spin or charge is going |

## Particles (ported: `src/particles.rs`)

| Address | Name | Notes |
|---|---|---|
| 0x40FDF0 | `SpawnParticle(x, y, z, type, mode)` ✔ | Mode table 0x4EC948 (0x18 bytes): u32 nibbles (vx 12, vy 8, vz 4, gravity 0) picking a randomisation of the value that follows (1: ((r&v) − v/2)·8, 2: (r&v)·8, 3: −(r&v)·8, 4/5: ± (v >> 4 & 0xFF0), 6: ((r&v) − v/2)·16), then i16 angle and spin |
| 0x40FAE0 | `CreateParticle(pos, vel, gravity, angle, spin, type)` ✔ | Not made beyond 6400 units of the camera (10240 for types 0x37/0x43/0x50); slot round-robin, else the least life. Type table 0x4EC160 (16 bytes): sprite, frame period (char, negative random), life, frames, death action, size w/h, flags, kind, rgb. Velocity / 2, gravity / 4, life × 2, period × 2; flags |= 0x200 |
| 0x410F40 | `UpdateParticles` ✔ (most kinds) | Life −1, vy += gravity, frames, spin; flag 1 ends on the floor (0x486280); beyond 10240 of the camera or with flags & 0x300 = 0 it ends with no death action; then the kind (+0x2F): sizes growing / shrinking by the period byte, 0xC / 0xD riding up Buzz (spin rings), 0x19 / 0x1C bouncing (sounds 0x1F / 0x68), 4 / 8 / 0x10 / 0x11 / 0x27 / 0x29 / 0x33 trails, 0x2A / 0x35 twinkles, …; then a colour fade (modes 1–7 from the type colour and life). Not ported: homing (flag 8), 0x1D / 0x26 / 0x2F / 0x30 / 0x34, lights (0x44F200), floor shadows (list 0x54F098) |
| 0x410B80 | `ParticleDeath` ✔ (3, 5) | 3: five dust puffs (0x25 mode 0x11) and a ring (0x28) on the floor, sound 0xB; 5: five sparks (0x29 mode 0xF); 1/2/4 → 0x410850 (not ported) |
| 0x445980 | `DrawParticles` ✔ | Sprite (0x557500) cell = frame; colour (a, r, g, b) with a = 0x40 for blend 0, else 0xFF; flags & 0x60 blend; flag 0x10 flat (10 units up, angle negated), else camera-facing rolled by the angle |
| 0x405D20 (part) | death bursts ✔ | Per type: 3 / 4 / 0xE / 0x14 / 0x18 → 3 fireballs (0x23 mode 0xE, random spin) and a light; 0x21 / 0x29 / 0x2E → 5 sparkles (99, modes 4 / 14, life 0x78 + r, grey) with smoke (0x11 mode 4); 0x10 / 0x19 / 0x36 → 6. The dropped coin is particle type 0x3D (kind 0x19 bounce) |
| 0x434EB0 (part) | spin rings ✔ | Spinning (> 0x14): type 0x16 + r&1 mode 2 at Buzz, angle ((−t) & 15) << 7; super spin: 0x14 + r&1 at 0x3000 up, angle ((−charge) & 15) · 0xC0 |
| 0x4100F0 | `ParticlesTouchBuzz` ✔ | Particles with flag 2 within size + 100 of Buzz's centre (y − 0x1CC0, heights halved). By sprite: 0/1/0x13 hurt; 4/5/0x33/0x34 hurt unless he spins (spin > 0x14 or super spin), when they fly back (vy −0x400, away at sin/cos >> 4, gravity 0x30, life 0x32, flags &= ~0xA, sound 7); 0x10 coin: count +1 (50: sound 0x4F), HUD coins, death action 5, jingle −1; 0x15/0x35/0x36 hurt with death actions. Any hurt: HurtBuzz(angle, 3) |
| 0x406220 | `Zurg3Routine` ✔ | Lean −= (lean + clamp(side, ±0x100)) / 16; hum 0x2C (owned) unless dying. Buzz out of its zone: animation 0 (sequence 7), timer +0x8A = 0x104. In it: timer −1; below 0: 0x168 and a dodge at facing ± 0x200 (random) of sin >> 4; crossing 200: animation 1 (sequence 8); 182 / 166: a shot (particle 0x26: vel (sin f >> 4, −0x200, cos f >> 4), gravity 0x40, spin 0x80) from 128 units to its right / left, sound 0xD; 136: animation 0 |
| 0x406CD0 jump table 0x407058 | creature routines | Type → +0x94: 4 ZURG3 0x406220, 5 Tin Man 0x416AB0, 6 sheep 0x416A60, 8 RC 0x406A60, 0xC 0x418CE0, 0xD 0x418610, 0xE 0x4064A0, 0xF 0x4189C0, 0x13 0x41BB80, 0x14 0x406620, 0x16 0x41B780, 0x17 0x4259B0, 0x18 0x406960, 0x19 0x4068E0, 0x1A 0x420AF0, 0x1B 0x41DF70, 0x1C 0x420ED0, 0x1F 0x425700, 0x20 0x41DDB0, 0x26 0x422C70, 0x28 0x428650, 0x29 0x406C70, 0x2B 0x41DEC0, 0x2D 0x4282D0, 0x2E 0x406A90, 0x2F 0x422660, 0x32–0x35 0x42C150, 0x3A 0x42D3E0, 0x3B 0x42D620, 0x3D 0x42BE60; ZURG1 (3), Mr Potato Head (9) and Hamm (10) have none |

## First person (ported: `src/look.rs`)

| Address | Name | Notes |
|---|---|---|
| 0x436220 (part) / 0x434910 | enter first person ✔ | L1 (pad 0x400) fresh, state bits & 0xFFF7F = 1: 0x50A13C = 3, velocity 0, charge 0, state bit 0x100 (the press is consumed). While ≠ 0 the move is code 8: gravity and slowing only |
| 0x405860 | `UpdatePlayerView` ~ | Fades (0x52B814), then the follow camera 0x4045E0, or 0x4038E0 when 0x50A13C ≠ 0; copies the view to 0x52ADC0 (blended by 0x50A140) |
| 0x4038E0 | `FirstPersonView(cam)` ✔ | Camera at Buzz − 0x3000. State 3: sound 0x2A, view on his facing, breathing phase 0x7F8, state 4. Targets: creatures in play with record +0x1A ≠ 0, not dying, flags & 3 = 3, within 4096 units (0x4037E0), plus with the grappling hook (0x882938) the visible points of `level.dat` list 59 (+1000). R1 cycles (marker particle 0x30 at the target, sound 0x29); a lost target picks the first. D-pad: turn/tilt speeds ±0x20 a tick to ±0x200 (decelerating to 0); held R1 with the d-pad still: lock, turning ×4 of the angle to the target (sound 0x28). Tilt kept in 200–0x720 about 0x400; camera pitch/yaw ease 1/8. Aim ray from − 0x2C00 (0x48C860 + 0x4A5870, no damage): dot particle 8, red over a creature. HUD lives/batteries/coins kept up. Specials 0x2D (helmet), 0x2E / 0x2F (arms, lagging 1/6) at half scale at the eyes, turned by the view plus a breathing bob (phase +0x20, +0x40 below two batteries; sounds 0x13 / 0x14). L1 again: state 5 (sound 0x2B): markers end, follow camera behind him (yaw = facing, distance 0x4B0), model hidden (scale 0), state 0 |
| 0x4A5370 | `FireGrapple` ✔ (`Player::fire_grapple`) | From 0x434990 at the shot, with hooks (0x882938), in first person, locked (0x50A540) on a list-59 target (0x50A4FC ≥ 1000): sound 0x51; from Buzz − 0x2C00 to the point (y − 0xC0, ×32), ray 0x48C860 → 0x882950 = 3 on a wall (stopping there) else 1; length 0x88292C = distance along the main axis of the direction ÷ half that axis's 4096-share; flag 4, pad 0x309 |
| 0x4A5540 | `GrappleTick` ✔ (`Player::grapple_tick`) | Out (1/3): facing the hook, clock +2; past the length: sound 0x52; 3 → hook used, done; 1 → 2 (pull), leaves first person. Pull: sound 0x53, rope drawn (0x44E100 type 0x22, brown, 0x1E), from tick 10 velocity = direction (to the point + 0x1600) / 3; after length × 4/3 + 10 ticks: hook used, placed at the point + 0x1600, stopped |
| 0x4A5340 | `CancelGrapple` ✔ | From 0x433ED0: a hook in use is lost |
| 0x4A4D60 / 0x4A4EC0 / 0x4A4E60 | rocket boots ✔ (`Player::start_rocket`) | 250 ticks (0x53C5E0); the pickup hidden meanwhile (0x882924) and restored after; trails 0x2F, sound 0x4C; each tick sound 0x40, under water a lift to −0x300. 0x436220: ground step 0x4A4F80 instead of 0x4346C0 (friction 0x20, accel 0xA0, caps 0x800, always thrusting, turn ≤ cap/8 with no snap), jump −0x640 |
| 0x433ED0 | `ClearMoves` ✔ (`Player::clear_moves`) | Clears mantle, fall timer, climb, zip, charge, spin, stomp, laser, ledge, float; cancels rocket, hook and hover |
| 0x417680 (room 1) | crib ✔ | Bars = creature slots 0 and 2 (COTBIT, health 0x50 = at rest). Record +0x1A = 4, or 5 (laser-hurtable) while Buzz is past x 0x4C274 (in the crib). A hit (health ≠ 0x50): +0x1A = 0, swing about Z (slot 0 up to 0x700, slot 2 down to 0x900; speed +4/tick, bouncing at half, sound 0x32, settles back to 0x50). Both down: 0x52F56C = 0x90, mover 9 (the crib side) off, specials 0x13 / 0x14 / 0x1A drop (speed +0x10, bouncing off y 0x11C00). Otherwise mover 9 is on except under Buzz on surface 0xE (the mattress) |

## Conversations and text (ported: `src/dialog.rs`, `src/cutscene.rs`, `src/level1.rs`)

| Address | Name | Notes |
|---|---|---|
| 0x4027F0 | `Talk(slot, list, text, buzzFacing, npcFacing, token)` ✔ | Nothing while one is running (0x50A1F8). Leaves first person. Facings −1: from the list's first two points. Runs the script 0x4DF6CC (with these written into it): list; Buzz on point 0, the character on point 1; their facings; camera from point 2, looking at point (n − 2) / 2 + 2 (0x402030(target, position)); glide; wait. Freezes play (0x52B816 bit 1). Token ≥ 0: 0x50A1F8 = token + 10. Text: 0x401A00, sound 0x1D |
| 0x402A10 | `PlayCutscene` ✔ (talk script) | Commands 0 list, 1 place, 2 facing, 3 / 4 camera points, 7 wait, 10 camera between points; the camera glides along the list (speed easing to the next length, ≥ 0x80). When the speech box is gone: play resumes, sound 0x1E, and 0x50A1F8 ≥ 10 → 0x4A0DB0(token) |
| 0x4A0DB0 | `RevealToken(k, quiet)` ✔ | Once per token (0x830CD0): shows it (and in the full form looks at it for 180 ticks, 0x4020F0, leaving first person) |
| 0x401A00 | `LayoutText` ✔ | 36-character lines wrapped at spaces, wrapped lines padded; `^` toggles highlighting and takes no room |
| 0x401C30 | `SpeechBox` ✔ | Opens/closes 0x100 a tick of 0x1000 (box 0x401B60: black edges, half-transparent red inside, centred at (257, 44)); types a character every 2 ticks into a 72-cell ring of two rows (y 0x20 / 0x28, x 16 + 8n in the 320 screen; normal (0x80, 0x80, 0), highlighted (0, 0x80, 0)); waits with a blinking "press jump to continue"; jump while typing fills the page (sound 0x3D on, 0x3E closing); Square closes |
| 0x401FB0 / 0x49B630 | `DrawString` / `DrawChar` ✔ | Font = sprite 0x14: a–z cells 0–25, 0–9 from 0x1A, '.' 0x24, ',' 0x25, '?' 0x28, '!' 0x29, '>' 0x2D, ''' 0x30, '-' 0x31, '*' 0x32; '@' / '~' are sprite 0x26 icons; 8 pixels apart |
| 0x401B60 | `DrawBox(x, y, w, h, rgb)` ✔ | Sprite 6 cell 1: 2-pixel black sides, 1-pixel top and bottom, inside at (x + 2, y + 1) half-transparent |
| 0x416A60 | `SheepRoutine` ✔ | Touched (flag 0x200): 0x52B7D8 +1, sparks 0x410410 (five 0x29 mode 0xF), sound 0x20, removed |
| 0x417680 (room 4) | Bo Peep ✔ | Slot 1 touched, count ≥ 0: five → voice 0xB4, talk (list 0x1C, "you found my sheep…", token 1), count −1; else voice 0xB3, talk ("please find my five…"). HUD element 5 shows the count (icon sprite 0x32) |
| 0x4A1CE0 | `HammRoutine(slot, list, buzzFacing, hammFacing, token)` ✔ | Level 1: (0x1E, 0x1D, 0xE10, 0x6E0, 0). Until the token is given, a call (sample from 0x503410) every 240 + 2r ticks; touched: 50 coins or more → sample 0x503430, 0x830D1C = 1, talk "well done buzz…" with the token; else 0x503420, "if you can bring me fifty coins…" |
| 0x4A2480 | `PotatoHeadRoutine` ✔ | Level 1: slot 0x1F, list 0x19, facings 0x200 / 0xA00. Calls (0x503440); touched: item 0x830D48 = 0 → "the cosmic shield will only…"; > 0 → "you need a cosmic shield… find my missing ear" (0x503450); < 0 (picked up: the "Other" pickup flips its sign) → 0x503460, the items go (0x44F840), "wow thanks buzz…", 0x830D48 = 0, abilities 0x52F2D8 |= 0x503A23[level] (level 1: 1, the cosmic shield) |
| 0x416F30 | `RCRoutine` ✔ | (Reached from 0x406A60.) Engine sound 0x37 (owned) while moving forward. Racing (0x52F2F8 > 1): animation 1 / seq 0xB when below 8 × top speed, 2 / 3 / seq 0xC skidding (side speed above forward), with dust 0x2A mode 10 from both back wheels and sound 0x36 |
| 0x417680 (room 2) | RC race ✔ | Car = slot 0x1D. Touched before token 2: talk (list 0x1B, sample 0x4F042C), 0x52F2F8 = 1, health 0xCA (keeps it in play anywhere, 0x4086F0). Talk over: flag 4 (walk to target), state 2. The car follows level.dat list 30 (within 600 units: next waypoint; after 0x2C or the end, a lap 0x52F4F8; three laps, then it stops past z −0x1A000). Buzz's lap 0x52AD64: quarter flags (z < −0x1A000, x > 0x2900, x < 0x21000, z > −0x1E000) going 0xF → 0xE after being re-armed by 0xE → 0xF. Three laps first: token 2 (0x4A0DB0), state 3. Car first: state 3 (lost). Leaving room 2 resets the race and the car. HUD element 6: laps to go (sprite 3 digit, flag sprite 0x1A, pulsing on the last) |
| 0x417680 (room 2) | lift ✔ | Pole 8 grabbed (0x53C5E4 = 0x30, pole index × 6): 0x52F504 = 2, sound 0x21. Then height 0x52F5BC += speed, speed += 0x20 a tick, until 0x52F5B4 (start + 0xB800), speed −1. Special 6 at (0x52F5B8, height, 0x52F5C0); pole 8's bottom y (list 61 converted: 4-byte count, 24-byte entries x, y, z, kind, length) = 0x52F530 + height (level start: 0x52F530 = pole y − special y) |
| 0x417680 (room 3) | spring ✔ | Special 0x17: y scale |n| × 0x80, turned |n| × 0x20 about Z (0x52F4FC = n). On surface 8 (0x52B81C) with n = 0: a stomp (0x53C64C) starts n = 2. n > 0 counts up each tick; at 7: reward sparkle 0 of group 1 goes (0x49FAB0), stomp cleared, 0x434090(−0x1280, 2), sound 0x1C, velocity (sin, cos)(0x11E) × 16384, facing and target 0x11E, flags +0x40 |= 0x44, camera lag 0x52AD98 = 0x40. At 0x21: n = −0x20, counting back to 0 |
| 0x436220 (flag 0x40) | thrown ✔ (camera lag not ported) | While +0x40 & 0x40 and airborne, the move profile is side deceleration 0, friction 0, side and forward caps 0xB80; landing clears 0x44 |
| 0x417680 (room 4) | fountain, vents, hot spot ✔ | Within 0x280 (× 256) of (0xB3E1A, 0x26720, −0x814C1): every 64 ticks a particle 0xC (0x40FAE0, gravity 0x80, spin (r − 0x80) >> 2) from list 22's first point towards its point 0x52F52C: d = horizontal distance >> 8, reach = 7d / 2, velocity (sin, cos)(heading) × reach >> 13 and vy = ((d << 12) / reach) × −0x80 / 64; next point + 1 + (r & 15), wrapping. Every 120 ticks the next of the vents 0x4F0EB8 (3 points) puffs 0x22 mode 2 (0x800 up); every 2 ticks 0x10 mode 10 there (life (r & 15) + 0x20). Within 0x280 of (0xB74DC, 0x23F3E, −0x6634A): within 30 it burns Buzz (0x4071E0(0, 2)); every 8 ticks smoke 0x11 mode 10 |
| 0x49F400 | `Near(p, q, r)` ✔ | ((p − q) >> 8)² summed < r²: returns it + 1, else 0 |
| 0x416AB0 | `TinManRoutine` ✔ | Flash: 0x52F590 = 60 on a hit, counting down; lit (+0x34 = 1, light +0x24..0x28 = 0x2000) on alternate ticks (0x52F5C8). Record +0x1E (acceleration) = (health + 14) × 12 as a byte. Animations 3 / 5 (open): record state 7 (laser hurts), else 4; health ≠ 0x52F5A8 (set to his health at level start): below 10 → script pointer 0x4E02D8 + 0xCA (death: animation 7), state 4, harmless, five fireballs 0x23 mode 0xE at ellipsoid 0's offset, a light 0x49EE50, jingle −2, 0x52F578 = 3; else + 0xB4 (flinch, animation 6); wait 0. Animation 7: from frame 13 token 4 (0x4A0DB0), smoke 0x11 mode 10 above him, sparks 4 mode 4 at frame 14. Buzz in the zone with y in (−0x3213A, 0): state 0 and on the ground and y < −0x2BD6E → talk (8, list 0x1A, "ha ha ha ha… defeat the tin robot", 0xE23, 0x700), state 1; state 2 → 0x50A118 = his position. Outside the zone in animation 2: script back to + 0x14. HUD bar 0x52B7E0 = (health − 9) × 54 / 11. Engine 0x35 (owned) while 0x830E3E (forward × 2 + 0x800, falling 0x20 a tick) > 0x200 |
| 0x49EC00 | `TiltMover(obj, special, v, mode, lo, hi, div)` ✔ | Returns the new swing speed v. Nothing if obj = 0x559E80. Buzz standing on it (0x52F38E and 0x488580(obj) & 3 = 2): his offset from the special along the mover's yaw (x for yaw 0) `a`, d = √(a² + dy²) >> 5 (level units), v ±= ((0x53C810 × d >> 9) + d) × ticks / 2 by the sign of a (0x53C810 = his vy after the move: landing hard pushes more). Else mode 1: v += 4 × ticks; mode 2: towards level when |tilt| > 8 (±3). Then v towards 0 by 2 × ticks; tilt z (mover table, 0x4000 a turn) += v / div; beyond lo × 4 / hi × 4: clamped, v = ∓|3v / 4|. Sets the mover's spin to the change (0x487970) and copies its angles to the special (0x488BD0) |
| 0x488AA0 / 0x487970 / 0x487A20 / 0x488AF0 / 0x488BD0 | mover angles ✔ | Mover table 0x728700 + 0x34 × i: +0x2C angles (0x4000 a turn), +0x40 spin added each tick, +0x52 bit 8 = turned. 0x488AA0 sets angles (× 4) and bit 8; 0x487970 sets the spin; 0x487A20 reads raw angles, 0x488AF0 reads them >> 2; 0x488BD0 puts them (>> 2) on a special (0x4CCCE0) |
| 0x485940 (movers) / 0x4896A0 | turned movers ✔ | The gatherer's mover loop ignores flag 0x100 (only the type matters); a turned mover's triangles are tested with Buzz taken into its frame (0x450C70 matrix). Standing on a turned mover, Buzz is carried by its spin (0x4896A0: old and new matrices) |
| 0x417680 (room 5) | Tin Man ✔, see-saw ✔ | 0x49EC00(0xF, 0xF, 0x52F58C, 1, −0x1C0, 0x1C0, 8): the see-saw (level start: 0x488AA0(0xF, 0, 0, 0)), resting tipped +0x1C0 (12-bit) with its +x end down. State 1 and the conversation over: state 2, slot 8 script + 0x14, flag 8 (chase), wait 0. His y kept ≤ −0x2DE11. Anywhere: Buzz with y in (−0x436DC, −0x21A16), z > −0x4725B and state 2 → HUD element 7 timer 0x5A |
| 0x4045E0 (part) | boss focus ✔ | 0x50A118 ≠ 0x80000000: orbit yaw −= (yaw − atan(boss − Buzz)) / 8, then cleared |
| 0x4A1E60 | `HintCharacter(slot, list, texts)` ✔ | Level 1: (0x23, 0x27, 0x4F0F7C). 0x830D24 = −1 at level start; first in play: r & 3. In view and no scene: calls (sample 0x5033F0) every 2r + 0xF0 ticks. Touched: tokens collected (0x52F0D7[level]) < 0x1F → sample 0x503400; skip hints whose token is collected, advance (wrapping after 4), talk with that hint; all tokens: "hey! you have all the tokens buzz!" (0x502868). Talk facings −1 / 0, no token |
| 0x4A26F0 | `CharacterCall(slot, sound)` ✔ | Sound 0xB5: only while the race hasn't started (0x52F2F8 = 0); others only while 0x52B7D8 ≥ 0. In view, no scene: sound every 2r + 0xF0 ticks (timer 0x830D40). Level 1: (1, 0xB2), Bo Peep |
| 0x4025C0 | `SetInfoScenes(table)` ✔ | Copies up to 10 16-byte entries {pickup id, level.dat list, text, Buzz facing} (to a negative id) into 0x50A150. Level 1: 0x4F0EE8 (9 entries, ids 0x3E–0x47: camera modes, swing bars, stomp, push, double jump, poles, zip lines, visor and laser, attacks) |
| 0x402610 | `PlayInfoScene(id)` ✔ | Pickup kind 4 (the pickup stays). Nothing while a scene runs. Leaves first person; script 0x4DF69C (Buzz on point 0, camera look-at from point 2, position from the middle, glide, wait) on the entry's list; text and sound 0x1D; Buzz faces the entry's facing |
| 0x4A2080 | `LevelAbilities` ✔ (level 1) | Level start: resets the characters' timers (0x830C98 = 200, 0x830D40 = 0xFA, 0x830D34 = 0x96, 0x830CA4 = 100, 0x830D24 = −1). Without the level's ability, 0x830D48 = its item; with it, the item's object goes. Per ability bit without it: the table's pickups (0x503A44 bit 1, 0x503A54 bit 2, 0x503AA4 bit 4, 0x503AB8 bit 8, 0x503AC8 bit 16) can't be taken (radius byte 0) |
| 0x4A27A0 | `OpenAbilityGates(table, scale)` ✔ | Entries {level, cage special, pickup id, cage mover} to 0xFF: for this level the cage special is hidden, the pickup scaled (scale << 10) and given radius 0x1F, the mover switched off. 0x503A44 (cosmic shield, scale 8): level 1 (0x19, 0x31, 10), level 8 (1, 0x30, 2), level 11 (0x52, 0x6A, 0x18) |
| 0x4A1A50 / 0x4A1B00 | level start / frame script dispatch ✔ (`src/script.rs`) | Tables 0x4A1AB8 / 0x4A1B68 by level − 1: starts 0x4171D0, 0x418E50, 0x41A8A0, 0x41C190, 0x41E390, 0x41FFB0, 0x421090, 0x423020, 0x424390, 0x425B60, 0x429D70, 0x42B300, 0x42C930, 0x42E600, 0x42FAA0, 0x430930, 0x430950; frames 0x417680, 0x4190C0, 0x41AA10, 0x41C640, 0x41E880, 0x420060, 0x421340, 0x423200, 0x424490, 0x425F60, 0x42A130, 0x42B3A0, 0x42CA60, 0x42E790, 0x42FC50, 0x430940, 0x430960. Levels 11–15 use level01..05/level1.* (not the folders containing their Pconv configs); all five core asset sets verified 2026-10-19 |
| 0x418E50 / 0x4190C0 | level 2 ✔ (part, `src/level2.rs`) | Start: tokens 0x4F154C / rewards 0x41, token 3 shown, boxes 0x4335D0(0x4F1518), info scenes 0x4F153C, water surface 0x4400, see-saw yaws 0x488AA0(0, 0, 0x961, 0) / (1, 0, 0xB4A, 0), kite no coin, slot 4 timer 100, movers 5 / 4 / 0xC off, mover 7 at special 6, specials 0x15 / 0x20 / 0x21 hidden, 0x1B stretched 0x118, pole 0 at special 0x19. Frame: see-saws 0x49EC00(0, 0, v, 0, −0x1C0, 0x1C0, 8) / (1, 1, v, 0, −0x1C0, 0xE0, 8); water mode 1 at 0x4400 past x −0x246FF, else mode 2 at 0x1800; Hamm (0xF, 8, 0xC00, 0x400, 0); hint Hamm (0x1B, 0xB, 0x4F1528); the sarge (slot 6, list 7, facings 0x700 / 0x200, token 1, call 0xB2); level 1's RC race code for slot 0x1D (no such slot); stomp switches on surfaces 8 / 9 (buttons 2 / 3 sink 0x40 a tick, markers 0x20 / 0x21, sparkles 0 / 1, sound 0x38 at (0x59280, 0x1000, −0x5C127)); one down: planks 4, 8, 9, 10 sway (±((sin >> 9) + 0x20)); both: mover 5 on, 6 off, then settle and special 0x15 appears, 4 goes, sound 0x3B; the spring on surface 10 (0x52F66C, after the door; vy −0x1080 heading 0, sparkle 2); the rope (pole 0 grabbed: special 0x19 sinks, +0x10 a tick to 0x800, until −0x2500; pole kind 2 meanwhile, length grows; special 0x1B y scale (y + 0x5826C) × 0x100 / 0x55D2); the kite (0x52F664: Buzz standing on the roof, y < −0x72000, x 0x20CC6–0x35FC6, z 0x2D174–0x4C7F4 → talk (0x1A, 9); then script + 0x1C; bar health × 54 / 10; gone → 120 ticks → token 4). Not ported: drawbridge 0x52F5E4 / 0x52F5F0 (specials 5, 0xB; movers 2–4), the tree 0x52F5EC (0x1E / 0x1F, movers 0xB / 0xC), the ball (special 6 / mover 7), the squash pad (surface 0xB, 0x52F690), the falling sparkles and kite string effects |
| 0x4189C0 | `KiteRoutine` ✔ | See `Creatures::kite`: flash 0x52F5D8 / blink 0x52F63C, bob 0x52F6EC (+0x24, lean = sin >> 6), wave 0x52F6B8 (+0x40), faces Buzz; state 2 and Buzz above −0x71FFF: HUD element 7, camera focus; else home, state 4, wait 0x32. Out of its zone: swoop at Buzz (target y = sin(wave) − 0x3000 + max(y, −0x721AD), capped −0x741AD), spin (health × −0x14 + 0x108 a tick), engine 0x3C, velocity along its facing >> 5. In it: hit → flash 60, wait 0, sound 0x98; hover sin(wave) − 0xA000 above; dodge every 120 (±0x200, >> 3). Otherwise y = (sin(wave) >> 3) − 0x7C000. Never below −0x741AD |
| 0x436220 / 0x4340D0 (water) | water ✔ | 0x53C5E8 mode (1: from 0x2000 below 0x53C628; 2 / 3: from 0x53C628) sets flag 0x100: the airborne step at k = 4 (gravity 0x10, jump −0x300, cuts and double jump / 4) and falls capped at 0x400 (mode 1) or 0x40 (modes 2 / 3, with +0x9C = 6 while sinking) |
| 0x41A8A0 / 0x41AA10 | level 3: the slime ✔ (`src/level3.rs`) | Start: no tokens table, boxes 0x4335D0(0), sizes 0x52F75C / 0x52F6F0 = 0x1000, 0x52F704 = 1, taunt timers 0xB4, regrow scene 0x52F758 = 0xB4; slot 0 (SLIME, 99 health) no coin, facing 0x800, z + 0x10000; special 0x32 hidden, the first pickup of list 63 hidden (y stashed in 0x52F72C); special 0 at the slime − 0x2AF8 up, scaled 0x1388. Frame: ambient 0x6F / 0x67; phases 0x52F754 0 → (Buzz within 0x17C flat, 0x49F3C0) look-at 300 ticks at slime − 11000 up, distance 0x10 → 1 (sound 0x46 at 240, jingle −4 at 150, size easing 1/32) → 999 (script + 0x12): taunt 0xD1 every r + 0x708, hurt mode 3 within 0x32; health ≠ 0x52F730: flash 4, target − 0x300, health restored; below 0x200: round over (full + 0x800, blobs '?' (0x3F) end, from 0x3000 special 0x32 and the pickup appear; past 0x37FF: dies — script + 0xBC, look-at 0x1A4, flags & ~0x180, 0x52F0D7[level] = 0x80, phase 1000; else record state 4, look-at 0x52F758 (+ 0x28) at − 14000, script + 0x9E, velocity 0, animation 1 / 0x10). Regrowing: state 4 during the scene (< 0x78: target += (full − target) / 32, jingle −4 at 0x77; else wait 5), otherwise + 0x20 a tick, to full; size += (target − size) / 8. Timer 1: blob 0x3F (sound 0xD); 3: camera shake 0x28. 1000: kill at scene < 0xB4 → 0x3E9 (special 0 drops: + 0x60 a tick², bounce ½ with sound 0x45 above 0x80); scene over → 0x3EA → … 0x406: 0x830CC4 = 1. Size → model scale (+0x2C–0x30) and the ellipsoid table (+0xC4: offset y −12000 − size, radius size / 16). Bar ((0x3800 − full) >> 11) × 54 / 5, shown and the camera turns to it from phase 999. Flash: lit with 0x2000; else colour wobble (+0x24–0x28 = sin >> 4 of three phases). Not ported: the light, fog, drips, camera shake |
| 0x4020F0 | `LookAt(target, ticks, distance)` ✔ | Leaves first person, freezes play (0x52B816 bit 1, flag 4), camera 0x50A520 = target + (sin, cos)(angle(target − Buzz) − 0x800) >> 4 × distance, looking at 0x50A500 = target, for 0x50A1F4 ticks (counted down by 0x402A10) |
| 0x48ACC0 | `PathMover(mover, special, &pc, &wait, &ramp)` ✔ (`script::PathRunner`) | Path programs of i16 words: 0 n back; 1 dx dy dz speed (to the special's home + offset); 2 n wait; 3 mask base random wait; 4 list point speed (to a level.dat point); 6 / 7 mask wait for shared flags 0x72D2A4 set / clear; 8 / 9 mask set / clear them; 10 list point speed (steady). Moves (1 / 4) ramp 0 → 0x40 by 2 a tick and back down from −0x40 within speed × 4 + 0x30 of the target, arriving within speed + 8. Velocity = normalise(target − mover) (to 0x1000, 0x451FD0) × speed >> 7 into mover table +0x34 (flag 0x80) — level units a tick; the special is put where the mover is |
| 0x488E10 (part) | riding movers ✔ | Buzz standing on a moving mover moves with it (`World::mover_motion`) |
| 0x41C190 / 0x41C640 | level 4 ✔ (part, `src/level4.rs`) | See the module notes: hanging platforms (path programs 0x4F1BA4 / 0x4F1BC8 / 0x4F1BF4 / 0x4F1C1C, sway 0x49EC00 mode 2 ±0xE0 / 16, stop while Buzz is on them or their low platforms 7 / 8 / 9 / 6; rig 0x41BEE0), shuttles 0xE–0x11 (0x4F1C64 / 0x4F1C40), presses 0x41BC20 (specials 0x24–0x26, movers 10 / 12 / 11, 300 / 0x147 / 0x15D), drawbridge (surface 0x24, mover 0x1A, specials 0x1C / 0x1D), buttons 0x16–0x18 and the lift (mover 0x15, special 0x44; heights −0x15E00 / −0x3B600 / −0x63380), water (box x −0x5C801…0x15F9F, z −0x83718…−0x10E28, y ≥ −0x8D8A: mode 2 at −0x7A78), the timed challenge (slot 0x1A, list 0x1F, call 0xB5, clock 0x52AD64 150 → 100 every 64 ticks, five collectables 0x52F814 / specials 0x6E–0x72, token 2), Mr Potato Head (0x15, 0x1E, facings 0x440 / 0xC40), Hamm (0x14, 0x1D), hint (0x11, 0x23, 0x4F1CB8), finder (0x12, 0x20, token 1), the drill (slot 0x18, list 0x22, facings 0x580 / 0xDC0, Buzz standing below −0x9BFC5 within 300; script + 0x18; token 4). Not ported: the crane puzzle (surfaces 0x20–0x23, slot 4's arm +0x30–0x34, 0x4F1B64 lamp table; token 3) |
| 0x41B780 | `DrillRoutine` ✔ | Hum 0x99 (owned), touch 0x9A; camera shake near Buzz; fight (0x52F804 > 1): flash 60 after a hit; record state 4, or 5 while fighting unless the weapon (0x882968) is 6; keeps to lanes (0x4F1CCC x / 0x4F1CDC z thresholds, snapping between 0x4F1CEC / 0x4F1CFC pairs inside blocks); y ≥ −0x9C3E0; dust 4 mode 9 (life 0x20) every 4 ticks before the end; Buzz above −0x7DE25: rocks 0x54 every 32 ticks (one in four thrown at him), bar health × 54 / 30 |
| 0x503A22 (data) | per-level item / ability | Two bytes per level (from 0x503A22 + 2 × level): Mr Potato Head's part and the ability bit: level 1 (8, 1 cosmic shield), 4 (9, 4 disk launcher), 7 (1, 2), 10 (4, 0x10), 13 (5, 8) |
| 0x424390 / 0x424490 | level 9 ✔ (`src/level9.rs`) | Boss slot 0 (type 54, 26 health), flails 1–6 (type 55, +0x2AB apart, turning 4 a tick; tip local (0, 0x96, 0x5AA) knocks Buzz within 0x4B), minions 7–11 (respawn 10000). Phases 0x52FC04: arrival (Buzz z > −0x1BB58, look-at 0x168, voice 0xD5), fight (each hit: health 0x1A − round, next pair from 0x4F2F58, hurt 0x52FBC0 = 600 held while > 0x1E0 until the pair is beaten, then 0x78 and round + 1), round 7 the chase (turn 2, speed 0x14), below 11 dying (look-at 0xF0), done. Beam: cool-down 0x52FC00, Buzz within 400, ±0x200 of its facing, sound 0x87, hurts within 0x1E (0xD7). Bar (health − 10) × 54 / 16 |
| 0x4086F0 (rule) | creature activity | In play if (dist >> 8)² < ((sphere radius >> 3) + 400)²; health 0xCA, or level 6, always |
| 0x49DFE0 (rule) | pad mask | While Buzz flag 4 is set (dizzy, fall recovery 0x53C838 < 0, launched, token sequence) the pad is masked to 0x309 |
| 0x830CC4 (data) | token sequence | 1 stop Buzz, 2 wait until still, 9 menu (normal levels) or 3 victory (boss levels, 0x830CAC = 1), 4, end |
| 0x53C824 (data) | laser power-up | 1200 from the Timer pickup; held fire shoots kind 3 every 6 ticks, −10 each; HUD bar power × 0x98 |
| 0x425B60 / 0x425F60 | level 10 ✔ (part, `src/level10.rs`) | Spinners (specials 0–3 about X at the wind speed 0x52FC38, cycling on 0x52FC28 + 6 a tick: up to 0x80 below 0xA01, else down; 7–15 about Y at the fans' power; 4 / 6 / 5). Air shafts: lists 0, 1 (vent 5 open), 10 (vent 4 open), bottom / top pairs: r² < 0x2400 and 0 < depth < height − 0xC0 (eighths of level units) → 0x53C64C = 0, 0x53C608 = 1, 0x52F38E = 0, vy −0x80 a tick to −0x1000; steam 0x425EB0 within 0x580 of each end (top + 0x5DC). Fans: list 8 while power 0x52FC34 ≠ 0, r² < 0x6400, height × power / 4096, vy −((power × 0x80 >> 12) + 0x80) to −0x800, first only. Wind: list 2 while speed > 0x20, push along +x (0x48B660) speed × 0x20 − speed × dx × 0x20 / len. Lifts: 0x48ACC0 on (1, 0x34, 0x4F3688) and (0, 0x37, 0x4F36DC); groups 0x4F3614 / 0x4F3628, specials 0x4F35B4 / 0x4F35D0 (full), 0x4F35EC / 0x4F3600 (quarter); cables 0x4B / 0x4D y scale (dy + 0x4EE80) / 0x4EE80; sounds 0x90 / 0x8F on flags 1 / 2 (0x100 / 0x200). Vents: stomping on mover 4 / 5 turns it (0x180 about X / Z) with special 0x11 / 0x10. Switches: stomps on surfaces 0x20 / 0x21 / 0x22 shift markers (0x52FC50 / 0x52FC30 / 0x52FCD8, 0x425680 on list 9); all 1 → mover 0x13 off, jingle −5, lights 0x46–0x4F, barrier 0x50 / 0x51 shrinks. Spider: Buzz on the floor with −0x1C2509 < y < −0x1AC1C9 → talk (8, 0x1F), script + 0x1C; fans' ramp 0x52FCFC + 0x20 a tick to a random period ((r + 0x140) × 0x40); slot 8 gone → token 4 after 0x78. Runaway token (0x52F2F8): slot 0x16 list 0x21, clock 0x8C, token 2 shown, after 150 ticks along list 14 at 0x5D2 / 65536 a tick; Buzz inside x −0x23560…0x24820, z −0x13A40…0x13EC0 → 99; below 100 lost (0x4A0E60). Characters: Potato (7, 0x25 / 0x20), Hamm (6, 0x1D), hint (0x17, 0x26, 0x4F37D8), mice finder (0, 0x1E, token 1) |
| 0x425700 | `SpiderRoutine` ✔ | Type 0x1F: flash 60 after a hit (sound 0x8A); fighting: shot cycle 0x4F3780 (400, 40 × 4, 200): 0x59 then 0x77 lobs (sounds 0x8B / 0x89) from local (0, −500, −200) while Buzz y > −0x1C2509; power > 0x400 or within ±0x7A00: flag 0x10, target y home − 0x6000, animation 2; else home, animation 1 (sound 0x88); bar health × 54 / 30; Buzz above −0x1C2508: state 4 |
| 0x425EB0 | `Puff(pos, kind, n)` ✔ | Kind 4 (steam, every 7 ticks, particle 0x4F, sound 0x8C) or 1 (wind, every 16, 0x4E); velocity 0x4F37EC[kind] × n |
| 0x48B660 | `PushCollider(obj, dx, dy, dz)` ✔ (Buzz) | Not while mantling / climbing / on a zip line; refused against the contact normal (ramp reset); ramp 0x7295A0 + 0x100 to 0x1000; adds ramp × d >> 12 to the collider's move |
| 0x49F460 | `InBoxXZ(p, x0, x1, z0, z1)` | Strict x / z bounds, any height |


## Recovered campaign and Zurg (2026-10-19)

| Address | Status | Findings |
|---|---|---|
| 0x452FC0 | confirmed | Internal IDs above 10 subtract 10 for their folder and use level1.*; TERR1.ALL supplies collision. Campaign 3/6 select internal 6/3. |
| 0x42B300 / 0x42B3A0 | partial port, `level12.rs` | Zurg slot 0/type 57, initially 29 health; entrance at Buzz x >= -0x18B2A; 300-tick introduction, 302-tick attack cycles with shots at 146/138/126; 60-tick recovery; death below 10 health, 300-tick scene then completion. Lights/marker omitted; visual validation pending. |
| 0x43C1C0 | node attachment supported | Zurg gun uses node 1, local (-250,-250,0). Current animation matrix followed by actor rotation (yaw + pi), scale and object-space translation. Shares renderer math; numeric equivalence to original fixed-point code not established. |
| 0x42B090 / 0x42B250 | ported | Zurg lobbed shot arena floor/wall bounce and trail; homing-shot trail; central-hole removal for both. Shadows omitted. |
| 0x41C640 (paint) | ported, `level4.rs` | The paint puzzle: the crane is push box mover 0 on list 3 (0x4F1C88); slot 5 (PAINT) follows it, lit (+0x34) in colour +0x24–0x28, filled by height +0x2E. Stops 5–7 pour colours 1–3 from buttons on surfaces 0x21–0x23 (0x20 is colour 1, which pours nowhere); first/second colours 0x52F80C/0x52F764; mix index second × 3 − 1 + first; targets at stops 1–3 take 4/6, 5/9, 8/10; same colour twice (first + 3 × second = 4, 8, 12) drains (32 ticks, sound −6); delivery sound −5; signs 0x4F1B64 flash every 8 ticks; all three → token 3 |
| 0x423200 (claw) | ported, `level8.rs` | Claw machine (room 4): button surface 8 (specials 0xD/0xE, cooldown 60) steps 0x52FB88 1 sweep x, 2 sweep z (0x80 a tick over 0–0xFFFF folded at 0x8000), 3 drop to 0x5000; closes at 60 ticks (0xA → 0xB, sound 0x81), grips within 0xF (0x49F3C0, across only) of the prize; up; held → to the chute (x 0, z 0x8000), lets go; prize (token 3's pickup, untouchable until y > −0x6D41, then radius |= 0x28) falls, slides out, bounces at −0x1800; motor sound 0x82; first approach shuffles prize and sweeps |
| 0x429D70 / 0x42A130 | partial port, `level11.rs` | Al's Penthouse setup/frame: water 0x52FDFC towards 0x52FD30 (down 0x200, up 0x40 a tick, sound 0x95; surface = water + 0x2EA18, mode 1); five surface meshes 0x4F3FE8; special 0x5B grows at −64000 (0x52FDAC); valve buttons 0x4F3FCC / 0x4F3FAC; plug (0x2CC2E, 0x2EE2B, −0x2FC03) drains; pipe switches 0x4F3FDC; six turrets; doors behind targets in slots 9/10; mover 0xC spring (−0x980, stomp −0xC00); room 2 spouts, room 1 steam, room 4 drips 0x4F3ED4; Hamm (0, 0xF), critters (slot 1, list 0x11), horse challenge (slot 2, list 0x12, laps 0x7F, rooms 1/4/8 only), hint character (12, 0x14, hints 0x4F3F04), infos 0x4F3F24. Lights, room 5 overlay and the slot 31 / 0x53C6A8 constraint omitted. Simulation-tested only. |
| 0x428890 / 0x428BA0 | ported, `level11.rs` | Pipes 0x4F3F48 (bit, mesh, second frame, handle, handle angles when stopped); starting a pipe rewrites the boat's route table 0x4F3DF8 (in the exe's data, so the original keeps the edits across plays); flowing pipes swap frames 24 ticks of 32 |
| 0x4292C0 / 0x4293D0 / 0x428C80 | ported, `level11.rs` | Valve buttons sink 0xE10 (scale y 0x960) / rise (0xFFE); the pressed one blinks via special 0x2C; surface meshes 0x2B/0x14–0x17 by bit, all hidden with the camera under the water |
| 0x428E70 | ported, `level11.rs` | The toy boat (specials 0x26, child 0x50): follows level.dat lists by the route table, eases its yaw by a quarter, knocks Buzz back within 0x30 (speed 0x10), smoke 0x11/0x1B every 4 ticks and sound 0x97; pauses 180 ticks at the gate special 0x1E; list 0 → mover 0x1B on |
| 0x429FB0 | ported, `level11.rs` | Float (mover, special, reach): bob speed ±0x30 a tick about 0, halved crossing down (min 0x200), stomp 0x500, splash 0x39 once reached; mover follows home + depth (blended below 0x1000 deep), at most 0x800 a tick |
| 0x4295B0 / 0x428700 / 0x429910 / 0x429800 | ported, `level11.rs` | Turrets: in their room within 0x480, turn 8 a tick to Buzz, hurt him within 0x32 (mode 3, wrecked 1); every ~300 ticks (0x52FD8C) a lob (particle 0x5C, gravity 0x80, 48-tick flight, only from 2000 level units out, sound 0x92). Their buttons blink inside 0x3E0; stomped: 180-tick look, wreck after 60 (wreck specials 0x4F3E74, smoke 0x23, jingle −2) |
| 0x4282D0 | ported, `level11.rs` | The gunslinger (slot 11): flash 60 on a hit (sound 0xA5), home and unhurtable off the arena level (y outside 0x1BCA1–0x22405); shots from nodes 15 then 16 (particle 0x61, sounds 0x93/0x94); bar (health − 9) × 54 / 20; below 10: script +0x68, phase 3; home switches by z −0x5B4F0 |
| 0x428650 / 0x406C70 / 0x406A90 | ported, `creatures.rs` | Level 11's critter (0x28, as the friend types, sound 0x96); the hummer (0x29: sounds 0x57/0x58, animation = Buzz in zone); the side shooter (0x2E: armed shot 0x61 from 0xE0 off its facing, sound 0x56; the wall-trimmed life 0x48C860 is not carried) |
| 0x42C930 / 0x42CA60 | partial port, `level13.rs` | Airport Infiltration setup/frame: conveyor surfaces 0x20–0x24 push Buzz; two tilting belts (movers 3/4, mirrored to 6/7); five luggage carts on lists 2/4/5/6/7; stomp springs on movers 0/14 and the spring case; fan wind (phase +6 a tick) pushing along list 1 pairs in room 4; timed weights challenge (slot 5, count 0xA0); Hamm, Mr Potato Head, finder. Lighting omitted; simulation-tested only. |
| 0x42BE60 | ported, `level13.rs` / `level15.rs` | Prospector routine: Buzz within 200 → anim 3, shot after 44 ticks (particle 0x68, sound 0xA6); taunts 0xC2–0xC6; dies below 10 health |
| 0x42E600 / 0x42E790 | partial port, `level14.rs` | Tarmac Trouble setup/frame: the plane (mover 0, specials 0–0x1E and 0x54–0x56) turns 1 a tick about the origin, propellers hurt within 80; three-press lamp puzzle on surfaces 0x20–0x23 (lamps 0x1F–0x34, press count 0x36–0x39) drops the floating token 0x74; wing race (slot 38, count 170); luggage pickups (type 0x3B); boss slot 46. Rain, lightning and lights omitted; simulation-tested only. |
| 0x42D710 | ported, `level14::switch_bits` | Lamp button k sets lamp k and clears lamp k − 1 (wrapping), in the low nibble of 0x52FFD0; returns the number of lamps changed |
| 0x42FAA0 / 0x42FC50 | partial port, `level15.rs` | Prospector Showdown: the three bosses (slots 0–2) are parked until Buzz passes x −0x16BD9; intro scene with platform drops at ticks 281/336/361/431; simultaneous fight with a combined bar ((sum − 27) × 54 / 60); bosses kept apart and inside x −0x2BDD6; rescue scene once all three fall, then completion. Rotating arena camera and lights omitted. |
| 0x42F310 / 0x42F530 / 0x42F7B0 | ported, `level15.rs` | The three final bosses: melee with a node-4 shot (0x66); gunslinger firing from nodes 15/16 (particle 0x61); Prospector as 0x42BE60 |
| 0x410F40 flag 8 | partial | Buzz-targeted homing: vertical velocity stores target -1, gravity stores yaw; yaw eases by 1/8, pitch by 1/20. Creature-targeted and special-period homing remain unported. |
| 0x49F460 | confirmed | Strict x/z rectangle membership test, used for arena/death boundaries. |


### 2026-10-19 neighbourhood/music continuation

- 0x418E50 passes box list 0x4F1518 to 0x4335D0 (two objects).
- 0x4190C0: pump surface 11 uses Buzz's +0x9C coyote/contact counter. The pad
  compresses toward 0xC00, inflating ball special 6 up to 0x1000; launch speeds
  are 0x352/-0x712. Mover 7 follows the ball with a -0x1000 object-unit Y offset.
  Bridge specials 5/0xB swap movers 2/3 to 4 below angle 0x299. Tree specials
  0x1E/0x1F swap 0xB to 0xC when box 1 progress is nonzero above Y -0x102F9.
- 0x413150: audio filename pointers at 0x4ECCD8, indexes 0..21; joins CD root,
  audio/, name and .wav. 0x4986D0 campaign order table 0x50268C maps the first
  15 tracks to internal levels. Runtime reader avoids filename guesses.
