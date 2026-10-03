# QFS / RefPack compression

EA's LZ77 variant, also used by SimCity 4 and other EA titles. In SC3U it compresses the
pixels of UI images (`image.md`); IXF containers themselves are not compressed.

Decoder: `crates/sc3k-formats/src/qfs.rs`.

## Header
```
u8   flags     0x10 = standard; bit 0: a compressed-size field follows; bit 7: 4-byte sizes
u8   0xFB
[u24/u32 BE compressed size]   only if flags & 0x01
u24/u32 BE decompressed size   4 bytes if flags & 0x80
```

## Control codes
Each op copies `literal` bytes from the stream, then `copy` bytes from `offset` bytes back
in the output (copies may overlap the bytes they produce).

| First byte | Length | literal | copy | offset |
|---|---|---|---|---|
| `00–7F` | 2 | `b0 & 3` | `((b0 & 0x1C) >> 2) + 3` | `((b0 & 0x60) << 3) + b1 + 1` |
| `80–BF` | 3 | `b1 >> 6` | `(b0 & 0x3F) + 4` | `((b1 & 0x3F) << 8) + b2 + 1` |
| `C0–DF` | 4 | `b0 & 3` | `((b0 & 0x0C) << 6) + b3 + 5` | `((b0 & 0x10) << 12) + (b1 << 8) + b2 + 1` |
| `E0–FB` | 1 | `((b0 & 0x1F) << 2) + 4` | 0 | — |
| `FC–FF` | 1 | `b0 & 3` | 0 | — (end of stream) |
