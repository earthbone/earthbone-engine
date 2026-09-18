// SPDX-License-Identifier: GPL-3.0-or-later

//! Skyrim's voice and sound formats, read without a proprietary tool in the
//! loop, and the open codecs a mod author may ship instead.
//!
//! The engine has to read what vanilla ships, so this crate splits [`fuz`]
//! containers, frames [`lip`] mouth tracks and reads the [`xwma`] container.
//! It also decodes Ogg Vorbis and Ogg Opus into the same [`Pcm`] buffer, so a
//! mod can carry an open format and need no encoder anybody has to buy. That
//! split follows the engine's design rule: read every vanilla format a mod
//! can only express that way, and accept the open format wherever an author
//! has a choice.
//!
//! Almost nothing here is a decoder of our own. Vorbis is `lewton`, Opus is
//! `opus-decoder`, WAV is `hound`, and the WMA bitstream is ffmpeg behind the
//! [`wma::WmaDecoder`] trait and an off-by-default feature. What is written
//! here is the container work nobody has packaged: the FUZ split, the LIP
//! framing and the xWMA reader, each credited in `docs/audio.md` to whatever
//! open implementation it follows.
//!
//! Nothing here plays audio. Decoding to [`Pcm`] is where this crate stops,
//! and playback belongs to an existing device crate in the host.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod error;
pub mod fuz;
pub mod lip;
pub mod pcm;
pub mod riff;
pub mod source;
pub mod wma;
pub mod xwma;

#[cfg(feature = "opus")]
pub mod opus;
#[cfg(feature = "vorbis")]
pub mod vorbis;

pub use error::{Error, Result, Unsupported};
pub use fuz::{AudioKind, Fuz};
pub use lip::Lip;
pub use pcm::Pcm;
pub use source::{sniff, Container, Sound, SoundSource};
pub use wma::WmaDecoder;
pub use xwma::Xwma;
