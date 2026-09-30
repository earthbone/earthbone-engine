// SPDX-License-Identifier: GPL-3.0-or-later

//! Every named save section, one list per level. A row that adds saved
//! state appends one entry to the list for the level that owns the
//! state, and nothing else lives in this file: keep it that way so two
//! branches adding different entries merge cleanly (`.gitattributes`
//! marks this path `merge=union`). Keep each entry on its own single
//! line between the brackets, for example
//! `Section { name: "MAG-39", write: magic::write_summons, read: magic::read_summons },`.
//! The lists skip rustfmt so it never splits an entry across lines.

use super::save_sections::Section;
use super::{ActorState, RefState, World};

/// Sections read and written once, at the end of [`World::save_state`]
/// and [`World::load_state`].
#[rustfmt::skip]
pub(crate) const WORLD_SECTIONS: &[Section<World>] = &[
    Section { name: "SAV-46", write: write_saved_form_flags, read: read_saved_form_flags },
    Section { name: "CRF-02", write: super::save::write_brewed_potions, read: super::save::read_brewed_potions },
    super::story_manager::SECTION,
    Section { name: "SAV-48", write: super::record_save::write_pack_navmesh_section, read: super::record_save::read_pack_navmesh_section },
    Section { name: "SAV-59", write: super::record_save::write_unresolved_records, read: super::record_save::read_unresolved_records },
    Section { name: "SAV-10", write: super::quest_save::write_lifecycle_tasks, read: super::quest_save::read_lifecycle_tasks },
];

/// Sections read and written once per actor, at the end of every actor
/// record.
#[rustfmt::skip]
pub(crate) const ACTOR_SECTIONS: &[Section<ActorState>] = &[
    super::furniture::SECTION,
    Section { name: "PRG-01", write: super::super::progression::write_skill_experience, read: super::super::progression::read_skill_experience },
    Section { name: "AI-88", write: super::furniture::write_furniture_seat, read: super::furniture::read_furniture_seat },
    Section { name: "UI-14", write: super::actors::write_ui_14, read: super::actors::read_ui_14 },
];

/// Sections read and written once per reference, at the end of every
/// reference record.
#[rustfmt::skip]
pub(crate) const REF_SECTIONS: &[Section<RefState>] = &[
    Section { name: "SAV-15", write: write_reference_persistence, read: read_reference_persistence },
    Section { name: "MOD-03", write: write_reference_flags, read: read_reference_flags },
    Section { name: "CMB-37", write: write_destruction_stage, read: read_destruction_stage },
];

fn write_destruction_stage(state: &RefState, writer: &mut soe_papyrus::vm::state::Writer) {
    writer.i32(state.destruction_stage);
}

fn read_destruction_stage(
    state: &mut RefState,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
    state.destruction_stage = reader.i32()?;
    Ok(())
}

fn write_reference_persistence(state: &RefState, writer: &mut soe_papyrus::vm::state::Writer) {
    writer.bool(state.persistent);
}

fn read_reference_persistence(
    state: &mut RefState,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
    state.persistent = reader.bool()?;
    Ok(())
}

fn write_reference_flags(state: &RefState, writer: &mut soe_papyrus::vm::state::Writer) {
    writer.u32(state.record_flags);
}

fn read_reference_flags(
    state: &mut RefState,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
    state.record_flags = reader.u32()?;
    Ok(())
}

fn write_saved_form_flags(world: &World, writer: &mut soe_papyrus::vm::state::Writer) {
    let saved: Vec<_> = world
        .record_overrides
        .iter()
        .filter_map(|(id, value)| value.saved_form_flags.map(|flags| (*id, flags)))
        .collect();
    writer.len(saved.len());
    for (id, (flags, unknown)) in saved {
        super::save::write_form_id(writer, id);
        writer.u32(flags);
        writer.u16(unknown);
    }
}

fn read_saved_form_flags(
    world: &mut World,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
    let count = reader.len()?;
    if count > reader.remaining() / 12 {
        return Err(soe_papyrus::VmError::State(
            "SAV-46 record count exceeds remaining bytes".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..count {
        let id = super::save::read_form_id(reader)?;
        if !seen.insert(id) {
            return Err(soe_papyrus::VmError::State(
                "SAV-46 duplicate record override".into(),
            ));
        }
        let flags = reader.u32()?;
        let unknown = reader.u16()?;
        let value = world
            .record_overrides
            .get_mut(&id)
            .ok_or_else(|| soe_papyrus::VmError::State("SAV-46 missing record override".into()))?;
        value.saved_form_flags = Some((flags, unknown));
        value.record.flags = flags;
    }
    Ok(())
}
