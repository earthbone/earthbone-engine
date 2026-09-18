// SPDX-License-Identifier: GPL-3.0-or-later

//! The FUZ container, which is how every voice line ships: one LIP track and
//! one xWMA stream packed together under a twelve byte header.
//!
//! No open specification exists. The layout below was derived from the files
//! in `Skyrim - Voices_en0.bsa` and confirmed over all 75,408 of them by the
//! gated test, see `docs/audio.md` for the counts and for the one field that
//! is still inferred.

use crate::error::{Error, Result};

/// The four magic bytes every FUZ file opens with.
pub const MAGIC: [u8; 4] = *b"FUZE";

/// Magic, version and LIP size, in that order, four bytes each.
pub const HEADER_LEN: usize = 12;

/// The only version vanilla ships.
pub const VERSION: u32 = 1;

/// A FUZ file split into its two payloads, borrowed from the input.
///
/// `lip` is empty when the line has no mouth animation, which is normal:
/// vanilla ships such files with a zero size field rather than leaving the
/// track out of the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fuz<'a> {
    pub version: u32,
    pub lip: &'a [u8],
    pub audio: &'a [u8],
}

impl<'a> Fuz<'a> {
    /// Splits a FUZ file into its LIP and audio halves.
    ///
    /// # Errors
    ///
    /// [`Error::NotFuz`] when the magic is wrong, [`Error::FuzVersion`] when
    /// the version is not 1, and [`Error::Truncated`] when the declared LIP
    /// size runs past the end of the file.
    pub fn split(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() < HEADER_LEN {
            return Err(Error::Truncated {
                what: "FUZ header",
                needed: HEADER_LEN,
                have: bytes.len(),
            });
        }
        if bytes[..4] != MAGIC {
            return Err(Error::NotFuz);
        }
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != VERSION {
            return Err(Error::FuzVersion(version));
        }
        let lip_len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        let audio_at = HEADER_LEN.checked_add(lip_len).ok_or(Error::Malformed {
            what: "FUZ header",
            why: "LIP size overflows a usize".to_string(),
        })?;
        if audio_at > bytes.len() {
            return Err(Error::Truncated {
                what: "FUZ LIP block",
                needed: audio_at,
                have: bytes.len(),
            });
        }
        Ok(Self {
            version,
            lip: &bytes[HEADER_LEN..audio_at],
            audio: &bytes[audio_at..],
        })
    }

    /// True when the line carries no mouth animation.
    #[must_use]
    pub fn has_lip(&self) -> bool {
        !self.lip.is_empty()
    }

    /// Packs the two payloads back into a FUZ file. The round trip is exact,
    /// which is what the tests check.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        write(self.version, self.lip, self.audio)
    }
}

/// Packs a LIP track and an audio stream into a FUZ file.
#[must_use]
pub fn write(version: u32, lip: &[u8], audio: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + lip.len() + audio.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&(lip.len() as u32).to_le_bytes());
    out.extend_from_slice(lip);
    out.extend_from_slice(audio);
    out
}

#[cfg(test)]
mod tests {
    use super::{write, Error, Fuz, HEADER_LEN, VERSION};

    #[test]
    fn a_fuz_round_trips_through_split_and_write() {
        let lip = [1u8, 2, 3, 4, 5];
        let audio = b"RIFFxxxxXWMA".as_slice();
        let bytes = write(VERSION, &lip, audio);
        assert_eq!(bytes.len(), HEADER_LEN + lip.len() + audio.len());
        let fuz = Fuz::split(&bytes).unwrap();
        assert_eq!(fuz.version, VERSION);
        assert_eq!(fuz.lip, lip);
        assert_eq!(fuz.audio, audio);
        assert!(fuz.has_lip());
        assert_eq!(fuz.to_bytes(), bytes);
    }

    #[test]
    fn an_empty_lip_block_is_normal_and_not_an_error() {
        let bytes = write(VERSION, &[], b"RIFF");
        let fuz = Fuz::split(&bytes).unwrap();
        assert!(!fuz.has_lip());
        assert_eq!(fuz.audio, b"RIFF");
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = write(VERSION, &[], b"RIFF");
        bytes[0] = b'X';
        assert!(matches!(Fuz::split(&bytes), Err(Error::NotFuz)));
    }

    #[test]
    fn an_unknown_version_is_rejected_by_number() {
        let bytes = write(2, &[], b"RIFF");
        assert!(matches!(Fuz::split(&bytes), Err(Error::FuzVersion(2))));
    }

    #[test]
    fn a_lip_size_past_the_end_is_truncation_not_a_panic() {
        let mut bytes = write(VERSION, &[1, 2, 3], b"RIFF");
        bytes[8] = 0xff;
        assert!(matches!(Fuz::split(&bytes), Err(Error::Truncated { .. })));
    }

    #[test]
    fn a_short_file_is_truncation() {
        assert!(matches!(
            Fuz::split(b"FUZ"),
            Err(Error::Truncated { .. })
        ));
    }
}
