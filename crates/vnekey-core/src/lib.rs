//! `vnekey-core`: bộ máy gõ tiếng Việt thuần logic (không phụ thuộc hệ điều hành).
//!
//! Cách dùng: với mỗi phím ký tự người dùng gõ, gọi [`Engine::process`]; với phím
//! kết thúc từ (Space, Enter, dấu câu) gọi [`Engine::boundary`]. Kết quả [`Reaction`]
//! cho biết nên để phím đi qua nguyên vẹn, hay thay bằng "xoá N ký tự rồi gõ chuỗi mới".

mod charset;
mod charset_tables;
mod engine;
mod syllable;

pub use charset::Charset;
pub use engine::{Boundary, Engine, EngineConfig, InputMethod, Reaction};
