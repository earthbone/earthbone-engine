// SPDX-License-Identifier: MIT

//! Forms: the record database every other group reads.

use std::fmt;

/// A resolved runtime FormID. The top byte is the load-order slot of the
/// owning file, or `0xFE` followed by a twelve-bit light slot.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FormId(pub u32);

impl FormId {
    pub const LIGHT_PREFIX: u32 = 0xFE;
    pub const LIGHT_LOCAL_MASK: u32 = 0x0FFF;

    /// The regular load-order slot, or `None` for a light form.
    pub fn slot(self) -> Option<u8> {
        let top = (self.0 >> 24) as u8;
        (u32::from(top) != Self::LIGHT_PREFIX).then_some(top)
    }

    /// The light slot, or `None` for a regular form.
    pub fn light_slot(self) -> Option<u16> {
        (self.0 >> 24 == Self::LIGHT_PREFIX).then_some(((self.0 >> 12) & 0x0FFF) as u16)
    }

    /// The part of the id the owning file chose.
    pub fn local(self) -> u32 {
        match self.light_slot() {
            Some(_) => self.0 & Self::LIGHT_LOCAL_MASK,
            None => self.0 & 0x00FF_FFFF,
        }
    }

    pub fn is_light(self) -> bool {
        self.light_slot().is_some()
    }
}

impl fmt::Debug for FormId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FormId({:08X})", self.0)
    }
}

/// The record signature, four bytes as they appear in the file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FormType(pub [u8; 4]);

impl FormType {
    pub const fn new(sig: &[u8; 4]) -> Self {
        Self(*sig)
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("????")
    }
}

/// What every `RE::TESForm` method needs from a form.
pub trait Form {
    fn form_id(&self) -> FormId;
    fn form_type(&self) -> FormType;
    /// The editor id, when the engine kept it.
    fn editor_id(&self) -> Option<&str> {
        None
    }
    /// Keywords, the most-read form component across the sample.
    fn keywords(&self) -> &[FormId] {
        &[]
    }
}

/// `RE::TESDataHandler`: the form table plus the file list.
pub trait DataHandler {
    type Form: Form + ?Sized;

    fn lookup(&self, id: FormId) -> Option<&Self::Form>;
    fn lookup_by_editor_id(&self, editor_id: &str) -> Option<&Self::Form>;
    /// All forms of one type, in load-order-resolved winner form.
    fn forms_of_type(&self, kind: FormType) -> Vec<FormId>;
    /// Load-order file names in slot order, light files after regular.
    fn files(&self) -> Vec<String>;
    /// A resolved `TESFile` view, or `None` when the index is out of range.
    fn file(&self, index: usize) -> Option<FileInfo>;
}

/// Read-only `RE::TESFile` identity backed by a load-order entry.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FileInfo {
    pub name: String,
    pub regular_index: Option<u8>,
    pub light_index: Option<u16>,
}

impl FileInfo {
    pub fn is_light(&self) -> bool {
        self.light_index.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_and_light_ids_split_as_the_engine_does() {
        let regular = FormId(0x0501_2345);
        assert_eq!(regular.slot(), Some(5));
        assert_eq!(regular.light_slot(), None);
        assert_eq!(regular.local(), 0x01_2345);
        let light = FormId(0xFE00_1ABC);
        assert_eq!(light.slot(), None);
        assert_eq!(light.light_slot(), Some(0x001));
        assert_eq!(light.local(), 0xABC);
        assert!(light.is_light());
        assert_eq!(format!("{regular:?}"), "FormId(05012345)");
    }

    #[test]
    fn form_type_is_its_signature() {
        assert_eq!(FormType::new(b"REFR").as_str(), "REFR");
    }
}
