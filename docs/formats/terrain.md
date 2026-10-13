# `TERRAIN.ALL` / `TERR1.ALL`: collision geometry

Files: `data/levelNN/TERRAIN.ALL` (level NN) and `TERR1.ALL` (level NN + 10, the `level1.*` files). Despite the extension they are not character models.
Parser: `src/formats/terrain.rs`. Test: `tests/terrain.rs` (all 20 parse; the collision floor under each player start is within 4 units of the visible floor).
Source of truth: the setup routine 0x489C30, which builds `levelNN\` + `terrain`/`terr1`/`terr2`/`terr3` (picked by the level's mode bits) + `.all`, the loader 0x489980, the gatherer 0x485940 and the floor query 0x486520.

## Layout (little-endian)

```
u32 dir_offset                 in 16-bit words (byte offset = 2 × value)
object data blocks             from byte 4, back to back; each object's size is in its record
directory:
  u32 count
  count × 76-byte records
```

### Object record (76 bytes)
| Offset | Type | Meaning |
|---|---|---|
| 0x00 | i32 | Data size in 16-bit words (the cursor advances by this even for instances, which have 0) |
| 0x04 | i32 ×3 | Position x, y, z, level units (the game stores × 32) |
| 0x10 | i32 | Type: 6 static (all shipped static objects; 7 is accepted too), 8 moved/rotated by a linked level object, ≥ 0x100 not collision (a block starting 0x12345678 is chained into a list at 0x728CF0) |
| 0x1A | i16 | Linked level object (type 8): its position is written into that object's table entry at load |
| 0x28 | u16 | Flags: 0x100 = disabled (skipped by the gatherer), 0x400 = always checked (outside the 5×5 grid), 0x200 common, meaning open |
| 0x44 | i16 | Template id (< 100) |
| 0x46 | i16 | 1 = instance: reuses the polygons of the object that defined this template id, at its own position |

### Object data
Groups until a negative tag:
```
i16 tag (≥ 0), i16 n, i16 x, i16 w, i16 z, i16 d      bounds of the group (x..x+w, z..z+d)
n × 44-byte polygons
i16 −1 (+ padding to the stated size)
```

### Polygon (22 × i16), relative to the object
| Words | Meaning |
|---|---|
| 0–1 | x min, x size |
| 2 | bytes: y start (signed, × 64), y extent (× 64) |
| 3 | bytes: z start (signed, × 64, relative to v0.z), z extent (× 64) |
| 4–6 | v0 |
| 7–9, 10–12, 13–15 | e1, e2, e3: offsets from v0 |
| 16–18 | Unit normal × 16384. Floors point to −Y (+Y is down) |
| 19–21 | Quad: the second half's normal. Triangle: word 20 = 0x7FFF, others unknown (precomputed edge data?) |

Triangle: (v0, v0+e1, v0+e2). Quad: (v0, v0+e1, v0+e3) and (v0, v0+e3, v0+e2).

## Runtime structures (for reference)
- Object table 0x7295A8: 300 × 0x34 bytes (pos × 32, data pointer at +0x10, bounds, type at +0x2C, flags at +0x2E).
- 0x489C30 sorts static objects into a 5×5 grid over the level (cells at 0x7270A4, index list 0x72D2B0); type-8 objects and flag-0x400 objects go in an always-checked list.
- 0x485940 gathers polygons near a point (± 0x500 level units in x/z) into one of 8 caches (0x5C0 bytes each, up to 0xF0 polygons).

## Open
- The resolver (0x484380 → swept 0x482A00 and floor slide 0x483EF0, 3–4 passes per tick; Buzz's position is raised by 0xC0 + a per-mover offset during the test). We use our own floor ray + wall sphere against these triangles.
- Type-8 movers follow level objects (`level.dat` section 3 / object table 0x7286EC), not ported; drawn at their start positions.
- `level05/terrain.flr/.grd/.scn/.wal/.wuv` look like pipeline leftovers.
