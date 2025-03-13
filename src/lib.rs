//! `SnapFind` - Fast file search tool that understands content.

#![deny(
    warnings,
    missing_debug_implementations,
    missing_docs,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]

pub mod allocator;
pub mod crawler;
pub mod error;
pub mod search;
pub mod text;
pub mod types;
