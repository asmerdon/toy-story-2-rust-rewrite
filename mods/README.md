# Example mods

Each folder is a mod: a `mod.toml` that names assets from your own Toy Story 2 install. No game data is stored here.

| Mod | What it does |
| --- | --- |
| `play-as-woody` | Plays any level as Woody, using Buzz's controls, physics and moves. No laser. |
| `play-as-zurg-robot` | Plays as one of Zurg's red spring robots (level 1's hopping enemy, ZURG1): hold a direction to bounce along; Space is Buzz's full jump and double jump. No laser. |
| `play-as-tinman` | Plays as the level 1 boss at full size: hold a direction to bounce along in low hops; touching enemies knocks them out. Camera further back. No laser. |

## Trying one

From a checkout, with `TS2_GAME_DIR` pointing at your install:

```
cargo run --release --bin ts2-viewer -- --mod mods\play-as-woody "%TS2_GAME_DIR%\data\level01\level.ngn"
```

Press F to start playing. `--mod` can be repeated; later mods win when two set `[play_as]`.

In the launcher: these examples are copied into `%APPDATA%\OpenTS2\mods` the first time it starts (or on Rescan). Tick one in the Mods tab and press Play Modded. Your own mods go in the same folder.

## `[play_as]`

```toml
[play_as]
creature = "WOODY"                 # name in creatures.cfg
model = "data/level05/level1.ngn"  # optional, relative to the game folder; default: the level, then every level
animation = "retarget"             # Buzz's moves mapped onto the creature; or "hop": the bounce gait with the creature's own animations (idle = N, hop = N)
scale = 1.0                        # drawn size; collision stays Buzz's
laser = false                      # no firing, charging or first-person mode
map = [[0, 13]]                    # optional: [creature node, Buzz node] overrides to the automatic node map
[[play_as.rest]]                   # optional: turn nodes about a pivot before Buzz's motion applies
nodes = [5, 2, 3]
pivot = [42.0, -355.0, 5.0]
angles = [0.0, 0.0, 60.0]          # degrees about X, Y, Z
```

Also in `[play_as]` (before any `[[play_as.rest]]`), for `retarget`:

```toml
joints = [{ node = 2, parent = 5, at = [110.0, -358.0, 3.0] }]  # this part hangs off its parent at this point (creature rest pose); keeps longer limbs attached
hop_height = 4.0   # hop mode: the walking hop's height, as a multiple of the default low hop (about a third of Buzz's jump)
touch_damage = true   # touching an enemy hits it as Buzz's spin does; harmful ones don't hurt you
camera_distance = 2.5 # the follow camera's distance as a multiple of Buzz's, for big characters
```

`animation = "hop"` (Zurg robot, Tin Man): a held direction on the ground starts a low hop (about a third of Buzz's jump height) on every landing, so it only moves while airborne; Space is Buzz's normal jump and double jump; no direction stands still in the `idle` animation. The `hop` animation is stretched over each airtime. The mod loads the model from `data/level01/level.ngn` in any level.

The viewer prints the node map it used (`play_as: node map ...`). Unknown keys are an error; a creature that can't be found falls back to Buzz.

These mods are deviations from the original and only apply when enabled.
