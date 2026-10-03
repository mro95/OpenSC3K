//! Startup flow, independent of the window: copyright splash, then the main menu over the
//! title background (`cWinSC3`'s screen states).

use sc3k_assets::Assets;
use sc3k_ui::main_menu::{Choice, MainMenu};
use sc3k_ui::controls::Key;
use sc3k_ui::new_city::{NewCity, Outcome};
use sc3k_ui::title::{Splash, TitleBackground};
use sc3k_ui::Surface;

/// The original keeps the splash up while it loads; there is nothing to load yet, so it stays
/// for this long or until a click or key press.
const SPLASH_MS: u64 = 3000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    Splash { since_ms: u64 },
    MainMenu,
    /// The New City Options dialog over the main menu.
    NewCity,
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
    Key(Key),
    /// A typed character.
    Text(char),
}

pub struct Game {
    pub screen: Surface,
    splash: Splash,
    title: TitleBackground,
    menu: MainMenu,
    new_city: NewCity,
    scene: Scene,
    quit: bool,
}

impl Game {
    pub fn new(assets: &Assets, width: i32, height: i32) -> sc3k_assets::Result<Game> {
        let mut menu = MainMenu::load(assets)?;
        menu.layout(width, height);
        let mut new_city = NewCity::load(assets)?;
        new_city.layout(width, height);
        Ok(Game {
            screen: Surface::new(width, height),
            splash: Splash::load(assets)?,
            title: TitleBackground::load(assets)?,
            menu,
            new_city,
            scene: Scene::Splash { since_ms: 0 },
            quit: false,
        })
    }

    pub fn set_scene(&mut self, scene: Scene) {
        self.scene = scene;
    }

    pub fn music(&self) -> Music {
        match self.scene {
            Scene::Splash { .. } => Music::Silent,
            Scene::MainMenu | Scene::NewCity => Music::MenuLoop,
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
                Event::MouseUp(..) | Event::Key(_) | Event::Text(_) => {}
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
                };
                match outcome {
                    Some(Outcome::Start(settings)) => {
                        let name = |t: &[u8]| t.iter().map(|&b| b as char).collect::<String>();
                        eprintln!(
                            "new city: {:?}, mayor {:?}, difficulty {} (funds {}{}), {}, size {}, \
                             disasters {}, auto budget {}, schemes {:08X}/{:08X}/{:08X}",
                            name(&settings.city_name),
                            name(&settings.mayor_name),
                            settings.difficulty,
                            settings.funds,
                            if settings.loan { ", loan" } else { "" },
                            settings.start_year,
                            settings.size,
                            settings.disasters,
                            settings.auto_budget,
                            settings.landscape,
                            settings.flora,
                            settings.buildings,
                        );
                        eprintln!("new city: starting a city is not implemented yet");
                        self.show_menu();
                    }
                    Some(Outcome::Cancel) => self.show_menu(),
                    None => {}
                }
            }
        }
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
            Scene::NewCity => {}
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
        }
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
