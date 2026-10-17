//! Tầng Windows của vnekey.
//!
//! Các module thuần logic (`icon`, `keys`, `plan`, `autostart::*_args`) biên dịch và
//! kiểm thử được ở mọi nền tảng; phần gọi Windows API nằm sau `#[cfg(windows)]`.

pub mod autostart;
pub mod icon;
pub mod keys;
pub mod plan;

#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod runtime;
#[cfg(windows)]
mod send;
