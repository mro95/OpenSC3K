//! Controls of the GZ window framework (`GZWin.dll`), drawn like the originals. Positions are
//! relative to the owning window; `draw` takes that window's screen origin.

use crate::surface::{Rect, Sprite, Surface};
use crate::text::{draw_text, draw_text_clipped, TextColors};
use sc3k_formats::fbf::Font;
use std::rc::Rc;

/// `cGZWinFlatRect::tOutlineFlag`.
pub mod outline {
    pub const LEFT: u8 = 1;
    pub const TOP: u8 = 2;
    pub const RIGHT: u8 = 4;
    pub const BOTTOM: u8 = 8;
    pub const ALL: u8 = LEFT | TOP | RIGHT | BOTTOM;
}

/// `cGZWinFlatRect`: a filled rectangle with optional one-pixel edges.
pub struct FlatRect {
    pub area: Rect,
    pub fill: u32,
    pub outline: u8,
    /// Left, top, right, bottom.
    pub outline_colors: [u32; 4],
}

impl FlatRect {
    pub fn new(area: Rect, fill: u32, outline: u8, outline_color: u32) -> FlatRect {
        FlatRect { area, fill, outline, outline_colors: [outline_color; 4] }
    }

    pub fn draw(&self, s: &mut Surface, ox: i32, oy: i32) {
        let a = self.area.offset(ox, oy);
        s.fill_rect(a, self.fill);
        let (r, b) = (a.right() - 1, a.bottom() - 1);
        let [cl, ct, cr, cb] = self.outline_colors;
        if self.outline & outline::LEFT != 0 {
            s.line(a.x, a.y, a.x, b, cl);
        }
        if self.outline & outline::TOP != 0 {
            s.line(a.x, a.y, r, a.y, ct);
        }
        if self.outline & outline::RIGHT != 0 {
            s.line(r, a.y, r, b, cr);
        }
        if self.outline & outline::BOTTOM != 0 {
            s.line(a.x, b, r, b, cb);
        }
    }
}

/// `cGZWinText` as created by `CreateLabel`: one line, auto-sized to the text.
pub struct Label {
    pub area: Rect,
    text: Vec<u8>,
    font: Rc<Font>,
    colors: TextColors,
}

impl Label {
    pub fn new(x: i32, y: i32, text: Vec<u8>, font: Rc<Font>, colors: TextColors) -> Label {
        let area = Rect::new(x, y, font.text_width(&text), font.line_height());
        Label { area, text, font, colors }
    }

    pub fn draw(&self, s: &mut Surface, ox: i32, oy: i32) {
        draw_text(s, &self.font, ox + self.area.x, oy + self.area.y, &self.text, self.colors);
    }
}

/// Frames of a button image laid out side by side in `src` (a whole image, or one row of a
/// sheet such as the system buttons).
#[derive(Clone)]
pub struct Strip {
    sheet: Sprite,
    src: Rect,
    frames: i32,
}

impl Strip {
    pub fn new(sheet: Sprite, frames: i32) -> Strip {
        let src = sheet.area();
        Strip { sheet, src, frames }
    }

    /// Frames along row `row` of `row_h` pixels.
    pub fn row(sheet: Sprite, frames: i32, row: i32, row_h: i32) -> Strip {
        let src = Rect::new(0, row * row_h, sheet.width, row_h);
        Strip { sheet, src, frames }
    }

    pub fn frame_w(&self) -> i32 {
        self.src.w / self.frames
    }

    pub fn frame_h(&self) -> i32 {
        self.src.h
    }

    pub fn frame(&self, i: i32) -> Rect {
        Rect::new(self.src.x + i * self.frame_w(), self.src.y, self.frame_w(), self.src.h)
    }
}

/// Keys the controls react to; everything else is `Other`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    Other,
}

/// `cGZWinBtn` styles (`CreateButton` .. `CreateRadioButton`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Stretches: left third, tiled middle third, right third.
    Button,
    FixedButton,
    /// Stretches like `Button`.
    Toggle,
    FixedToggle,
    /// 8-frame image beside the caption.
    CheckBox,
    Radio,
}

impl Style {
    fn toggles(self) -> bool {
        !matches!(self, Style::Button | Style::FixedButton)
    }

    fn stretches(self) -> bool {
        matches!(self, Style::Button | Style::Toggle)
    }

    /// Image type 3 (8 frames, image left of the caption) rather than 1 (4 frames, caption on
    /// top of the image).
    fn boxed(self) -> bool {
        matches!(self, Style::CheckBox | Style::Radio)
    }
}

/// A button caption with (text, anti-alias) colours for the normal, hover and disabled states.
pub struct Caption {
    text: Vec<u8>,
    font: Rc<Font>,
    colors: [TextColors; 3],
}

/// `cGZWinBtn`.
pub struct Button {
    pub id: u32,
    pub area: Rect,
    style: Style,
    image: Strip,
    caption: Option<Caption>,
    /// Left, top, right, bottom.
    gutters: (i32, i32, i32, i32),
    /// State bit 1: checked / toggled on.
    pub on: bool,
    pub enabled: bool,
    /// 0x1000: the mouse went down on the button and has not come up yet.
    pressed: bool,
    /// 0x2000.
    hover: bool,
}

impl Button {
    pub fn new(id: u32, style: Style, image: Strip) -> Button {
        let area = Rect::new(0, 0, image.frame_w(), image.frame_h());
        Button { id, area, style, image, caption: None, gutters: (0, 0, 0, 0), on: false, enabled: true, pressed: false, hover: false }
    }

    /// `SetCaption` + `SetFont`: every state starts in `colors` (the font's colours).
    pub fn with_caption(mut self, text: Vec<u8>, font: Rc<Font>, colors: TextColors) -> Button {
        self.caption = Some(Caption { text, font, colors: [colors; 3] });
        self
    }

    pub fn set_disabled_colors(&mut self, colors: TextColors) {
        if let Some(c) = &mut self.caption {
            c.colors[2] = colors;
        }
    }

    pub fn set_gutters(&mut self, l: i32, t: i32, r: i32, b: i32) {
        self.gutters = (l, t, r, b);
    }

    pub fn move_to(&mut self, x: i32, y: i32) {
        self.area.x = x;
        self.area.y = y;
    }

    /// `GetMinSize`.
    pub fn min_size(&self) -> (i32, i32) {
        let (fw, fh) = (self.image.frame_w(), self.image.frame_h());
        let Some(c) = &self.caption else { return (fw, fh) };
        let (l, t, r, b) = self.gutters;
        let (tw, th) = (c.font.text_width(&c.text), c.font.line_height());
        if self.style.boxed() {
            (fw + l + r + tw, fh.max(t + b + th))
        } else {
            ((tw + l + r).max(fw), fh)
        }
    }

    /// `AutoSize`: resize to the minimum size, keeping the top-left corner.
    pub fn auto_size(&mut self) {
        (self.area.w, self.area.h) = self.min_size();
    }

    /// Frame of the image for the current state (`cGZWinBtn::GZPaint`).
    fn frame(&self) -> i32 {
        if self.style.boxed() {
            let off = !self.on as i32;
            if !self.enabled {
                6 + off
            } else if self.pressed {
                4 + off
            } else if self.hover {
                2 + off
            } else {
                off
            }
        } else if !self.enabled {
            0
        } else if self.pressed || (self.style.toggles() && self.on) {
            2
        } else if self.hover {
            3
        } else {
            1
        }
    }

    /// Pointer at (x, y) in the parent's coordinates. While pressed the button keeps its state.
    pub fn mouse_move(&mut self, x: i32, y: i32) {
        if !self.pressed {
            self.hover = self.enabled && self.area.contains(x, y);
        }
    }

    /// The pointer left the parent window.
    pub fn mouse_left(&mut self) {
        if !self.pressed {
            self.hover = false;
        }
    }

    /// Returns true when the button took the press.
    pub fn mouse_down(&mut self, x: i32, y: i32) -> bool {
        if self.enabled && self.area.contains(x, y) {
            self.pressed = true;
            self.hover = true;
            return true;
        }
        false
    }

    /// Returns true on a click: released over the button it was pressed on. Toggling styles
    /// flip their on state first.
    pub fn mouse_up(&mut self, x: i32, y: i32) -> bool {
        if !std::mem::take(&mut self.pressed) {
            return false;
        }
        let inside = self.area.contains(x, y);
        self.hover = inside;
        if inside && self.style.toggles() {
            self.on = !self.on;
        }
        inside
    }

    pub fn draw(&self, s: &mut Surface, ox: i32, oy: i32) {
        let a = self.area.offset(ox, oy);
        let src = self.image.frame(self.frame());
        let sheet = &self.image.sheet;
        if self.style.boxed() {
            s.blit(sheet, src, a.x, a.y + (a.h - src.h) / 2);
        } else if self.style.stretches() && a.w != src.w {
            let third = src.w / 3;
            let left = Rect::new(src.x, src.y, third, src.h);
            let mid = Rect::new(src.x + third, src.y, third, src.h);
            let right = Rect::new(src.x + src.w - third, src.y, third, src.h);
            s.blit(sheet, left, a.x, a.y);
            let fill = Rect::new(a.x + third, a.y, a.w - 2 * third, src.h);
            s.blit_tiled(sheet, mid, fill, fill);
            s.blit(sheet, right, a.right() - third, a.y);
        } else {
            s.blit(sheet, src, a.x, a.y);
        }

        let Some(c) = &self.caption else { return };
        let colors = c.colors[if !self.enabled { 2 } else if self.hover { 1 } else { 0 }];
        let (l, t, r, b) = self.gutters;
        let (x, y) = if self.style.boxed() {
            (a.x + l + src.w, a.y + t)
        } else {
            let (tw, th) = (c.font.text_width(&c.text), c.font.line_height());
            // Flag 0x80 (on by default): the caption sinks one pixel while pressed.
            let sink = self.pressed as i32;
            (a.x + l + ((a.w - r - l) - tw) / 2 + sink, a.y + t + ((a.h - b - t) - th) / 2 + sink)
        };
        draw_text(s, &c.font, x, y, &c.text, colors);
    }
}

/// `cGZWinOptGrp` as the game builds it: a caption label with a column of radio buttons under
/// it, indented 8 px (FUN_1005d9ae). Exactly one button is on.
pub struct OptionGroup {
    pub area: Rect,
    caption: Label,
    buttons: Vec<Button>,
}

impl OptionGroup {
    /// Lay `buttons` out under `caption`, whose position is the group's top-left corner.
    pub fn new(caption: Label, mut buttons: Vec<Button>) -> OptionGroup {
        let (x, mut y) = (caption.area.x, caption.area.bottom());
        let mut w = caption.area.w;
        for b in &mut buttons {
            b.auto_size();
            b.move_to(x + 8, y);
            y += b.area.h;
            w = w.max(b.area.w + 8);
        }
        let area = Rect::new(x, caption.area.y, w, y - caption.area.y);
        OptionGroup { area, caption, buttons }
    }

    pub fn buttons_mut(&mut self) -> impl Iterator<Item = &mut Button> {
        self.buttons.iter_mut()
    }

    /// Turn on the button `id` and the others off; false if `id` is not in the group.
    pub fn select(&mut self, id: u32) -> bool {
        if !self.buttons.iter().any(|b| b.id == id) {
            return false;
        }
        for b in &mut self.buttons {
            b.on = b.id == id;
        }
        true
    }

    /// ID of the button that is on.
    pub fn selected(&self) -> Option<u32> {
        self.buttons.iter().find(|b| b.on).map(|b| b.id)
    }

    pub fn draw(&self, s: &mut Surface, ox: i32, oy: i32) {
        self.caption.draw(s, ox, oy);
        for b in &self.buttons {
            b.draw(s, ox, oy);
        }
    }
}

/// `cGZWinLineInput` outline colours for outline type 3, a double sunken bevel: outer
/// top/left, outer bottom/right, inner top/left, inner bottom/right.
const INPUT_OUTLINE: [u32; 4] = [0x6C_71B9, 0xB3_B6E0, 0x31_367C, 0x7D_81BD];
/// `DefaultColor(0x11)`.
const INPUT_FILL: u32 = 0xFF_FFFF;

/// `cGZWinLineInput`: a one-line text box. Text is Windows-1252 bytes; typing accepts the
/// Latin-1 printable range. The caret shows while focused (it does not blink).
pub struct LineInput {
    pub area: Rect,
    text: Vec<u8>,
    font: Rc<Font>,
    colors: TextColors,
    /// Left, top, right, bottom.
    gutters: (i32, i32, i32, i32),
    max_len: usize,
    /// Insertion point, as an index into `text`.
    cursor: usize,
    /// First visible character.
    view: usize,
    pub focused: bool,
}

impl LineInput {
    pub fn new(area: Rect, text: Vec<u8>, font: Rc<Font>, colors: TextColors, max_len: usize) -> LineInput {
        let cursor = text.len();
        let mut input =
            LineInput { area, text, font, colors, gutters: (0, 0, 0, 0), max_len, cursor, view: 0, focused: false };
        input.scroll();
        input
    }

    /// `GetMinSize` height: gutters, the two 2 px bevels and one line.
    pub fn min_height(font: &Font, gutters: (i32, i32, i32, i32)) -> i32 {
        gutters.1 + gutters.3 + 4 + font.line_height()
    }

    pub fn set_gutters(&mut self, l: i32, t: i32, r: i32, b: i32) {
        self.gutters = (l, t, r, b);
        self.scroll();
    }

    pub fn text(&self) -> &[u8] {
        &self.text
    }

    pub fn focus(&mut self, on: bool) {
        self.focused = on;
    }

    fn text_x(&self) -> i32 {
        self.area.x + self.gutters.0
    }

    fn visible_w(&self) -> i32 {
        self.area.w - self.gutters.0 - self.gutters.2
    }

    /// Move the view so the cursor stays visible.
    fn scroll(&mut self) {
        self.view = self.view.min(self.cursor);
        while self.view < self.cursor && self.font.text_width(&self.text[self.view..self.cursor]) > self.visible_w() {
            self.view += 1;
        }
    }

    /// `FindTextIndexFromCursorLocation`: the character boundary nearest to x.
    fn index_at(&self, x: i32) -> usize {
        let mut pen = self.text_x();
        for (i, &c) in self.text.iter().enumerate().skip(self.view) {
            let adv = self.font.glyphs[c as usize].advance;
            if 2 * x < 2 * pen + adv {
                return i;
            }
            pen += adv;
        }
        self.text.len()
    }

    /// A press at (x, y) in the parent's coordinates focuses the box and places the cursor.
    /// Returns true when the press hit the box.
    pub fn mouse_down(&mut self, x: i32, y: i32) -> bool {
        if !self.area.contains(x, y) {
            return false;
        }
        self.focused = true;
        self.cursor = self.index_at(x);
        self.scroll();
        true
    }

    /// `GZOnKeyDown` editing keys. Returns false for keys left to the parent.
    pub fn key(&mut self, key: Key) -> bool {
        if !self.focused {
            return false;
        }
        match key {
            Key::Left => self.cursor = self.cursor.saturating_sub(1),
            Key::Right => self.cursor = (self.cursor + 1).min(self.text.len()),
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = self.text.len(),
            Key::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.text.remove(self.cursor);
                }
            }
            Key::Delete => {
                if self.cursor < self.text.len() {
                    self.text.remove(self.cursor);
                }
            }
            _ => return false,
        }
        self.scroll();
        true
    }

    /// `GZOnCharacter`: insert a printable character at the cursor, up to the maximum length.
    pub fn character(&mut self, c: char) -> bool {
        if !self.focused {
            return false;
        }
        let Ok(b) = u8::try_from(u32::from(c)) else { return false };
        if b < 0x20 || (0x7F..0xA0).contains(&b) || self.text.len() >= self.max_len {
            return false;
        }
        self.text.insert(self.cursor, b);
        self.cursor += 1;
        self.scroll();
        true
    }

    pub fn draw(&self, s: &mut Surface, ox: i32, oy: i32) {
        let a = self.area.offset(ox, oy);
        s.fill_rect(a, INPUT_FILL);
        // paint_outline, type 3, in the original's order (right and bottom exclusive).
        let (l, t, r, b) = (a.x, a.y, a.right(), a.bottom());
        let [outer_tl, outer_br, inner_tl, inner_br] = INPUT_OUTLINE;
        s.line(r - 1, t, r - 1, b - 1, outer_br);
        s.line(l, b - 1, r - 2, b - 1, outer_br);
        s.line(l, t, r - 2, t, outer_tl);
        s.line(l, t, l, b - 2, outer_tl);
        s.line(r - 2, t + 1, r - 2, b - 2, inner_br);
        s.line(l + 1, b - 2, r - 2, b - 2, inner_br);
        s.line(l + 1, t + 1, r - 3, t + 1, inner_tl);
        s.line(l + 1, t + 1, l + 1, b - 3, inner_tl);

        let (gl, gt, _, gb) = self.gutters;
        let x = a.x + gl;
        let y = a.y + gt + ((a.h - gt - gb) - self.font.line_height()) / 2;
        let clip = Rect::new(a.x + 2, a.y + 2, a.w - 4, a.h - 4);
        draw_text_clipped(s, &self.font, x, y, &self.text[self.view..], self.colors, clip);
        if self.focused {
            let caret_w = self.font.text_width(b"|");
            let cx = x + self.font.text_width(&self.text[self.view..self.cursor]) - caret_w / 2 - 1;
            draw_text_clipped(s, &self.font, cx, y, b"|", self.colors, clip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sc3k_formats::fbf::{Glyph, GLYPHS};

    /// A font where every character advances 5 px and lines are 10 px.
    fn font() -> Rc<Font> {
        let glyph = Glyph { left: 0, top: 0, right: 4, bottom: 10, advance: 5 };
        Rc::new(Font { width: 4, height: 10, pixels: vec![0; 40], palette: [0; 256], glyphs: [glyph; GLYPHS] })
    }

    /// 50 px wide with no gutters: 10 characters fit.
    fn input(text: &[u8]) -> LineInput {
        let mut e = LineInput::new(Rect::new(0, 0, 50, 16), text.to_vec(), font(), TextColors::new(0, 0), 12);
        e.focus(true);
        e
    }

    #[test]
    fn edits_at_the_cursor() {
        let mut e = input(b"abc");
        e.key(Key::Left);
        e.character('X');
        assert_eq!(e.text(), b"abXc");
        e.key(Key::Backspace);
        e.key(Key::Home);
        e.key(Key::Delete);
        assert_eq!(e.text(), b"bc");
        e.key(Key::End);
        e.character('é');
        assert_eq!(e.text(), b"bc\xE9");
    }

    #[test]
    fn rejects_controls_non_latin1_and_overflow() {
        let mut e = input(b"0123456789");
        assert!(!e.character('\u{7}'));
        assert!(!e.character('\u{85}'));
        assert!(!e.character('€'));
        assert!(e.character('a') && e.character('b'));
        assert!(!e.character('c'));
        assert_eq!(e.text(), b"0123456789ab");
    }

    #[test]
    fn ignores_keys_without_focus() {
        let mut e = input(b"abc");
        e.focus(false);
        assert!(!e.character('x'));
        assert!(!e.key(Key::Backspace));
        assert_eq!(e.text(), b"abc");
    }

    #[test]
    fn view_follows_the_cursor() {
        let mut e = input(b"0123456789ab");
        assert_eq!(e.view, 2);
        e.key(Key::Home);
        assert_eq!(e.view, 0);
        // A press places the cursor at the nearest character boundary.
        e.mouse_down(12, 5);
        assert_eq!(e.cursor, 2);
    }
}
