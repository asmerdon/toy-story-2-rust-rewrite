# Animations: `.ngn` creature tag 0x204 (= PS1 `.anm`)

Parser: `src/formats/anim.rs`. Test: `tests/anims.rs`. All 845 animations in the shipped levels decode, giving 272,081 node poses.
Source of truth: the PC exe's creature loader (`hobjload.c`, 0x4CA420), its animation store (0x4CA040), the per-object draw loop (0x4CD880) and the node sampler (0x4CD170 / 0x4CD1B0). These were found with Ghidra 12.1.4, starting from the debug-path strings. This page describes the format. No decompiler output is kept in the repository.

## Storage
Each 0x204 record is copied verbatim into a per-creature array (max 100). Playback indexes it by animation number. An object can play two at once: a current slot and a second slot (probably the blend source).

## Layout (little-endian)

| Word | Meaning |
|---|---|
| 0 | i16. If < 0, the header is `-value` bytes long (16 in practice). Otherwise it's 12. |
| 2 | u16 frame count |
| 4 | u16 frame stride, in 16-bit words |
| 5 | u16 node-table entries |
| 6 | u16 scale-mask length, in bytes |
| 7 | i16 special node index (−1 = none). It gets a vertical offset in one engine mode; not needed for basic playback. |

After the header:
1. **Node table:** one i16 per node. A value ≥ 0 is the word offset of that node's record within a frame. −1, −2 and −3 mean the node isn't animated (its matrix isn't updated).
2. **Scale mask:** bit *n* set means node *n*'s record carries scale.
3. **Frames:** `frame_count × stride` words. Some files (level03 SLIME) are a few words short in the last frame. The original reads past the buffer; we repeat the previous frame's words.

### Node record

| Word | Content |
|---|---|
| 0–2 | `t = value >> 2` (translation, mesh units); `value & 3` = low rotation bits |
| 3–4 | `packed = w4 << 16 \| w3`: three 10-bit fields |
| 5–7 | (if scale bit set) i16 scale, 4.12 fixed point (×1/4096) |

Angles are 12-bit (4096 = 360°):
- X = `((packed >> 20) & 0x3FF) << 2 | (w0 & 3)`
- Y = `((packed >> 10) & 0x3FF) << 2 | (w1 & 3)`
- Z = `(packed & 0x3FF) << 2 | (w2 & 3)`

The engine multiplies these by 16 to index a 65,536-entry sine table.

### Matrix
Row-vector convention (Direct3D, `v' = v · M`). The engine starts from identity and pre-multiplies X, then Y, then Z, giving `R = Rz · Ry · Rx`, which applies Z first. The node matrix is `Scale · R`, with the translation added to the bottom row. Then it's multiplied by the object's root matrix. **There is no parent/child chaining.** Every node's transform is a model-space delta applied to its model-space rest-pose mesh.

### Time and interpolation
Time is 16.16 fixed point: the high half is the frame and the low half is the fraction. The frame is clamped to `frame_count − 2` so frame+1 exists. Translation and scale are lerped as integers. Rotation is blended by building both matrices and lerping their 3×3 elements (not a slerp; a faithful port should do the same).

### Object root matrix (for context)
Object position is an int × 1/32. Rotation applies X, then Y + 0x800 (a half-turn offset in 12-bit units), then Z.

## Evidence it's right
- The record sizes implied by the node table plus scale mask exactly match the stride in every animation (e.g. last offset + 5 or + 8 = stride).
- Decoded Hamm idle frames: the body node stays at 0°. The four legs move in mirrored pairs within about ±25° and change smoothly frame to frame. The head turns +15° → −12°. Translations are tens of units against mesh offsets of hundreds, consistent with rest-pose deltas.
- **Not yet confirmed visually.** `ts2-tool export-pose` writes posed OBJs for checking in Blender.

## Open
- How the smooth-joint shared vertices (creature tags 0x207/0x208) follow the nodes.
- Which animation number is which move (idle, walk, jump…). That lives in the game code that selects slot `+0x12`.
