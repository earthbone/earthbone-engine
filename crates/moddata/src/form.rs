// SPDX-License-Identifier: GPL-3.0-or-later

//! How these formats name a form before anything is loaded.
//!
//! An author writes one of four things, and all three plugins agree on the
//! spelling: `0x123~Mod.esp` for a form in a named file, a bare `0x123` when
//! they are willing to gamble on the load order, `Mod.esp` to mean every form
//! the file owns, and anything else is an editor id. The `.esm`, `.esp` and
//! `.esl` suffix is what separates the third case from the fourth, which is
//! exactly the test `RawForm::IsMod` makes in Keyword Item Distributor.
//!
//! Resolution is separate from parsing on purpose. A config is read once with
//! no game data in hand, then resolved against a [`soe_esm::LoadOrder`], so a
//! file that names a plugin the user does not have parses fine and resolves
//! to nothing.

use soe_esm::{GlobalFormId, LoadOrder};

/// A form as written in a configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormRef {
    /// `0x1234~Skyrim.esm`, the only spelling that is unambiguous.
    InFile { object: u32, file: String },
    /// `0x1234` with no file. Resolved against the first file that owns it,
    /// which is what the plugins do and why they warn against writing it.
    Bare { object: u32 },
    /// `Skyrim.esm`, meaning every form the file owns.
    File { name: String },
    /// Anything else. Matched against a record's `EDID`.
    EditorId { id: String },
}

impl FormRef {
    /// Reads one form reference. Never fails, since an unrecognised string is
    /// a valid editor id as far as the format is concerned.
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        if let Some((id, file)) = text.split_once('~') {
            if let Some(object) = parse_hex(id.trim()) {
                return Self::InFile {
                    object,
                    file: file.trim().to_string(),
                };
            }
        }
        if is_plugin_name(text) {
            return Self::File {
                name: text.to_string(),
            };
        }
        if let Some(object) = parse_hex(text) {
            return Self::Bare { object };
        }
        Self::EditorId {
            id: text.to_string(),
        }
    }

    /// True when this names a whole file rather than one form.
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File { .. })
    }

    /// Resolves to a form in the load order, or `None` when the file is absent
    /// or nothing carries the editor id. A [`FormRef::File`] resolves to
    /// nothing because it names a set, see [`FormRef::matches`].
    pub fn resolve(&self, order: &LoadOrder) -> Option<GlobalFormId> {
        match self {
            Self::InFile { object, file } => {
                let at = order.position_of(file)?;
                let id = order.resolve(at, *object);
                order.contains(id).then_some(id)
            }
            Self::Bare { object } => (0..order.len())
                .map(|at| order.resolve(at, *object))
                .find(|id| order.contains(*id)),
            Self::File { .. } => None,
            Self::EditorId { id } => order
                .winners()
                .find(|(_, r)| {
                    order
                        .record(*r)
                        .editor_id()
                        .is_some_and(|e| e.eq_ignore_ascii_case(id))
                })
                .map(|(gid, _)| gid),
        }
    }

    /// True when `id` is the form this reference names, or is owned by the
    /// file it names. This is the test a filter makes, and it is why
    /// [`FormRef::File`] cannot resolve to a single form.
    pub fn matches(&self, order: &LoadOrder, id: GlobalFormId) -> bool {
        match self {
            Self::File { name } => order
                .position_of(name)
                .is_some_and(|at| usize::from(id.file) == at),
            _ => self.resolve(order) == Some(id),
        }
    }
}

/// `0x1234`, or bare hex digits, which the plugins also accept.
fn parse_hex(text: &str) -> Option<u32> {
    let body = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    if body.is_empty() || !body.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    // A bare decimal-looking string is an editor id far more often than a
    // form id, so only an explicit 0x counts when there is no hex letter.
    let has_prefix = text.len() != body.len();
    let looks_hex = body.chars().any(|c| c.is_ascii_alphabetic());
    if !has_prefix && !looks_hex {
        return None;
    }
    u32::from_str_radix(body, 16).ok()
}

/// The test `RawForm::IsMod` makes. Compared on the extension rather than the
/// whole string so a file called `Thing.ESP` counts and `notesp` does not.
fn is_plugin_name(text: &str) -> bool {
    text.rsplit_once('.').is_some_and(|(_, ext)| {
        ["esm", "esp", "esl"]
            .iter()
            .any(|e| ext.eq_ignore_ascii_case(e))
    })
}

#[cfg(test)]
mod tests {
    use super::FormRef;

    #[test]
    fn a_form_id_and_file_pair_splits_on_the_tilde() {
        assert_eq!(
            FormRef::parse("0x01A2B3~Skyrim.esm"),
            FormRef::InFile {
                object: 0x01A2B3,
                file: "Skyrim.esm".to_string()
            }
        );
    }

    #[test]
    fn spaces_around_a_pair_do_not_matter() {
        assert_eq!(
            FormRef::parse(" 0x12 ~ My Mod.esp "),
            FormRef::InFile {
                object: 0x12,
                file: "My Mod.esp".to_string()
            }
        );
    }

    #[test]
    fn a_plugin_suffix_makes_it_a_file_reference() {
        assert!(FormRef::parse("Dawnguard.esm").is_file());
        assert!(FormRef::parse("Thing.ESL").is_file());
        assert!(!FormRef::parse("Dawnguard").is_file());
    }

    #[test]
    fn a_bare_hex_id_needs_its_prefix_or_a_hex_letter() {
        assert_eq!(FormRef::parse("0x7A"), FormRef::Bare { object: 0x7A });
        assert_eq!(FormRef::parse("1A2B"), FormRef::Bare { object: 0x1A2B });
        // All digits and no prefix reads as an editor id, which is what an
        // author almost always meant.
        assert_eq!(
            FormRef::parse("12345"),
            FormRef::EditorId {
                id: "12345".to_string()
            }
        );
    }

    #[test]
    fn anything_else_is_an_editor_id() {
        assert_eq!(
            FormRef::parse("MyKeyword"),
            FormRef::EditorId {
                id: "MyKeyword".to_string()
            }
        );
    }
}
