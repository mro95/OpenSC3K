//! Parsers for SimCity 3000 Unlimited data files.

pub mod bmp;
pub mod csattrib;
pub mod fbf;
pub mod image;
pub mod ini;
pub mod ixf;
pub mod occupant;
pub mod pak;
pub mod qfs;
pub mod sprite;
pub mod text;
pub mod wav;

use std::path::PathBuf;

/// Environment variable pointing at the original game's install root.
pub const DATA_ENV: &str = "SC3K_DATA";

/// The original install root, if `SC3K_DATA` is set.
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os(DATA_ENV).map(PathBuf::from)
}
