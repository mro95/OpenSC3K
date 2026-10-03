# UI images

Every entry of the UI archives (`Apps/Res/UI/Shared/*.ixf`, `MAIN.IXF`, `SYS.IXF`,
`DLG.IXF`) is a flat image: 711 in total. Decoder: `crates/sc3k-formats/src/image.rs`.
Export with `sc3k-dump images <archive> <outdir>`.

```
u32 version        2
u32 data_size      entry size - 20 (bytes from the second size field to the end)
u32 width
u32 height
u32 format         7 = RGB565, 3 = 8-bit
u32 data_size      same value again
u8  pixels[]       QFS-compressed (qfs.md), rows top to bottom
```

| Format | Count | Pixels |
|---|---|---|
| 7 | 710 | little-endian RGB565, `width * height * 2` bytes |
| 3 | 1 (`DLG.IXF` `C2BF3646/02D32710`, a grey disc) | 8 bits per pixel, no palette stored; decoded as greyscale |

Transparency is a colour key chosen by the code that loads the image, not stored in the
file. The main menu sheets key magenta (`0xFF00FF`, RGB565 `F81F`); the Maxis logo keys black.

## Addressing
UI images have type `62B9DA24`; the code asks for them by (instance, group), e.g.
`cSC3Buffer(0x22729921, 0x82B9B75C, …)` for the title background (see `ixf.md`).

### Archive contents (group = archive name prefix)
| Archive | Group | Contents |
|---|---|---|
| `MAIN.IXF` | `82B9B75C` | Title screens and the main menu: splash `22721000` (localised variants `22721001`–`22721006`), title background `22729921` (German edition `01140000`), menu sheets `22729930`–`22729938`, logo `22729940` |
| `SYS.IXF` | `82B9B75B` | Shared controls (buttons, bars, frames) |
| `DLG.IXF` | `C2BF3646` | Dialog pieces |
| `02F78C6D_MainUI.ixf`, `00640480_MainUI.ixf`, `00800600_MainUI.ixf`, `01024768_MainUI.ixf` | file prefix | In-game toolbar per resolution |
| `52DB60A0_Advisors.ixf`, `831D112B_AdvTopics.ixf` | file prefix | Advisor portraits and topic art |
