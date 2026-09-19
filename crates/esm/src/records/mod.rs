// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed views over the raw subrecords of the record types the first
//! milestone needs. A view borrows nothing: it copies the fields it names
//! and leaves everything else in the `Record`, so an unknown subrecord is
//! never lost and a view is never stale.
//!
//! FormIDs inside views are raw, as written in the file, so a view can be
//! built without a load order. Resolve them with `LoadOrder::resolve`.

pub mod cell;
pub mod gameplay;
pub mod land;
pub mod navm;
pub mod race;
pub mod refr;
pub mod stat;
pub mod wrld;

use crate::cursor::{zstring, Fields};
use crate::error::Result;
use crate::plugin::Record;
use crate::tag::Tag;

/// A player-visible string. In a localized file the subrecord holds a string
/// table id instead of text, so the view keeps whichever it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Text {
    Plain(String),
    Localized(u32),
}

impl Text {
    pub fn plain(&self) -> Option<&str> {
        match self {
            Text::Plain(s) => Some(s),
            Text::Localized(_) => None,
        }
    }
}

pub(crate) fn text(rec: &Record, tag: Tag, localized: bool) -> Option<Text> {
    let d = rec.find(tag)?;
    if localized && d.len() == 4 {
        Some(Text::Localized(u32::from_le_bytes([
            d[0], d[1], d[2], d[3],
        ])))
    } else {
        Some(Text::Plain(zstring(d)))
    }
}

pub(crate) fn form_id(rec: &Record, tag: Tag) -> Result<Option<u32>> {
    match rec.find(tag) {
        None => Ok(None),
        Some(d) => {
            let mut f = Fields::new(rec.tag, tag, d);
            f.exact(4)?;
            Ok(Some(f.u32()?))
        }
    }
}

pub(crate) fn f32_field(rec: &Record, tag: Tag) -> Result<Option<f32>> {
    match rec.find(tag) {
        None => Ok(None),
        Some(d) => {
            let mut f = Fields::new(rec.tag, tag, d);
            f.exact(4)?;
            Ok(Some(f.f32()?))
        }
    }
}

pub(crate) fn form_id_list(rec: &Record, tag: Tag) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    for d in rec.all(tag) {
        let mut f = Fields::new(rec.tag, tag, d);
        if d.len() % 4 != 0 {
            f.exact(d.len() / 4 * 4)?;
        }
        while f.remaining() >= 4 {
            out.push(f.u32()?);
        }
    }
    Ok(out)
}

pub(crate) fn u16_list(rec: &Record, tag: Tag) -> Result<Vec<u16>> {
    let mut out = Vec::new();
    for d in rec.all(tag) {
        let mut f = Fields::new(rec.tag, tag, d);
        if d.len() % 2 != 0 {
            f.exact(d.len() / 2 * 2)?;
        }
        while f.remaining() >= 2 {
            out.push(f.u16()?);
        }
    }
    Ok(out)
}

pub(crate) fn zstring_field(rec: &Record, tag: Tag) -> Option<String> {
    rec.find(tag).map(zstring)
}

/// Six-float position and rotation, as REFR DATA and XTEL carry it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Transform {
    /// World units.
    pub position: [f32; 3],
    /// Radians, X Y Z.
    pub rotation: [f32; 3],
}

impl Transform {
    pub(crate) fn read(f: &mut Fields<'_>) -> Result<Self> {
        let mut t = Transform::default();
        for v in t.position.iter_mut() {
            *v = f.f32()?;
        }
        for v in t.rotation.iter_mut() {
            *v = f.f32()?;
        }
        Ok(t)
    }

    pub fn to_bytes(&self) -> [u8; 24] {
        let mut out = [0u8; 24];
        for (i, v) in self.position.iter().chain(self.rotation.iter()).enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        out
    }
}
