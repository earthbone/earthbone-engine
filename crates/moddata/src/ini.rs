// SPDX-License-Identifier: GPL-3.0-or-later

//! The INI dialect the three distributor plugins share.
//!
//! All three load their files through SimpleIni with `SetMultiKey`, so the
//! same key repeats down a file and order matters, which rules out a map.
//! Base Object Swapper also sets `SetAllowKeyOnly`, so a line with no `=` is
//! an entry whose key carries the whole payload. Entries written before any
//! `[Section]` belong to the anonymous section, which is where SPID and KID
//! put everything.
//!
//! Nothing here interprets a value. Each format's own module splits the
//! payload, because the field order differs between them.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

/// One `key = value` line, or one key-only line with an empty value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    /// 1 based, so it matches what an editor shows.
    pub line: usize,
}

/// A section and its entries in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Empty for the anonymous section before any header.
    pub name: String,
    pub entries: Vec<Entry>,
}

/// One parsed file, sections in file order, nothing merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ini {
    pub path: PathBuf,
    pub sections: Vec<Section>,
}

impl Ini {
    /// Reads a file from disk.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the file cannot be read.
    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        Ok(Self::parse(path, &text))
    }

    /// Parses text that is already in hand. Never fails: a line that is not an
    /// entry is skipped, which is what SimpleIni does and what the shipped
    /// files rely on.
    pub fn parse(path: impl Into<PathBuf>, text: &str) -> Self {
        let path = path.into();
        let mut sections = vec![Section {
            name: String::new(),
            entries: Vec::new(),
        }];

        for (i, raw) in text.lines().enumerate() {
            let line = i + 1;
            let trimmed = strip_comment(raw).trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(name) = section_header(trimmed) {
                sections.push(Section {
                    name: name.to_string(),
                    entries: Vec::new(),
                });
                continue;
            }
            let entry = match trimmed.split_once('=') {
                Some((k, v)) => Entry {
                    key: k.trim().to_string(),
                    value: v.trim().to_string(),
                    line,
                },
                // Key only, which Base Object Swapper allows.
                None => Entry {
                    key: trimmed.to_string(),
                    value: String::new(),
                    line,
                },
            };
            if let Some(last) = sections.last_mut() {
                last.entries.push(entry);
            }
        }

        // Drop the anonymous section when the file opened with a header, so a
        // caller iterating sections does not meet an empty one first.
        if sections.first().is_some_and(|s| s.entries.is_empty()) && sections.len() > 1 {
            sections.remove(0);
        }
        Self { path, sections }
    }

    /// Entries of the anonymous section, where SPID and KID write everything.
    pub fn anonymous(&self) -> &[Entry] {
        self.sections
            .iter()
            .find(|s| s.name.is_empty())
            .map_or(&[], |s| s.entries.as_slice())
    }
}

/// `[Name]` with nothing but space around it.
fn section_header(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('[')?;
    let end = rest.rfind(']')?;
    if rest[end + 1..].trim().is_empty() {
        Some(rest[..end].trim())
    } else {
        None
    }
}

/// Cuts a trailing comment. A `;` or `#` inside a value would be unusual in
/// these formats, and SimpleIni cuts on the character wherever it appears.
fn strip_comment(line: &str) -> &str {
    match line.find([';', '#']) {
        Some(at) => &line[..at],
        None => line,
    }
}

/// Splits on a separator, trims each part, and drops empty ones. Every one of
/// the three formats splits its lists this way.
pub fn split_list(text: &str, sep: char) -> Vec<String> {
    text.split(sep)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// True when a field carries something. The plugins treat `NONE` as an
/// explicitly empty field, which lets an author skip a field positionally.
pub fn is_valid_entry(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && !t.eq_ignore_ascii_case("NONE")
}

/// The field at `index` of a pipe separated record, or an empty string when
/// the record stops short. Every format pads this way rather than erroring.
pub fn field(fields: &[String], index: usize) -> &str {
    fields.get(index).map_or("", String::as_str)
}

#[cfg(test)]
mod tests {
    use super::{field, is_valid_entry, split_list, Ini};

    #[test]
    fn entries_before_a_header_land_in_the_anonymous_section() {
        let ini = Ini::parse("t.ini", "Keyword = A|Armor\nKeyword = B|Weapon\n");
        assert_eq!(ini.anonymous().len(), 2);
        assert_eq!(ini.anonymous()[0].key, "Keyword");
        assert_eq!(ini.anonymous()[0].value, "A|Armor");
        assert_eq!(ini.anonymous()[1].line, 2);
    }

    #[test]
    fn a_repeated_key_keeps_every_instance_in_order() {
        let ini = Ini::parse("t.ini", "Spell = one\nSpell = two\nSpell = three\n");
        let values: Vec<_> = ini.anonymous().iter().map(|e| e.value.as_str()).collect();
        assert_eq!(values, ["one", "two", "three"]);
    }

    #[test]
    fn a_key_only_line_keeps_the_whole_payload_as_the_key() {
        let ini = Ini::parse("t.ini", "[Forms]\n0x123~A.esp|0x456~B.esp\n");
        let e = &ini.sections[0].entries[0];
        assert_eq!(e.key, "0x123~A.esp|0x456~B.esp");
        assert!(e.value.is_empty());
    }

    #[test]
    fn a_section_name_may_carry_a_condition_list() {
        let ini = Ini::parse("t.ini", "[Forms|Riverwood,-Whiterun]\nx|y\n");
        assert_eq!(ini.sections[0].name, "Forms|Riverwood,-Whiterun");
    }

    #[test]
    fn comments_and_blank_lines_are_dropped() {
        let ini = Ini::parse("t.ini", "; a comment\n\nSpell = x ; trailing\n# another\n");
        assert_eq!(ini.anonymous().len(), 1);
        assert_eq!(ini.anonymous()[0].value, "x");
    }

    #[test]
    fn the_anonymous_section_goes_away_when_the_file_opens_with_a_header() {
        let ini = Ini::parse("t.ini", "[Forms]\na|b\n");
        assert_eq!(ini.sections.len(), 1);
        assert_eq!(ini.sections[0].name, "Forms");
    }

    #[test]
    fn a_list_splits_and_trims_and_drops_empties() {
        assert_eq!(split_list(" a , b ,, c ", ','), ["a", "b", "c"]);
    }

    #[test]
    fn none_counts_as_an_empty_field() {
        assert!(is_valid_entry("Guard"));
        assert!(!is_valid_entry("NONE"));
        assert!(!is_valid_entry("none"));
        assert!(!is_valid_entry("   "));
    }

    #[test]
    fn a_short_record_pads_with_empty_fields() {
        let f = vec!["a".to_string(), "b".to_string()];
        assert_eq!(field(&f, 0), "a");
        assert_eq!(field(&f, 5), "");
    }
}
