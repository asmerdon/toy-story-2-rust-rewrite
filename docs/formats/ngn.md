# `.ngn` — PC data containers

Files: `data/gfx/*.ngn` (load screens and UI textures), `data/level00/*.ngn` (front end), `data/levelNN/level*.ngn` (levels). They're written by TT's `pconv.exe` from the PS1 data (`level.dat`, `.raw`, `chars*/*.all`, `.anm`). See `data/mkdat.btm` and `Pconv.cfg`.
Parser: `src/formats/ngn.rs`. Tests: `tests/ngn_files.rs` (every `.ngn` in the install parses with no unread bytes).

## Chunks

```
repeat { u32 id, u32 len, u8[len] }     level files end with id 0, len 0
```

| id | Contents | Status |
|---|---|---|
| `0x104` | Textures | ✅ |
| `0x106` | Creatures: models, skeleton names, animations | ✅ structure; animations not decoded |
| `0x100` | Mesh set (level geometry); two per level | ✅ |
| `0x101` | Instance set (placements for the preceding `0x100`) | ✅ |
| `0x102`, `0x103` | Count 0x16 + floats/ints. Only in some levels. Paths, triggers or cameras? | ❌ |
| `0x105` | Count + u16 pairs. Visibility or object table? | ❌ |

## 0x104 textures

```
u32 count
count × { u32 size, u32 name_len, char name[name_len], u8 bmp[size] }
```
Each `bmp` is a complete Windows BMP file. Level textures are 256×256 24-bit, named `texNN`. Load screens are 800×600 (24-bit or 8-bit palettised). `level06/level1t1..t3.ngn` are the same geometry with fewer textures and still reference the missing names (presumably reduced-VRAM variants).

## Mesh tags (shared by 0x100 and creature 0x203)

A mesh is a run of `{u32 tag, u32 len, body}` records ending with tag `0x00`.

| tag | body |
|---|---|
| `0x40` | name: u8 len + chars (e.g. "Shape0"). Levels only. |
| `0x41` | texture list: `u16 n, u16 n, u8[2]`, then n × (u8 len + name), then 26 × n bytes (unknown) |
| `0x42` | materials: `u32 n`, n × `{u32 flags, u32 len, data}`. Flags 0x81 → `rgb, u16 tex`. Flags 0xC1 → `rgb, u32 mode, u16 tex`. `tex` indexes the 0x41 list; 0xFFFF means untextured. |
| `0x43` | vertices: `u32 format (0x17), u32 stride (36), u32 count`, count × `{f32 pos[3], f32 normal[3], u8 color[4], f32 uv[2]}` |
| `0x44` | groups: `u32 n`, n × `{u32 prim, u16 material, u16 count, u16 idx[count]}` |

The primitive codes are the engine's own:
- `1`: triangle list
- `4`: quad list (perimeter order). Every count is divisible by 4 across all levels.
- `5`: 5-vertex strip
- `6`: 5-vertex fan

Level vertices have a placeholder normal of (1, 0, 0); their lighting is baked into `color`. Bytes 1–3 of `color` vary like an RGB light value and byte 0 is 0x80/0xFF. **The byte order is unconfirmed.**

`0x100` body: `u32 mesh_count`, then the meshes.

## 0x101 instances

```
u32 count, u32 stride (44)
count × { f32 pos[3], u32 rot[3], f32 scale[3], u32 mesh_index, u32 flags }
```
`mesh_index == i` in all shipped files. **Only the translation is applied.** Rotation and scale are already baked into the vertices. Evidence on level01: counting world-space vertices shared between different instances gave 507 shared with rotation ignored vs 388 with it applied, for the 352 rotated instances. Every axis order and sign was tried, and ignoring rotation won in all cases. Scale made no difference either way. What `rot` and `flags` are for (collision? culling? animated objects?) is open.

## 0x106 creatures

```
u32 slots (63)
slots × (u8 len + name)       non-empty only for creatures used in this level; slot = creatures.cfg id
then, per present creature (in slot order), tagged records until tag 0:
  0x200  u32 node_count
  0x202  node_count × (u8 len + name)      "node00".."nodeNN"
  0x201  node_count × 64 bytes             all zero on disk: runtime matrix buffer
  0x203  node_count × { u16 flags, u16 0xFFFF, [mesh tags… 0] if flags & 1 }
  0x205  texture list (0x41 format)        ┐ smooth-joint geometry shared between nodes
  0x206  materials    (0x42 format)        │ (Buzz, Bo Peep, …): present on most
  0x207  vertices     (0x43 format)        │ creatures
  0x208  ?                                  ┘ starts u32 count; probably joint ↔ vertex map
  0x204  animation (repeated; Buzz has 35). Same F0 FF 03 00 header as PS1 .anm. Not decoded.
```
- Node meshes are in model space and assembled in rest pose. Hamm's legs sit mirrored at x = ±0.46, like the PS1 `.all` parts.
- Node counts match the PS1 `.all` part counts (Hamm 11, Woody 20, Buzz 23 with smooth joints).
- Flag bit 2 appears only on smooth-jointed characters.
- 146 creature records across all levels parse fully.

## Open questions (best answered from `toy2.exe` in Ghidra)
1. Vertex colour byte order and the meaning of byte 0.
2. `0x41` 26-byte per-material tail, and `0x42` `mode` values (0x02, 0x06, 0x10, 0x18).
3. Instance `rot`/`flags`; chunks `0x102`, `0x103`, `0x105`.
4. `0x208` and how the shared vertices attach to nodes.
5. **Animation encoding (`0x204` / `.anm`)**: needed for anything to move.
