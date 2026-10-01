// SPDX-License-Identifier: GPL-3.0-or-later

//! Every named save section, one list per level. A row that adds saved
//! state appends one entry to the list for the level that owns the
//! state, and nothing else lives in this file: keep it that way so two
//! branches adding different entries merge cleanly (`.gitattributes`
//! marks this path `merge=union`). Keep each entry on its own single
//! line between the brackets, for example
//! `Section { name: "MAG-39", write: magic::write_summons, read: magic::read_summons },`.
//! The lists skip rustfmt so it never splits an entry across lines.

use super::save::FormReader;
use super::save_sections::Section;
use super::{ActorState, RefState, World};
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Writer};
use soe_papyrus::VmError;

/// Sections read and written once, at the end of [`World::save_state`]
/// and [`World::load_state`].
#[rustfmt::skip]
pub(crate) const WORLD_SECTIONS: &[Section<World>] = &[
    Section { name: "ANM-58", write: write_animation_waits, read: read_animation_waits },
    Section { name: "PRG-01.player-level", write: write_player_level, read: read_player_level },
    Section { name: "SAV-53", write: write_base_object_section, read: read_base_object_section },
    Section { name: "SAV-42", write: write_static_object_section, read: read_static_object_section },
    Section { name: "SAV-44", write: write_appa_idlm_section, read: read_appa_idlm_section },
    Section { name: "SAV-50", write: write_woop_mgef_section, read: read_woop_mgef_section },
    Section { name: "AI-16", write: super::save::write_script_calls, read: super::save::read_script_calls },
    Section { name: "SAV-46", write: write_saved_form_flags, read: read_saved_form_flags },
    Section { name: "CRF-02", write: super::save::write_brewed_potions, read: super::save::read_brewed_potions },
    super::story_manager::SECTION,
    Section { name: "SAV-48", write: super::record_save::write_pack_navmesh_section, read: super::record_save::read_pack_navmesh_section },
    Section { name: "SAV-59", write: super::record_save::write_unresolved_records, read: super::record_save::read_unresolved_records },
    Section { name: "SAV-10", write: super::quest_save::write_lifecycle_tasks, read: super::quest_save::read_lifecycle_tasks },
    super::magic::SAVE_SECTION,
];

fn write_player_level(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.u32(world.player_level);
}

fn read_player_level(
    world: &mut World,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    world.player_level = reader.u32()?;
    Ok(())
}

fn write_character_progression(state: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::super::progression::write_character_progression(state, writer);
}

fn read_character_progression(
    state: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::super::progression::read_character_progression(state, reader)
}

fn write_days_in_jail(state: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::crime::write_days_in_jail(state, writer);
}

fn read_days_in_jail(
    state: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::crime::read_days_in_jail(state, reader)
}

fn write_base_object_section(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    World::write_base_object_section(world, writer);
}

fn read_base_object_section(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    World::read_base_object_section(world, reader)
}

fn write_static_object_section(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    World::write_static_object_section(world, writer);
}

fn read_static_object_section(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    World::read_static_object_section(world, reader)
}

fn write_appa_idlm_section(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    World::write_appa_idlm_section(world, writer);
}

fn read_appa_idlm_section(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    World::read_appa_idlm_section(world, reader)
}

fn write_woop_mgef_section(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    World::write_woop_mgef_section(world, writer);
}

fn read_woop_mgef_section(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    World::read_woop_mgef_section(world, reader)
}

/// Sections read and written once per actor, at the end of every actor
/// record.
#[rustfmt::skip]
pub(crate) const ACTOR_SECTIONS: &[Section<ActorState>] = &[
    Section { name: "CMB-38", write: write_ragdoll_override, read: read_ragdoll_override },
    super::furniture::SECTION,
    Section { name: "PRG-01", write: super::super::progression::write_skill_experience, read: super::super::progression::read_skill_experience },
    Section { name: "AI-21", write: super::save::write_driver_state_section, read: super::save::read_driver_state_section },
    Section { name: "AI-88", write: super::furniture::write_furniture_seat, read: super::furniture::read_furniture_seat },
    Section { name: "PRG-07", write: write_character_progression, read: read_character_progression },
    Section { name: "PRG-13", write: write_days_in_jail, read: read_days_in_jail },
    Section { name: "MOD-06", write: write_package_overrides, read: read_package_overrides },
    Section { name: "AI-20", write: write_sit_procedure, read: read_sit_procedure },
    Section { name: "AI-29", write: write_voice_recovery, read: read_voice_recovery },
    Section { name: "AI-36", write: write_use_weapon_procedure, read: read_use_weapon_procedure },
    Section { name: "UI-14", write: super::actors::write_ui_14, read: super::actors::read_ui_14 },
];

/// Sections read and written once per reference, at the end of every
/// reference record.
#[rustfmt::skip]
pub(crate) const REF_SECTIONS: &[Section<RefState>] = &[
    super::destruction::SECTION,
    Section { name: "HST-16", write: write_reference_translation, read: read_reference_translation },
    Section { name: "SAV-15", write: write_reference_persistence, read: read_reference_persistence },
    Section { name: "MOD-03", write: write_reference_flags, read: read_reference_flags },
];

fn write_animation_waits(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::events::write_animation_waits(world, writer);
}

fn read_animation_waits(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::events::read_animation_waits(world, reader)
}

fn write_ragdoll_override(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::knockback::write_ragdoll_override(actor, writer);
}

fn read_ragdoll_override(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::knockback::read_ragdoll_override(actor, reader)
}

fn write_reference_translation(state: &RefState, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::reference_translation::write_section(state, writer);
}

fn read_reference_translation(
    state: &mut RefState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::reference_translation::read_section(state, reader)
}

fn write_package_overrides(
    state: &ActorState,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    writer.len(state.package_overrides.len());
    for (index, package) in &state.package_overrides {
        writer.len(*index);
        super::save::write_form_id(writer, *package);
    }
}

fn read_package_overrides(
    state: &mut ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    let count = reader.len()?;
    if count > reader.remaining() / 8 {
        return Err(soe_papyrus::VmError::State(
            "MOD-06 package count exceeds remaining bytes".into(),
        ));
    }
    for _ in 0..count {
        let index = reader.len()?;
        let package = super::save::read_form_id(reader)?;
        state.package_overrides.push((index, package));
    }
    Ok(())
}

fn write_voice_recovery(
    state: &ActorState,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    writer.bool(state.voice_recovery_until.is_some());
    if let Some(until) = state.voice_recovery_until {
        writer.f64(until);
    }
}

fn read_voice_recovery(
    state: &mut ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    if reader.bool()? {
        let until = reader.f64()?;
        if !until.is_finite() || until < 0.0 {
            return Err(soe_papyrus::VmError::State(
                "AI-29 invalid voice recovery deadline".into(),
            ));
        }
        state.voice_recovery_until = Some(until);
    }
    Ok(())
}

fn write_sit_procedure(
    state: &ActorState,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    writer.bool(state.ai.procedure == crate::ai::Procedure::Sit);
}

fn read_sit_procedure(
    state: &mut ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    if reader.bool()? {
        state.ai.procedure = crate::ai::Procedure::Sit;
    }
    Ok(())
}

fn write_use_weapon_procedure(
    state: &ActorState,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    writer.bool(state.ai.procedure == crate::ai::Procedure::UseWeapon);
}

fn read_use_weapon_procedure(
    state: &mut ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    if reader.bool()? {
        state.ai.procedure = crate::ai::Procedure::UseWeapon;
    }
    Ok(())
}

fn write_reference_persistence(
    state: &RefState,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    writer.bool(state.persistent);
}

fn read_reference_persistence(
    state: &mut RefState,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    state.persistent = reader.bool()?;
    Ok(())
}

fn write_reference_flags(
    state: &RefState,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    writer.u32(state.record_flags);
}

fn read_reference_flags(
    state: &mut RefState,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    state.record_flags = reader.u32()?;
    Ok(())
}

fn write_saved_form_flags(
    world: &World,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
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
    _table: &soe_papyrus::vm::state::ArrayTable,
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
