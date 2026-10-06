// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved INFO, SMQN and SCEN changes. xEdit's `wbDefinitionsTES5Saves.pas`
//! defines bit zero as six bytes of form flags and bit 31 as a state marker.
//! UESP supplies the change type indices. FallrimTools keeps these three
//! change types as opaque data. The external save corpus shows additional
//! SMQN and SCEN data behind bit 31 that xEdit does not describe.
//! SCEN widths follow all 807 corpus payloads. Action discriminators also
//! follow CommonLibSSE-NG's `BGSSceneAction.h` and xEdit's plugin SCEN schema.

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
    /// Exact scene runtime bytes, validated by [`SceneRuntime::read`].
    pub active_data: Vec<u8>,
}

/// Action discriminators shared by the plugin SCEN schema and saved runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneActionKind {
    /// Saved discriminator zero.
    Dialogue,
    /// Saved discriminator one.
    Package,
    /// Saved discriminator two, carrying two additional bytes.
    Timer,
}

/// One counted SCEN runtime action, retaining uninterpreted state bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneActionState {
    /// The discriminator that selects the action's serialized width.
    pub kind: SceneActionKind,
    /// The action's floating-point value as exact bits.
    pub time_bits: u32,
    /// Two state bytes present for every action type.
    pub flags: [u8; 2],
    /// Two additional state bytes, present only for timer actions.
    pub timer_flags: Option<[u8; 2]>,
}

/// SCEN bit-31 runtime layout observed across the external save corpus.
///
/// Counts and action types establish all byte boundaries. Flag byte meanings
/// remain unassigned, and no flag byte gates another field in this layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneRuntime {
    /// Leading byte, observed as one in every corpus payload.
    pub marker: u8,
    /// Saved phase index, including the `u32::MAX` sentinel.
    pub phase: u32,
    /// Two scene state bytes preceding the action count.
    pub flags: [u8; 2],
    /// Actions in their serialized order.
    pub actions: Vec<SceneActionState>,
    /// Two scene state bytes following the actions.
    pub suffix_flags: [u8; 2],
    /// The scene's floating-point value as exact bits.
    pub time_bits: u32,
    /// Three state bytes per counted phase, retained without interpretation.
    pub phases: Vec<[u8; 3]>,
    /// Final four-byte value, observed as zero or one in the corpus.
    pub trailing: u32,
}

impl SceneRuntime {
    /// Decode the complete runtime payload without the optional form flags.
    ///
    /// # Errors
    /// Rejects truncation, impossible counts, unknown action discriminators
    /// and bytes after the final field.
    pub fn read(data: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(data, "SCEN active data");
        let marker = cursor.u8()?;
        let phase = cursor.u32()?;
        let flags = [cursor.u8()?, cursor.u8()?];
        let count = cursor.u32()? as usize;
        if count > cursor.remaining().saturating_sub(14) / 10 {
            return Err(invalid("scene action count exceeds remaining payload"));
        }
        let mut actions = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = match cursor.u32()? {
                0 => SceneActionKind::Dialogue,
                1 => SceneActionKind::Package,
                2 => SceneActionKind::Timer,
                _ => return Err(invalid("unknown scene action discriminator")),
            };
            let time_bits = cursor.u32()?;
            let flags = [cursor.u8()?, cursor.u8()?];
            let timer_flags = if kind == SceneActionKind::Timer {
                Some([cursor.u8()?, cursor.u8()?])
            } else {
                None
            };
            actions.push(SceneActionState {
                kind,
                time_bits,
                flags,
                timer_flags,
            });
        }
        let suffix_flags = [cursor.u8()?, cursor.u8()?];
        let time_bits = cursor.u32()?;
        let count = cursor.u32()? as usize;
        if count > cursor.remaining().saturating_sub(4) / 3 {
            return Err(invalid("scene phase count exceeds remaining payload"));
        }
        let mut phases = Vec::with_capacity(count);
        for _ in 0..count {
            phases.push([cursor.u8()?, cursor.u8()?, cursor.u8()?]);
        }
        let trailing = cursor.u32()?;
        finish(&cursor)?;
        Ok(Self {
            marker,
            phase,
            flags,
            actions,
            suffix_flags,
            time_bits,
            phases,
            trailing,
        })
    }

    /// Rebuild the runtime bytes from typed fields and counted arrays.
    ///
    /// # Errors
    /// Rejects oversized counts and timer fields on the wrong action type.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let actions =
            u32::try_from(self.actions.len()).map_err(|_| invalid("too many scene actions"))?;
        let phases =
            u32::try_from(self.phases.len()).map_err(|_| invalid("too many scene phases"))?;
        let mut out = vec![self.marker];
        out.extend(self.phase.to_le_bytes());
        out.extend(self.flags);
        out.extend(actions.to_le_bytes());
        for action in &self.actions {
            let kind: u32 = match action.kind {
                SceneActionKind::Dialogue => 0,
                SceneActionKind::Package => 1,
                SceneActionKind::Timer => 2,
            };
            if (action.kind == SceneActionKind::Timer) != action.timer_flags.is_some() {
                return Err(invalid("scene timer fields disagree with action type"));
            }
            out.extend(kind.to_le_bytes());
            out.extend(action.time_bits.to_le_bytes());
            out.extend(action.flags);
            if let Some(flags) = action.timer_flags {
                out.extend(flags);
            }
        }
        out.extend(self.suffix_flags);
        out.extend(self.time_bits.to_le_bytes());
        out.extend(phases.to_le_bytes());
        for phase in &self.phases {
            out.extend(phase);
        }
        out.extend(self.trailing.to_le_bytes());
        Ok(out)
    }
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
    /// Rejects unknown change bits, truncated fields, invalid runtime counts
    /// or action types and trailing bytes.
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
            let data = cursor.rest();
            SceneRuntime::read(data)?;
            data.to_vec()
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
    /// Decode the runtime fields when the bit-31 marker is present.
    ///
    /// # Errors
    /// Rejects malformed runtime bytes or bytes without the state marker.
    pub fn runtime_data(&self) -> Result<Option<SceneRuntime>> {
        if self.active {
            SceneRuntime::read(&self.active_data).map(Some)
        } else if self.active_data.is_empty() {
            Ok(None)
        } else {
            Err(invalid("inactive scene has active data"))
        }
    }

    /// Rebuild the change flag word from the present sections and state marker.
    pub fn change_flags(&self) -> u32 {
        flags(self.form_flags, self.active)
    }

    /// Rebuild the typed payload. `Save::write` retains the original payload.
    ///
    /// # Errors
    /// Rejects malformed runtime bytes or bytes without the state marker.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        self.runtime_data()?;
        let mut out = bytes(self.form_flags);
        if self.active {
            out.extend(&self.active_data);
        }
        Ok(out)
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
