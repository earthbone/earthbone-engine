// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved INFO, SMQN and SCEN changes. xEdit's `wbDefinitionsTES5Saves.pas`
//! defines bit zero as six bytes of form flags and bit 31 as a state marker.
//! UESP supplies the change type indices. FallrimTools keeps these three
//! change types as opaque data. The external save corpus shows additional
//! SMQN and SCEN data behind bit 31 that xEdit does not describe.

use crate::{cursor::Cursor, quest::FormFlags, ChangeForm, Error, Result};

const FORM_FLAGS: u32 = 1;
const STATE: u32 = 1 << 31;

/// An INFO change, including its said-once state marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InfoChange {
    /// The six-byte form flag section, when present.
    pub form_flags: Option<FormFlags>,
    /// Whether the saved change marks the topic as said once.
    pub said_once: bool,
}

/// An SMQN change, including its last-run marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoryManagerQuestNodeChange {
    /// The six-byte form flag section, when present.
    pub form_flags: Option<FormFlags>,
    /// Bit patterns of saved last-run times, present when bit 31 is set.
    /// The `u32` representation retains NaN payloads and signed zero exactly.
    pub last_run_times: Option<Vec<u32>>,
}

/// A SCEN change, including its active state marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneChange {
    /// The six-byte form flag section, when present.
    pub form_flags: Option<FormFlags>,
    /// Whether the saved change marks the scene active.
    pub active: bool,
    /// Scene runtime bytes whose internal field boundaries remain unknown.
    pub active_data: Vec<u8>,
}

impl ChangeForm {
    /// Decode an INFO change. Returns `None` for other record types.
    ///
    /// # Errors
    /// Rejects unknown change bits, truncated form flags and trailing bytes.
    pub fn info_change(&self) -> Result<Option<InfoChange>> {
        if self.record_type() != "INFO" {
            return Ok(None);
        }
        check_flags(self.change_flags)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "INFO change");
        let form_flags = read_form_flags(&mut cursor, self.change_flags)?;
        finish(&cursor)?;
        let said_once = self.change_flags & STATE != 0;
        Ok(Some(InfoChange {
            form_flags,
            said_once,
        }))
    }

    /// Decode an SMQN change. Returns `None` for other record types.
    ///
    /// # Errors
    /// Rejects unknown change bits, truncated fields, impossible counts and
    /// trailing bytes.
    pub fn story_manager_quest_node_change(&self) -> Result<Option<StoryManagerQuestNodeChange>> {
        if self.record_type() != "SMQN" {
            return Ok(None);
        }
        check_flags(self.change_flags)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "SMQN change");
        let form_flags = read_form_flags(&mut cursor, self.change_flags)?;
        let last_run_times = if self.change_flags & STATE != 0 {
            let count = cursor.u32()? as usize;
            if count > cursor.remaining() / 4 {
                return Err(invalid("last-run count exceeds remaining payload"));
            }
            let mut times = Vec::with_capacity(count);
            for _ in 0..count {
                times.push(cursor.u32()?);
            }
            Some(times)
        } else {
            None
        };
        finish(&cursor)?;
        Ok(Some(StoryManagerQuestNodeChange {
            form_flags,
            last_run_times,
        }))
    }

    /// Decode a SCEN change. Returns `None` for other record types.
    ///
    /// # Errors
    /// Rejects unknown change bits, truncated form flags and extra bytes when
    /// the active state flag is absent. The active data itself remains opaque.
    pub fn scene_change(&self) -> Result<Option<SceneChange>> {
        if self.record_type() != "SCEN" {
            return Ok(None);
        }
        check_flags(self.change_flags)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "SCEN change");
        let form_flags = read_form_flags(&mut cursor, self.change_flags)?;
        let active = self.change_flags & STATE != 0;
        let active_data = if active {
            cursor.rest().to_vec()
        } else {
            finish(&cursor)?;
            Vec::new()
        };
        Ok(Some(SceneChange {
            form_flags,
            active,
            active_data,
        }))
    }
}

impl InfoChange {
    /// Rebuild the change flag word from the present sections and state marker.
    pub fn change_flags(self) -> u32 {
        flags(self.form_flags, self.said_once)
    }

    /// Rebuild the typed payload. `Save::write` retains the original payload.
    pub fn data_bytes(self) -> Vec<u8> {
        bytes(self.form_flags)
    }
}

impl StoryManagerQuestNodeChange {
    /// Rebuild the change flag word from the present sections and state marker.
    pub fn change_flags(&self) -> u32 {
        flags(self.form_flags, self.last_run_times.is_some())
    }

    /// Rebuild the typed payload. `Save::write` retains the original payload.
    ///
    /// # Errors
    /// Rejects a time count exceeding the saved 32-bit count field.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let mut out = bytes(self.form_flags);
        if let Some(times) = &self.last_run_times {
            let count =
                u32::try_from(times.len()).map_err(|_| invalid("too many last-run times"))?;
            out.extend(count.to_le_bytes());
            for time in times {
                out.extend(time.to_le_bytes());
            }
        }
        Ok(out)
    }
}

impl SceneChange {
    /// Rebuild the change flag word from the present sections and state marker.
    pub fn change_flags(&self) -> u32 {
        flags(self.form_flags, self.active)
    }

    /// Rebuild the typed payload. `Save::write` retains the original payload.
    pub fn data_bytes(&self) -> Vec<u8> {
        let mut out = bytes(self.form_flags);
        if self.active {
            out.extend(&self.active_data);
        }
        out
    }
}

fn check_flags(flags: u32) -> Result<()> {
    if flags & !(FORM_FLAGS | STATE) != 0 {
        return Err(invalid("unknown change flags"));
    }
    Ok(())
}

fn read_form_flags(cursor: &mut Cursor<'_>, flags: u32) -> Result<Option<FormFlags>> {
    let form_flags = if flags & FORM_FLAGS != 0 {
        Some(FormFlags {
            flags: cursor.u32()?,
            unknown: cursor.u16()?,
        })
    } else {
        None
    };
    Ok(form_flags)
}

fn finish(cursor: &Cursor<'_>) -> Result<()> {
    if !cursor.is_empty() {
        return Err(invalid("trailing change bytes"));
    }
    Ok(())
}

fn flags(form_flags: Option<FormFlags>, state: bool) -> u32 {
    u32::from(form_flags.is_some()) | if state { STATE } else { 0 }
}

fn bytes(form_flags: Option<FormFlags>) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(form_flags) = form_flags {
        out.extend(form_flags.flags.to_le_bytes());
        out.extend(form_flags.unknown.to_le_bytes());
    }
    out
}

fn invalid(reason: &str) -> Error {
    Error::DialogueScene {
        reason: reason.to_owned(),
    }
}
