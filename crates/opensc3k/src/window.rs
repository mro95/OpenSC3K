//! The desktop window: presents the game's fixed-resolution screen scaled to fit (nearest
//! neighbour, letterboxed) and maps input back to screen coordinates.

use crate::game::{Event, Game, Music};
use sc3k_ui::controls::Key;
use sc3k_audio::{Audio, MENU_LOOP_ID};
use sc3k_formats::wav::Wav;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WKey, NamedKey};
use winit::window::{Window, WindowId};

const FRAME: Duration = Duration::from_millis(16);

struct Gfx {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
}

/// Where the scaled screen lands in the window.
#[derive(Clone, Copy, Default)]
struct View {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl View {
    fn fit(src_w: i32, src_h: i32, win_w: i32, win_h: i32) -> View {
        let scale = (win_w as f64 / src_w as f64).min(win_h as f64 / src_h as f64);
        let (w, h) = ((src_w as f64 * scale) as i32, (src_h as f64 * scale) as i32);
        View { x: (win_w - w) / 2, y: (win_h - h) / 2, w, h }
    }
}

/// Sound output and the sounds it needs. `None` parts mean silence (muted, no device, or the
/// file failed to load).
pub struct Sound {
    pub audio: Option<Audio>,
    pub menu_loop: Option<Wav>,
    /// 0..=1024, the original's music volume scale.
    pub music_volume: u32,
}

impl Sound {
    fn sync(&mut self, music: Music) {
        let Some(audio) = &mut self.audio else { return };
        match (music, &self.menu_loop) {
            (Music::MenuLoop, Some(wav)) => audio.play_all_purpose(MENU_LOOP_ID, wav, true, self.music_volume),
            _ => {
                if audio.all_purpose_id() == Some(MENU_LOOP_ID) {
                    audio.stop_all_purpose();
                }
            }
        }
    }
}

struct App {
    game: Game,
    sound: Sound,
    start: Instant,
    gfx: Option<Gfx>,
    view: View,
    /// Last pointer position in screen coordinates (winit reports it only on CursorMoved).
    cursor: Option<(i32, i32)>,
    error: Option<String>,
}

impl App {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    fn to_screen(&self, px: f64, py: f64) -> (i32, i32) {
        let v = self.view;
        if v.w == 0 || v.h == 0 {
            return (-1, -1);
        }
        let x = ((px - v.x as f64) * self.game.screen.width as f64 / v.w as f64).floor() as i32;
        let y = ((py - v.y as f64) * self.game.screen.height as f64 / v.h as f64).floor() as i32;
        (x, y)
    }

    fn redraw(&mut self) -> Result<(), String> {
        let now = self.now_ms();
        self.game.update(now);
        self.sound.sync(self.game.music());
        self.game.draw();
        let Some(gfx) = &mut self.gfx else { return Ok(()) };
        let size = gfx.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return Ok(());
        };
        gfx.surface.resize(w, h).map_err(|e| e.to_string())?;
        let (ww, wh) = (size.width as i32, size.height as i32);
        let screen = &self.game.screen;
        self.view = View::fit(screen.width, screen.height, ww, wh);
        let v = self.view;
        let mut buf = gfx.surface.buffer_mut().map_err(|e| e.to_string())?;
        buf.fill(0);
        let xmap: Vec<usize> = (0..v.w).map(|x| (x as i64 * screen.width as i64 / v.w as i64) as usize).collect();
        for y in 0..v.h {
            let sy = (y as i64 * screen.height as i64 / v.h as i64) as usize;
            let src = &screen.pixels[sy * screen.width as usize..(sy + 1) * screen.width as usize];
            let row = ((v.y + y) * ww + v.x) as usize;
            for (d, &sx) in buf[row..row + v.w as usize].iter_mut().zip(&xmap) {
                *d = src[sx];
            }
        }
        buf.present().map_err(|e| e.to_string())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let (w, h) = (self.game.screen.width as u32, self.game.screen.height as u32);
        let attrs = Window::default_attributes()
            .with_title("OpenSC3K")
            .with_inner_size(LogicalSize::new(w, h))
            .with_min_inner_size(LogicalSize::new(w / 2, h / 2));
        let result = el.create_window(attrs).map_err(|e| e.to_string()).and_then(|window| {
            let window = Rc::new(window);
            let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
            let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
            Ok(Gfx { window, surface })
        });
        match result {
            Ok(gfx) => self.gfx = Some(gfx),
            Err(e) => {
                self.error = Some(e);
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = self.to_screen(position.x, position.y);
                self.cursor = Some((x, y));
                self.game.handle(Event::MouseMove(x, y));
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.game.handle(Event::MouseLeft);
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                if let Some((x, y)) = self.cursor {
                    self.game.handle(match state {
                        ElementState::Pressed => Event::MouseDown(x, y),
                        ElementState::Released => Event::MouseUp(x, y),
                    });
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.game.handle(Event::Key(map_key(&event.logical_key)));
                for c in event.text.iter().flat_map(|t| t.chars()).filter(|c| !c.is_control()) {
                    self.game.handle(Event::Text(c));
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.redraw() {
                    self.error = Some(e);
                    el.exit();
                }
            }
            _ => {}
        }
        if self.game.wants_quit() {
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(gfx) = &self.gfx {
            gfx.window.request_redraw();
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + FRAME));
    }
}

fn map_key(key: &WKey) -> Key {
    match key {
        WKey::Named(NamedKey::ArrowLeft) => Key::Left,
        WKey::Named(NamedKey::ArrowRight) => Key::Right,
        WKey::Named(NamedKey::Home) => Key::Home,
        WKey::Named(NamedKey::End) => Key::End,
        WKey::Named(NamedKey::Backspace) => Key::Backspace,
        WKey::Named(NamedKey::Delete) => Key::Delete,
        WKey::Named(NamedKey::Enter) => Key::Enter,
        WKey::Named(NamedKey::Escape) => Key::Escape,
        WKey::Named(NamedKey::Tab) => Key::Tab,
        _ => Key::Other,
    }
}

pub fn run(game: Game, sound: Sound) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App { game, sound, start: Instant::now(), gfx: None, view: View::default(), cursor: None, error: None };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.error.map_or(Ok(()), Err)
}
