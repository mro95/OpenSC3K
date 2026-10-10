//! Full-screen title images.
//!
//! - The copyright splash (`cWinSC3::DoCopyRightScreen`, `SC3U.exe+0x3e4a3`): image
//!   `22721000` in a `cSC3WinLogo` centred on screen, with the copyright line in the
//!   bottom-right corner.
//! - The title background behind the main menu (`cWinSC3::DoTitleBackgroundScreen`,
//!   `SC3U.exe+0x3e05b`): image `22729921`, centred; cropped when the screen is smaller,
//!   bordered when it is larger.
//!
//! Localised builds can override both through the `SPLASHBMP` / `WINSC3BMP` settings; the
//! defaults below are what the English Unlimited release uses.
//! Unchecked: no check runs SC3U.exe yet.

use crate::main_menu::MAIN_GROUP;
use crate::surface::{Sprite, Surface};
use crate::text::{draw_text, TextColors};
use sc3k_assets::Assets;
use sc3k_formats::fbf::Font;

const SPLASH_IMAGE: u32 = 0x2272_1000;
const TITLE_BACKGROUND: u32 = 0x2272_9921;
/// "© 2000 Electronic Arts Inc. All Rights Reserved." (`SEStringsUI.IXF`). The splash has a
/// second, bottom-left label (`63DE4715/0x18`), but no shipped string table defines it.
const COPYRIGHT: (u32, u32) = (0x041F_2625, 0x197);
/// `cGZWinCtrlMgr::SystemFont(0xC)`, registered from `title9.fbf`.
const SPLASH_FONT: &str = "title9.fbf";
/// `cSC3WinLogo::create_special_label`.
const COPYRIGHT_TEXT: TextColors = TextColors::new(0xD47A45, 0x202020);
const COPYRIGHT_SHADOW: TextColors = TextColors::new(0x202020, 0x363640);

pub struct TitleBackground {
    image: Sprite,
}

impl TitleBackground {
    pub fn load(assets: &Assets) -> sc3k_assets::Result<TitleBackground> {
        Ok(TitleBackground { image: Sprite::new(&assets.image(MAIN_GROUP, TITLE_BACKGROUND)?, None) })
    }

    pub fn draw(&self, s: &mut Surface) {
        s.fill(0);
        s.blit_centered(&self.image);
    }
}

pub struct Splash {
    image: Sprite,
    font: Font,
    copyright: Vec<u8>,
}

impl Splash {
    pub fn load(assets: &Assets) -> sc3k_assets::Result<Splash> {
        Ok(Splash {
            image: Sprite::new(&assets.image(MAIN_GROUP, SPLASH_IMAGE)?, None),
            font: assets.font(SPLASH_FONT)?,
            copyright: assets.string(COPYRIGHT.0, COPYRIGHT.1)?,
        })
    }

    pub fn draw(&self, s: &mut Surface) {
        s.fill(0);
        s.blit_centered(&self.image);
        // cSC3WinLogo::layout: right-aligned 2 px from the right edge, 4 px above the bottom.
        let left = (s.width - self.image.width) / 2;
        let top = (s.height - self.image.height) / 2;
        let w = self.font.text_width(&self.copyright);
        let h = self.font.line_height();
        let x = left + self.image.width - w - 2;
        let y = top + self.image.height - (h + 4);
        draw_text(s, &self.font, x + 1, y + 1, &self.copyright, COPYRIGHT_SHADOW);
        draw_text(s, &self.font, x, y, &self.copyright, COPYRIGHT_TEXT);
    }
}
