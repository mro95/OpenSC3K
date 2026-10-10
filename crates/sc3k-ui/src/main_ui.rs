//! The city view's main interface (`cSC3MainUIMgr`, SIMUI.DLL): the menu panel on the right,
//! the navigator in the bottom-right corner, the RCI meter and the date/cash/title bar along
//! the bottom. See `docs/ui/main-ui.md`.
//!
//! Only the frame is drawn so far: each window's background art, the main menu buttons'
//! hover and open images, and the open submenu's panel image.

use crate::menu::{Menus, ROOT};
use crate::surface::{Rect, Sprite, Surface};
use sc3k_assets::Assets;

/// Art groups in `Res/UI/Shared/<group>_MainUI.ixf`, one per screen layout.
pub const GROUP_640: u32 = 0x0064_0480;
pub const GROUP_800: u32 = 0x02F7_8C6D;
pub const GROUP_800_BAR: u32 = 0x0080_0600;
pub const GROUP_1024: u32 = 0x0102_4768;

/// Every main UI image keys out magenta (`SetColorKey(0xFF, 0, 0xFF)`).
const MAGENTA_565: u16 = 0xF81F;

/// Window IDs (`cSC3MainUIMgr::Init`).
pub const ID_PANEL: u32 = 0x42FB_7DEC;
pub const ID_NAV: u32 = 0x42FB_7DED;
pub const ID_RCI: u32 = 0x42FB_7DEA;
pub const ID_BAR: u32 = 0x42FB_7DEB;

/// `cSC3WinNav::SetArea` (libSimUI Ghidra 0x16B9C0) fixes the navigator's size.
/// Unchecked: no Windows address known yet.
pub const NAV_SIZE: (i32, i32) = (0xA0, 0xA4);

/// The windows' areas after `cSC3MainUIMgr::place_windows`
/// (libSimUI Ghidra 0xC78F4, SIMUI.DLL 0x100148D1). `None` where the window does not exist.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub panel: Option<Rect>,
    pub nav: Option<Rect>,
    pub rci: Option<Rect>,
    pub bar: Option<Rect>,
}

/// The windows' sizes before placement: what each one's `Init` gave itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sizes {
    pub panel: Option<(i32, i32)>,
    pub nav: Option<(i32, i32)>,
    pub rci: Option<(i32, i32)>,
    pub bar: Option<(i32, i32)>,
}

/// `place_windows`. The navigator goes in the bottom-right corner, the RCI meter to its left,
/// the bar from the left edge to the meter, and the menu panel down the right edge to 6 px
/// below the navigator's top. The panel's own `SetArea` then sets its final size
/// ([`PanelArt::area`]); this returns the area it is given.
pub fn place_windows(screen: Rect, sizes: &Sizes) -> Layout {
    let (right, bottom) = (screen.right(), screen.bottom());
    let height = screen.h;
    let mut out = Layout::default();
    // Without a navigator the others line up on the screen's corner.
    let (mut nav_x, mut nav_y) = (right, bottom);
    if let Some((w, h)) = sizes.nav {
        // At 640x480 the navigator's lower 12 rows hang off the screen.
        let y = if height == 480 { bottom - 0x98 } else { bottom - h };
        let r = Rect::new(right - w, y, w, h);
        (nav_x, nav_y) = (r.x, r.y);
        out.nav = Some(r);
    }
    let mut rci_x = right;
    if let Some((w, h)) = sizes.rci {
        let y = if height == 600 { bottom - 0x50 } else { bottom - h };
        let r = Rect::new(nav_x - w, y, w, h);
        rci_x = r.x;
        out.rci = Some(r);
    }
    if let Some((_, h)) = sizes.bar {
        out.bar = Some(Rect::new(screen.x, bottom - h, rci_x - screen.x, h));
    }
    if let Some((w, _)) = sizes.panel {
        out.panel = Some(Rect::new(right - w, screen.y, w, nav_y + 6 - screen.y));
    }
    out
}

/// Which of the panel's pieces exist, from the screen height
/// (`cSC3WinMenuPanelMain::Init`, libSimUI Ghidra 0x164124). Each is (group, instance) of an
/// image.
/// Unchecked: no Windows address known yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelPieces {
    pub top: Option<(u32, u32)>,
    pub main: (u32, u32),
    pub bottom: Option<(u32, u32)>,
    /// Tiled between the main piece and the bottom piece.
    pub filler: Option<(u32, u32)>,
}

impl PanelPieces {
    /// `None` for heights the original has no art for (below 601, except 480).
    pub fn for_height(h: i32) -> Option<PanelPieces> {
        match h {
            480 => Some(PanelPieces { top: None, main: (GROUP_640, 0xFFFF), bottom: None, filler: None }),
            600 => Some(PanelPieces { top: None, main: (GROUP_800, 0xFFFF), bottom: Some((GROUP_800, 0xFFFE)), filler: None }),
            h if h > 600 => Some(PanelPieces {
                top: Some((GROUP_1024, 0xFFFD)),
                main: (GROUP_800, 0xFFFF),
                bottom: Some((GROUP_1024, 0xFFFE)),
                filler: (h > 768).then_some((GROUP_1024, 0xFFFC)),
            }),
            _ => None,
        }
    }
}

/// Where the panel's pieces go, relative to the panel
/// (`cSC3WinMenuPanelMain::SetArea`, libSimUI Ghidra 0x163E78).
/// Unchecked: no Windows address known yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PanelArea {
    /// The panel's final area.
    pub area: Rect,
    pub top: Option<Rect>,
    pub main: Rect,
    pub bottom: Option<Rect>,
    /// The span the filler is tiled over.
    pub filler: Option<Rect>,
}

/// `SetArea` with piece sizes (w, h): the panel is as wide as the main
/// piece and at least as tall as the pieces together. The top piece sits at the top, the main
/// piece under it, the bottom piece at the bottom and the filler between the last two.
pub fn panel_area(given: Rect, top: Option<(i32, i32)>, main: (i32, i32), bottom: Option<(i32, i32)>, filler: Option<(i32, i32)>) -> PanelArea {
    let th = top.map_or(0, |t| t.1);
    let bh = bottom.map_or(0, |b| b.1);
    let w = main.0.max(1);
    let h = given.h.max(main.1 + th + bh).max(1);
    let area = Rect::new(given.x, given.y, w, h);
    let main_r = Rect::new(0, th, main.0, main.1);
    let bottom_r = bottom.map(|(bw, bh)| Rect::new(0, h - bh, bw, bh));
    let filler_r = filler.map(|(fw, _)| {
        let from = main_r.bottom();
        let to = bottom_r.map_or(h, |b| b.y);
        Rect::new(0, from, fw, to - from)
    });
    PanelArea { area, top: top.map(|(tw, th)| Rect::new(0, 0, tw, th)), main: main_r, bottom: bottom_r, filler: filler_r }
}

/// `cSC3WinMenuBtnMain::tMenuBtnInfoMain`, from `get_menu_btn_info_main`
/// (libSimUI Ghidra 0x165724, SIMUI.DLL 0x1004C3E9). Positions are relative to the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MainButtonInfo {
    /// The button's area: 56x32.
    pub area: Rect,
    /// Drawn while hovered (instance in `GROUP_800`).
    pub hover: u32,
    /// Drawn while hovered with its submenu open.
    pub hover_open: Option<u32>,
    /// The panel image shown while the submenu is open: (group, instance) and its offset.
    pub submenu: Option<((u32, u32), (i32, i32))>,
}

/// The nine main buttons' IDs, `0x1001..=0x1009`, in the root menu's order.
pub fn main_button_info(id: u32, screen_h: i32) -> Option<MainButtonInfo> {
    let n = id.checked_sub(0x1001).filter(|&n| n < 9)?;
    if screen_h != 480 && screen_h != 600 && screen_h < 0x259 {
        return None;
    }
    // (y at 480, y at 600, y above 600) and the submenu's y offset likewise.
    const Y: [[i32; 3]; 9] = [
        [3, 3, 100],
        [0x27, 0x27, 0x88],
        [0x4B, 0x4B, 0xAC],
        [0x6F, 0x6F, 0xD0],
        [0x93, 0x93, 0xF4],
        [0xDB, 0x123, 0x184],
        [0xFF, 0x147, 0x1A8],
        [0x123, 0x16B, 0x1CC],
        [0xB7, 0xDB, 0x13C],
    ];
    const SUB_Y: [[i32; 3]; 9] = [
        [0, 0, 0x61],
        [0xB, 0xB, 0x6C],
        [0xB, 0xB, 0x6C],
        [0xB, 0xB, 0x6C],
        [0xB, 0xB, 0x6C],
        [0, 0, 0x61],
        [0x2D, 0x76, 0xD7],
        [0, 0x76, 0xD7],
        [0x53, 0x77, 0xD8],
    ];
    let col = match screen_h {
        480 => 0,
        600 => 1,
        _ => 2,
    };
    let base = 0x1001_0000 + n * 0x1_0000;
    let area = Rect::new(0x21, Y[n as usize][col], 0x38, 0x20);
    // The Meet button (0x1006) opens no submenu and has only a hover image.
    if id == 0x1006 {
        return Some(MainButtonInfo { area, hover: base + 1, hover_open: None, submenu: None });
    }
    let group = if screen_h == 480 { GROUP_640 } else { GROUP_800 };
    Some(MainButtonInfo {
        area,
        hover: base + 1,
        hover_open: Some(base + 5),
        submenu: Some(((group, base + 3), (0, SUB_Y[n as usize][col]))),
    })
}

/// `cSC3WinDateCashTitle::get_layout_info` (libSimUI Ghidra 0xF5190, SIMUI.DLL 0x100270E5):
/// the bar's art group by screen width.
pub fn bar_group(screen_w: i32) -> Option<u32> {
    match screen_w {
        640 => Some(GROUP_640),
        800 => Some(GROUP_800_BAR),
        w if w > 800 => Some(GROUP_1024),
        _ => None,
    }
}

/// The bar's background, and the piece tiled leftwards from its right edge where the screen
/// is wider than the background (`cSC3WinDateCashTitle::Init` and `GZPaint`).
const BAR_IMAGE: u32 = 0x8279_0741;
const BAR_FILLER: (u32, u32) = (GROUP_800, 0x8279_0742);
/// The navigator's three pieces (`cSC3WinNav::update_background_buffers`): top left, top
/// right beside it, and the bottom part under the first.
const NAV_PIECES: [u32; 3] = [0x22F9_6E32, 0x22F9_6E33, 0x22F9_6E34];
/// The RCI meter's background (`cSC3WinRCI::Init`).
const RCI_IMAGE: (u32, u32) = (GROUP_800, 0x42FB_77F6);

struct MainButton {
    id: u32,
    info: MainButtonInfo,
    hover: Sprite,
    hover_open: Option<Sprite>,
    submenu: Option<(Sprite, (i32, i32))>,
}

struct Panel {
    origin: (i32, i32),
    area: PanelArea,
    top: Option<Sprite>,
    main: Sprite,
    bottom: Option<Sprite>,
    filler: Option<Sprite>,
    buttons: Vec<MainButton>,
}

pub struct MainUi {
    pub menus: Menus,
    pub layout: Layout,
    panel: Option<Panel>,
    nav: Option<[Sprite; 3]>,
    rci: Option<Sprite>,
    bar: Option<(Sprite, Sprite)>,
    /// Index into the panel's buttons.
    hover: Option<usize>,
    open: Option<usize>,
}

/// What a click on the interface did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// The click was on the interface; the city view must not see it.
    Handled,
    /// A menu item was chosen: its item ID (`[SC3MenuItemInfo]`).
    Item(u32),
}

fn sprite(assets: &Assets, (group, instance): (u32, u32)) -> sc3k_assets::Result<Sprite> {
    Ok(Sprite::new(&assets.image(group, instance)?, Some(MAGENTA_565)))
}

impl MainUi {
    /// Load the art for a `w` x `h` screen and lay the windows out. A window whose art the
    /// original lacks for this size is left out, as its `Init` fails there.
    pub fn load(assets: &Assets, w: i32, h: i32) -> sc3k_assets::Result<MainUi> {
        let menus = Menus::parse(&assets.sys_ini("MenuItem.INI")?)
            .map_err(|e| sc3k_assets::Error::Missing { what: e.to_string() })?;
        let nav = [sprite(assets, (GROUP_800, NAV_PIECES[0]))?, sprite(assets, (GROUP_800, NAV_PIECES[1]))?, sprite(assets, (GROUP_800, NAV_PIECES[2]))?];
        let rci = sprite(assets, RCI_IMAGE)?;
        let bar = match bar_group(w) {
            Some(g) => Some((sprite(assets, (g, BAR_IMAGE))?, sprite(assets, BAR_FILLER)?)),
            None => None,
        };
        let pieces = PanelPieces::for_height(h);
        let panel = match pieces {
            Some(p) => {
                let opt = |k: Option<(u32, u32)>| k.map(|k| sprite(assets, k)).transpose();
                let mut buttons = Vec::new();
                for &id in menus.buttons_of(ROOT) {
                    let Some(info) = main_button_info(id, h) else { continue };
                    let submenu = match info.submenu {
                        Some((key, offset)) => Some((sprite(assets, key)?, offset)),
                        None => None,
                    };
                    buttons.push(MainButton {
                        id,
                        info,
                        hover: sprite(assets, (GROUP_800, info.hover))?,
                        hover_open: opt(info.hover_open.map(|i| (GROUP_800, i)))?,
                        submenu,
                    });
                }
                Some(Panel {
                    origin: (0, 0),
                    area: PanelArea::default(),
                    top: opt(p.top)?,
                    main: sprite(assets, p.main)?,
                    bottom: opt(p.bottom)?,
                    filler: opt(p.filler)?,
                    buttons,
                })
            }
            None => None,
        };
        let mut ui = MainUi { menus, layout: Layout::default(), panel, nav: Some(nav), rci: Some(rci), bar, hover: None, open: None };
        ui.layout(w, h);
        Ok(ui)
    }

    fn sizes(&self) -> Sizes {
        let size = |s: &Sprite| (s.width, s.height);
        Sizes {
            panel: self.panel.as_ref().map(|p| size(&p.main)),
            nav: self.nav.as_ref().map(|_| NAV_SIZE),
            rci: self.rci.as_ref().map(size),
            bar: self.bar.as_ref().map(|(b, _)| size(b)),
        }
    }

    pub fn layout(&mut self, w: i32, h: i32) {
        let layout = place_windows(Rect::new(0, 0, w, h), &self.sizes());
        if let (Some(p), Some(given)) = (&mut self.panel, layout.panel) {
            let size = |s: &Sprite| (s.width, s.height);
            p.area = panel_area(given, p.top.as_ref().map(size), size(&p.main), p.bottom.as_ref().map(size), p.filler.as_ref().map(size));
            p.origin = (p.area.area.x, p.area.area.y);
        }
        self.layout = layout;
    }

    /// The button under (x, y), as an index into the panel's buttons.
    fn button_at(&self, x: i32, y: i32) -> Option<usize> {
        let p = self.panel.as_ref()?;
        let (ox, oy) = p.origin;
        p.buttons.iter().position(|b| b.info.area.offset(ox, oy).contains(x, y))
    }

    /// True if (x, y) is on one of the interface's windows.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let l = &self.layout;
        let panel = self.panel.as_ref().map(|p| p.area.area);
        [panel, l.nav, l.rci, l.bar].iter().flatten().any(|r| r.contains(x, y))
    }

    /// Close the open submenu and forget the pointer, for a new city view.
    pub fn reset(&mut self) {
        self.hover = None;
        self.open = None;
    }

    pub fn mouse_move(&mut self, x: i32, y: i32) {
        self.hover = self.button_at(x, y);
    }

    pub fn mouse_left(&mut self) {
        self.hover = None;
    }

    /// `cSC3WinMenuBtnMain::GZOnMouseDownL`: a button with a submenu opens it, closing any
    /// other, or closes it if it was open. A button without one closes the open submenu.
    pub fn mouse_down(&mut self, x: i32, y: i32) -> Option<Action> {
        let Some(i) = self.button_at(x, y) else {
            return self.contains(x, y).then_some(Action::Handled);
        };
        let has_submenu = self.panel.as_ref().is_some_and(|p| p.buttons[i].submenu.is_some());
        self.open = if has_submenu && self.open != Some(i) { Some(i) } else { None };
        Some(Action::Handled)
    }

    /// `cSC3WinMenuBtnMain::GZOnMouseUpL`: releasing over a button runs its item, if it has
    /// one (only Meet does among the main buttons).
    pub fn mouse_up(&mut self, x: i32, y: i32) -> Option<Action> {
        let Some(i) = self.button_at(x, y) else {
            return self.contains(x, y).then_some(Action::Handled);
        };
        let id = self.panel.as_ref()?.buttons[i].id;
        let item = self.menus.buttons.get(&id).map(|b| b.item).filter(|i| self.menus.items.contains_key(i));
        Some(item.map_or(Action::Handled, Action::Item))
    }

    /// The open submenu's button ID, if any.
    pub fn open_menu(&self) -> Option<u32> {
        Some(self.panel.as_ref()?.buttons[self.open?].id)
    }

    /// The windows in the order `cSC3MainUIMgr::Init` creates them, later ones on top: the
    /// bar, the RCI meter, the panel, the navigator. The navigator's top 6 rows cover the
    /// panel's bottom.
    pub fn draw(&self, s: &mut Surface) {
        if let (Some((bg, filler)), Some(r)) = (&self.bar, self.layout.bar) {
            // `GZPaint`: the filler from the right edge leftwards while it is right of the
            // background, then the background at the left.
            let mut x = r.right() - filler.width;
            while x + filler.width > r.x + bg.width {
                s.blit_clip(filler, filler.area(), x, r.y, r);
                x -= filler.width;
            }
            s.blit_clip(bg, bg.area(), r.x, r.y, r);
        }
        if let (Some(img), Some(r)) = (&self.rci, self.layout.rci) {
            s.blit_clip(img, img.area(), r.x, r.y, r);
        }
        if let Some(p) = &self.panel {
            self.draw_panel(s, p);
        }
        if let (Some([a, b, c]), Some(r)) = (&self.nav, self.layout.nav) {
            s.blit_clip(a, a.area(), r.x, r.y, r);
            s.blit_clip(b, b.area(), r.x + a.width, r.y, r);
            s.blit_clip(c, c.area(), r.x, r.y + a.height, r);
        }
    }

    /// `cSC3WinMenuPanelMain::GZPaint` (libSimUI Ghidra 0x1647CC): top, main, filler, bottom,
    /// then the open submenu's image; then each button's `GZPaint`.
    /// Unchecked: no Windows address known yet.
    fn draw_panel(&self, s: &mut Surface, p: &Panel) {
        let (ox, oy) = p.origin;
        let clip = p.area.area;
        if let (Some(img), Some(r)) = (&p.top, p.area.top) {
            s.blit_clip(img, img.area(), ox + r.x, oy + r.y, clip);
        }
        s.blit_clip(&p.main, p.main.area(), ox + p.area.main.x, oy + p.area.main.y, clip);
        if let (Some(img), Some(r)) = (&p.filler, p.area.filler) {
            s.blit_tiled(img, img.area(), r.offset(ox, oy), clip);
        }
        if let (Some(img), Some(r)) = (&p.bottom, p.area.bottom) {
            s.blit_clip(img, img.area(), ox + r.x, oy + r.y, clip);
        }
        if let Some((img, (dx, dy))) = self.open.and_then(|i| p.buttons[i].submenu.as_ref()) {
            s.blit_clip(img, img.area(), ox + dx, oy + dy, clip);
        }
        // `cSC3WinMenuBtnMain::GZPaint`: only a hovered button draws, with its open image if
        // its submenu is open.
        if let Some(i) = self.hover {
            let b = &p.buttons[i];
            let img = if self.open == Some(i) { b.hover_open.as_ref() } else { Some(&b.hover) };
            if let Some(img) = img {
                let r = b.info.area.offset(ox, oy);
                s.blit_clip(img, img.area(), r.x, r.y, r);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes(w: i32, h: i32) -> Sizes {
        let (panel, bar) = match (w, h) {
            (640, 480) => ((96, 334), (440, 64)),
            (800, 600) => ((96, 417), (600, 56)),
            _ => ((96, 417), (824, 64)),
        };
        Sizes { panel: Some(panel), nav: Some(NAV_SIZE), rci: Some((41, 88)), bar: Some(bar) }
    }

    #[test]
    fn places_windows_at_800x600() {
        let l = place_windows(Rect::new(0, 0, 800, 600), &sizes(800, 600));
        assert_eq!(l.nav, Some(Rect::new(640, 436, 160, 164)));
        assert_eq!(l.rci, Some(Rect::new(599, 520, 41, 88)));
        assert_eq!(l.bar, Some(Rect::new(0, 544, 599, 56)));
        assert_eq!(l.panel, Some(Rect::new(704, 0, 96, 442)));
        // The pieces fill the panel exactly.
        let a = panel_area(l.panel.unwrap(), None, (96, 417), Some((96, 25)), None);
        assert_eq!(a.area.h, 442);
        assert_eq!(a.bottom, Some(Rect::new(0, 417, 96, 25)));
    }

    #[test]
    fn places_windows_at_640x480_and_1024x768() {
        let l = place_windows(Rect::new(0, 0, 640, 480), &sizes(640, 480));
        assert_eq!(l.nav, Some(Rect::new(480, 328, 160, 164)));
        assert_eq!(l.panel, Some(Rect::new(544, 0, 96, 334)));
        let l = place_windows(Rect::new(0, 0, 1024, 768), &sizes(1024, 768));
        assert_eq!(l.panel, Some(Rect::new(928, 0, 96, 610)));
        let a = panel_area(l.panel.unwrap(), Some((96, 97)), (96, 417), Some((96, 96)), None);
        assert_eq!((a.main.y, a.bottom.unwrap().y), (97, 514));
    }

    #[test]
    fn filler_spans_main_to_bottom() {
        let given = Rect::new(1184, 0, 96, 866);
        let a = panel_area(given, Some((96, 97)), (96, 417), Some((96, 96)), Some((96, 32)));
        assert_eq!(a.filler, Some(Rect::new(0, 514, 96, 866 - 96 - 514)));
    }

    #[test]
    fn button_table() {
        let b = main_button_info(0x1001, 600).unwrap();
        assert_eq!(b.area, Rect::new(0x21, 3, 0x38, 0x20));
        assert_eq!(b.hover, 0x1001_0001);
        assert_eq!(b.submenu, Some(((GROUP_800, 0x1001_0003), (0, 0))));
        assert_eq!(main_button_info(0x1006, 480).unwrap().submenu, None);
        assert_eq!(main_button_info(0x1009, 768).unwrap().area.y, 0x13C);
        assert!(main_button_info(0x1001, 500).is_none());
        assert!(main_button_info(0x100A, 600).is_none());
    }
}
