# FBF bitmap fonts

`Apps/Res/Text/<LANG>/*.FBF`. Loaded by `cGZFont::LoadFromFile` (Loki `0x083670c8`).
Parser: `crates/sc3k-formats/src/fbf.rs`; drawing: `crates/sc3k-ui/src/text.rs`.

```
u32 color_type      1 = 8-bit paletted atlas (all shipped fonts); otherwise 16-bit
u32 width           640 for every font
u32 height
u32 unknown         1
u8  atlas[width * height]          palette indices
u16 palette[256]                   RGB565 (8-bit atlases only)
glyph[256], 20 bytes each:         indexed by Windows-1252 byte
  i32 left, top, right, bottom     cell in the atlas (right/bottom exclusive)
  i32 advance                      pen step, usually cell width + 1
```

- Line height = glyph 0's cell height (`cGZFont::InitBitmapped`).
- String width = sum of `advance` (`cGZFont::GetStringWidth`).
- Palette indices at draw time (`cGZFont::SetFontColorAndAntialiasColor`): 1 = text colour;
  2..=15 step linearly towards the anti-alias colour, `text + (aa - text) * (i - 1) / 16`;
  16 = the anti-alias colour. Index 16 is also the atlas' pixel (0,0), which `InitBitmapped`
  makes the colour key, so it is never drawn. 17 fills unused atlas space.

## System fonts
`SC3U.exe+0xf388..0xfa8c` registers the English fonts as `cGZWinCtrlMgr` system fonts. Asian
languages (`0xF`, `0x11`–`0x14`) use other files.

| File | System font IDs | Used by |
|---|---|---|
| `system9.fbf` | 0, 0xB | default |
| `main10.fbf` | 1, 2, 0xE | |
| `title9.fbf` | 3, 0xC | splash copyright line |
| `Serif17.fbf` | 0x10 | main menu labels |
| `main9.fbf` | 0xF | |
| `systemloadsave.fbf` | 0x11, 0x3E9 | load/save dialogs |
| `title12.fbf` | 0xD, `E2B14587` | |
