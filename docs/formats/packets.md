# `.raw` packet files (levels and `gfx/`)

Parser: `src/formats/packets.rs`. Tests: `tests/packets.rs`. All 827 packets in the install unpack to exactly their stated size and end exactly on the end-of-stream code.
Found via the exe's level loader (`InitLevelPlay`, 0x452FC0), which builds `levelNN\level.raw` and passes it to the packet loader (0x452310, "Loading packet data %s"). That loader walks the packets and decompresses each with 0x47B170.

The PC game still loads these PS1-era files for gameplay data. The graphics come from `.ngn`.

## Container

```
repeat until the next 4 bytes are FF FF FF FF:
  u32 big-endian  unpacked size
  u32 big-endian  packed length (bytes after this 14-byte header)
  u8[6]           unknown (checksum / flags?)
  u8[packed]      LZ stream
```
Unpacked data starts with a little-endian u32 **packet type**:

| Type | Contents |
|---|---|
| 35 | Creature list ("CreatListRam"): 2,048 bytes of 32-byte spawn records |
| 36 | Map/grid data: width/height fields and a byte grid. Not decoded |
| 37 | Not decoded |
| 0–31 | Not decoded (various small level tables) |
| 257–259 | PS1 texture pages (the exe has `CONVPSXTPAGE4/8/16` converters). Not needed on PC. |

## LZ stream
- Bits are read MSB-first from a byte register refilled one byte at a time. The first control bit of each stream is skipped.
- The copy distance is offset + 1, so offset 0 repeats the previous byte.
- `0`: literal byte.
- `1 0` + byte: 2-byte match with an 8-bit offset.
- `1 1 0`: 3-byte match with a long offset.
- `1 1 1` + byte *n*: if *n* > 0, an (*n*+8)-byte match with a long offset. If *n* = 0, a following `0` bit ends the stream (a `1` continues).
- `1 0 0` + `x` then `0`: (4+*x*)-byte match with a long offset.
- `1 0 0` + `x` then `1 y`: length code `y + (x+3)·2`, giving 6–8 bytes with a long offset. Code 9 instead means 4 bits *n*, then (*n*+3)·4 literal bytes.
- **Long offset:** a prefix code for the high part (`0` → 0, `110` → 1, `100x` → 2–3, `1u1c1` → 4–7, `1u1c0d` → 8–15), then the low byte. That gives a 4 KB window.

## Creature list (type 35)
32-byte records after the type word, ending at the first all-zero record:

| Bytes | Meaning |
|---|---|
| 0–11 | i32 x, y, z: world position in level units (same space as `.ngn` instances; +Y down) |
| 12 | creature id (`data/creatures.cfg`, `.ngn` creature slot) |
| 13–31 | not decoded. Varies by creature type, so probably behaviour parameters, facing, paths and triggers. |

Level 1 has 36 records. Their ids match the level's cast (many Zurg robots, 5 sheep, one each of Tin Man, Bo Peep, RC, Mr. Potato Head, Hamm and Rex), and every position falls inside the level geometry's bounds.

**The player (Buzz, id 0) is not in this list.** The player start is somewhere else, probably type 36/37 or one of the small tables.
