# Sprites

`Apps/Res/Sprites/*.DAT` (plus `BuildingSets/`, `FloraSets/`), loaded as
`cSC3DBSegmentSprite`. The containers are IXF archives (`ixf.md`). Parser:
`crates/sc3k-formats/src/sprite.rs`; export with `sc3k-dump sprites <file> <outdir>`.

Each sprite is two records with the same (group, instance):

| Type | Contents |
|---|---|
| `00000000` | the pixels, described below |
| `00000001` | an 8-byte image-info block (not decoded yet) |

The `.SII` text files next to some archives hold registration points and spans; they are not
parsed yet.

## Record
```
u32 code        low byte: colour type (5 = RGB555, 7 = RGB565); next byte: kind (0 buffer, 1 span)
u32 flags       0x00080000 = QFS ("LZ2") compressed; 0x00008000 = "LZ1" (never shipped);
                0x10000000 = 8-bit alpha mask
u32 width
u32 height
u32 size        bytes after this field
u8  stream[]    QFS-compressed (qfs.md)
```

Only three encodings occur in the whole install, and all are QFS-compressed:

| Encoding | Count | Kind / flags |
|---|---|---|
| Span buffer, RGB565 | 62,462 | span, colour type 7 |
| Span buffer, RGB555 | 90 | span, colour type 5; widened to 565 on load (green's low bit copies its top bit) |
| Alpha mask | 1,139 | buffer, flag `0x10000000` |

## Span buffer
`cGZSpanBuffer::ImportFromData`. After decompression:
```
u32 size            whole decompressed size
u16 width, height   same as the record header
u16 4
u16 colour type
u32 key             transparent colour (low 16 bits)
row[height], 8 bytes each:
  u32 offset        index of the run's first pixel in pixels[]
  u16 x             first covered column
  u16 count         run length; bit 0x8000 = solid (no key test needed)
u16 pixels[]
```
Each row has one run of covered pixels; the rest of the row is transparent. Inside a run that
is not solid, pixels equal to the key are transparent too.

## Alpha mask
`width * height` bytes, one per pixel, 0 (transparent) to 31 (opaque). A mask pairs with the
colour sprite of the same size: the colour sprite has instance `…0000`, the mask `…0001`. The
New City preview blends flora and building sets this way (`docs/ui/new-city.md`).

## UI sprites
`GAME_UI.DAT` holds the sprites the UI draws directly, such as the New City scheme previews.
`crates/sc3k-assets` mounts it at start-up; look sprites up with `Assets::sprite(group,
instance)`.
