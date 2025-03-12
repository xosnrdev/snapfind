//! `SnapFind` - Fast file search tool that understands content.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(
    warnings,
    missing_debug_implementations,
    missing_docs,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]

#[cfg(not(feature = "std"))]
extern crate core as std;
#[cfg(feature = "std")]
extern crate std;

pub mod allocator;
pub mod crawler;
pub mod error;
pub mod search;
pub mod text;
pub mod types;
