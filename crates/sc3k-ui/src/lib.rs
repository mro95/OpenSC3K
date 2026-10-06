//! The game's 2D interface, drawn in software like the original GZ window system.

pub mod controls;
pub mod main_menu;
pub mod main_ui;
pub mod menu;
pub mod new_city;
pub mod surface;
pub mod text;
pub mod title;

pub use surface::{Rect, Sprite, Surface};
