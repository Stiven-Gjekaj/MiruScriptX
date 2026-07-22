//! MiruScriptX: a small, general-purpose scripting language written in Rust.
//!
//! This crate exposes the language as a library. The `miru` binary in
//! `src/main.rs` wraps it with a command line interface and a REPL.
//!
//! Learn the language in the `wiki/` folder and look things up in
//! `docs/language-reference.md`.

/// The MiruScriptX version, taken from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
