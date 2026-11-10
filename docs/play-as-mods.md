# Play-as mods: Woody and the Zurg robot (brief)

Owner's request (2026-11-10), rewritten as an implementation brief. Purpose: footage for the
OpenToyStory2 launch trailer that shows mods. Two mods, both made only of a `mod.toml` that
names assets in the player's own install. No game data goes in the repo.

1. **Woody** (`mods/play-as-woody/`): Buzz's level, Buzz's controls and physics, drawn as
   Woody, walking/running/jumping as Buzz would. No laser.
2. **Zurg robot** (`mods/play-as-zurg-robot/`, ZURG1: level 1's red, horned spring robot with claw arms,
   the owner's reference picture). The first helper built it on TINMAN (the grey level 1 boss) by
   mistake; that mod is kept as an extra (`mods/play-as-tinman/`). The text below says Tin Man for
   the gait; it applies to both as the player. It gets
   around by **bouncing**: holding a direction makes it hop repeatedly. No laser.

Both are deviations from the original by the owner's request; they only apply when the mod
is enabled. Mark the code "Deviation (mod):" and list them in the README's deviations.

## What the data has (checked)

`cargo run --release --example cast` lists each level's creatures (`id:NAME(nodes, anims)`):

- **WOODY** (creatures.cfg id 1) is only in `data/level05/level1.ngn` (campaign level 15):
  20 rigid nodes, **1 animation**. `data/chars/woody.anm` is tiny (one animation too). So
  there is no walk cycle for Woody in the game: his moves must come from Buzz's.
- **TINMAN** (id 5) is in `data/level01/level.ngn`: 17 nodes, 8 animations. The boss routine
  `0x416AB0` (`src/creatures.rs`, "the Tin Man fight") already plays them; read which
  animation numbers it uses for idle, hop/bounce, and landing.
- **BUZZ** (id 0) is in every level: 23 nodes (smooth joints), 35 animations, driven by the
  move table (`src/buzz.rs`, `BuzzAnimator`, 0x401000).

## Mod format (extends docs/opents2-design.md §3)

The repo gets a top-level `mods/` folder with the two example mods. Each is a folder with a
`mod.toml`. Add an optional `[play_as]` table:

```toml
id = "play-as-woody"
name = "Play as Woody"
version = "0.1.0"
authors = ["lewiloon2k"]
description = "Run through any level as Woody, using Buzz's moves."
engine = "0.1"

[play_as]
creature = "WOODY"                 # creatures.cfg name
model = "data/level05/level1.ngn"  # optional: where to load the model; default = search the
                                   # current level, then every level's .ngn, first match wins
animation = "retarget"             # "retarget": Buzz's move-table animations mapped onto the
                                   # creature (below); "hop": the bounce gait (below)
scale = 1.0                        # drawn size relative to the model; collision stays Buzz's
laser = false                      # no firing, no charge, no first-person helmet/arms
```

Tin Man:

```toml
[play_as]
creature = "TINMAN"
animation = "hop"
scale = 0.5            # pick so it stands about Buzz's height; check in a shot
laser = false
idle = 0               # animation numbers: fill in from the boss routine
hop = 1
```

Engine loading:
- `ts2-viewer` reads mod folders from `OPENTS2_MODS` (`;`-separated, the launcher already
  sets it, `crates/launcher/src/launch.rs`) and from a repeatable `--mod <dir>` argument (for
  testing from the repo). Parse `mod.toml` with serde/toml. If several mods set `play_as`, the
  last in load order wins; print which one is used.
- Unknown keys are an error with the mod's path in the message; a missing creature prints a
  clear message and falls back to Buzz.
- Keep it small: no file-override VFS in this piece (that is the next modding step).

## Woody: retargeting Buzz's animations

Animations are model-space deltas per node with no hierarchy (docs/formats/anim.md): node *i*
is drawn at `v' = v·R_i + t_i` from its rest-pose mesh. Retarget per frame, without a
skeleton:

1. **Node map, Woody node → Buzz node.** Match body parts by their rest-pose bounding-box
   centres after normalising both models to the same height (head↔head, torso, pelvis, upper
   and lower arms, hands, thighs, shins, feet; left↔left). Compute it automatically, print it,
   and allow an override list in `mod.toml` (`map = [[woody_node, buzz_node], ...]`) in case
   a part matches wrongly. Woody's nodes without a match follow the nearest mapped part.
2. **Pose.** With `c_b` and `c_w` the rest-pose centres of the Buzz and Woody parts and
   `s` = Woody height / Buzz height: Buzz's centre moves to `c_b' = c_b·R + t`; draw Woody's
   part with the same rotation about its own centre, moved by Buzz's scaled displacement:
   `R_w = R`, `t_w = c_w − c_w·R + s·(c_b' − c_b)`. If limbs visibly separate at the joints,
   use the joint end of each part (the end nearest the torso) instead of the centre.
3. Do the mapping once at load; per frame it is a few matrix products, same blending as Buzz.

The model comes from `level05/level1.ngn` whatever level is being played, so its textures must
load from that file too.

## Tin Man: the bounce gait

Keep Buzz's controller, collision, camera and jump physics. When `animation = "hop"`:
- Standing on the ground with a direction held: start a small jump automatically every time
  it lands (a low hop, about a third of Buzz's jump height; tune in a shot), and play the hop
  animation. It moves only while in the air, so it covers ground in bounces.
- Space: Buzz's normal (full) jump and double jump, with the hop animation.
- No direction: idle animation, standing still.
- Turning and air control as Buzz.
- Drawn with Tin Man's own animations at `scale`, not retargeted.

## Out of scope

Shooting, Woody's or the Tin Man's voices, changing the HUD, enemies reacting differently,
the file-override VFS, the launcher UI (the Mods tab already lists and orders mods; it only
needs to see the new `[play_as]` key without erroring).

## Done when

- `cargo build --release` and `cargo test --release` (with `TS2_GAME_DIR`) pass; new tests:
  manifest parsing with and without `[play_as]`, the Woody node map (every Woody node gets a
  partner; head maps to head), and the hop gait (holding forward on flat ground gives repeated
  landings and forward progress).
- Offscreen shots (never the interactive viewer) in level 1, saved in `local/shots/mods/`:
  Woody standing, mid-run and mid-jump; the Tin Man idle and mid-hop. Inspect them; the
  main agent checks them before reporting.
- `mods/play-as-woody/mod.toml`, `mods/play-as-tinman/mod.toml`, `mods/README.md` (how to
  try them: `--mod` from the repo, or copy into `%APPDATA%\OpenTS2\mods` and enable in the
  launcher).
- README deviations list, HANDOFF and MODLOG updated. Nothing committed.

Test commands:

```bash
set TS2_GAME_DIR=C:\Program Files\Disney Interactive\Toy Story 2
cargo run --release --bin ts2-viewer -- --mod mods\play-as-woody "%TS2_GAME_DIR%\data\level01\level.ngn"
```
