# `.all` — character models

Files: `data/chars*/<name>.all` (68 files, 0.6–19 KB). Paired with `<name>.anm`. Listed by id in `data/creatures.cfg`.
Parser: `src/formats/all.rs`. Test: `tests/all_models.rs`.

## Summary

The geometry section is a stream of **PS1 GPU polygon packets** with the screen XY replaced by 3D model-space XYZ. Pconv kept the PS1 data almost unchanged for PC.

```
u32 header                      unknown. Not the file size or the geometry size. Woody 0x1620, box 0x15C.
stream of blocks until 0x12345678 or an unknown block:
  u32 0                         end of part (one rigid body part / bone)
  polygon record                byte[3] in 0x20..0x3F
  normal block                  u16 count, u16 ?, count × {i16 x, y, z, u16 w}
u32 0x12345678                  geometry end marker; trailer follows (not decoded)
```

### Polygon record

| Field | Size |
|---|---|
| r0, g0, b0, **code** | 4 |
| n × { i16 x, i16 y, i16 z, u8 u, u8 v } | 8n |
| (n−1) × { r, g, b, **tag** } | 4(n−1) |

n = 4 if `code & 0x08`, else 3. Codes seen: `0x34` (gouraud textured tri), `0x3C` (gouraud textured quad). Bit meanings follow the PS1 GPU: `0x10` gouraud, `0x08` quad, `0x04` textured, `0x02` semi-transparent, `0x01` raw texture.

- Quad vertices are in **perimeter order** (0-1-2-3), not the PS1 strip order. Triangulate as (0,1,2),(0,2,3).
- **+Y is down** (PS1 convention). Units: about 256 per metre feels right (Woody ≈ 2.1 units tall at 1/256 scale).
- `tag` bytes vary (`00`, `05`, `06`, `11`, `1F`, `32`, `91`, `96`, …). Probably CLUT / texture page / per-vertex flags. **Open.**

### Normal blocks

Every entry has |(x, y, z)| ≈ 4096, so they are unit normals in 4.12 fixed point. The `w` is often `0x0048`. Seen in Buzz, Bo Peep, Zurg (emperor), Shiny, Bub, Fsauce and the Buzz interior model. In those files the next block, starting `00 00 01 00` / `04 00 01 00`, is **not yet understood**. It is probably a smooth/skinned mesh with shared vertices. These models are the main characters, so this is a priority.

### Parts are in model space

Per-part bounding boxes show the parts already assembled in a bind pose (Woody: head above torso, arms mirrored at x = ±0.55, legs at the bottom). The exported OBJ is a complete character. `.anm` probably stores per-part rotations and offsets from this pose.

## Results (2026-10-13)

- 68/68 models yield geometry, 8,435 polygons in total.
- 34/68 reach the `0x12345678` marker (all rigid-part characters: Woody, Jessie, Rex, Hamm, Mr Potato Head, Zurg, ...).
- The rest stop at either the unknown skinned block (above) or a different trailer that has no marker (`u32` counts such as `1`, `2`, `3`, or values like `0xA9800000`). Those single-part props have all their polygons parsed. Only the trailer is unread.

## Trailer (after 0x12345678): open

Repeated records `78 56 34 12 | u16 count | u16 id | count × {i16 x, y, z, u16 0/1}`. Each record holds pairs of points with a 0/1 flag. This looks like attachment, collision or bone-pivot data. The ids (0x71, 0x73, 0x135, …) increase. **To decode next**, together with `.anm`.
