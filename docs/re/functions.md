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
| 0x49FB40 | `RewardSparkles` ✔ | Spawns sparkle particles 0x71/0x73 at `level.dat` list 58 points within 600 (>> 8) of the camera, four points per frame. List 58 is split by a (0,0,0) point: group 0 (count `0x830E28`) for pushable-box spots (0x433700), group 1 for challenge rewards; 0x49FAB0 kills a point's sparkle |
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

| Address | Name | Notes |
|---|---|---|
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
| 0x486520 | `FloorHeight` ~ | Height under a point from the gathered polygons (rotates type-8 movers by their level object's angles) |
| 0x486280 / 0x4863A0 | `FloorBelow` / `PlayerFloor` ✔ | Wrappers: gather + 0x486520; the player version probes from y − 0x400 and handles standing on movers |
| 0x484380 | `MoveAndCollideCore` ~ | Swept resolve: 0x482A00 (polygons) + 0x483EF0 (floor), 3–4 passes; writes floor contact to `+0x8E` |
| 0x49FAB0 | `SpawnChallengeReward` ~ | Matches type-58 positions to objects 0x71/0x73 |

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
