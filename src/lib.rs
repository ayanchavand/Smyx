//! myx — a lean, beautiful terminal Navidrome / OpenSubsonic player.
//!
//! FE: the design-token system (noodle's visual language) ported to ratatui,
//! plus album-art-reactive theming with cross-fades.
//! Backend (`streaming` feature): rodio audio streaming + FFT visualizer.

use std::path::PathBuf;

pub mod anim;
pub mod color;
pub mod components;
pub mod config;
pub mod cover;
pub mod gradient;
pub mod login_modal;
pub mod reactive;
pub mod subsonic;
pub mod theme;

#[cfg(feature = "streaming")]
pub mod audio;
#[cfg(feature = "streaming")]
pub mod engine;

/// Cross-platform home directory. Uses `HOME` on Unix, `USERPROFILE` on Windows.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    let var = "HOME";
    #[cfg(windows)]
    let var = "USERPROFILE";
    std::env::var(var).ok().map(PathBuf::from)
}
