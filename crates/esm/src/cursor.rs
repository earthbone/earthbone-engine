// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounds-checked little-endian reads over a byte slice. `base` is the
//! absolute file offset of the slice's first byte so errors report file
//! positions even when reading an inflated buffer.

use crate::error::{Error, Result};
use crate::tag::Tag;

pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
    base: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8], base: usize) -> Self {
        Cursor { data, pos: 0, base }
    }

    /// Absolute file offset of the next byte.
    pub fn offset(&self) -> usize {
        self.base + self.pos
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(Error::Truncated {
                at: self.offset(),
                need: n,
                have: self.remaining(),
            });
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn peek_tag(&self) -> Result<Tag> {
        if self.remaining() < 4 {
            return Err(Error::Truncated {
                at: self.offset(),
                need: 4,
                have: self.remaining(),
            });
        }
        Ok(Tag::from_slice(&self.data[self.pos..self.pos + 4]))
    }

    pub fn tag(&mut self) -> Result<Tag> {
        Ok(Tag::from_slice(self.take(4)?))
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// A sub-cursor over the next `n` bytes, advancing this one past them.
    pub fn sub(&mut self, n: usize) -> Result<Cursor<'a>> {
        let base = self.offset();
        let data = self.take(n)?;
        Ok(Cursor { data, pos: 0, base })
    }
}

/// Fixed-size little-endian field reads on a subrecord's bytes, with the
/// record and subrecord names for error messages.
pub struct Fields<'a> {
    cur: Cursor<'a>,
    record: Tag,
    tag: Tag,
}

impl<'a> Fields<'a> {
    pub fn new(record: Tag, tag: Tag, data: &'a [u8]) -> Self {
        Fields {
            cur: Cursor::new(data, 0),
            record,
            tag,
        }
    }

    fn need(&self, n: usize, what: &str) -> Result<()> {
        if self.cur.remaining() < n {
            return Err(Error::SubrecordSize {
                record: self.record,
                tag: self.tag,
                len: self.cur.data.len(),
                expected: format!("at least {} for {what}", self.cur.pos + n),
            });
        }
        Ok(())
    }

    pub fn remaining(&self) -> usize {
        self.cur.remaining()
    }

    pub fn u8(&mut self) -> Result<u8> {
        self.need(1, "u8")?;
        self.cur.u8()
    }

    pub fn u16(&mut self) -> Result<u16> {
        self.need(2, "u16")?;
        self.cur.u16()
    }

    pub fn i16(&mut self) -> Result<i16> {
        self.need(2, "i16")?;
        self.cur.i16()
    }

    pub fn u32(&mut self) -> Result<u32> {
        self.need(4, "u32")?;
        self.cur.u32()
    }

    pub fn i32(&mut self) -> Result<i32> {
        self.need(4, "i32")?;
        self.cur.i32()
    }

    pub fn f32(&mut self) -> Result<f32> {
        self.need(4, "f32")?;
        self.cur.f32()
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.need(n, "bytes")?;
        self.cur.take(n)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    /// Fail unless the subrecord is exactly `n` bytes long.
    pub fn exact(&self, n: usize) -> Result<()> {
        if self.cur.data.len() != n {
            return Err(Error::SubrecordSize {
                record: self.record,
                tag: self.tag,
                len: self.cur.data.len(),
                expected: format!("exactly {n}"),
            });
        }
        Ok(())
    }
}

/// Decode a NUL-terminated Windows-1252 string. Skyrim files use the code
/// page, not UTF-8, so bytes 0x80 to 0x9F map through the 1252 table and the
/// rest are Latin-1.
pub fn zstring(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    cp1252(&bytes[..end])
}

pub fn cp1252(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80 => '\u{20AC}',
            0x82 => '\u{201A}',
            0x83 => '\u{0192}',
            0x84 => '\u{201E}',
            0x85 => '\u{2026}',
            0x86 => '\u{2020}',
            0x87 => '\u{2021}',
            0x88 => '\u{02C6}',
            0x89 => '\u{2030}',
            0x8A => '\u{0160}',
            0x8B => '\u{2039}',
            0x8C => '\u{0152}',
            0x8E => '\u{017D}',
            0x91 => '\u{2018}',
            0x92 => '\u{2019}',
            0x93 => '\u{201C}',
            0x94 => '\u{201D}',
            0x95 => '\u{2022}',
            0x96 => '\u{2013}',
            0x97 => '\u{2014}',
            0x98 => '\u{02DC}',
            0x99 => '\u{2122}',
            0x9A => '\u{0161}',
            0x9B => '\u{203A}',
            0x9C => '\u{0153}',
            0x9E => '\u{017E}',
            0x9F => '\u{0178}',
            b => b as char,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_report_absolute_offsets() {
        let mut c = Cursor::new(&[1, 0, 0, 0, 2], 100);
        assert_eq!(c.u32().unwrap(), 1);
        let err = c.u32().unwrap_err();
        assert_eq!(
            err,
            Error::Truncated {
                at: 104,
                need: 4,
                have: 1
            }
        );
    }

    #[test]
    fn zstring_stops_at_nul_and_decodes_1252() {
        assert_eq!(zstring(b"Whiterun\0junk"), "Whiterun");
        assert_eq!(
            zstring(&[0x93, b'a', 0x94, 0xE9]),
            "\u{201C}a\u{201D}\u{E9}"
        );
        assert_eq!(zstring(b"no terminator"), "no terminator");
    }
}
