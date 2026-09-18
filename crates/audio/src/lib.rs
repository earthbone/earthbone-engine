// SPDX-License-Identifier: GPL-3.0-or-later

//! Skyrim's voice and sound formats, read without a proprietary tool in the
//! loop, and the open codecs a mod author may ship instead.
//!
//! The engine has to read what vanilla ships, so this crate splits [`fuz`]
//! containers, parses [`lip`] mouth tracks and reads the [`xwma`] container.
//! It also decodes Ogg Vorbis and Ogg Opus into the same [`Pcm`] buffer, so a
//! mod can carry an open format and need no encoder anybody has to buy. That
//! split follows the engine's design rule: read every vanilla format a mod
//! can only express that way, and accept the open format wherever an author
//! has a choice.
//!
//! Nothing here plays audio. Decoding to [`Pcm`] is where this crate stops,
//! the host decides what to do with the samples.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod error;
pub mod fuz;

pub use error::{Error, Result, Unsupported};
pub use fuz::Fuz;
