//! Tầng Windows của vnekey.
//!
//! Các module thuần logic (`controller`, `icon`, `keys`, `log`, `plan`, `autostart::*_args`)
//! biên dịch và kiểm thử được ở mọi nền tảng; phần gọi Windows API nằm sau `#[cfg(windows)]`.

pub mod autostart;
pub mod controller;
pub mod icon;
pub mod keys;
pub mod log;
pub mod plan;

#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod runtime;
#[cfg(windows)]
mod send;
