// SPDX-License-Identifier: GPL-3.0-or-later

//! DEST damage and stage selection from TES5Edit's DEST/DSTD layout and the
//! Creation Kit ObjectReference destruction natives.

use soe_esm::GlobalFormId;
use soe_papyrus::{Value, VmError};

use super::{save_sections::Section, Alloc, Handle, Pending, RefState, World, T};

#[derive(Clone)]
struct Stage {
    health_percent: u8,
    model_index: u8,
    flags: u8,
    model: Option<String>,
}

fn destruction_data(world: &World, reference: GlobalFormId) -> Option<(f32, Vec<Stage>)> {
    let base = world.reference_ref(reference)?.base?;
    let record = world.record(base)?;
    let dest = record.find(T(b"DEST"))?;
    if dest.len() < 8 {
        return None;
    }
    let health = i32::from_le_bytes(dest[..4].try_into().ok()?);
    if health <= 0 {
        return None;
    }
    let mut stages = Vec::new();
    for subrecord in &record.subrecords {
        match subrecord.tag {
            tag if tag == T(b"DSTD") => {
                if subrecord.data.len() != 20 {
                    return None;
                }
                stages.push(Stage {
                    health_percent: subrecord.data[0],
                    model_index: subrecord.data[2],
                    flags: subrecord.data[3],
                    model: None,
                });
            }
            tag if tag == T(b"DMDL") => {
                let path = subrecord
                    .data
                    .split(|byte| *byte == 0)
                    .next()
                    .filter(|path| !path.is_empty())
                    .map(|path| String::from_utf8_lossy(path).into_owned());
                if let Some(stage) = stages.last_mut() {
                    stage.model = path;
                }
            }
            _ => {}
        }
    }
    if stages.len() != usize::from(dest[4]) {
        return None;
    }
    Some((health as f32, stages))
}

impl World {
    /// Current destruction stage, or `-1` before damage.
    #[must_use]
    pub fn destruction_stage(&mut self, reference: GlobalFormId) -> i32 {
        self.reference(reference)
            .map_or(-1, |state| state.destruction_stage)
    }

    /// Model path selected by the reference's current DEST stage, when present.
    #[must_use]
    pub fn destruction_model(&self, reference: GlobalFormId) -> Option<String> {
        let reference_state = self.reference_ref(reference)?;
        let (_, stages) = destruction_data(self, reference)?;
        let stage_number = usize::try_from(reference_state.destruction_stage).ok()?;
        let stage = stages.get(stage_number)?;
        if stage.model_index == 0 {
            return None;
        }
        stage.model.clone()
    }

    /// Apply external damage and queue one stage-change event per crossed stage.
    pub fn damage_object(&mut self, reference: GlobalFormId, damage: f32, alloc: Alloc<'_>) {
        if !damage.is_finite() || damage <= 0.0 {
            return;
        }
        let Some((maximum, stages)) = destruction_data(self, reference) else {
            return;
        };
        let Some(state) = self.reference(reference) else {
            return;
        };
        let health = state.destruction_health.unwrap_or(maximum);
        let current_stage = state.destruction_stage;
        let Some(next) = stages.get(usize::try_from(current_stage + 1).unwrap_or(usize::MAX))
        else {
            return;
        };
        if next.flags & 0b1000 != 0 {
            return;
        }
        let mut remaining = damage;
        if next.flags & 0b0001 != 0 {
            let threshold = maximum * f32::from(next.health_percent) / 100.0;
            remaining = remaining.min((health - threshold).max(0.0));
        }
        let mut new_health = (health - remaining).max(0.0);
        let mut crossed = Vec::new();
        let mut stage_index = current_stage;
        while let Some(stage) = stages.get(usize::try_from(stage_index + 1).unwrap_or(usize::MAX)) {
            let threshold = maximum * f32::from(stage.health_percent) / 100.0;
            if new_health > threshold {
                break;
            }
            stage_index += 1;
            crossed.push((stage_index, stage.flags));
        }
        if next.flags & 0b0001 != 0 && !crossed.is_empty() {
            new_health = maximum * f32::from(next.health_percent) / 100.0;
        }
        state.destruction_health = Some(new_health);
        state.destruction_stage = stage_index;
        for (_, flags) in &crossed {
            if flags & 0b0010 != 0 {
                state.enabled = false;
            }
            if flags & 0b0100 != 0 {
                state.destroyed = true;
            }
        }
        if crossed.is_empty() {
            return;
        }
        let object = self.object_of(Handle::Form(reference), alloc);
        for (stage, _) in crossed {
            self.pending.push(Pending::Event {
                object,
                name: "OnDestructionStageChanged".into(),
                args: vec![Value::Int(stage)],
            });
        }
    }

    /// Restore the authored model and health, reporting the reset stage.
    pub fn clear_destruction(&mut self, reference: GlobalFormId, alloc: Alloc<'_>) {
        let Some((maximum, _)) = destruction_data(self, reference) else {
            return;
        };
        let Some(state) = self.reference(reference) else {
            return;
        };
        let previous = state.destruction_stage;
        state.destruction_health = Some(maximum);
        state.destruction_stage = -1;
        state.destroyed = false;
        state.enabled = true;
        if previous >= 0 {
            let object = self.object_of(Handle::Form(reference), alloc);
            self.pending.push(Pending::Event {
                object,
                name: "OnDestructionStageChanged".into(),
                args: vec![Value::Int(-1)],
            });
        }
    }
}

pub(super) fn write(value: &RefState, writer: &mut soe_papyrus::vm::state::Writer) {
    writer.bool(value.destruction_health.is_some());
    if let Some(health) = value.destruction_health {
        writer.f32(health);
    }
    writer.i32(value.destruction_stage);
}

pub(super) fn read(
    value: &mut RefState,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), VmError> {
    let health = if reader.bool()? {
        let health = reader.f32()?;
        if !health.is_finite() || health < 0.0 {
            return Err(VmError::State("invalid CMB-37 destruction health".into()));
        }
        Some(health)
    } else {
        None
    };
    let stage = reader.i32()?;
    if stage < -1 {
        return Err(VmError::State("invalid CMB-37 destruction stage".into()));
    }
    value.destruction_health = health;
    value.destruction_stage = stage;
    Ok(())
}

pub(super) const SECTION: Section<RefState> = Section {
    name: "CMB-37",
    write,
    read,
};
