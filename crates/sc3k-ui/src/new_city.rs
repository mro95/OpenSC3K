//! The New City Options dialog (`cSC3WinProcCityScheme` in a `cSC3WinGen`, SimUI.dll), opened
//! by "Start New City" on the main menu. See `docs/ui/new-city.md`.

use crate::controls::{outline, Button, FlatRect, Key, Label, LineInput, OptionGroup, Strip, Style};
use crate::surface::{Rect, Sprite, Surface};
use crate::text::{draw_text, TextColors};
use sc3k_assets::Assets;
use sc3k_formats::fbf::Font;
use sc3k_formats::ini::{parse_u32, parse_u32_list};
use sc3k_formats::sprite::{self, AlphaMask};
use sc3k_sim::city::NewCityInfo;
use std::rc::Rc;

/// Group of the framework's system images in `Res/UI/Shared/SYS.IXF`.
pub const SYS_GROUP: u32 = 0x82B9_B75B;
/// `cSC3WinGen` background: 256x256 clouds, tiled from the window origin.
const CLOUDS: u32 = 0xE2B1_4588;
/// String tables: `SC3StringsWindow.IXF` and `SEStringsUI.IXF`.
const WINDOW_STRINGS: u32 = 0x0295_41F4;
const SE_STRINGS: u32 = 0x041F_2625;
/// `SystemFont(0xF)` and `SystemFont(0xD)`.
const MAIN_FONT: &str = "main9.fbf";
const TITLE_FONT: &str = "title12.fbf";

/// `cWinCtrlMgr` default colours (`DefaultColor`).
const INK: u32 = 0x31_367C; // 62E56A31: text of every system font but the title's
const BAR_FILL: u32 = 0x7C_90FF; // 62E56A2C
const BAR_OUTLINE: u32 = 0xA8_B4FF; // 62E56A2D

/// `cSC3WinGen` gutters, between the bevel and the content.
const GUTTER: i32 = 4;
/// Height of a system button frame (`62B19CE9` rows); the title bar fits one plus 2 px.
const SYS_BUTTON_H: i32 = 25;
/// Top of the bottom bar: the bottom of the left pane (`0x14a`, or lower if the pane is taller).
const BOTTOM_BAR_TOP: i32 = 330;
/// OK button height plus 2 (`A2DEFD9A` is 32 high).
const BOTTOM_BAR_H: i32 = 34;

/// Top of the left pane's first label.
const LEFT_TOP: i32 = 0x24;
/// `cGZWinLineInput` gutters as the dialog sets them (left, top, right, bottom).
const EDIT_GUTTERS: (i32, i32, i32, i32) = (4, 1, 2, 1);

/// The name boxes: (label string, default text string), x, width and maximum length.
const NAME_FIELDS: [(u32, u32); 2] = [(0x229, 0x282), (0x22A, 0x281)]; // City Name, Mayor Name
const EDIT_X: i32 = 23;
const EDIT_W: i32 = 190;
const NAME_MAX: usize = 50;

/// (table, string, x, y) of the labels of the right pane.
const RIGHT_LABELS: [(u32, u32, i32, i32); 4] = [
    (SE_STRINGS, 0x28, 0xE8, 0x2C),    // Landscape:
    (SE_STRINGS, 0x29, 0xE8, 0x6A),    // Trees:
    (SE_STRINGS, 0x2A, 0xE8, 0xBC),    // Buildings:
    (SE_STRINGS, 0x2B, 0x130, 0x124),  // User-Made Buildings:
];

/// Window IDs of the dialog's own buttons.
const CLOSE: u32 = 0x42B7_C353; // cSC3WinGenTitleBar close button; the dialog treats it as Cancel
const OK: u32 = 0x2552_483D;
const SELECT: u32 = 0x2552_485A; // opens the building replacement manager

/// System images: the title bar buttons (4 rows of 25 px: close, back, minimise, maximise,
/// 4 frames of 24 px each), the OK check mark and the SC3 text button.
const SYS_BUTTONS: u32 = 0x62B1_9CE9;
const OK_IMAGE: u32 = 0xA2DE_FD9A;
const TEXT_BUTTON: u32 = 0xE2B6_6DB8;
/// `SystemFont(0xC)`, used by `cSC3CtrlCreator::create_SC3TextBtn`.
const TEXT_BUTTON_FONT: &str = "title9.fbf";

/// Radio button (8 frames of 12x12, keyed magenta) and check box (8 frames of 13x13) images.
const RADIO_IMAGE: u32 = 0x0000_000A;
const RADIO_KEY: u16 = 0xF81F;
const CHECK_IMAGE: u32 = 0xC2D6_E93A;

/// Option groups of the left pane: (caption string, [(window ID, string)]), all in
/// `WINDOW_STRINGS`. The difficulty strings take the starting funds as `%s`.
const DIFFICULTY: (u32, [(u32, u32); 3]) = (0x22B, [(0x2552_4845, 0x22C), (0x2552_4846, 0x22D), (0x2552_4847, 0x22E)]);
const START_DATE: (u32, [(u32, u32); 3]) = (0x22F, [(0x2552_484A, 0x230), (0x2552_484B, 0x231), (0x2552_484C, 0x232)]);
const CITY_SIZE: (u32, [(u32, u32); 4]) =
    (0x233, [(0x2552_484F, 0x2CE), (0x2552_4850, 0x234), (0x2552_4851, 0x2D3), (0x2552_4852, 0x235)]);
/// Starting funds per difficulty, as `%s` in the difficulty strings.
const FUNDS: [u32; 3] = [50_000, 20_000, 10_000];
/// Check boxes: (window ID, string).
const DISASTERS: (u32, u32) = (0x2552_4854, 0x21D);
const AUTO_BUDGET: (u32, u32) = (0x2552_4855, 0x21E);

/// Funds as the game prints them: a simoleon sign (cp1252 0xA7) and thousands separators.
fn money(amount: u32) -> Vec<u8> {
    let digits = amount.to_string();
    let mut out = vec![0xA7];
    for (i, c) in digits.bytes().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(b',');
        }
        out.push(c);
    }
    out
}

/// Replace the first `%s` of `template` with `arg`.
fn format_s(template: &[u8], arg: &[u8]) -> Vec<u8> {
    match template.windows(2).position(|w| w == b"%s") {
        Some(i) => [&template[..i], arg, &template[i + 2..]].concat(),
        None => template.to_vec(),
    }
}

/// What the player chose, as `cSC3WinProcCityScheme` hands it on when OK is pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Windows-1252 bytes.
    pub city_name: Vec<u8>,
    pub mayor_name: Vec<u8>,
    /// 1 easy, 2 medium, 3 hard.
    pub difficulty: u32,
    /// Starting funds; on hard they are a loan.
    pub funds: u32,
    pub loan: bool,
    pub start_year: u32,
    /// Map edge in tiles: 0x40, 0x80, 0xC0 or 0x100.
    pub size: u32,
    pub disasters: bool,
    pub auto_budget: bool,
    /// Keys of the chosen `SC3CityScheme.ini` entries.
    pub landscape: u32,
    pub flora: u32,
    pub buildings: u32,
}

impl Settings {
    /// `cSC3CmdNewCity::Execute` (Loki libSimInit 0x4A834) copies the dialog into a
    /// `cSC3NewCityInfo` in this order: name, mayor, funds, debt, difficulty, size, year,
    /// auto budget, disasters, dirt generator. The original cuts names to 127 characters.
    /// The schemes go to `cSC3CitySchemeMgr` instead.
    /// Unchecked: no Windows address known yet.
    pub fn new_city_info(&self) -> NewCityInfo {
        let mut info = NewCityInfo {
            city_name: self.city_name.iter().copied().take(0x7F).collect(),
            mayor_name: self.mayor_name.iter().copied().take(0x7F).collect(),
            funds: self.funds as i64,
            funds_are_debt: self.loan,
            difficulty: self.difficulty as i32,
            start_year: self.start_year,
            auto_budget: self.auto_budget,
            disasters: self.disasters,
            ..NewCityInfo::default()
        };
        info.set_city_size(self.size);
        info
    }
}

/// How the dialog closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Start(Settings),
    Cancel,
}

/// Size set by the dialog's init (`SetSize(0x27f, 0x16d)`); the height grows to 368 once the
/// bottom bar is placed.
pub const WIDTH: i32 = 639;
pub const HEIGHT: i32 = 368;

/// What a scheme looks like in the preview.
enum Preview {
    /// The landscape: a whole span sprite.
    Picture(Sprite),
    /// A flora or building set: a colour sprite blended through its alpha mask.
    Blended(Sprite, AlphaMask),
}

/// One choice of `SC3CityScheme.ini` (`cSC3CitySchemeMgr`), shown as a toggle button.
struct Scheme {
    /// The entry's key; 0 is the default (`DetermineDefaultScheme`).
    key: u32,
    button: Button,
    preview: Preview,
}

/// The three rows of scheme buttons: (section, first window ID, button bottom, slot width).
/// Arguments of FUN_1005b0d7 in `cSC3WinProcCityScheme`'s init. Only the landscape row's first ID
/// (0xE558C351) is a constant there; the other two come from member fields and are
/// placeholders here.
const SCHEME_ROWS: [(&str, u32, i32, i32); 3] = [
    ("LandScapes", 0xE558_C351, 0x5C, 0x2B),
    ("FloraSets", 0xE558_C361, 0xA6, 0x27),
    ("BuildingSets", 0xE558_C371, 0xF9, 0x31),
];
/// Left edge of the first slot of each row.
const SCHEME_LEFT: i32 = 0xF8;

struct SchemeRow {
    schemes: Vec<Scheme>,
    selected: usize,
}

impl SchemeRow {
    fn current(&self) -> &Scheme {
        &self.schemes[self.selected]
    }
}

fn missing(what: String) -> sc3k_assets::Error {
    sc3k_assets::Error::Missing { what }
}

fn span(assets: &Assets, group: u32, instance: u32) -> sc3k_assets::Result<Sprite> {
    match assets.sprite(group, instance)? {
        sprite::Sprite::Span(s) => Ok(Sprite::from_span(&s)),
        sprite::Sprite::Alpha(_) => Err(missing(format!("colour sprite {group:08X}/{instance:08X}"))),
    }
}

fn mask(assets: &Assets, group: u32, instance: u32) -> sc3k_assets::Result<AlphaMask> {
    match assets.sprite(group, instance)? {
        sprite::Sprite::Alpha(m) => Ok(m),
        sprite::Sprite::Span(_) => Err(missing(format!("alpha mask {group:08X}/{instance:08X}"))),
    }
}

/// Load one section of the scheme file and lay its buttons out: each centred in its slot,
/// bottoms aligned, the default (key 0) toggled on.
fn load_row(assets: &Assets, ini: &sc3k_formats::ini::Ini, row: usize) -> sc3k_assets::Result<SchemeRow> {
    let (section, first_id, bottom, slot) = SCHEME_ROWS[row];
    let entries = ini.section(section).ok_or_else(|| missing(format!("SC3CityScheme.ini [{section}]")))?;
    let mut schemes = Vec::new();
    for (i, (key, value)) in entries.iter().enumerate() {
        let bad = || missing(format!("SC3CityScheme.ini [{section}] {key}"));
        let key = parse_u32(key).ok_or_else(bad)?;
        let f = parse_u32_list(value).filter(|f| f.len() >= 6).ok_or_else(bad)?;
        // LandScapes: icon group/instance, palette group/instance, preview group/instance, ...
        // Flora/BuildingSets: icon group/instance, colour group/instance, mask group/instance, ...
        let preview = if row == 0 {
            Preview::Picture(span(assets, f[4], f[5])?)
        } else {
            Preview::Blended(span(assets, f[2], f[3])?, mask(assets, f[4], f[5])?)
        };
        // KNOWN ISSUE: the original resolves the icon through cSC3MenuBtnDef::GetMenuBtnDefID,
        // not by loading image (f[0], f[1]) directly. Loading it directly pairs the wrong icons
        // with some previews: FloraSets key 0 gets the palm icon (2003) but the conifer sprite
        // (3FBE0000). The button-definition table is not traced yet.
        let icon = Strip::new(Sprite::new(&assets.image(f[0], f[1])?, None), 4);
        let mut button = Button::new(first_id + i as u32, Style::FixedToggle, icon);
        let (w, h) = (button.area.w, button.area.h);
        button.move_to(SCHEME_LEFT + slot * i as i32 + slot / 2 - w / 2, bottom - h);
        schemes.push(Scheme { key, button, preview });
    }
    let selected = schemes.iter().position(|s| s.key == 0).unwrap_or(0);
    if let Some(s) = schemes.get_mut(selected) {
        s.button.on = true;
    }
    if schemes.is_empty() {
        return Err(missing(format!("SC3CityScheme.ini [{section}] entries")));
    }
    Ok(SchemeRow { schemes, selected })
}

pub struct NewCity {
    /// Landscape, flora, buildings.
    rows: Vec<SchemeRow>,
    clouds: Sprite,
    title_font: Rc<Font>,
    title: Vec<u8>,
    title_bar: FlatRect,
    bars: Vec<FlatRect>,
    labels: Vec<Label>,
    /// Close, OK, Select, then the Disasters and Auto Budget check boxes.
    buttons: Vec<Button>,
    /// Difficulty, start date, city size.
    groups: Vec<OptionGroup>,
    /// City name, mayor name.
    inputs: Vec<LineInput>,
    /// Top-left of the window on screen.
    origin: (i32, i32),
}

impl NewCity {
    pub fn load(assets: &Assets) -> sc3k_assets::Result<NewCity> {
        let main = Rc::new(assets.font(MAIN_FONT)?);
        let title_font = Rc::new(assets.font(TITLE_FONT)?);
        let ink = TextColors::ink(&main, INK);

        // cSC3WinGenTitleBar: full width inside the gutters, tall enough for the caption and
        // the close button.
        let bar_h = 8.max(title_font.line_height() + 8).max(SYS_BUTTON_H + 2);
        let title_bar =
            FlatRect::new(Rect::new(GUTTER, GUTTER, WIDTH - 2 * GUTTER, bar_h), BAR_FILL, outline::ALL, BAR_OUTLINE);
        let user_top = title_bar.area.bottom() + GUTTER;

        // AddBlueBar: two vertical separators around the left pane and the bottom strip.
        let bar = |l, t, r, b| FlatRect::new(Rect::new(l, t, r - l, b - t), BAR_FILL, outline::ALL, BAR_OUTLINE);
        let pane_bottom = BOTTOM_BAR_TOP - GUTTER;
        let bars = vec![
            bar(GUTTER, user_top, GUTTER + 10, pane_bottom),
            bar(0xDA, user_top, 0xE4, pane_bottom),
            bar(GUTTER, BOTTOM_BAR_TOP, WIDTH - GUTTER, BOTTOM_BAR_TOP + BOTTOM_BAR_H),
        ];

        // Left pane (FUN_1005bbd5): each label is followed by its edit box and a 2 px gap.
        let mut labels = Vec::new();
        let mut y = LEFT_TOP;
        let mut inputs = Vec::new();
        let edit_h = LineInput::min_height(&main, EDIT_GUTTERS);
        for (label, default) in NAME_FIELDS {
            labels.push(Label::new(15, y, assets.string(WINDOW_STRINGS, label)?, main.clone(), ink));
            y += main.line_height();
            let area = Rect::new(EDIT_X, y, EDIT_W, edit_h);
            let text = assets.string(WINDOW_STRINGS, default)?;
            // Drawn in the font's colours, which the game has already re-inked to INK.
            let mut input = LineInput::new(area, text, main.clone(), ink, NAME_MAX);
            let (l, t, r, b) = EDIT_GUTTERS;
            input.set_gutters(l, t, r, b);
            inputs.push(input);
            y += edit_h + 2;
        }
        // The city name has the focus when the dialog opens.
        inputs[0].focus(true);
        for &(table, id, x, y) in &RIGHT_LABELS {
            labels.push(Label::new(x, y, assets.string(table, id)?, main.clone(), ink));
        }

        // Close button: row 0 of the system buttons, right-aligned in the title bar.
        let sys = Sprite::new(&assets.image(SYS_GROUP, SYS_BUTTONS)?, None);
        let mut close = Button::new(CLOSE, Style::FixedButton, Strip::row(sys, 4, 0, SYS_BUTTON_H));
        let t = title_bar.area;
        close.move_to(t.right() - 4 - close.area.w, t.y + (t.h - close.area.h) / 2);

        // OK: right end of the bottom bar, centred vertically (FUN_1005e570).
        let ok_image = Strip::new(Sprite::new(&assets.image(SYS_GROUP, OK_IMAGE)?, None), 4);
        let mut ok = Button::new(OK, Style::Button, ok_image);
        let bottom = bars[2].area;
        ok.move_to(bottom.right() - 4 - ok.area.w, bottom.y + (bottom.h - ok.area.h) / 2);

        // Select: an SC3TextBtn 5 px right of "User-Made Buildings:", at least 78 wide.
        // OPEN ISSUE: drawn without a colour key, so the image's blue background (7B92FF) shows
        // as a box on the clouds. The key, if any, is set by the buffer loader, not the file.
        let text_font = Rc::new(assets.font(TEXT_BUTTON_FONT)?);
        let text_image = Strip::new(Sprite::new(&assets.image(SYS_GROUP, TEXT_BUTTON)?, None), 4);
        let mut select = Button::new(SELECT, Style::Button, text_image).with_caption(
            assets.string(SE_STRINGS, 0x2F)?,
            text_font.clone(),
            TextColors::ink(&text_font, INK),
        );
        select.set_disabled_colors(TextColors::new(0x40_4040, 0x80_8080));
        select.set_gutters(10, 0, 16, 4);
        select.auto_size();
        select.area.w = select.area.w.max(0x4E);
        let user_made = labels.last().map_or(0x1AE, |l: &Label| l.area.right());
        select.move_to(user_made + 5, 0x11A);

        // Left pane, below the edit boxes (FUN_1005bbd5).
        let radio = Strip::new(Sprite::new(&assets.image(SYS_GROUP, RADIO_IMAGE)?, Some(RADIO_KEY)), 8);
        let check = Strip::new(Sprite::new(&assets.image(SYS_GROUP, CHECK_IMAGE)?, None), 8);
        let group = |x, y, caption: u32, items: &[(u32, u32)], text: &dyn Fn(usize, Vec<u8>) -> Vec<u8>| {
            let caption = Label::new(x, y, assets.string(WINDOW_STRINGS, caption)?, main.clone(), ink);
            let mut buttons = Vec::new();
            for (i, &(id, string)) in items.iter().enumerate() {
                let label = text(i, assets.string(WINDOW_STRINGS, string)?);
                let mut b = Button::new(id, Style::Radio, radio.clone()).with_caption(label, main.clone(), ink);
                b.set_gutters(2, 2, 2, 2);
                buttons.push(b);
            }
            Ok::<_, sc3k_assets::Error>(OptionGroup::new(caption, buttons))
        };
        let plain = |_, t| t;
        let difficulty = group(15, y, DIFFICULTY.0, &DIFFICULTY.1, &|i, t| format_s(&t, &money(FUNDS[i])))?;
        y += difficulty.area.h;
        let date = group(15, y, START_DATE.0, &START_DATE.1, &plain)?;
        let size = group(100, y, CITY_SIZE.0, &CITY_SIZE.1, &plain)?;
        y += date.area.h.max(size.area.h) + 2;
        let mut groups = vec![difficulty, date, size];
        // Defaults when there are no saved choices: Easy, 1900 and (on any machine fast
        // enough) Large.
        for id in [DIFFICULTY.1[0].0, START_DATE.1[0].0, CITY_SIZE.1[3].0] {
            groups.iter_mut().for_each(|g| {
                g.select(id);
            });
        }

        // Check boxes, stacked, then pushed down so the lower one ends 2 px above the pane
        // bottom (0x14a at least).
        let mut checks = Vec::new();
        for (id, string) in [DISASTERS, AUTO_BUDGET] {
            let mut b = Button::new(id, Style::CheckBox, check.clone())
                .with_caption(assets.string(WINDOW_STRINGS, string)?, main.clone(), ink);
            b.set_gutters(3, 1, 2, 2);
            b.auto_size();
            b.move_to(17, y);
            y += b.area.h;
            checks.push(b);
        }
        let pane_bottom = (y + 2).max(BOTTOM_BAR_TOP);
        debug_assert_eq!(pane_bottom, BOTTOM_BAR_TOP, "the left pane outgrew the dialog");
        let mut cy = pane_bottom - (checks[0].area.h + checks[1].area.h + 2);
        for b in &mut checks {
            b.move_to(17, cy);
            cy += b.area.h;
        }

        let mut buttons = vec![close, ok, select];
        buttons.extend(checks);

        let ini = assets.sys_ini("SC3CityScheme.ini")?;
        let rows = (0..SCHEME_ROWS.len()).map(|r| load_row(assets, &ini, r)).collect::<Result<_, _>>()?;

        Ok(NewCity {
            rows,
            clouds: Sprite::new(&assets.image(SYS_GROUP, CLOUDS)?, None),
            title: assets.string(SE_STRINGS, 0x2D)?,
            title_font,
            title_bar,
            bars,
            labels,
            buttons,
            groups,
            inputs,
            origin: (0, 0),
        })
    }

    /// Centre the window on a screen of the given size.
    pub fn layout(&mut self, screen_w: i32, screen_h: i32) {
        self.origin = ((screen_w - WIDTH) / 2, (screen_h - HEIGHT) / 2);
    }

    fn buttons_mut(&mut self) -> impl Iterator<Item = &mut Button> {
        let schemes = self.rows.iter_mut().flat_map(|r| r.schemes.iter_mut().map(|s| &mut s.button));
        let radios = self.groups.iter_mut().flat_map(|g| g.buttons_mut());
        self.buttons.iter_mut().chain(radios).chain(schemes)
    }

    fn local(&self, x: i32, y: i32) -> (i32, i32) {
        (x - self.origin.0, y - self.origin.1)
    }

    /// Pointer moved to screen (x, y).
    pub fn mouse_move(&mut self, x: i32, y: i32) {
        let (x, y) = self.local(x, y);
        self.buttons_mut().for_each(|b| b.mouse_move(x, y));
    }

    pub fn mouse_left(&mut self) {
        self.buttons_mut().for_each(|b| b.mouse_left());
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        let (x, y) = self.local(x, y);
        if let Some(i) = self.inputs.iter().position(|e| e.area.contains(x, y)) {
            for (j, e) in self.inputs.iter_mut().enumerate() {
                e.focus(false);
                if j == i {
                    e.mouse_down(x, y);
                }
            }
            return;
        }
        for b in self.buttons_mut() {
            if b.mouse_down(x, y) {
                break;
            }
        }
    }

    /// Left button released at screen (x, y); a click may close the dialog.
    pub fn mouse_up(&mut self, x: i32, y: i32) -> Option<Outcome> {
        let (x, y) = self.local(x, y);
        let clicked = self.buttons_mut().fold(None, |c, b| if b.mouse_up(x, y) { Some(b.id) } else { c });
        self.command(clicked?)
    }

    /// Keys go to the focused name box first. Tab moves the focus to the next box;
    /// Escape closes the window (`cSC3WinGen::GZOnKeyDown`).
    pub fn key(&mut self, key: Key) -> Option<Outcome> {
        if self.inputs.iter_mut().any(|e| e.key(key)) {
            return None;
        }
        match key {
            Key::Escape => Some(Outcome::Cancel),
            Key::Tab => {
                let n = self.inputs.len();
                let next = self.inputs.iter().position(|e| e.focused).map_or(0, |i| (i + 1) % n);
                for (i, e) in self.inputs.iter_mut().enumerate() {
                    e.focus(i == next);
                }
                None
            }
            _ => None,
        }
    }

    /// A typed character, for the focused name box.
    pub fn character(&mut self, c: char) {
        self.inputs.iter_mut().for_each(|e| {
            e.character(c);
        });
    }

    /// The current choices (`cSC3WinProcCityScheme`'s OK handler).
    pub fn settings(&self) -> Settings {
        let selected = |g: usize| self.groups[g].selected().unwrap_or(0);
        let difficulty = DIFFICULTY.1.iter().position(|&(id, _)| id == selected(0)).unwrap_or(0);
        let year = START_DATE.1.iter().position(|&(id, _)| id == selected(1)).unwrap_or(0);
        let size = CITY_SIZE.1.iter().position(|&(id, _)| id == selected(2)).unwrap_or(3);
        let checked = |id| self.buttons.iter().any(|b| b.id == id && b.on);
        Settings {
            city_name: self.inputs[0].text().to_vec(),
            mayor_name: self.inputs[1].text().to_vec(),
            difficulty: difficulty as u32 + 1,
            funds: FUNDS[difficulty],
            loan: difficulty == 2,
            start_year: [1900, 1950, 2000][year],
            size: 0x40 * (size as u32 + 1),
            disasters: checked(DISASTERS.0),
            auto_budget: checked(AUTO_BUDGET.0),
            landscape: self.rows[0].current().key,
            flora: self.rows[1].current().key,
            buildings: self.rows[2].current().key,
        }
    }

    /// `cSC3WinProcCityScheme::GZOnCommand`. The original plays UI sound 0xD for OK/Cancel
    /// and 0x92 for a scheme button.
    fn command(&mut self, id: u32) -> Option<Outcome> {
        match id {
            OK => return Some(Outcome::Start(self.settings())),
            CLOSE => return Some(Outcome::Cancel),
            SELECT => eprintln!("new city: the building replacement manager is not implemented yet"),
            _ if self.groups.iter_mut().any(|g| g.select(id)) => {}
            _ => {
                for row in &mut self.rows {
                    if let Some(i) = row.schemes.iter().position(|s| s.button.id == id) {
                        // Clicking the current choice keeps it on; another one moves the selection.
                        for (j, s) in row.schemes.iter_mut().enumerate() {
                            s.button.on = j == i;
                        }
                        row.selected = i;
                    }
                }
            }
        }
        None
    }

    fn area(&self) -> Rect {
        Rect::new(self.origin.0, self.origin.1, WIDTH, HEIGHT)
    }

    pub fn draw(&self, s: &mut Surface) {
        let a = self.area();
        // cSC3WinGen::GZPaint: tiled background, then a one-pixel bevel.
        s.blit_tiled(&self.clouds, self.clouds.area(), a, a);
        let (r, b) = (a.right() - 1, a.bottom() - 1);
        s.line(a.x, a.y, a.x, b, 0xFFFFFF);
        s.line(a.x, a.y, r, a.y, 0xFFFFFF);
        s.line(r, a.y, r, b, 0x000000);
        s.line(a.x, b, r, b, 0x000000);

        let (ox, oy) = self.origin;
        self.title_bar.draw(s, ox, oy);
        // The title keeps title12's own colours (it is the one font the game does not re-ink).
        let t = self.title_bar.area.offset(ox, oy);
        let ty = t.y + (t.h - self.title_font.line_height()) / 2;
        draw_text(s, &self.title_font, t.x + 4, ty, &self.title, TextColors::of_font(&self.title_font));
        for b in &self.bars {
            b.draw(s, ox, oy);
        }
        for l in &self.labels {
            l.draw(s, ox, oy);
        }
        for b in &self.buttons {
            b.draw(s, ox, oy);
        }
        for g in &self.groups {
            g.draw(s, ox, oy);
        }
        for e in &self.inputs {
            e.draw(s, ox, oy);
        }
        for row in &self.rows {
            for scheme in &row.schemes {
                scheme.button.draw(s, ox, oy);
            }
        }
        self.draw_preview(s, a);
    }

    /// `cSC3WinProcCityScheme::GZPaint`: the landscape, a tree group behind the building, the
    /// building, then a second tree group in front, all clipped to the window.
    fn draw_preview(&self, s: &mut Surface, a: Rect) {
        let (landscape, flora, building) = (self.rows[0].current(), self.rows[1].current(), self.rows[2].current());
        if let Preview::Picture(p) = &landscape.preview {
            s.blit_clip(p, p.area(), a.x + 0x1BC, a.y + 0x5C, a);
        }
        let blend = |s: &mut Surface, p: &Preview, x: &dyn Fn(i32) -> i32, bottom: i32| {
            if let Preview::Blended(c, m) = p {
                s.blit_alpha(c, m, a.x + x(c.width), a.y + bottom - c.height, a);
            }
        };
        blend(s, &flora.preview, &|w| 0x24D - w / 2, 0xDE);
        blend(s, &building.preview, &|w| 600 - w, 0xF2);
        blend(s, &flora.preview, &|w| 0x1E8 - w / 2, 0xF2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_groups_thousands() {
        assert_eq!(money(50_000), b"\xA750,000");
        assert_eq!(money(999), b"\xA7999");
        assert_eq!(money(1_000_000), b"\xA71,000,000");
    }

    #[test]
    fn format_s_fills_the_first_placeholder() {
        assert_eq!(format_s(b"Hard  (%s Loan)", b"x"), b"Hard  (x Loan)");
        assert_eq!(format_s(b"none", b"x"), b"none");
    }

    #[test]
    fn default_settings_to_info() {
        let settings = Settings {
            city_name: b"New City".to_vec(),
            mayor_name: b"Defacto".to_vec(),
            difficulty: 1,
            funds: FUNDS[0],
            loan: false,
            start_year: 1900,
            size: 0x100,
            disasters: false,
            auto_budget: false,
            landscape: 0,
            flora: 0,
            buildings: 0,
        };
        let info = settings.new_city_info();
        assert_eq!((info.funds, info.funds_are_debt, info.difficulty), (50_000, false, 1));
        assert_eq!((info.x_size, info.y_size, info.z_size), (0x100, 0x100, 0x100));
        assert_eq!((info.start_year, info.city_type), (1900, 7));
        assert_eq!(info.city_name, b"New City");
    }
}
