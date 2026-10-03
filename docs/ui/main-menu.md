# Main menu

Recreated in `crates/sc3k-ui/src/main_menu.rs` (menu) and `title.rs` (splash, background).
Run it with `cargo run --release -p opensc3k`.

Sources: `SC3U.exe` with names ported from the Loki Linux build (`tools/match`), and the Linux
build itself for the button and label classes (they are unnamed on Windows).

| Function | Windows | Linux |
|---|---|---|
| `cWinSC3::DoCopyRightScreen(bool)` | `0x43e4a3` | |
| `cWinSC3::DoTitleBackgroundScreen` | `0x43e05b` | |
| `cWinSC3::ActivateMainMenu` | `0x43e9d8` | `0x08208d70` |
| `get_main_menu_buffer_info` | `0x43a31e` | `0x0820a6a0` |
| `cSC3MainMenu::Init` | `0x43a8b9` | `0x0820bc04` |
| `cSC3MainMenu::GZOnCommand` | | `0x0820fda0` |
| `cSC3WinMainMenuBtn::*` | | `0x0820a1ac`–`0x0820fb54` |
| `cSC3WinMainMenuLabel::*` | | `0x08209e84`–`0x0820f244` |
| `cSC3WinLogo::*` (splash window) | | `0x08205024`–`0x08209974` |

## Startup screens
1. `INTRO.TGQ` (EA TGQ video, not implemented).
2. **Copyright splash**: `MAIN.IXF` `22721000` (640x480, "SimCity 3000 Unlimited" logo) in a
   `cSC3WinLogo` centred on a black screen. Label: `041F2625/0x197` in `title9.fbf`, text
   `D47A45` with a `202020` shadow at +1,+1, right-aligned 2 px from the right edge and 4 px
   above the bottom. With wait type `25E74BA2` it closes on a click or key press; `25E74BA1`
   closes once painted twice. Localised builds override the image with the `SPLASHBMP` setting.
3. **Title background**: `22729921` (800x600, no logo), centred. Larger screens get borders,
   smaller ones a centred crop. Override: `WINSC3BMP`.
4. **Main menu**: a 640x480 `cSC3MainMenu` window centred over the background.

## Layout
Coordinates are relative to the menu window. They come from a table at `0x4fa9b8..0x4faa2c`,
which is zero in the file and filled by a static initializer (`0x4394de..0x439566`).
Labels are centred on `button origin + label point`.

| Window ID | Entry | Sheet (`82B9B75C/…`) | Frame | Origin | Label | Label point | AA colour |
|---|---|---|---|---|---|---|---|
| `712BF5BE` | Start New City | `22729936` | 281x203 | 31,137 | `03C09AFF/1` | 136,132 | `40601F` |
| `712BF5BF` | Load City | `22729932` | 273x218 | 333,156 | `03C09AFF/2` | 151,146 | `707F80` |
| `712BF5C1` | Starter Town | `22729937` | 134x99 | 194,73 | `03C09AFF/9` | 63,53 | `707F60` |
| `712BF5C2` | Preferences | `22729934` | 157x135 | 180,313 | `03C09AFF/4` | 73,54 | `7F7F96` |
| `712BF5C5` | Play Scenario | `22729938` | 135x138 | 353,10 | `041F2625/1` | 63,88 | `707040` |
| `712BF5C0` | Real City Terrain | `22729935` | 155x122 | 319,331 | `03C09AFF/0x13` | 77,66 | `807860` |
| `712BF5C3` | Maxis logo → credits | `22729933` | 102x68 | 10,402 | — | | |
| `712BF5C4` | Exit | `22729930` (click: `22729931`) | 91x94 | 520,367 | `03C09AFF/6` | 49,83 | `50606F` |

The table is in creation order, which is also the drawing order: later entries are on top.
`get_main_menu_buffer_info` maps slots 1–9 to the sheets and lets a `MAINMENUBMP` setting
override them (`<slot=group,instance>` pairs) for localised art.

Every sheet holds 8 frames in 4 columns x 2 rows (`tImageInfo::SetFrame`). The colour key is
magenta, except the Maxis logo, which keys black.

Labels use system font 0x10 (`Serif17.fbf`). Each is drawn twice: a black shadow at +1,+1,
then the text in `FFD778`, or `FFFF20` while its button is hovered. The anti-alias colour
differs per button and matches the art behind the label.

## Button behaviour (`cSC3WinMainMenuBtn`)
Flags (constructor default `0x14`):

| Flag | Meaning | Set on |
|---|---|---|
| `0x01` | animate the click (frames 0→7, then send the command) | none |
| `0x04` | hit-test opaque pixels of the current frame, not the box | all but the Maxis logo |
| `0x08` | play once: rest on frame 7, hold frame 6 while hovered | Maxis logo |
| `0x10` | freeze on the current frame when the pointer leaves | all but Exit |

States `65DCC508`–`65DCC50C`: idle, hover, leaving, clicked, clicked-done. `GZPaint` advances
one frame when at least 101 ms have passed since the last advance:
- hover: loop 0→7; with `0x08`, stop at 7, go idle and show frame 6.
- leaving (only without `0x10`): keep animating until frame 0, then idle. This is how the
  Exit door finishes its cycle.
- clicked: at frame 7, post the command.

Pointer handling follows GZ mouse capture. The topmost button whose hit test passes takes
the capture on a mouse move ("enter": label highlighted, state hover). The captor releases it
when the pointer is off its pixels ("leave"). The release does not pass the move on. A click
sends `GZOnCommand(1, window id)` straight away.

## Commands
`cSC3MainMenu::GZOnCommand` plays UI sound `0xD`, then:
- Preferences → `do_prefs` (command `C3001401`). The menu stays open.
- Maxis logo → `do_credits` (`cSC3WinCredits`, logo `22729940`). The menu stays open.
- Everything else → `end_dialog(id)`. `cWinSC3::ActivateMainMenu` then opens:
  - Start New City: `E3270FE9`, the New City Options dialog (`new-city.md`).
  - Load City (`83172AD7`), Real City Terrain (`23172AF2`), Starter Town (`63172B74`) and
    Play Scenario (`A42C7725`): load dialogs of different file types.
  - Exit: a quit confirmation.

## Not yet implemented
The intro video, the UI sounds (the menu music plays), the custom cursors
(`Res/UI/Shared/Cursors`), the quit confirmation, and every dialog behind the menu entries
except Start New City.
