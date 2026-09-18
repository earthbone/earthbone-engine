// SPDX-License-Identifier: GPL-3.0-or-later

//! The LIP track, the mouth animation that rides along with a voice line
//! inside a [`crate::fuz`] container.
//!
//! What is confirmed against the shipped files is the outer framing: a
//! version word of 1 in every one of the 74,070 blocks in the English voice
//! archive, and a second word that takes exactly two values there, 13 in
//! 73,118 blocks and 9 in 952. Two values and nothing else looks like a
//! track or curve count rather than a constant, but that is a reading of a
//! histogram and not a decode, so the reader reports the number and draws no
//! conclusion from it. What is **not** confirmed is how the body encodes
//! phonemes and their weights, so this module refuses to invent a track.
//! [`Lip::parse`] gives the framing and hands the body back as bytes, and
//! `docs/audio.md` records the probes that ruled the obvious encodings out.
//!
//! This is the one piece of the audio work that is still open. A caller can
//! already tell whether a line has mouth data, how long the block is and what
//! its header says, which is enough to carry it through the pipeline intact
//! and enough to drive a fallback mouth animation from the audio envelope.

use crate::error::{Error, Result};
use crate::riff::u32_at;

/// The only version the shipped files carry.
pub const VERSION: u32 = 1;

/// The version word plus the five words after it, all of which the files fill
/// in before the curve body starts.
pub const HEADER_WORDS: usize = 6;

/// The header in bytes.
pub const HEADER_LEN: usize = HEADER_WORDS * 4;

/// Word 2 of the header. Its meaning is not established, so the reader
/// records it rather than acting on it.
pub const CONSTANT_WORD_INDEX: usize = 2;

/// The only two values word 2 takes across the English voice archive.
pub const KNOWN_WORD_2: [u32; 2] = [9, 13];

/// A LIP block: its header words, and its body still encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lip<'a> {
    pub version: u32,
    /// The six header words verbatim, index 0 being the version. Only index
    /// 0 and index 2 have known values, see the module docs.
    pub words: [u32; HEADER_WORDS],
    /// Everything after the header, the float curve data, undecoded.
    pub body: &'a [u8],
}

impl<'a> Lip<'a> {
    /// Reads the framing of a LIP block.
    ///
    /// # Errors
    ///
    /// [`Error::Truncated`] when the block is shorter than its header, and
    /// [`Error::Malformed`] when the version word is not 1.
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() < HEADER_LEN {
            return Err(Error::Truncated {
                what: "LIP header",
                needed: HEADER_LEN,
                have: bytes.len(),
            });
        }
        let mut words = [0u32; HEADER_WORDS];
        for (i, w) in words.iter_mut().enumerate() {
            *w = u32_at(bytes, i * 4).unwrap_or_default();
        }
        if words[0] != VERSION {
            return Err(Error::Malformed {
                what: "LIP header",
                why: format!("version word is {}, expected {VERSION}", words[0]),
            });
        }
        Ok(Self {
            version: words[0],
            words,
            body: &bytes[HEADER_LEN..],
        })
    }

    /// Header word 2, so a caller can see whether a file departs from the two
    /// values the shipped archive uses.
    #[must_use]
    pub fn constant_word(&self) -> u32 {
        self.words[CONSTANT_WORD_INDEX]
    }

    /// True when word 2 is one of the two values vanilla writes. A block that
    /// fails this still parses, it is just outside what has been measured.
    #[must_use]
    pub fn matches_vanilla_shape(&self) -> bool {
        self.version == VERSION && KNOWN_WORD_2.contains(&self.constant_word())
    }
}

#[cfg(test)]
mod tests {
    use super::{Lip, HEADER_LEN, KNOWN_WORD_2, VERSION};
    use crate::error::Error;

    fn block(words: [u32; 6], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for w in words {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn the_header_words_and_body_come_back_whole() {
        let bytes = block(
            [VERSION, 22468, 13, 196663, 0xffff_fff5, 196624],
            &[7, 7, 7],
        );
        let lip = Lip::parse(&bytes).unwrap();
        assert_eq!(lip.version, VERSION);
        assert_eq!(lip.constant_word(), 13);
        assert_eq!(lip.words[1], 22468);
        assert_eq!(lip.body, &[7, 7, 7]);
        assert_eq!(bytes.len(), HEADER_LEN + 3);
        assert!(lip.matches_vanilla_shape());
    }

    #[test]
    fn a_block_that_departs_from_the_vanilla_shape_is_flagged_not_refused() {
        let bytes = block([VERSION, 1, 99, 0, 0, 0], &[]);
        let lip = Lip::parse(&bytes).unwrap();
        assert!(!lip.matches_vanilla_shape());
        assert_eq!(lip.constant_word(), 99);
    }

    #[test]
    fn both_values_the_archive_writes_count_as_the_vanilla_shape() {
        for word in KNOWN_WORD_2 {
            let bytes = block([VERSION, 1, word, 0, 0, 0], &[]);
            assert!(Lip::parse(&bytes).unwrap().matches_vanilla_shape());
        }
    }

    #[test]
    fn a_bad_version_is_refused_with_the_number() {
        let bytes = block([4, 0, 13, 0, 0, 0], &[]);
        match Lip::parse(&bytes) {
            Err(Error::Malformed { why, .. }) => assert!(why.contains('4'), "{why}"),
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn a_short_block_is_truncation() {
        assert!(matches!(
            Lip::parse(&[1, 0, 0, 0]),
            Err(Error::Truncated { .. })
        ));
    }
}
