# New City Options dialog

Opened by "Start New City" on the main menu (`E3270FE9`). It is a `cSC3WinGen` window
driven by `cSC3WinProcCityScheme` (SimUI.dll). Code: `crates/sc3k-ui/src/new_city.rs`,
with controls in `crates/sc3k-ui/src/controls.rs`.

Positions below are relative to the window. At 640x480 the window's origin is (0, 56).

## Window
- Size: `SetSize(0x27F, 0x16D)`. The height grows to 368 once the bottom bar is placed, so the
  window is 639x368, centred on screen.
- Background: clouds `82B9B75B/E2B14588` (256x256), tiled from the window origin.
- Bevel: 1 px, white on the left and top, black on the right and bottom.
- Gutters: 4 px between the bevel and the content.
- Title bar (`cSC3WinGenTitleBar`):
  - Area: (4, 4), full width inside the gutters.
  - Height: max(8, title12 line height + 8, 27); 27 is a system button (25) plus 2.
  - Fill `7C90FF`, outline `A8B4FF`.
  - Caption "New City Options" (`041F2625/0x2D`) in title12's own file colours, at
    (4, (H − line height) / 2).
  - Close button: row 0 of `62B19CE9` (4 frames, 24x25), right-aligned 4 px from the bar's
    right edge, centred vertically. ID `42B7C353`; the dialog treats it as Cancel.
- Blue bars (`AddBlueBar`, same fill and outline): (4, top)–(14, 326), (218, top)–(228, 326)
  and the bottom bar (4, 330)–(635, 364).

## Default colours (`cWinCtrlMgr::DefaultColor`)
| ID | Colour | Use |
|---|---|---|
| `62E56A2C` | `7C90FF` | bar fill |
| `62E56A2D` | `A8B4FF` | bar outline |
| `62E56A31` | `31367C` | ink |
| `0x11` | white | line input fill |

`cSC3App::ctrlmgr_setup` re-inks every system font except title12 to `31367C`. The fonts keep
their own anti-alias colour.

## Left pane (FUN_1005bbd5)
Strings are in table `029541F4` unless noted. All text is main9 (system font `0xF`).

Layout from the top, starting at y = `0x24`:
1. "City Name:" (`0x229`) label at x 15.
2. City name line input at (23, label bottom), 190 wide. Default "New City" (`0x282`).
3. A 2 px gap, then "Mayor Name:" (`0x22A`) and its line input the same way. Default
   "Defacto" (`0x281`).
4. Difficulty option group at (15, y), caption `0x22B`:

   | ID | String | Shown as | Value |
   |---|---|---|---|
   | `25524845` | `0x22C` "Easy  (%s)" | Easy  (§50,000) | 1, §50,000 |
   | `25524846` | `0x22D` "Medium  (%s)" | Medium  (§20,000) | 2, §20,000 |
   | `25524847` | `0x22E` "Hard  (%s Loan)" | Hard  (§10,000 Loan) | 3, §10,000 loan |

   `MakeMoneyString` puts the currency sign (cp1252 `0xA7`, §) before the amount and groups
   digits in threes.
5. Below the difficulty group, two groups side by side:
   - Start Date at x 15, caption `0x22F`: 1900 / 1950 / 2000 (`0x230`–`0x232`, each with a
     trailing space). IDs `2552484A`–`2552484C`.
   - City Size at x 100, caption `0x233`: Miniature `0x2CE`, Small `0x234`, Medium `0x2D3`,
     Large `0x235`. IDs `2552484F`–`25524852`. Map sizes `0x40`, `0x80`, `0xC0`, `0x100`.
6. y advances by the taller group plus 2.
7. Check boxes at x 17: Disasters (`0x21D`, ID `25524854`), then Auto Budget (`0x21E`, ID
   `25524855`) under it.
   - The pane bottom is the lower box's bottom plus 2, but at least `0x14A` (330).
   - The boxes are then moved down so they end 2 px above the pane bottom.

Defaults: Easy, 1900, Large (on any machine fast enough), both boxes off. The original
restores earlier choices from the preferences; where they are stored is not known yet.

## Right pane
Labels in main9, table `041F2625`:

| String | Text | Position |
|---|---|---|
| `0x28` | Landscape: | (`0xE8`, `0x2C`) |
| `0x29` | Trees: | (`0xE8`, `0x6A`) |
| `0x2A` | Buildings: | (`0xE8`, `0xBC`) |
| `0x2B` | User-Made Buildings: | (`0x130`, `0x124`) |

### Scheme buttons
The rows come from `SC3CityScheme.ini` (`docs/formats/sys-pak.md`), in file order. Each choice
is a FixedToggle button with a 4-frame icon.

| Row | Section | First ID | Button bottom | Slot width |
|---|---|---|---|---|
| Landscape | `LandScapes` | `E558C351` | `0x5C` | `0x2B` |
| Trees | `FloraSets` | from a member field | `0xA6` | `0x27` |
| Buildings | `BuildingSets` | from a member field | `0xF9` | `0x31` |

- Slots start at x `0xF8`. Each button is centred in its slot, with bottoms aligned.
- Key 0 starts selected (`DetermineDefaultScheme`).
- Clicking a button selects it and deselects the rest of its row. Clicking the current choice
  keeps it on.
- The icon-to-scheme mapping is not traced yet (`cSC3MenuBtnDef::GetMenuBtnDefID`), so some
  icons do not match their previews.

### Preview (`cSC3WinProcCityScheme::GZPaint`)
Drawn in this order, clipped to the window:
1. The landscape sprite at (`0x1BC`, `0x5C`).
2. A flora group, centred on x `0x24D`, bottom at `0xDE`.
3. The building, right edge at 600, bottom at `0xF2`.
4. A flora group again, centred on x `0x1E8`, bottom at `0xF2`.

Flora and building sets are colour sprites blended through their alpha masks
(`docs/formats/sprite.md`).

### Select button
"Select" (`041F2625/0x2F`) is an SC3TextBtn (`cSC3CtrlCreator::create_SC3TextBtn`), ID
`2552485A`. It opens the building replacement manager, which is not implemented.
- Image `E2B66DB8`, a stretching 4-frame button.
- Font title9 (system font `0xC`), ink `31367C`; disabled colours `404040` / `808080`.
- Gutters (10, 0, 16, 4), then AutoSize, at least `0x4E` wide.
- Position: 5 px right of "User-Made Buildings:", y `0x11A`.
- Open issue: the image's `7B92FF` background is drawn as an opaque box. The colour key, if
  any, comes from the buffer loader; check against the original.

## Bottom bar
OK (`A2DEFD9A`, ID `2552483D`) sits at the right end of the bottom bar, 4 px in, centred
vertically (FUN_1005e570).

## Controls (GZWin.dll)
### cGZWinBtn
| Style | Image | Drawing |
|---|---|---|
| Button, Toggle | 4 frames | 3-slice: left third, tiled middle third, right third |
| FixedButton, FixedToggle | 4 frames | as is |
| CheckBox, Radio | 8 frames | image left of the caption |

- 4-frame images: 0 disabled, 1 normal, 2 pressed or toggled on, 3 hover.
- 8-frame images: pairs of (on, off) for normal 0/1, hover 2/3, pressed 4/5, disabled 6/7.
- Caption, 4-frame styles: centred inside the gutters. It sinks 1 px while pressed.
- Caption, check box and radio: image at (0, (h − image h) / 2), text at
  (gutter left + image w, gutter top).
- Minimum size, check box and radio: w = image w + gutters + text w; h = max(image h, top and
  bottom gutters + line height).
- Mouse:
  - Hover is ignored while the button is pressed.
  - A click is a release inside the button.
  - Every style except Button and FixedButton flips its on state when clicked.

### Radio buttons and check boxes
- Radio: SYS image `0000000A`, 96x12 (8 frames of 12x12), colour key magenta `F81F`. Gutters
  2, 2, 2, 2.
- Check box: SYS image `C2D6E93A`, 104x13 (8 frames of 13x13), no key. Gutters 3, 1, 2, 2.

### cGZWinOptGrp
- The caption label (main9, ink `31367C`) sits at the group's top-left.
- Radio buttons are stacked from the caption's bottom, then shifted 8 px right
  (FUN_1005d9ae). The group is 8 px wider.
- Selecting one radio button turns the others off.

### cGZWinLineInput
- Fill: `DefaultColor(0x11)`, white.
- Outline type 3, a double sunken bevel. Lines in draw order, with r and b exclusive:

  | Line | Colour |
  |---|---|
  | (r−1, t)–(r−1, b−1) | `B3B6E0` |
  | (l, b−1)–(r−2, b−1) | `B3B6E0` |
  | (l, t)–(r−2, t) | `6C71B9` |
  | (l, t)–(l, b−2) | `6C71B9` |
  | (r−2, t+1)–(r−2, b−2) | `7D81BD` |
  | (l+1, b−2)–(r−2, b−2) | `7D81BD` |
  | (l+1, t+1)–(r−3, t+1) | `31367C` |
  | (l+1, t+1)–(l+1, b−3) | `31367C` |

- The dialog sets gutters to (4, 1, 2, 1) and a maximum of 50 characters.
- Minimum height: top and bottom gutters + 4 + line height.
- Text position: (left + gutter left, top + gutter top + ((h − top and bottom gutters) − line
  height) / 2). It is drawn in the font's colours, which are the re-inked `31367C`.
- Caret: a '|' glyph, shown whenever the box has focus (it does not blink). Its x is
  text x + width(text[view..cursor]) − caret width / 2 − 1.
- The view scrolls so the cursor stays visible.
- A mouse press focuses the box and puts the cursor at the nearest character boundary.
- Keys:
  - Left, Right, Home and End move the cursor.
  - Backspace and Delete remove a character.
  - Enter, Escape and Tab go to the parent.
- `GZOnCharacter` inserts characters from `0x20` up. The remake accepts Latin-1 printable
  characters only.

## Keys
- Tab moves the focus to the next name box.
- Escape cancels (`cSC3WinGen::GZOnKeyDown`).

## Result
The OK handler collects the settings below and, in new-city mode, generates the terrain
(`docs/sim/new-city.md`, `docs/sim/terrain-gen.md`). The remake does the same, then builds
the city model, prints it, and opens the city scene. That scene draws the terrain isometrically
(`docs/render/terrain.md`); Escape returns to the menu. `--seed N` fixes the terrain seed, which otherwise
comes from the clock.
- City name and mayor name.
- Difficulty, starting funds, and whether the funds are a loan.
- Start year.
- Map size.
- The Disasters and Auto Budget check boxes.
- The landscape, flora and building scheme keys.

The original plays UI sound `0xD` for OK and Cancel, and `0x92` for a scheme button. The remake
plays no UI sounds yet.
