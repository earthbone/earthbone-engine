// SPDX-License-Identifier: GPL-3.0-or-later

//! Skyrim localized string tables. The directory layout and string kinds follow
//! xEdit `wbLocalization.pas`. SE encodings follow `xEdit/xeInit.pas` and
//! `Core/wbInterface.pas`, with UTF-8 before language fallbacks outside English.

use std::collections::BTreeMap;

use encoding_rs::{
    Encoding, UTF_8, WINDOWS_1250, WINDOWS_1251, WINDOWS_1252, WINDOWS_1253, WINDOWS_1254,
    WINDOWS_1256,
};

use crate::{Error, Result};

/// The three localized table encodings selected by record field type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StringTableKind {
    Dl,
    Il,
    Plain,
}

impl StringTableKind {
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Dl => "dlstrings",
            Self::Il => "ilstrings",
            Self::Plain => "strings",
        }
    }

    #[must_use]
    pub const fn length_prefixed(self) -> bool {
        !matches!(self, Self::Plain)
    }
}

/// One plugin and language's string ids.
#[derive(Debug, Clone, Default)]
pub struct StringTable {
    strings: BTreeMap<u32, String>,
}

impl StringTable {
    /// Read an indexed `.strings`, `.dlstrings` or `.ilstrings` payload.
    ///
    /// # Errors
    /// Rejects truncated directories, invalid offsets, duplicate IDs, malformed
    /// terminators and text that neither selected encoding can decode.
    pub fn parse(bytes: &[u8], kind: StringTableKind, language: &str) -> Result<Self> {
        let count = u32_at(bytes, 0)? as usize;
        let data_size = u32_at(bytes, 4)? as usize;
        let directory = 8usize
            .checked_add(count.checked_mul(8).ok_or_else(overflow)?)
            .ok_or_else(overflow)?;
        let end = directory.checked_add(data_size).ok_or_else(overflow)?;
        if bytes.len() < directory || bytes.len() < end {
            return Err(Error::Invalid("localized string table is truncated".into()));
        }
        let mut strings = BTreeMap::new();
        for index in 0..count {
            let entry = 8 + index * 8;
            let id = u32_at(bytes, entry)?;
            let offset = u32_at(bytes, entry + 4)? as usize;
            let start = directory.checked_add(offset).ok_or_else(overflow)?;
            if start >= end {
                return Err(Error::Invalid(
                    "localized string offset is outside data".into(),
                ));
            }
            let text = if kind.length_prefixed() {
                if start.checked_add(4).ok_or_else(overflow)? > end {
                    return Err(Error::Invalid(
                        "localized string length is outside data".into(),
                    ));
                }
                let length = u32_at(bytes, start)? as usize;
                if length == 0 {
                    return Err(Error::Invalid("localized string length is zero".into()));
                }
                let text_start = start.checked_add(4).ok_or_else(overflow)?;
                let text_end = text_start.checked_add(length).ok_or_else(overflow)?;
                if text_end > end || bytes[text_end - 1] != 0 {
                    return Err(Error::Invalid("localized string is malformed".into()));
                }
                &bytes[text_start..text_end - 1]
            } else {
                let remaining = &bytes[start..end];
                let length = remaining
                    .iter()
                    .position(|byte| *byte == 0)
                    .ok_or_else(|| Error::Invalid("localized string has no terminator".into()))?;
                &remaining[..length]
            };
            if strings.insert(id, decode_text(text, language)?).is_some() {
                return Err(Error::Invalid("localized string id is repeated".into()));
            }
        }
        Ok(Self { strings })
    }

    #[must_use]
    pub fn get(&self, id: u32) -> Option<&str> {
        self.strings.get(&id).map(String::as_str)
    }
}

fn decode_text(bytes: &[u8], language: &str) -> Result<String> {
    if !language.eq_ignore_ascii_case("english") {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return Ok(text.to_owned());
        }
    }
    encoding_for(language)
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(std::borrow::Cow::into_owned)
        .ok_or_else(|| Error::Invalid("invalid localized text encoding".into()))
}

fn encoding_for(language: &str) -> &'static Encoding {
    match language.to_ascii_lowercase().as_str() {
        "polish" | "czech" | "hungarian" => WINDOWS_1250,
        "russian" => WINDOWS_1251,
        "greek" => WINDOWS_1253,
        "turkish" => WINDOWS_1254,
        "arabic" => WINDOWS_1256,
        "japanese" | "chinese" => UTF_8,
        _ => WINDOWS_1252,
    }
}

fn u32_at(bytes: &[u8], start: usize) -> Result<u32> {
    let end = start.checked_add(4).ok_or_else(overflow)?;
    let value: [u8; 4] = bytes
        .get(start..end)
        .ok_or_else(|| Error::Invalid("localized string table is truncated".into()))?
        .try_into()
        .map_err(|_| Error::Invalid("localized string table is malformed".into()))?;
    Ok(u32::from_le_bytes(value))
}

fn overflow() -> Error {
    Error::Invalid("localized string table size overflows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(kind: StringTableKind, text: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        let mut data = Vec::new();
        if kind.length_prefixed() {
            data.extend_from_slice(&((text.len() + 1) as u32).to_le_bytes());
        }
        data.extend_from_slice(text);
        data.push(0);
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&7_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(data.as_slice());
        bytes
    }

    #[test]
    fn japanese_uses_utf8() {
        let table = StringTable::parse(
            &table(StringTableKind::Plain, "こんにちは".as_bytes()),
            StringTableKind::Plain,
            "japanese",
        )
        .unwrap();
        assert_eq!(table.get(7), Some("こんにちは"));
    }

    #[test]
    fn se_french_prefers_utf8_and_accepts_legacy_fallback() {
        for bytes in ["café".as_bytes(), b"caf\xe9".as_slice()] {
            let parsed = StringTable::parse(
                &table(StringTableKind::Plain, bytes),
                StringTableKind::Plain,
                "french",
            )
            .unwrap();
            assert_eq!(parsed.get(7), Some("café"));
        }
        assert!(StringTable::parse(
            &table(StringTableKind::Plain, &[0xFF]),
            StringTableKind::Plain,
            "japanese",
        )
        .is_err());
    }

    #[test]
    fn rejects_a_length_prefix_outside_declared_data() {
        let mut bytes = table(StringTableKind::Dl, b"x");
        bytes[4..8].copy_from_slice(&1_u32.to_le_bytes());
        assert!(StringTable::parse(&bytes, StringTableKind::Dl, "english").is_err());
    }

    #[test]
    fn rejects_a_missing_plain_terminator() {
        let mut bytes = table(StringTableKind::Plain, b"x");
        bytes.pop();
        bytes[4..8].copy_from_slice(&1_u32.to_le_bytes());
        assert!(StringTable::parse(&bytes, StringTableKind::Plain, "english").is_err());
    }

    #[test]
    fn reads_each_shipped_table_layout() {
        for kind in [
            StringTableKind::Plain,
            StringTableKind::Dl,
            StringTableKind::Il,
        ] {
            let table = StringTable::parse(&table(kind, b"Hulda"), kind, "english").unwrap();
            assert_eq!(table.get(7), Some("Hulda"));
        }
    }
}
