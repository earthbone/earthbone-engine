// SPDX-License-Identifier: GPL-3.0-or-later

//! The LIP track, the mouth animation that rides along with a voice line
//! inside a [`crate::fuz`] container.
//!
//! LIP is FaceFX, a closed facial animation package, and the Creation Kit
//! writes it from a wav plus the line's text. The header is no longer a
//! guess: `FaceFXWrapper` declares it as three 32 bit fields, version, size
//! and flags, with a static assertion pinning the struct to 12 bytes, and
//! every one of the 74,070 blocks in the shipped English voice archive agrees
//! (`../research/lip-format.md`). An earlier reading here used a 24 byte
//! header, which put the body slice twelve bytes late.
//!
//! The flags are named rather than counted. Vanilla writes two values, 13 and
//! 9, which differ in [`HAS_GESTURES`] alone, and the 952 blocks without it
//! are creature voices. `size` is larger than the block in every shipped file,
//! median ratio 3.62, so it measures the animation expanded rather than the
//! bytes on disk.
//!
//! The body is still handed back undecoded, but it is not opaque and, despite
//! the [`COMPRESSED`] flag, it is not byte compressed: little endian floats
//! sit at byte boundaries in ascending runs that plateau at 1.0 and fall away,
//! which is an animation curve. The reader refuses to invent a track from
//! that until a decode is confirmed, so a caller gets the framing, the flags
//! and the bytes.

use crate::error::{Error, Result};
use crate::riff::u32_at;

/// The only version the shipped files carry.
pub const VERSION: u32 = 1;

/// Version, size and flags, three 32 bit fields.
pub const HEADER_LEN: usize = 12;

/// The body is stored in FaceFX's own packed form. It does not mean a
/// general purpose compressor was applied.
pub const COMPRESSED: u32 = 1;

/// Byte order of the body. No shipped file sets it.
pub const BIG_ENDIAN: u32 = 2;

/// The block carries gesture curves as well as mouth targets. Vanilla creature
/// voices are the files that lack it.
pub const HAS_GESTURES: u32 = 4;

/// The target count is stored rather than assumed.
pub const VARIABLE_TARGETS: u32 = 8;

/// Mouth targets Skyrim animates when a block carries gestures, against 43 for
/// Fallout 4. A gesture free block carries [`TARGETS_NO_GESTURES`].
pub const TARGETS: u16 = 16;

/// Mouth targets a creature voice animates. The eight a creature loses are the
/// gesture targets.
pub const TARGETS_NO_GESTURES: u16 = 8;

/// How far into a block [`Lip::targets`] looks for the marker. The widest
/// preamble measured puts it at 19.
const MARKER_SEARCH_END: usize = 32;

/// The two flag combinations the English voice archive writes.
pub const KNOWN_FLAGS: [u32; 2] = [
    COMPRESSED | VARIABLE_TARGETS,
    COMPRESSED | HAS_GESTURES | VARIABLE_TARGETS,
];

/// A LIP block: its header, and its body still encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lip<'a> {
    /// Always 1 in shipped files.
    pub version: u32,
    /// The animation's size expanded, not the length of `body`.
    pub size: u32,
    /// The flag word, read through the accessors below.
    pub flags: u32,
    /// Everything after the header, the curve data, undecoded.
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
        let version = u32_at(bytes, 0).unwrap_or_default();
        if version != VERSION {
            return Err(Error::Malformed {
                what: "LIP header",
                why: format!("version word is {version}, expected {VERSION}"),
            });
        }
        Ok(Self {
            version,
            size: u32_at(bytes, 4).unwrap_or_default(),
            flags: u32_at(bytes, 8).unwrap_or_default(),
            body: &bytes[HEADER_LEN..],
        })
    }

    /// True when the body is in FaceFX's packed form, which every shipped
    /// file is.
    #[must_use]
    pub fn compressed(&self) -> bool {
        self.flags & COMPRESSED != 0
    }

    /// True when the body is big endian. No shipped file sets this.
    #[must_use]
    pub fn big_endian(&self) -> bool {
        self.flags & BIG_ENDIAN != 0
    }

    /// True when gesture curves follow the mouth targets. Vanilla creature
    /// voices are the files where this is false.
    #[must_use]
    pub fn has_gestures(&self) -> bool {
        self.flags & HAS_GESTURES != 0
    }

    /// True when the target count is stored in the body rather than assumed.
    #[must_use]
    pub fn variable_targets(&self) -> bool {
        self.flags & VARIABLE_TARGETS != 0
    }

    /// True when the flags are one of the two combinations vanilla writes. A
    /// block that fails this still parses, it is just outside what has been
    /// measured.
    #[must_use]
    pub fn matches_vanilla_shape(&self) -> bool {
        self.version == VERSION && KNOWN_FLAGS.contains(&self.flags)
    }

    /// How many targets the curve data drives, 16 with gestures and 8 without.
    ///
    /// The preamble between the header and this field is not a fixed width, so
    /// the read anchors on a marker instead: the first `i32` in the range -32
    /// to -1, which sits at offset 16 in 65,815 shipped blocks, at 17 in 4,955
    /// and at 19 in 102. The count is the `u16` four bytes past it, and it
    /// agreed with [`Lip::has_gestures`] in all 70,872 blocks where the marker
    /// was found.
    ///
    /// `None` when no marker appears, which is 3,198 of the 74,070 shipped
    /// blocks. Those are not covered by this reading and nothing here guesses
    /// at them. See `../research/lip-format.md`.
    #[must_use]
    pub fn targets(&self) -> Option<u16> {
        // The header is three words, so the body is what the search walks.
        let end = MARKER_SEARCH_END.saturating_sub(HEADER_LEN);
        let limit = end.min(self.body.len());
        for at in 0..limit {
            let word = u32_at(self.body, at)? as i32;
            if (-32..0).contains(&word) {
                let lo = at + 4;
                let bytes = self.body.get(lo..lo + 2)?;
                return Some(u16::from_le_bytes([bytes[0], bytes[1]]));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Lip, COMPRESSED, HAS_GESTURES, HEADER_LEN, KNOWN_FLAGS, TARGETS, TARGETS_NO_GESTURES,
        VARIABLE_TARGETS, VERSION,
    };
    use crate::error::Error;

    fn block(version: u32, size: u32, flags: u32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for w in [version, size, flags] {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn the_header_fields_and_body_come_back_whole() {
        let bytes = block(VERSION, 22468, 13, &[7, 7, 7]);
        let lip = Lip::parse(&bytes).unwrap();
        assert_eq!(lip.version, VERSION);
        assert_eq!(lip.size, 22468);
        assert_eq!(lip.flags, 13);
        assert_eq!(lip.body, &[7, 7, 7]);
        assert_eq!(bytes.len(), HEADER_LEN + 3);
        assert!(lip.matches_vanilla_shape());
    }

    #[test]
    fn the_body_starts_at_twelve_bytes_not_twenty_four() {
        let body: Vec<u8> = (0u8..32).collect();
        let bytes = block(VERSION, 1, 13, &body);
        let lip = Lip::parse(&bytes).unwrap();
        assert_eq!(lip.body[0], 0, "body must start right after three words");
        assert_eq!(lip.body.len(), body.len());
    }

    #[test]
    fn the_two_vanilla_flag_words_decode_to_their_names() {
        let gestures = block(VERSION, 1, 13, &[]);
        let with = Lip::parse(&gestures).unwrap();
        assert!(with.compressed() && with.has_gestures() && with.variable_targets());
        assert!(!with.big_endian());

        let creature = block(VERSION, 1, 9, &[]);
        let without = Lip::parse(&creature).unwrap();
        assert!(without.compressed() && without.variable_targets());
        assert!(
            !without.has_gestures(),
            "9 is the creature voice case, gestures absent"
        );
    }

    #[test]
    fn the_known_flag_words_are_the_two_the_archive_writes() {
        assert_eq!(KNOWN_FLAGS, [9, 13]);
        assert_eq!(COMPRESSED | VARIABLE_TARGETS, 9);
        assert_eq!(COMPRESSED | HAS_GESTURES | VARIABLE_TARGETS, 13);
    }

    fn with_marker(flags: u32, marker_at: usize, targets: u16) -> Vec<u8> {
        let mut body = vec![0u8; marker_at];
        body.extend_from_slice(&(-3i32).to_le_bytes());
        body.extend_from_slice(&targets.to_le_bytes());
        body.extend_from_slice(&[0xaa; 16]);
        block(VERSION, 1, flags, &body)
    }

    #[test]
    fn the_target_count_is_read_four_bytes_past_the_marker() {
        // Absolute offset 16 is the common case, which is body offset 4.
        let bytes = with_marker(13, 4, TARGETS);
        assert_eq!(Lip::parse(&bytes).unwrap().targets(), Some(TARGETS));
    }

    #[test]
    fn a_creature_block_reports_eight_targets() {
        let bytes = with_marker(9, 4, TARGETS_NO_GESTURES);
        let lip = Lip::parse(&bytes).unwrap();
        assert!(!lip.has_gestures());
        assert_eq!(lip.targets(), Some(TARGETS_NO_GESTURES));
    }

    #[test]
    fn the_marker_is_found_at_every_width_the_archive_uses() {
        // Absolute 16, 17 and 19, so body offsets 4, 5 and 7.
        for at in [4, 5, 7] {
            let bytes = with_marker(13, at, TARGETS);
            assert_eq!(
                Lip::parse(&bytes).unwrap().targets(),
                Some(TARGETS),
                "marker at body offset {at}"
            );
        }
    }

    #[test]
    fn a_block_with_no_marker_reports_nothing_rather_than_guessing() {
        let bytes = block(VERSION, 1, 13, &[0x11; 40]);
        assert_eq!(Lip::parse(&bytes).unwrap().targets(), None);
    }

    #[test]
    fn a_block_that_departs_from_the_vanilla_shape_is_flagged_not_refused() {
        let bytes = block(VERSION, 1, 99, &[]);
        let lip = Lip::parse(&bytes).unwrap();
        assert!(!lip.matches_vanilla_shape());
        assert_eq!(lip.flags, 99);
    }

    #[test]
    fn a_bad_version_is_refused_with_the_number() {
        let bytes = block(4, 0, 13, &[]);
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
