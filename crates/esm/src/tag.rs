// SPDX-License-Identifier: GPL-3.0-or-later

//! Four-byte record and subrecord signatures such as `CELL` or `EDID`.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tag(pub [u8; 4]);

impl Tag {
    pub const fn new(bytes: &[u8; 4]) -> Self {
        Tag(*bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> Self {
        let mut t = [0u8; 4];
        t.copy_from_slice(&bytes[..4]);
        Tag(t)
    }

    pub fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            if b.is_ascii_graphic() || b == b' ' {
                write!(f, "{}", b as char)?;
            } else {
                write!(f, "\\x{b:02x}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({self})")
    }
}

impl From<&[u8; 4]> for Tag {
    fn from(b: &[u8; 4]) -> Self {
        Tag(*b)
    }
}

impl From<[u8; 4]> for Tag {
    fn from(b: [u8; 4]) -> Self {
        Tag(b)
    }
}

pub const GRUP: Tag = Tag::new(b"GRUP");
pub const TES4: Tag = Tag::new(b"TES4");
pub const XXXX: Tag = Tag::new(b"XXXX");
pub const CELL: Tag = Tag::new(b"CELL");
pub const REFR: Tag = Tag::new(b"REFR");
pub const ACHR: Tag = Tag::new(b"ACHR");
pub const PGRE: Tag = Tag::new(b"PGRE");
pub const PHZD: Tag = Tag::new(b"PHZD");
pub const PMIS: Tag = Tag::new(b"PMIS");
pub const PARW: Tag = Tag::new(b"PARW");
pub const PBEA: Tag = Tag::new(b"PBEA");
pub const PFLA: Tag = Tag::new(b"PFLA");
pub const PCON: Tag = Tag::new(b"PCON");
pub const PBAR: Tag = Tag::new(b"PBAR");
pub const WRLD: Tag = Tag::new(b"WRLD");
pub const LAND: Tag = Tag::new(b"LAND");
pub const NAVM: Tag = Tag::new(b"NAVM");
pub const STAT: Tag = Tag::new(b"STAT");
pub const RACE: Tag = Tag::new(b"RACE");
pub const EDID: Tag = Tag::new(b"EDID");
pub const FULL: Tag = Tag::new(b"FULL");
