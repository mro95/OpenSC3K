# Main city interface

The interface over the city view. Code: `crates/sc3k-ui/src/main_ui.rs` (layout, drawing,
main buttons) and `crates/sc3k-ui/src/menu.rs` (menu definitions). Checked against
`SIMUI.DLL` by `tools/diffcheck/run.py ui`.

Addresses are Loki `libSimUI.so` in Ghidra (ELF + 0x10000) and Windows `SIMUI.DLL`.

## Windows
`cSC3MainUIMgr::Init(cIGZWin*)` (Ghidra 0xC603C) creates four windows by class ID and adds them
to the main window. Each sizes itself in its own `Init`.

| Order | Class | Class ID | Window ID | Size |
|---:|---|---|---|---|
| 1 | `cSC3WinDateCashTitle` | `02F95D57` | `42FB7DEB` | its background image |
| 2 | `cSC3WinRCI` | `12FAF305` | `42FB7DEA` | its background image, 41x88 |
| 3 | `cSC3WinMenuPanelMain` | `02F61B00` | `42FB7DEC` | the main piece, see below |
| 4 | `cSC3WinNav` | `E2F95AF5` | `42FB7DED` | always 160x164 (`SetArea`, Ghidra 0x16B9C0) |

Later windows draw on top. `Init` also gives the menu panel the root menu `D1001000`
(`AddMenuItems`).

### `place_windows`
Ghidra 0xC78F4, SIMUI.DLL 0x100148D1. It runs against the main window's area (`GetArea`,
Windows slot 0xAC), in this order:

1. **Navigator:** right-aligned. Its top is the area's bottom minus its height, but at
   exactly 480 high it is bottom − 0x98, so 12 rows hang off the screen.
2. **RCI meter:** right edge at the navigator's left. Its top is bottom − its height, but at
   exactly 600 high it is bottom − 0x50 (8 rows off the screen).
3. **Date bar:** `SetArea(left, bottom − its height, the RCI's left, bottom)`.
4. **Menu panel:** `SetArea(right − its width, top, right, the navigator's top + 6)`.

A missing window is skipped. The navigator's place then falls back to the bottom-right corner,
and the RCI's left to the right edge.

| Screen | Panel | Navigator | RCI | Bar |
|---|---|---|---|---|
| 640x480 | (544, 0) 96x334 | (480, 328) | (439, 392) | (0, 416) 439x64 |
| 800x600 | (704, 0) 96x442 | (640, 436) | (599, 520) | (0, 544) 599x56 |
| 1024x768 | (928, 0) 96x610 | (864, 604) | (823, 680) | (0, 704) 823x64 |

## Art
`Res/UI/Shared/<group>_MainUI.ixf`, image type `62B9DA24`. Every piece keys out magenta.

| Group | Use |
|---|---|
| `00640480` | 640x480: panel, submenu panels, bar |
| `02F78C6D` | 800x600 and the default: panel, hover images, navigator, RCI, bar filler |
| `00800600` | 800x600 bar |
| `01024768` | above 600: panel top, bottom and filler; bar wider than 800 |

## Menu panel (`cSC3WinMenuPanelMain`)
### Pieces
`Init` (Ghidra 0x164124) picks them by the main window's height:

| Height | Top | Main | Bottom | Filler |
|---|---|---|---|---|
| 480 | | `00640480/FFFF` 96x334 | | |
| 600 | | `02F78C6D/FFFF` 96x417 | `02F78C6D/FFFE` 96x25 | |
| 601–768 | `01024768/FFFD` 96x97 | `02F78C6D/FFFF` | `01024768/FFFE` 96x96 | |
| above 768 | as above | | | `01024768/FFFC` 96x32 |

Any other height below 601 makes `Init` fail, and there is no panel.

### `SetArea`
Ghidra 0x163E78.
- **Size:** as wide as the main piece; as tall as asked, but at least the pieces' heights
  together. At 480, 600 and 768 the pieces fill the panel exactly.
- **Placement:** the top piece goes at the top, the main piece under it, the bottom piece at
  the bottom. The filler is tiled down from the main piece's bottom and overdrawn by the
  bottom piece.
- **Paint order** (`GZPaint`, Ghidra 0x1647CC): top, main, filler, bottom, then the open
  submenu's image.

### Main buttons (`cSC3WinMenuBtnMain`)
The root menu's nine buttons. `get_menu_btn_info_main` (Ghidra 0x165724, SIMUI.DLL
0x1004C3E9) gives each one:
- **Area:** 56x32 at x 0x21.
- **Images:** hover image `100X0001` and hover-while-open image `100X0005`, both in
  `02F78C6D`. The submenu panel `100X0003` is in `00640480` at 480 and in `02F78C6D`
  otherwise. X is the button's last digit.
- **Meet (`1006`):** only a hover image. It has no submenu.

| Button | Item | y at 480 | at 600 | above 600 | Submenu y at 480 / 600 / above |
|---|---|---:|---:|---:|---|
| `1001` | Landscape | 3 | 3 | 100 | 0 / 0 / 0x61 |
| `1002` | Zoning | 0x27 | 0x27 | 0x88 | 0xB / 0xB / 0x6C |
| `1003` | Transportation | 0x4B | 0x4B | 0xAC | 0xB / 0xB / 0x6C |
| `1004` | Utilities | 0x6F | 0x6F | 0xD0 | 0xB / 0xB / 0x6C |
| `1005` | Civic | 0x93 | 0x93 | 0xF4 | 0xB / 0xB / 0x6C |
| `1006` | Meet | 0xDB | 0x123 | 0x184 | none |
| `1007` | Review | 0xFF | 0x147 | 0x1A8 | 0x2D / 0x76 / 0xD7 |
| `1008` | Options | 0x123 | 0x16B | 0x1CC | 0 / 0x76 / 0xD7 |
| `1009` | Emergency | 0xB7 | 0xDB | 0x13C | 0x53 / 0x77 / 0xD8 |

- **Other heights:** at heights other than 480 and 600 but below 601, there is no button.
- **Submenu image:** it is the whole panel column with the submenu's own buttons drawn in.
  `ShowSubMenu` hands it to the panel (`SetActiveMenu`), and the panel draws it at the
  button's offset.

Behaviour:
- **`GZPaint`:** a button draws only while hovered. It uses `100X0005` if its submenu is
  open, `100X0001` otherwise.
- **`GZOnMouseDownL`:**
  - A button with a submenu opens it and closes any other.
  - If its submenu is already open, the button closes it.
  - A button without one closes the open submenu.
- **`GZOnMouseUpL`:** releasing over a button sends its item's command, message `025F0A91`.
  Of the main buttons only Meet has an item.
- **Not ported yet:** the tooltips and the click sound (`BX` slot 0x1C, sound 10 on hover,
  12 on press).

## Navigator (`cSC3WinNav`)
`update_background_buffers` (Ghidra 0x16BEC0) loads three pieces from `02F78C6D`:
- `22F96E32` (72x70) at (0, 0);
- `22F96E33` (88x70) to its right;
- `22F96E34` (160x94) under both.

The mini map (76x18 strip, `0xC470D325`), the zoom and rotate buttons, the north indicator and
the clock are not ported.

## RCI meter (`cSC3WinRCI`)
Background `02F78C6D/42FB77F6` (41x88). The R, C and I bars (`rci_rect_update`) are not
ported.

## Date/cash/title bar (`cSC3WinDateCashTitle`)
`get_layout_info` (Ghidra 0xF5190, SIMUI.DLL 0x100270E5) chooses by the screen width:
- **640:** group `00640480`;
- **800:** group `00800600`;
- **wider than 800:** group `01024768`;
- **any other width:** fails.

Background: `82790741` in that group (440x64, 600x56, 824x64).

`GZPaint` tiles the filler `02F78C6D/82790742` (16x64) leftwards from the right edge while it is
right of the background, then draws the background at the left.

The layout info also places the text fields, the news ticker and the pause, play, speed and
help controls. At 800 wide:
- four text fields: (8, 0x1A)–(0x90, 0x2C), (0x92, 0x1A)–(0x102, 0x2C),
  (0x104, 0x1A)–(0x198, 0x2C) and (0x18C, 0x1A)–(500, 0x2C);
- the ticker: (8, 5)–(0x1E1, 0x16);
- pause at (0x1E8, 9), play at (0x1FF, 9), speed at (0x1E9, 0x1D), help at (0x223, 0xB).

None of these is ported yet.

## Menu definitions: `Sys/MenuItem.INI`
Read by `cSC3MenuMgr::Init` (Ghidra 0xC82A0) from SYS.PAK. Each value is a comma-separated
field list ending in `END`. The text after `END` is a comment.

| Section | Key | Fields |
|---|---|---|
| `SC3MenuItemInfo` | item ID | version (1), name (table, ID), tooltip (table, ID), `COMM` or `PMSG`, four data words |
| `SC3MenuBtnDefs` | button ID | item ID, submenu ID or 0 |
| `SC3MenuDescs` | menu ID | name (table, ID), two colours, count, button IDs |
| `SC3MenuSets` | `SC3MSET_<id>` | two words, the INI path |
| `SC3MSET_<id>` | menu ID | menu ID |

The install has 90 items, 21 menus and one set (`0x1000`). The root menu `D1001000` lists the
nine main buttons `1001`–`1009`. Their items `10001001`–`10001009` are not defined, except
Meet's `10001006` (`PMSG 724A82D0`).

The field meanings come from the order of the parser's reads and from the file's own comments.
The parsers themselves are not checked against `SIMUI.DLL`.
