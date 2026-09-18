// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounds-checked little-endian reads over a byte slice, plus the three save
//! specific primitives from the UESP File Format Conventions page: `wstring`,
//! `vsval` and `RefID`.
//!
//! Two of those do not round trip through their decoded value, so both keep
//! their original bytes. A `vsval` may be written wider than its value needs,
//! and ReSaver stores the raw bytes for exactly that reason, so re-encoding a
//! parsed save from the decoded number alone can change the file. A `RefID` is
//! three bytes big-endian while everything around it is little-endian.

use crate::error::{Error, Result};

pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
    region: &'static str,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8], region: &'static str) -> Self {
        Cursor {
            data,
            pos: 0,
            region,
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn seek(&mut self, pos: usize, name: &'static str) -> Result<()> {
        if pos > self.data.len() {
            return Err(Error::TableOffset {
                name,
                offset: pos,
                len: self.data.len(),
            });
        }
        self.pos = pos;
        Ok(())
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
                region: self.region,
                at: self.pos,
                need: n,
                have: self.remaining(),
            });
        }
        let taken = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(taken)
    }

    pub fn rest(&mut self) -> &'a [u8] {
        let rest = &self.data[self.pos..];
        self.pos = self.data.len();
        rest
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

    pub fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// A `wstring`: a uint16 byte length then that many Windows-1252 bytes,
    /// with no terminator.
    pub fn wstring(&mut self) -> Result<WString> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        Ok(WString {
            bytes: bytes.to_vec(),
        })
    }

    pub fn vsval(&mut self) -> Result<VsVal> {
        let first = self.u8()?;
        let width = (first & 0x3) as usize;
        let rest = self.take(width.min(2))?;
        let mut bytes = [0u8; 3];
        bytes[0] = first;
        bytes[1..=rest.len()].copy_from_slice(rest);
        Ok(VsVal {
            bytes,
            len: 1 + rest.len(),
        })
    }

    /// A `RefID`: three bytes, big-endian, the top two bits naming the kind.
    pub fn ref_id(&mut self) -> Result<RefId> {
        let b = self.take(3)?;
        Ok(RefId(
            u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]),
        ))
    }
}

/// A length-prefixed string. The bytes are kept as read so a write reproduces
/// them, since Windows-1252 decoding is lossy for bytes no code page assigns.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct WString {
    bytes: Vec<u8>,
}

impl WString {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn to_text(&self) -> String {
        let (text, _, _) = encoding_rs::WINDOWS_1252.decode(&self.bytes);
        text.into_owned()
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        let len = u16::try_from(self.bytes.len()).unwrap_or(u16::MAX) as usize;
        out.extend_from_slice(&(len as u16).to_le_bytes());
        out.extend_from_slice(&self.bytes[..len]);
    }

    pub fn from_text(text: &str) -> Self {
        let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(text);
        WString {
            bytes: bytes.into_owned(),
        }
    }
}

impl std::fmt::Debug for WString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_text())
    }
}

impl std::fmt::Display for WString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_text())
    }
}

/// A variable-sized value. The low two bits of the first byte give the width,
/// the value is the remaining bits shifted down by two.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VsVal {
    bytes: [u8; 3],
    len: usize,
}

impl VsVal {
    pub fn value(&self) -> u32 {
        let raw = match self.len {
            1 => u32::from(self.bytes[0]),
            2 => u32::from(self.bytes[0]) | u32::from(self.bytes[1]) << 8,
            _ => {
                u32::from(self.bytes[0])
                    | u32::from(self.bytes[1]) << 8
                    | u32::from(self.bytes[2]) << 16
            }
        };
        raw >> 2
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.bytes[..self.len]);
    }

    /// The narrowest encoding of `value`, or `None` when it does not fit.
    ///
    /// The widths are six, fourteen and twenty two bits, because two bits of
    /// the first byte are the width tag. Both references get the boundaries
    /// wrong in the same direction: the UESP page says a value up to 0x40 is
    /// one byte and ReSaver's encoder follows it, but 0x40 shifted left by two
    /// is 0x100 and does not survive the cast to a byte, so it writes zero.
    /// The largest one byte value is 0x3F. The same off by one applies at
    /// 0x4000, and the three byte cap is 0x3FFFFF rather than the 0x40000000
    /// both sources state, which would need a fourth byte.
    pub fn from_value(value: u32) -> Option<Self> {
        let shifted = value.checked_shl(2)?;
        if value <= 0x3F {
            Some(VsVal {
                bytes: [shifted as u8, 0, 0],
                len: 1,
            })
        } else if value <= 0x3FFF {
            Some(VsVal {
                bytes: [(shifted as u8) | 0x1, (shifted >> 8) as u8, 0],
                len: 2,
            })
        } else if value <= 0x3F_FFFF {
            Some(VsVal {
                bytes: [
                    (shifted as u8) | 0x2,
                    (shifted >> 8) as u8,
                    (shifted >> 16) as u8,
                ],
                len: 3,
            })
        } else {
            None
        }
    }
}

impl std::fmt::Debug for VsVal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value())
    }
}

/// A three byte form reference. The top two bits say how to turn the lower 22
/// into a FormID, which needs the save's own plugin list and form id array, so
/// resolution lives on [`crate::Save`] rather than here.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RefId(u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefKind {
    /// An index into the save's form id array, 1 based, 0 meaning form 0.
    Index,
    /// A form from the first plugin in the list, which is Skyrim.esm.
    Default,
    /// A form created during play, plugin index 0xFF.
    Created,
    Invalid,
}

impl RefId {
    /// Builds a reference from its three stored bytes, in file order.
    pub fn from_bytes(bytes: [u8; 3]) -> Self {
        RefId(u32::from(bytes[0]) << 16 | u32::from(bytes[1]) << 8 | u32::from(bytes[2]))
    }

    pub fn raw(self) -> u32 {
        self.0
    }

    pub fn kind(self) -> RefKind {
        match (self.0 >> 22) & 0x3 {
            0 => RefKind::Index,
            1 => RefKind::Default,
            2 => RefKind::Created,
            _ => RefKind::Invalid,
        }
    }

    pub fn value(self) -> u32 {
        self.0 & 0x3F_FFFF
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn write(self, out: &mut Vec<u8>) {
        out.push((self.0 >> 16) as u8);
        out.push((self.0 >> 8) as u8);
        out.push(self.0 as u8);
    }
}

impl std::fmt::Debug for RefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RefId({:#08x}, {:?})", self.0, self.kind())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vsval_round_trips_through_its_bytes() {
        for value in [0u32, 1, 0x3F, 0x40, 0x3FFF, 0x4000, 0x4001, 0x3F_FFFF] {
            let Some(encoded) = VsVal::from_value(value) else {
                panic!("{value} should encode");
            };
            let mut bytes = Vec::new();
            encoded.write(&mut bytes);
            let mut cursor = Cursor::new(&bytes, "test");
            let Ok(decoded) = cursor.vsval() else {
                panic!("{value} should decode");
            };
            assert_eq!(decoded.value(), value, "value {value}");
            assert_eq!(cursor.remaining(), 0, "value {value} left bytes behind");
        }
    }

    #[test]
    fn a_vsval_too_large_for_three_bytes_is_refused() {
        assert!(VsVal::from_value(0x3F_FFFF).is_some());
        assert!(
            VsVal::from_value(0x40_0000).is_none(),
            "twenty two bits is the cap, both references claim more"
        );
    }

    #[test]
    fn a_wide_vsval_keeps_its_width() {
        // 0 written in the three byte form. The value is 0 either way, but a
        // save that stored it wide must be written back wide.
        let wide = [0x02u8, 0x00, 0x00];
        let mut cursor = Cursor::new(&wide, "test");
        let decoded = cursor.vsval().expect("decodes");
        assert_eq!(decoded.value(), 0);
        let mut out = Vec::new();
        decoded.write(&mut out);
        assert_eq!(out, wide, "a wide zero must not narrow on write");
    }

    #[test]
    fn a_ref_id_is_big_endian_with_the_kind_on_top() {
        // The UESP page's worked example: DragonsAbsorbed 0x0001C0F2 becomes
        // 41 C0 F2, a Default reference to value 0x01C0F2.
        let bytes = [0x41u8, 0xC0, 0xF2];
        let mut cursor = Cursor::new(&bytes, "test");
        let id = cursor.ref_id().expect("decodes");
        assert_eq!(id.kind(), RefKind::Default);
        assert_eq!(id.value(), 0x01_C0F2);
        let mut out = Vec::new();
        id.write(&mut out);
        assert_eq!(out, bytes);
    }

    #[test]
    fn a_wstring_round_trips() {
        let mut bytes = Vec::new();
        WString::from_text("Dragonborn").write(&mut bytes);
        let mut cursor = Cursor::new(&bytes, "test");
        let decoded = cursor.wstring().expect("decodes");
        assert_eq!(decoded.to_text(), "Dragonborn");
        let mut out = Vec::new();
        decoded.write(&mut out);
        assert_eq!(out, bytes);
    }

    #[test]
    fn a_short_read_names_its_region() {
        let mut cursor = Cursor::new(&[0u8; 2], "header");
        let error = cursor.u32().expect_err("two bytes cannot hold a u32");
        assert_eq!(
            error,
            Error::Truncated {
                region: "header",
                at: 0,
                need: 4,
                have: 2
            }
        );
    }
}
