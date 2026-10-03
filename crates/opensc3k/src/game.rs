//! Startup flow, independent of the window: copyright splash, then the main menu over the
//! title background (`cWinSC3`'s screen states).

use sc3k_assets::Assets;
use sc3k_sim::city::City;
use sc3k_render::palette::{load_land_palettes, ColorTable, DirtPalettes};
use sc3k_render::background::Background;
use sc3k_render::camera::{Camera, Scroll};
use sc3k_render::terrain::{TerrainScene, MAX_ZOOM};
use sc3k_sim::dirt;
use std::collections::HashMap;
use sc3k_ui::main_menu::{Choice, MainMenu};
use sc3k_ui::controls::Key;
use sc3k_ui::new_city::{NewCity, Outcome, Settings};
use sc3k_ui::title::{Splash, TitleBackground};
use sc3k_ui::Surface;

/// The original keeps the splash up while it loads; there is nothing to load yet, so it stays
/// for this long or until a click or key press.
const SPLASH_MS: u64 = 3000;
/// The original scrolls one step per paint of the city view (`cSC3WinCityView::GZPaint`
/// calls `paintScroll`); its frame rate is not fixed, so this assumes 30 paints a second.
const SCROLL_TICK_MS: u64 = 33;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    Splash { since_ms: u64 },
    MainMenu,
    /// The New City Options dialog over the main menu.
    NewCity,
    /// A city: its terrain in the isometric view. Arrow keys and the screen edges scroll,
    /// PageUp/PageDown, `+`/`-` and the mouse wheel zoom, `,`/`.` rotate.
    City,
}

/// What the music should be doing, polled every frame like `cBoxX::UpdateMusic`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Music {
    Silent,
    /// `Music\3kloop.wav` looped in the all-purpose slot. The original plays it in every
    /// non-city audio mode (1, 2 and 4), so it keeps going through the menu's dialogs.
    MenuLoop,
}

pub enum Event {
    MouseMove(i32, i32),
    MouseLeft,
    MouseDown(i32, i32),
    MouseUp(i32, i32),
    /// Mouse wheel notches, positive away from the user.
    Wheel(i32),
    Key(Key),
    KeyUp(Key),
    /// A typed character.
    Text(char),
}

pub struct Game {
    pub screen: Surface,
    splash: Splash,
    title: TitleBackground,
    menu: MainMenu,
    new_city: NewCity,
    /// Terrain seed for new cities; `None` seeds from the clock like the original.
    seed: Option<u32>,
    city: Option<City>,
    /// Landscape palettes by `LandScapes` scheme key.
    land_palettes: HashMap<u32, ColorTable>,
    /// Water and map-edge palettes.
    dirt_palettes: DirtPalettes,
    /// The texture behind the map, per zoom.
    background: Background,
    /// The city view: the terrain and the camera on it.
    view: Option<(TerrainScene, Camera)>,
    /// Zoom and rotation of new city views.
    zoom: u32,
    rotation: u32,
    /// Arrow keys held, and the screen edge the pointer is at.
    scroll_keys: Scroll,
    scroll_edge: Scroll,
    next_scroll_ms: u64,
    scene: Scene,
    quit: bool,
}

impl Game {
    pub fn new(assets: &Assets, width: i32, height: i32) -> Result<Game, String> {
        let e = |e: sc3k_assets::Error| e.to_string();
        let mut menu = MainMenu::load(assets).map_err(e)?;
        menu.layout(width, height);
        let mut new_city = NewCity::load(assets).map_err(e)?;
        new_city.layout(width, height);
        Ok(Game {
            screen: Surface::new(width, height),
            splash: Splash::load(assets).map_err(e)?,
            title: TitleBackground::load(assets).map_err(e)?,
            menu,
            new_city,
            seed: None,
            city: None,
            land_palettes: load_land_palettes(assets)?,
            dirt_palettes: DirtPalettes::load(assets)?,
            background: Background::load(assets)?,
            view: None,
            zoom: MAX_ZOOM,
            rotation: 0,
            scroll_keys: Scroll::default(),
            scroll_edge: Scroll::default(),
            next_scroll_ms: 0,
            scene: Scene::Splash { since_ms: 0 },
            quit: false,
        })
    }

    /// Without a city, `Scene::City` starts one from the dialog's current settings.
    pub fn set_scene(&mut self, scene: Scene) {
        if scene == Scene::City && self.city.is_none() {
            self.start_city(&self.new_city.settings());
        }
        self.scene = scene;
    }

    pub fn set_seed(&mut self, seed: Option<u32>) {
        self.seed = seed;
    }

    /// Zoom for city views started from now on; clamped to 0..=MAX_ZOOM.
    pub fn set_zoom(&mut self, zoom: u32) {
        self.zoom = zoom.min(MAX_ZOOM);
    }

    /// Rotation for city views started from now on, 0..=3.
    pub fn set_rotation(&mut self, rotation: u32) {
        self.rotation = rotation & 3;
    }

    pub fn music(&self) -> Music {
        match self.scene {
            Scene::Splash { .. } => Music::Silent,
            Scene::MainMenu | Scene::NewCity => Music::MenuLoop,
            // The original switches to the city tracks; not done yet.
            Scene::City => Music::Silent,
        }
    }

    pub fn wants_quit(&self) -> bool {
        self.quit
    }

    pub fn handle(&mut self, event: Event) {
        match self.scene {
            Scene::Splash { .. } => {
                if matches!(event, Event::MouseDown(..) | Event::Key(_)) {
                    self.show_menu();
                }
            }
            Scene::MainMenu => match event {
                Event::MouseMove(x, y) => self.menu.mouse_move(x, y),
                Event::MouseLeft => self.menu.mouse_left(),
                Event::MouseDown(x, y) => {
                    if let Some(c) = self.menu.mouse_down(x, y) {
                        self.choose(c);
                    }
                }
                _ => {}
            },
            Scene::NewCity => {
                let outcome = match event {
                    Event::MouseMove(x, y) => {
                        self.new_city.mouse_move(x, y);
                        None
                    }
                    Event::MouseLeft => {
                        self.new_city.mouse_left();
                        None
                    }
                    Event::MouseDown(x, y) => {
                        self.new_city.mouse_down(x, y);
                        None
                    }
                    Event::MouseUp(x, y) => self.new_city.mouse_up(x, y),
                    Event::Key(k) => self.new_city.key(k),
                    Event::Text(c) => {
                        self.new_city.character(c);
                        None
                    }
                    Event::Wheel(_) | Event::KeyUp(_) => None,
                };
                match outcome {
                    Some(Outcome::Start(settings)) => self.start_city(&settings),
                    Some(Outcome::Cancel) => self.show_menu(),
                    None => {}
                }
            }
            Scene::City => self.city_event(event),
        }
    }

    fn city_event(&mut self, event: Event) {
        let (w, h) = (self.screen.width, self.screen.height);
        let Some((scene, camera)) = &mut self.view else {
            if matches!(event, Event::Key(Key::Escape)) {
                self.leave_city();
            }
            return;
        };
        let t = scene.terrain();
        let keys = &mut self.scroll_keys;
        match event {
            Event::Key(Key::Escape) => self.leave_city(),
            Event::Key(Key::Up) => keys.up = true,
            Event::Key(Key::Down) => keys.down = true,
            Event::Key(Key::Right) => keys.right = true,
            Event::Key(Key::Left) => keys.left = true,
            Event::KeyUp(Key::Up) => keys.up = false,
            Event::KeyUp(Key::Down) => keys.down = false,
            Event::KeyUp(Key::Right) => keys.right = false,
            Event::KeyUp(Key::Left) => keys.left = false,
            Event::Key(Key::PageUp) | Event::Text('+' | '=') => camera.zoom_in(t),
            Event::Key(Key::PageDown) | Event::Text('-' | '_') => camera.zoom_out(t),
            Event::Wheel(n) if n > 0 => camera.zoom_in(t),
            Event::Wheel(n) if n < 0 => camera.zoom_out(t),
            Event::Text(',' | '<') => camera.rotate_ccw(t),
            Event::Text('.' | '>') => camera.rotate_cw(t),
            Event::MouseMove(x, y) => self.scroll_edge = Scroll::from_edge(x, y, w, h),
            Event::MouseLeft => self.scroll_edge = Scroll::default(),
            _ => {}
        }
    }

    fn leave_city(&mut self) {
        self.city = None;
        self.view = None;
        self.scroll_keys = Scroll::default();
        self.scroll_edge = Scroll::default();
        self.show_menu();
    }

    pub fn update(&mut self, now_ms: u64) {
        match self.scene {
            Scene::Splash { since_ms } => {
                if now_ms.saturating_sub(since_ms) >= SPLASH_MS {
                    self.show_menu();
                }
            }
            Scene::MainMenu => {
                if let Some(c) = self.menu.tick(now_ms) {
                    self.choose(c);
                }
            }
            Scene::City => self.scroll(now_ms),
            Scene::NewCity => {}
        }
    }

    /// One scroll step per tick while an arrow key is held or the pointer is at an edge.
    fn scroll(&mut self, now_ms: u64) {
        let Some((scene, camera)) = &mut self.view else { return };
        let Some((dx, dy)) = self.scroll_keys.union(self.scroll_edge).step() else {
            self.next_scroll_ms = now_ms;
            return;
        };
        if now_ms >= self.next_scroll_ms {
            camera.translate(scene.terrain(), dx, dy);
            self.next_scroll_ms = now_ms + SCROLL_TICK_MS;
        }
    }

    pub fn draw(&mut self) {
        match self.scene {
            Scene::Splash { .. } => self.splash.draw(&mut self.screen),
            Scene::MainMenu => {
                self.title.draw(&mut self.screen);
                self.menu.draw(&mut self.screen);
            }
            Scene::NewCity => {
                self.title.draw(&mut self.screen);
                self.menu.draw(&mut self.screen);
                self.new_city.draw(&mut self.screen);
            }
            Scene::City => {
                match &self.view {
                    Some((scene, camera)) => {
                        self.background.draw(&mut self.screen, &camera.view);
                        scene.draw(&mut self.screen, &camera.view);
                    }
                    None => self.screen.fill(0),
                }
            }
        }
    }

    /// OK in the New City dialog. Like `cSC3WinProcCityScheme::ReadValuesFromWindow`, the
    /// terrain is generated first, with the dialog's fixed generator arguments; then the city
    /// is built from the info (`cSC3CmdNewCity::Execute`, `cSC3City::Init`).
    fn start_city(&mut self, settings: &Settings) {
        let seed = self.seed.unwrap_or_else(clock_seed);
        let mut info = settings.new_city_info();
        info.terrain = Some(dirt::generate(info.x_size, info.difficulty, dirt::Params::new_city(seed)));
        let city = City::new(&info);
        let name = |t: &[u8]| t.iter().map(|&b| b as char).collect::<String>();
        eprintln!(
            "new city: {:?}, mayor {:?}, difficulty {}, funds {} (borrowed {}), \
             {}-{:02}-{:02}, {}x{} cells, disasters {}, auto budget {}, \
             schemes {:08X}/{:08X}/{:08X}, terrain seed {seed}, sea level {}",
            name(&city.name),
            name(&city.mayor),
            city.difficulty,
            city.funds,
            city.total_borrowed(),
            city.date.year,
            city.date.month,
            city.date.day,
            city.x_size,
            city.y_size,
            city.disasters,
            city.auto_budget,
            settings.landscape,
            settings.flora,
            settings.buildings,
            city.terrain.as_ref().map_or(0, |t| t.sea_level),
        );
        // The landscape scheme picks the land palette (`SetAltitudePalette`).
        let palette = self
            .land_palettes
            .get(&settings.landscape)
            .or_else(|| self.land_palettes.get(&0))
            .or_else(|| self.land_palettes.values().next());
        self.view = match (&city.terrain, palette) {
            (Some(t), Some(p)) => {
                let camera = Camera::new(t, self.zoom, self.rotation, self.screen.width, self.screen.height);
                Some((TerrainScene::new(t.clone(), p, &self.dirt_palettes, seed), camera))
            }
            _ => None,
        };
        self.city = Some(city);
        self.scene = Scene::City;
    }

    fn show_menu(&mut self) {
        self.menu.reset();
        self.scene = Scene::MainMenu;
    }

    fn choose(&mut self, choice: Choice) {
        match choice {
            // The original asks for confirmation first (cWinSC3::ActivateMainMenu).
            Choice::Exit => self.quit = true,
            Choice::StartNewCity => self.scene = Scene::NewCity,
            other => eprintln!("main menu: {other:?} is not implemented yet"),
        }
    }

    pub fn menu_mut(&mut self) -> &mut MainMenu {
        &mut self.menu
    }
}

/// The original seeds the generator with `0xFFFFFFFF`, which `cRZRandom` replaces with
/// `timeGetTime`. Milliseconds of the wall clock stand in for it.
fn clock_seed() -> u32 {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    now.as_millis() as u32
}
