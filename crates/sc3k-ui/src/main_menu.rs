//! The main menu (`cSC3MainMenu`), recreated from `cSC3MainMenu::Init` (`SC3U.exe+0x3a8b9`),
//! its coordinate table (`0x4fa9b8..0x4faa2c`, filled by a static initializer at
//! `0x4394de..0x439566`) and the Loki Linux build's `cSC3WinMainMenuBtn` / `cSC3WinMainMenuLabel`.
//! See `docs/ui/main-menu.md`.
//!
//! The menu is a 640x480 window centred over the title background. Each entry is an
//! animated button (an 8-frame sprite sheet, 4 columns x 2 rows) with a text label centred
//! on a fixed offset from the button's origin.

use crate::surface::{Rect, Sprite, Surface};
use crate::text::{draw_text, TextColors};
use sc3k_assets::Assets;
use sc3k_formats::fbf::Font;

/// Group of the UI images in `Res/UI/Shared/MAIN.IXF`.
pub const MAIN_GROUP: u32 = 0x82B9_B75C;
/// String tables (groups): `SC3StringsMenu.IXF` and `SEStringsUI.IXF` (Unlimited additions).
const MENU_STRINGS: u32 = 0x03C0_9AFF;
const SE_STRINGS: u32 = 0x041F_2625;
/// `cGZWinCtrlMgr::SystemFont(0x10)`, registered from `Serif17.fbf` (`SC3U.exe+0xf5c3`).
const LABEL_FONT: &str = "Serif17.fbf";

pub const WIDTH: i32 = 640;
pub const HEIGHT: i32 = 480;

/// Label colours from `cSC3MainMenu::Init`: (text, anti-alias) per state. The anti-alias colour
/// is per button, picked to match the art behind the label.
const LABEL_TEXT: u32 = 0xFFD778;
const LABEL_HIGHLIGHT: u32 = 0xFFFF20;
const LABEL_SHADOW: u32 = 0x000000;

/// Magenta, the colour key of every menu sheet except the Maxis logo (which keys black).
const MAGENTA_565: u16 = 0xF81F;

/// `cSC3WinMainMenuBtn::tFlag`.
mod flag {
    /// Play the 8-frame animation on click, then send the command (no menu button sets it).
    pub const ANIMATE_CLICK: u32 = 0x01;
    /// Hit-test against non-transparent pixels instead of the bounding box.
    pub const PIXEL_HIT: u32 = 0x04;
    /// Play the animation once and rest on frame 7 (frame 6 while hovered).
    pub const PLAY_ONCE: u32 = 0x08;
    /// Freeze on the current frame when the pointer leaves, instead of animating back to 0.
    pub const STOP_ON_LEAVE: u32 = 0x10;
    /// Constructor default.
    pub const DEFAULT: u32 = PIXEL_HIT | STOP_ON_LEAVE;
}

/// Minimum time between animation frames: `cSC3WinMainMenuBtn::GZPaint` advances when at least
/// 0x65 ms have passed.
pub const FRAME_MS: u64 = 101;

/// What the player picked. Window IDs `0x712BF5BE..0x712BF5C5`, handled by
/// `cSC3MainMenu::GZOnCommand` and `cWinSC3::ActivateMainMenu`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    StartNewCity,
    LoadCity,
    RealCityTerrain,
    StarterTown,
    /// `do_prefs`: opens the preferences dialog on top of the menu.
    Preferences,
    /// The Maxis logo: `do_credits` opens the credits on top of the menu.
    Credits,
    Exit,
    PlayScenario,
}

struct LabelDef {
    table: u32,
    string: u32,
    /// Label centre, relative to the button origin.
    center: (i32, i32),
    antialias: u32,
}

struct ButtonDef {
    window_id: u32,
    choice: Choice,
    /// Instance in `MAIN.IXF` (from `get_main_menu_buffer_info`, `SC3U.exe+0x3a31e`).
    image: u32,
    click_image: Option<u32>,
    origin: (i32, i32),
    color_key: u16,
    flags: u32,
    label: Option<LabelDef>,
}

/// In creation order, which is also the drawing order.
const BUTTONS: [ButtonDef; 8] = [
    ButtonDef {
        window_id: 0x712B_F5BE,
        choice: Choice::StartNewCity,
        image: 0x2272_9936,
        click_image: None,
        origin: (31, 137),
        color_key: MAGENTA_565,
        flags: flag::DEFAULT,
        label: Some(LabelDef { table: MENU_STRINGS, string: 1, center: (136, 132), antialias: 0x40601F }),
    },
    ButtonDef {
        window_id: 0x712B_F5BF,
        choice: Choice::LoadCity,
        image: 0x2272_9932,
        click_image: None,
        origin: (333, 156),
        color_key: MAGENTA_565,
        flags: flag::DEFAULT,
        label: Some(LabelDef { table: MENU_STRINGS, string: 2, center: (151, 146), antialias: 0x707F80 }),
    },
    ButtonDef {
        window_id: 0x712B_F5C1,
        choice: Choice::StarterTown,
        image: 0x2272_9937,
        click_image: None,
        origin: (194, 73),
        color_key: MAGENTA_565,
        flags: flag::DEFAULT,
        label: Some(LabelDef { table: MENU_STRINGS, string: 9, center: (63, 53), antialias: 0x707F60 }),
    },
    ButtonDef {
        window_id: 0x712B_F5C2,
        choice: Choice::Preferences,
        image: 0x2272_9934,
        click_image: None,
        origin: (180, 313),
        color_key: MAGENTA_565,
        flags: flag::DEFAULT,
        label: Some(LabelDef { table: MENU_STRINGS, string: 4, center: (73, 54), antialias: 0x7F7F96 }),
    },
    ButtonDef {
        window_id: 0x712B_F5C5,
        choice: Choice::PlayScenario,
        image: 0x2272_9938,
        click_image: None,
        origin: (353, 10),
        color_key: MAGENTA_565,
        flags: flag::DEFAULT,
        label: Some(LabelDef { table: SE_STRINGS, string: 1, center: (63, 88), antialias: 0x707040 }),
    },
    ButtonDef {
        window_id: 0x712B_F5C0,
        choice: Choice::RealCityTerrain,
        image: 0x2272_9935,
        click_image: None,
        origin: (319, 331),
        color_key: MAGENTA_565,
        flags: flag::DEFAULT,
        label: Some(LabelDef { table: MENU_STRINGS, string: 0x13, center: (77, 66), antialias: 0x807860 }),
    },
    ButtonDef {
        window_id: 0x712B_F5C3,
        choice: Choice::Credits,
        image: 0x2272_9933,
        click_image: None,
        origin: (10, 402),
        color_key: 0x0000,
        flags: (flag::DEFAULT | flag::PLAY_ONCE) & !flag::PIXEL_HIT,
        label: None,
    },
    ButtonDef {
        window_id: 0x712B_F5C4,
        choice: Choice::Exit,
        image: 0x2272_9930,
        click_image: Some(0x2272_9931),
        origin: (520, 367),
        color_key: MAGENTA_565,
        flags: flag::DEFAULT & !flag::STOP_ON_LEAVE,
        label: Some(LabelDef { table: MENU_STRINGS, string: 6, center: (49, 83), antialias: 0x50606F }),
    },
];

/// `cSC3WinMainMenuBtn` animation states (`0x65DCC508..0x65DCC50C`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Hover,
    Leaving,
    Clicked,
    ClickedDone,
}

struct Label {
    text: Vec<u8>,
    /// Relative to the menu origin.
    area: Rect,
    antialias: u32,
    highlight: bool,
}

struct Button {
    def: &'static ButtonDef,
    sheet: Sprite,
    click_sheet: Option<Sprite>,
    frame_w: i32,
    frame_h: i32,
    frame: u32,
    state: State,
    last_advance: Option<u64>,
    label: Option<Label>,
}

impl Button {
    fn area(&self) -> Rect {
        Rect::new(self.def.origin.0, self.def.origin.1, self.frame_w, self.frame_h)
    }

    fn has(&self, f: u32) -> bool {
        self.def.flags & f != 0
    }

    /// Source rectangle of the current frame (`tImageInfo::SetFrame`).
    fn frame_rect(&self) -> Rect {
        let (col, row) = (self.frame % 4, self.frame / 4);
        Rect::new(col as i32 * self.frame_w, row as i32 * self.frame_h, self.frame_w, self.frame_h)
    }

    /// `IsPointInWindowScreenCoordinates`: box test, then pixel test when `PIXEL_HIT` is set.
    fn hit(&self, x: i32, y: i32) -> bool {
        let a = self.area();
        if !a.contains(x, y) {
            return false;
        }
        if !self.has(flag::PIXEL_HIT) {
            return true;
        }
        let f = self.frame_rect();
        !self.sheet.is_transparent(f.x + x - a.x, f.y + y - a.y)
    }

    /// `GZOnCaptureChanged(this)`: the pointer entered.
    fn enter(&mut self) {
        self.set_highlight(true);
        self.state = State::Hover;
    }

    /// `GZOnCaptureChanged(other)`: the pointer left.
    fn leave(&mut self) {
        if matches!(self.state, State::Clicked | State::ClickedDone) {
            return;
        }
        self.set_highlight(false);
        if !self.has(flag::STOP_ON_LEAVE) {
            self.state = State::Leaving;
        } else {
            self.state = State::Idle;
            if self.has(flag::PLAY_ONCE) {
                self.frame = 7;
            }
        }
    }

    /// `reset_animation`, also sent to the Exit button by `cSC3MainMenu::Init`.
    fn reset(&mut self) {
        self.state = State::Idle;
        self.frame = if self.has(flag::PLAY_ONCE) { 7 } else { 0 };
        self.set_highlight(false);
    }

    fn set_highlight(&mut self, on: bool) {
        if let Some(l) = &mut self.label {
            l.highlight = on;
        }
    }

    /// The animation step of `GZPaint`. Returns true when a click animation finished and the
    /// command should be sent.
    fn tick(&mut self, now_ms: u64, captured: bool) -> bool {
        if matches!(self.state, State::Idle | State::ClickedDone) {
            return false;
        }
        if self.last_advance.is_some_and(|t| now_ms.saturating_sub(t) < FRAME_MS) {
            return false;
        }
        self.frame = (self.frame + 1) % 8;
        let mut fire = false;
        match self.state {
            State::Leaving | State::Hover if self.has(flag::PLAY_ONCE) => {
                if self.frame == 7 {
                    self.state = State::Idle;
                    self.frame = if captured { 6 } else { 7 };
                }
            }
            State::Leaving if self.frame == 0 => self.state = State::Idle,
            State::Clicked if self.frame == 7 => {
                self.state = State::ClickedDone;
                fire = true;
            }
            _ => {}
        }
        self.last_advance = Some(now_ms);
        fire
    }

    fn draw(&self, s: &mut Surface, ox: i32, oy: i32) {
        let sheet = match (self.state, &self.click_sheet) {
            (State::Clicked | State::ClickedDone, Some(click)) => click,
            _ => &self.sheet,
        };
        let a = self.area();
        s.blit(sheet, self.frame_rect(), ox + a.x, oy + a.y);
    }
}

pub struct MainMenu {
    font: Font,
    buttons: Vec<Button>,
    /// Top-left of the 640x480 menu window on screen.
    origin: (i32, i32),
    /// The button holding the mouse capture (the one under the pointer).
    capture: Option<usize>,
}

impl MainMenu {
    pub fn load(assets: &Assets) -> sc3k_assets::Result<MainMenu> {
        let font = assets.font(LABEL_FONT)?;
        let mut buttons = Vec::new();
        for def in &BUTTONS {
            let sheet = Sprite::new(&assets.image(MAIN_GROUP, def.image)?, Some(def.color_key));
            let click_sheet = match def.click_image {
                Some(id) => Some(Sprite::new(&assets.image(MAIN_GROUP, id)?, Some(def.color_key))),
                None => None,
            };
            let (frame_w, frame_h) = (sheet.width / 4, sheet.height / 2);
            let label = match &def.label {
                Some(l) => {
                    let text = assets.string(l.table, l.string)?;
                    // AutoSize: string width x line height, then centred on the label point.
                    let (w, h) = (font.text_width(&text), font.line_height());
                    let x = def.origin.0 + l.center.0 - w / 2;
                    let y = def.origin.1 + l.center.1 - h / 2;
                    Some(Label { text, area: Rect::new(x, y, w, h), antialias: l.antialias, highlight: false })
                }
                None => None,
            };
            let mut b = Button {
                def,
                sheet,
                click_sheet,
                frame_w,
                frame_h,
                frame: 0,
                state: State::Idle,
                last_advance: None,
                label,
            };
            b.reset();
            buttons.push(b);
        }
        Ok(MainMenu { font, buttons, origin: (0, 0), capture: None })
    }

    /// Centre the menu window on a screen of the given size.
    pub fn layout(&mut self, screen_w: i32, screen_h: i32) {
        self.origin = ((screen_w - WIDTH) / 2, (screen_h - HEIGHT) / 2);
    }

    /// Back to the initial state, as when the menu is shown again.
    pub fn reset(&mut self) {
        self.capture = None;
        for b in &mut self.buttons {
            b.reset();
        }
    }

    fn local(&self, x: i32, y: i32) -> (i32, i32) {
        (x - self.origin.0, y - self.origin.1)
    }

    /// Topmost button under the point (later buttons are on top).
    fn hit(&self, x: i32, y: i32) -> Option<usize> {
        self.buttons.iter().rposition(|b| b.hit(x, y))
    }

    /// Pointer moved to screen (x, y). Mirrors `cSC3WinMainMenuBtn::GZOnMouseMove`: the button
    /// under the pointer takes the capture; the capturing button releases it once the pointer
    /// is off its pixels. Like the original, the release does not hand the event on.
    pub fn mouse_move(&mut self, x: i32, y: i32) {
        let (x, y) = self.local(x, y);
        match self.capture {
            Some(i) => {
                let b = &self.buttons[i];
                if b.state != State::Clicked && !b.hit(x, y) {
                    self.release();
                }
            }
            None => {
                if let Some(i) = self.hit(x, y) {
                    self.capture = Some(i);
                    self.buttons[i].enter();
                }
            }
        }
    }

    /// The pointer left the window.
    pub fn mouse_left(&mut self) {
        self.release();
    }

    fn release(&mut self) {
        if let Some(i) = self.capture.take() {
            self.buttons[i].leave();
        }
    }

    /// Left button pressed at screen (x, y) (`cSC3WinMainMenuBtn::GZOnMouseDownL`).
    pub fn mouse_down(&mut self, x: i32, y: i32) -> Option<Choice> {
        let (lx, ly) = self.local(x, y);
        let i = self.capture.or_else(|| self.hit(lx, ly))?;
        let b = &mut self.buttons[i];
        if !b.has(flag::ANIMATE_CLICK) {
            return Some(self.command(i));
        }
        if b.state != State::Clicked {
            b.state = State::Clicked;
            b.frame = 0;
        }
        None
    }

    /// `cSC3MainMenu::GZOnCommand`. The original also plays UI sound 0xD here.
    fn command(&mut self, i: usize) -> Choice {
        let choice = self.buttons[i].def.choice;
        if !matches!(choice, Choice::Preferences | Choice::Credits) {
            // end_dialog: the menu closes, so the next activation starts fresh.
            self.reset();
        }
        choice
    }

    /// Advance animations (`GZPaint`). Returns a choice when a click animation completes.
    pub fn tick(&mut self, now_ms: u64) -> Option<Choice> {
        let mut fired = None;
        for (i, b) in self.buttons.iter_mut().enumerate() {
            if b.tick(now_ms, self.capture == Some(i)) && fired.is_none() {
                fired = Some(i);
            }
        }
        fired.map(|i| self.command(i))
    }

    pub fn draw(&self, s: &mut Surface) {
        let (ox, oy) = self.origin;
        for b in &self.buttons {
            b.draw(s, ox, oy);
            if let Some(l) = &b.label {
                // cSC3WinMainMenuLabel::GZPaint: shadow one pixel down-right, then the text.
                let (x, y) = (ox + l.area.x, oy + l.area.y);
                draw_text(s, &self.font, x + 1, y + 1, &l.text, TextColors::new(LABEL_SHADOW, l.antialias));
                let text = if l.highlight { LABEL_HIGHLIGHT } else { LABEL_TEXT };
                draw_text(s, &self.font, x, y, &l.text, TextColors::new(text, l.antialias));
            }
        }
    }

    /// Window ID of the button under the pointer, for debugging.
    pub fn hovered(&self) -> Option<u32> {
        self.capture.map(|i| self.buttons[i].def.window_id)
    }
}
