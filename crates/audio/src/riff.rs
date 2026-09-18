// SPDX-License-Identifier: GPL-3.0-or-later

//! The RIFF chunk walk both the xWMA reader and the WAV reader need. Small
//! enough to keep here rather than take a dependency for it.

use crate::error::{Error, Result};

/// One chunk of a RIFF file, its payload borrowed from the input.
#[derive(Debug, Clone, Copy)]
pub struct Chunk<'a> {
    pub id: [u8; 4],
    pub data: &'a [u8],
}

/// The form type of a RIFF file plus every chunk in it, in file order.
#[derive(Debug, Clone)]
pub struct Riff<'a> {
    pub form: [u8; 4],
    pub chunks: Vec<Chunk<'a>>,
}

impl<'a> Riff<'a> {
    /// Walks a RIFF file into its chunks.
    ///
    /// A chunk whose declared size runs past the end of the file is an error
    /// rather than a truncation of the last chunk, because a reader that
    /// guesses there produces silent corruption further on.
    ///
    /// # Errors
    ///
    /// [`Error::NotRiff`] when the magic is wrong, and [`Error::Truncated`]
    /// when the header or any chunk runs past the end.
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() < 12 {
            return Err(Error::Truncated {
                what: "RIFF header",
                needed: 12,
                have: bytes.len(),
            });
        }
        if &bytes[..4] != b"RIFF" {
            return Err(Error::NotRiff);
        }
        let mut form = [0u8; 4];
        form.copy_from_slice(&bytes[8..12]);

        let mut chunks = Vec::new();
        let mut at = 12usize;
        while at + 8 <= bytes.len() {
            let mut id = [0u8; 4];
            id.copy_from_slice(&bytes[at..at + 4]);
            let size =
                u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]])
                    as usize;
            let start = at + 8;
            let end = start.checked_add(size).ok_or(Error::Malformed {
                what: "RIFF chunk",
                why: "chunk size overflows a usize".to_string(),
            })?;
            if end > bytes.len() {
                return Err(Error::Truncated {
                    what: "RIFF chunk",
                    needed: end,
                    have: bytes.len(),
                });
            }
            chunks.push(Chunk {
                id,
                data: &bytes[start..end],
            });
            // Chunks are padded to an even offset, and the pad byte is not
            // counted in the size field.
            at = end + (end & 1);
        }
        Ok(Self { form, chunks })
    }

    /// The first chunk with this id, if the file has one.
    #[must_use]
    pub fn chunk(&self, id: &[u8; 4]) -> Option<&Chunk<'a>> {
        self.chunks.iter().find(|c| &c.id == id)
    }
}

/// Reads a little endian `u16` at `off`, or `None` past the end.
#[must_use]
pub fn u16_at(bytes: &[u8], off: usize) -> Option<u16> {
    let end = off.checked_add(2)?;
    let slice = bytes.get(off..end)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

/// Reads a little endian `u32` at `off`, or `None` past the end.
#[must_use]
pub fn u32_at(bytes: &[u8], off: usize) -> Option<u32> {
    let end = off.checked_add(4)?;
    let slice = bytes.get(off..end)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::{u16_at, u32_at, Riff};
    use crate::error::Error;

    fn riff(form: [u8; 4], chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&form);
        for (id, data) in chunks {
            body.extend_from_slice(*id);
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(data);
            if data.len() % 2 == 1 {
                body.push(0);
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn a_two_chunk_file_walks_in_order() {
        let bytes = riff(*b"XWMA", &[(b"fmt ", &[1, 2, 3, 4]), (b"data", &[9, 9])]);
        let r = Riff::parse(&bytes).unwrap();
        assert_eq!(&r.form, b"XWMA");
        assert_eq!(r.chunks.len(), 2);
        assert_eq!(r.chunk(b"fmt ").unwrap().data, &[1, 2, 3, 4]);
        assert_eq!(r.chunk(b"data").unwrap().data, &[9, 9]);
        assert!(r.chunk(b"dpds").is_none());
    }

    #[test]
    fn an_odd_chunk_is_padded_and_the_next_one_still_reads() {
        let bytes = riff(*b"XWMA", &[(b"fmt ", &[7]), (b"data", &[5, 6])]);
        let r = Riff::parse(&bytes).unwrap();
        assert_eq!(r.chunk(b"fmt ").unwrap().data, &[7]);
        assert_eq!(r.chunk(b"data").unwrap().data, &[5, 6]);
    }

    #[test]
    fn a_chunk_past_the_end_is_truncation() {
        let mut bytes = riff(*b"XWMA", &[(b"data", &[1, 2, 3, 4])]);
        bytes[16] = 0xff;
        assert!(matches!(Riff::parse(&bytes), Err(Error::Truncated { .. })));
    }

    #[test]
    fn bad_magic_and_short_files_are_named_apart() {
        let mut bytes = riff(*b"XWMA", &[]);
        bytes[1] = b'X';
        assert!(matches!(Riff::parse(&bytes), Err(Error::NotRiff)));
        assert!(matches!(Riff::parse(b"RIFF"), Err(Error::Truncated { .. })));
    }

    #[test]
    fn the_integer_readers_stop_at_the_end() {
        assert_eq!(u16_at(&[1, 0], 0), Some(1));
        assert_eq!(u16_at(&[1], 0), None);
        assert_eq!(u32_at(&[1, 0, 0, 0], 0), Some(1));
        assert_eq!(u32_at(&[1, 0, 0], 0), None);
    }
}
