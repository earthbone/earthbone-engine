// SPDX-License-Identifier: GPL-3.0-or-later

//! Skyrim LIP framing and native frame-indexed weight tracks.
//!
//! The 12-byte header follows `FaceFXWrapper/FFXW32/LipSynchAnim.h`.
//! The body layout comes from the installed voice archive survey documented
//! in `docs/audio.md`: zero-run compression, three metadata words, then
//! frame-major float weights. Every one of 74,070 blocks expands to the
//! declared size minus 16 and matches its frame/channel dimensions.
//!
//! Frame indices and target order stay as stored. The decoder does not
//! assign seconds, morph names or Rhubarb shapes to those samples.

use crate::error::{Error, Result, Unsupported};
use crate::riff::u32_at;

/// The only version the shipped files carry.
pub const VERSION: u32 = 1;

/// Version, size and flags, three 32 bit fields.
pub const HEADER_LEN: usize = 12;

/// Zero bytes in the body are stored as a zero marker followed by a
/// little-endian 16-bit run length. Nonzero bytes are literals.
pub const COMPRESSED: u32 = 1;

/// Byte order of the body. No shipped file sets it.
pub const BIG_ENDIAN: u32 = 2;

/// The block carries gesture curves as well as mouth targets. Vanilla creature
/// voices are the files that lack it.
pub const HAS_GESTURES: u32 = 4;

/// The target count is stored rather than assumed.
pub const VARIABLE_TARGETS: u32 = 8;

/// The mouth target count observed in humanoid Skyrim tracks.
pub const TARGETS: u16 = 16;

/// The mouth target count observed in creature Skyrim tracks.
pub const TARGETS_NO_GESTURES: u16 = 8;

/// Additional channels per frame when [`HAS_GESTURES`] is set. Their stored
/// order is preserved without assigning names to them.
pub const GESTURE_CHANNELS: usize = 17;

/// Maximum expanded body accepted by [`Lip::decode`], including metadata.
/// Limits allocations from untrusted counts and compressed zero runs.
pub const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;

const BODY_HEADER_LEN: usize = 12;
const SIZE_BIAS: usize = 16;

/// The two flag combinations the English voice archive writes.
pub const KNOWN_FLAGS: [u32; 2] = [
    COMPRESSED | VARIABLE_TARGETS,
    COMPRESSED | HAS_GESTURES | VARIABLE_TARGETS,
];

/// A borrowed LIP header and encoded body. Call [`Lip::decode`] for weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lip<'a> {
    /// Always 1 in shipped files.
    pub version: u32,
    /// Expanded body length plus 16, as observed in Skyrim version 1 tracks.
    pub size: u32,
    /// The flag word, read through the accessors below.
    pub flags: u32,
    /// Everything after the header, including encoded frame metadata.
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

    /// The stored mouth target count, read from decoded body metadata.
    ///
    /// Returns `None` for unsupported flags, malformed metadata or a count
    /// that cannot fit the historical 16-bit return type. Use [`Lip::decode`]
    /// to validate the complete body and obtain a typed error.
    #[must_use]
    pub fn targets(&self) -> Option<u16> {
        let (_, metadata) = self.reader().ok()?;
        u16::try_from(metadata.targets).ok()
    }

    /// Decode all stored weights on their native integer frame grid.
    ///
    /// Supports version 1 little-endian bodies with [`VARIABLE_TARGETS`],
    /// with or without zero-run compression and gesture channels. Weights
    /// are finite floats preserved without clamping or renormalization.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for other flags, [`Error::Truncated`]
    /// for missing bytes, or [`Error::Malformed`] for inconsistent sizes,
    /// zero-length runs, non-finite weights or an expansion larger than
    /// [`MAX_DECODED_BYTES`].
    pub fn decode(&self) -> Result<LipWeightTrack> {
        let (mut reader, metadata) = self.reader()?;
        let mut weights = Vec::new();
        weights
            .try_reserve_exact(metadata.samples)
            .map_err(|error| malformed(format!("cannot allocate weight samples: {error}")))?;
        for _ in 0..metadata.samples {
            let weight = f32::from_bits(reader.word()?);
            if !weight.is_finite() {
                return Err(malformed("non-finite weight"));
            }
            weights.push(weight);
        }
        reader.finish()?;
        Ok(LipWeightTrack {
            first_frame: metadata.first_frame,
            targets: metadata.targets,
            gestures: metadata.gestures,
            frame_count: metadata.frame_count,
            weights,
        })
    }

    fn reader(&self) -> Result<(BodyReader<'a>, Metadata)> {
        if self.version != VERSION {
            return Err(malformed(format!("unsupported version {}", self.version)));
        }
        if self.flags & !(COMPRESSED | HAS_GESTURES | VARIABLE_TARGETS) != 0
            || !self.variable_targets()
        {
            return Err(Error::Unsupported(Unsupported::LipFlags(self.flags)));
        }
        let expanded = (self.size as usize)
            .checked_sub(SIZE_BIAS)
            .ok_or_else(|| malformed("declared size is smaller than the body size bias"))?;
        if !(BODY_HEADER_LEN..=MAX_DECODED_BYTES).contains(&expanded) {
            return Err(malformed(format!(
                "expanded body size {expanded} is out of bounds"
            )));
        }
        let mut reader = BodyReader::new(self.body, self.compressed(), expanded);
        let frame_count = reader.word()?;
        let first_frame = reader.word()? as i32;
        let targets = reader.word()? as usize;
        if targets == 0 {
            return Err(malformed("zero mouth targets"));
        }
        let gestures = if self.has_gestures() {
            GESTURE_CHANNELS
        } else {
            0
        };
        let samples = targets
            .checked_add(gestures)
            .and_then(|stride| stride.checked_mul(frame_count as usize))
            .ok_or_else(|| malformed("frame dimensions overflow"))?;
        let expected = samples
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(BODY_HEADER_LEN))
            .ok_or_else(|| malformed("expanded body size overflows"))?;
        if expected != expanded {
            return Err(malformed(format!(
                "frame dimensions require {expected} bytes, declared {expanded}"
            )));
        }
        // CONTEXT: Validate expansion before allocating from the declared size.
        let mut validation = reader.clone();
        validation.finish()?;
        Ok((
            reader,
            Metadata {
                frame_count,
                first_frame,
                targets,
                gestures,
                samples,
            },
        ))
    }
}

/// Weights decoded from a LIP body, retaining its native frame indices.
/// Named morph binding and conversion from playback seconds belong to the
/// consumer. Neither can be inferred from this byte stream alone.
#[derive(Debug, Clone, PartialEq)]
pub struct LipWeightTrack {
    first_frame: i32,
    targets: usize,
    gestures: usize,
    frame_count: u32,
    weights: Vec<f32>,
}

/// One stored frame, with mouth and additional gesture channels separated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LipFrame<'a> {
    /// Weights in the file's mouth target order.
    pub targets: &'a [f32],
    /// Additional channels in file order, empty when gestures are absent.
    /// These can be negative or greater than one.
    pub gestures: &'a [f32],
}

impl LipWeightTrack {
    /// Signed native index of the first stored frame, including any pre-roll.
    #[must_use]
    pub fn first_frame(&self) -> i32 {
        self.first_frame
    }

    /// Number of frames stored in the body.
    #[must_use]
    pub fn frame_count(&self) -> u32 {
        self.frame_count
    }

    /// Number of mouth target weights in each frame.
    #[must_use]
    pub fn target_count(&self) -> usize {
        self.targets
    }

    /// Number of additional gesture channels in each frame.
    #[must_use]
    pub fn gesture_count(&self) -> usize {
        self.gestures
    }

    /// Stored frames in order, starting at [`Self::first_frame`].
    pub fn frames(&self) -> impl ExactSizeIterator<Item = LipFrame<'_>> {
        self.weights
            .chunks_exact(self.targets + self.gestures)
            .map(|weights| {
                let (targets, gestures) = weights.split_at(self.targets);
                LipFrame { targets, gestures }
            })
    }

    /// Stored weights at a native frame index. Returns `None` outside the
    /// track, without inventing an interpolation or a rest pose.
    #[must_use]
    pub fn frame_at(&self, native_frame: i64) -> Option<LipFrame<'_>> {
        let offset = native_frame.checked_sub(i64::from(self.first_frame))?;
        let index = usize::try_from(offset).ok()?;
        self.frames().nth(index)
    }
}

struct Metadata {
    frame_count: u32,
    first_frame: i32,
    targets: usize,
    gestures: usize,
    samples: usize,
}

#[derive(Clone)]
struct BodyReader<'a> {
    bytes: &'a [u8],
    compressed: bool,
    at: usize,
    produced: usize,
    zeros: usize,
    expanded: usize,
}

impl<'a> BodyReader<'a> {
    fn new(bytes: &'a [u8], compressed: bool, expanded: usize) -> Self {
        Self {
            bytes,
            compressed,
            at: 0,
            produced: 0,
            zeros: 0,
            expanded,
        }
    }

    fn word(&mut self) -> Result<u32> {
        let mut word = [0; 4];
        for byte in &mut word {
            *byte = self.byte()?;
        }
        Ok(u32::from_le_bytes(word))
    }

    fn byte(&mut self) -> Result<u8> {
        if self.produced == self.expanded {
            return Err(malformed("body exceeds its declared expanded size"));
        }
        if self.zeros != 0 {
            self.zeros -= 1;
            self.produced += 1;
            return Ok(0);
        }
        let byte = *self.bytes.get(self.at).ok_or(Error::Truncated {
            what: "LIP body",
            needed: self.at + 1,
            have: self.bytes.len(),
        })?;
        self.at += 1;
        if self.compressed && byte == 0 {
            let count = self
                .bytes
                .get(self.at..self.at + 2)
                .ok_or(Error::Truncated {
                    what: "LIP zero run",
                    needed: self.at + 2,
                    have: self.bytes.len(),
                })?;
            self.at += 2;
            let zeros = u16::from_le_bytes([count[0], count[1]]) as usize;
            if zeros == 0 {
                return Err(malformed("zero-length zero run"));
            }
            if zeros > self.expanded - self.produced {
                return Err(malformed("zero run exceeds the declared expanded size"));
            }
            self.zeros = zeros - 1;
            self.produced += 1;
            return Ok(0);
        }
        self.produced += 1;
        Ok(byte)
    }

    fn finish(&mut self) -> Result<()> {
        while self.produced < self.expanded {
            if self.zeros != 0 {
                self.produced += self.zeros;
                self.zeros = 0;
            } else {
                self.byte()?;
            }
        }
        if self.zeros != 0 || self.at != self.bytes.len() {
            return Err(malformed("trailing bytes after the declared expanded body"));
        }
        Ok(())
    }
}

fn malformed(why: impl Into<String>) -> Error {
    Error::Malformed {
        what: "LIP body",
        why: why.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Lip, COMPRESSED, HAS_GESTURES, HEADER_LEN, KNOWN_FLAGS, VARIABLE_TARGETS, VERSION,
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
