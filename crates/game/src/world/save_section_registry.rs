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

fn write_spatial_payloads(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::spatial_restore::write_spatial_payloads(world, writer);
}

fn read_spatial_payloads(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::spatial_restore::read_spatial_payloads(world, reader)
}

fn read_projectile_section(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::record_save::read_projectile_section(world, reader)
}

fn write_scr_14_actor(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.bool(actor.keep_offset.is_some());
    if let Some(offset) = actor.keep_offset {
        super::save::write_form_id(writer, offset.target);
        for value in offset.offset.into_iter().chain(offset.angles) {
            writer.f32(value);
        }
        writer.f32(offset.catch_up_radius);
        writer.f32(offset.follow_radius);
    }
    super::save::write_opt_form_id(writer, actor.forced_landing_marker);
}

fn read_scr_14_actor(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    actor.keep_offset = if reader.bool()? {
        let target = super::save::read_form_id(reader)?;
        let offset = [reader.f32()?, reader.f32()?, reader.f32()?];
        let angles = [reader.f32()?, reader.f32()?, reader.f32()?];
        let catch_up_radius = reader.f32()?;
        let follow_radius = reader.f32()?;
        Some(super::ActorKeepOffset {
            target,
            offset,
            angles,
            catch_up_radius,
            follow_radius,
        })
    } else {
        None
    };
    actor.forced_landing_marker = super::save::read_opt_form_id(reader)?;
    Ok(())
}

fn write_scr_15_actor(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::save::write_opt_form_id(writer, actor.other_spell);
    super::save::write_opt_form_id(writer, actor.instant_spell);
}

fn read_scr_15_actor(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    actor.other_spell = super::save::read_opt_form_id(reader)?;
    actor.instant_spell = super::save::read_opt_form_id(reader)?;
    Ok(())
}

fn write_scr_74_actor(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.len(actor.prevent_equipment_removal.len());
    for form in &actor.prevent_equipment_removal {
        super::save::write_form_id(writer, *form);
    }
}

fn read_scr_74_actor(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let count = reader.len()?;
    for _ in 0..count {
        actor
            .prevent_equipment_removal
            .insert(super::save::read_form_id(reader)?);
    }
    Ok(())
}

/// Sections read and written once, at the end of [`World::save_state`]
/// and [`World::load_state`].
#[rustfmt::skip]
pub(crate) const WORLD_SECTIONS: &[Section<World>] = &[
    Section { name: "SAV-28", write: write_spatial_payloads, read: read_spatial_payloads },
    Section { name: "ANM-58", write: write_animation_waits, read: read_animation_waits },
    Section { name: "CRF-03", write: super::save::write_learned_enchantment_effects, read: super::save::read_learned_enchantment_effects },
    Section { name: "MAG-67", write: write_vampire_lord_state, read: read_vampire_lord_state },
    Section { name: "AI-04", write: write_empty_section, read: read_pending_script_calls },
    Section { name: "PRG-01.player-level", write: write_player_level, read: read_player_level },
    Section { name: "SAV-17", write: write_created_bases, read: read_created_bases },
    Section { name: "SAV-53", write: write_base_object_section, read: read_base_object_section },
    Section { name: "SAV-42", write: write_static_object_section, read: read_static_object_section },
    Section { name: "SAV-44", write: write_appa_idlm_section, read: read_appa_idlm_section },
    Section { name: "SAV-50", write: write_woop_mgef_section, read: read_woop_mgef_section },
    Section { name: "AI-16", write: super::save::write_script_calls, read: super::save::read_script_calls },
    Section { name: "SAV-46", write: write_saved_form_flags, read: read_saved_form_flags },
    Section { name: "SAV-48", write: super::record_save::write_pack_navmesh_section, read: super::record_save::read_pack_navmesh_section },
    super::magic::SAVE_SECTION,
    Section { name: "CRF-02", write: super::save::write_brewed_potions, read: super::save::read_brewed_potions },
    super::story_manager::SECTION,
    Section { name: "SAV-32", write: super::record_save::write_activator_section, read: super::record_save::read_activator_section },
    Section { name: "SAV-34", write: super::record_save::write_item_section, read: super::record_save::read_item_section },
    Section { name: "SAV-36", write: super::record_save::write_book_note_key_section, read: super::record_save::read_book_note_key_section },
    Section { name: "SAV-24", write: super::record_save::write_projectile_section, read: read_projectile_section },
    Section { name: "SAV-58", write: write_empty_section, read: super::record_save::read_unresolved_records },
    Section { name: "SAV-59", write: super::record_save::write_unresolved_records, read: super::record_save::read_unresolved_records },
    Section { name: "SAV-10", write: super::quest_save::write_lifecycle_tasks, read: super::quest_save::read_lifecycle_tasks_section },
    Section { name: "TST-01", write: write_rng_streams, read: read_rng_streams },
    super::magic::REANIMATE_SECTION,
    Section { name: "DLG-09", write: write_scene_runtime, read: read_scene_runtime },
    Section { name: "DLG-16", write: write_training_sessions, read: read_training_sessions },
    Section { name: "MAG-56", write: write_cloaks_section, read: read_cloaks_section },
    Section { name: "DLG-17", write: write_follower_services, read: read_follower_services },
    Section { name: "DLG-19", write: write_scene_action_progress, read: read_scene_action_progress },
    Section {
        name: "SCR-19",
        write: World::write_pending_cell_resets,
        read: World::read_pending_cell_resets,
    },
    Section { name: "SCR-48", write: crate::dialogue::write_known_topics, read: crate::dialogue::read_known_topics },
    Section { name: "SCR-24", write: |world, writer, _pool| world.write_effect_shader_section(writer), read: |world, reader, _table| world.read_effect_shader_section(reader) },
    Section { name: "SCR-60", write: write_scr_60_game_flags, read: read_scr_60_game_flags },
    Section { name: "AI-80", write: write_faction_attacks, read: read_faction_attacks },
    Section { name: "MAG-64", write: write_reference_bindings, read: read_reference_bindings },
    Section { name: "SCR-01", write: World::write_reference_events, read: World::read_reference_events },
    super::magic::GRANTED_PERK_SECTION,
    super::image_space::SECTION,
    Section { name: "SCR-27", write: write_sun_gaze_modifier, read: read_sun_gaze_modifier },
    super::magic::TAPER_SECTION,
    Section { name: "RND-18", write: super::sky::write_sky, read: super::sky::read_sky },
];
fn write_empty_section(_world: &World, _writer: &mut Writer, _pool: &mut ArrayPool) {}

fn write_sun_gaze_modifier(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    world.write_sun_gaze_modifier(writer);
}

fn read_sun_gaze_modifier(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    world.read_sun_gaze_modifier(reader)
}

fn read_pending_script_calls(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::save::read_pending_script_calls(world, reader)
}

fn write_training_sessions(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::training::write_training_sessions(world, writer);
}

fn write_scr_60_game_flags(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    for enabled in world.player_control_flags {
        writer.bool(enabled);
    }
    writer.i32(world.player_control_pov_type);
    writer.bool(world.chargen_disable_saving);
    writer.bool(world.chargen_disable_waiting);
    writer.bool(world.chargen_show_controls_disabled_message);
}

fn read_scr_60_game_flags(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    for enabled in &mut world.player_control_flags {
        *enabled = reader.bool()?;
    }
    world.player_control_pov_type = reader.i32()?;
    world.chargen_disable_saving = reader.bool()?;
    world.chargen_disable_waiting = reader.bool()?;
    world.chargen_show_controls_disabled_message = reader.bool()?;
    world.player_controls = world.player_control_flags.iter().all(|enabled| *enabled);
    Ok(())
}

fn read_training_sessions(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::training::read_training_sessions(world, reader)
}

fn write_expression_override(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    World::write_expression_override(actor, writer);
}

fn read_expression_override(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    World::read_expression_override(actor, reader)
}

fn write_scr_12(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    World::write_scr_12(actor, writer);
}

fn read_scr_12(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    World::read_scr_12(actor, reader)
}

fn write_scr_11(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    World::write_scr_11(actor, writer);
}

fn read_scr_11(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    World::read_scr_11(actor, reader)
}

fn write_character_progression(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::super::progression::write_character_progression(actor, writer);
}

fn read_character_progression(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::super::progression::read_character_progression(actor, reader)
}

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

fn write_created_bases(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::reference_restore::write_created_base_section(world, writer);
}

fn read_created_bases(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::reference_restore::read_created_base_section(world, reader)
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

fn write_cloaks_section(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    World::write_cloaks(world, writer);
}

fn read_cloaks_section(
    world: &mut World,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    World::read_cloaks(world, reader)
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
    Section { name: "SCR-14", write: write_scr_14_actor, read: read_scr_14_actor },
    Section { name: "SCR-15", write: write_scr_15_actor, read: read_scr_15_actor },
    Section { name: "CMB-38", write: write_ragdoll_override, read: read_ragdoll_override },
    Section { name: "MAG-57", write: write_werewolf_transform, read: read_werewolf_transform },
    Section { name: "MAG-54", write: write_stagger_metadata, read: read_stagger_metadata },
    super::furniture::SECTION,
    Section { name: "ANM-59", write: write_expression_override, read: read_expression_override },
    Section { name: "PRG-01", write: super::super::progression::write_skill_experience, read: super::super::progression::read_skill_experience },
    Section { name: "AI-21", write: super::save::write_driver_state_section, read: super::save::read_driver_state_section },
    Section { name: "AI-88", write: super::furniture::write_furniture_seat, read: super::furniture::read_furniture_seat },
    Section { name: "AI-60", write: write_voice_recovery, read: read_voice_recovery },
    Section { name: "PRG-07", write: write_character_progression, read: read_character_progression },
    Section { name: "AI-67", write: write_being_ridden, read: read_being_ridden },
    Section { name: "MOD-06", write: write_package_overrides, read: read_package_overrides },
    Section { name: "AI-20", write: write_sit_procedure, read: read_sit_procedure },
    Section { name: "AI-29", write: write_voice_recovery, read: read_voice_recovery },
    Section { name: "AI-36", write: write_use_weapon_procedure, read: read_use_weapon_procedure },
    Section { name: "UI-14", write: super::actors::write_ui_14, read: super::actors::read_ui_14 },
    Section { name: "PRG-13", write: super::crime::write_days_in_jail, read: super::crime::read_days_in_jail },
    Section { name: "AI-43", write: write_trespassing, read: read_trespassing },
    Section { name: "DLG-14", write: write_force_greet, read: read_force_greet },
    Section { name: "DLG-17.detail", write: write_can_do_favors, read: read_can_do_favors },
    Section { name: "DLG-19.actor", write: write_scene_package, read: read_scene_package },
    Section { name: "SCR-12", write: write_scr_12, read: read_scr_12 },
    Section { name: "SCR-11", write: write_scr_11, read: read_scr_11 },
    Section { name: "AI-76", write: write_known_magic_effects, read: read_known_magic_effects },
    Section { name: "AI-64", write: write_movement_flags, read: read_movement_flags },
    Section { name: "AI-15", write: write_package_tree, read: read_package_tree },
    Section { name: "SHM-13", write: write_favorites, read: read_favorites },
    Section { name: "PLY-06", write: super::package_overrides::write_overrides, read: super::package_overrides::read_overrides },
    Section { name: "SCR-74", write: write_scr_74_actor, read: read_scr_74_actor },
];

fn write_favorites(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.len(actor.favorites.len());
    for form in &actor.favorites {
        super::save::write_form_id(writer, *form);
    }
}

fn read_favorites(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let count = reader.len()?;
    if count > reader.remaining() / 6 {
        return Err(VmError::State(
            "SHM-13 favorite count exceeds remaining bytes".into(),
        ));
    }
    let mut favorites = std::collections::BTreeSet::new();
    for _ in 0..count {
        if !favorites.insert(super::save::read_form_id(reader)?) {
            return Err(VmError::State("SHM-13 duplicate favorite form".into()));
        }
    }
    actor.favorites = favorites;
    Ok(())
}

fn write_stagger_metadata(
    state: &ActorState,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    writer.f64(state.stagger_magnitude);
    super::save::write_opt_form_id(writer, state.stagger_direction);
}

fn read_stagger_metadata(
    state: &mut ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    let magnitude = reader.f64()?;
    if !magnitude.is_finite() || magnitude < 0.0 {
        return Err(soe_papyrus::VmError::State(
            "invalid stagger magnitude".into(),
        ));
    }
    state.stagger_magnitude = magnitude;
    state.stagger_direction = super::save::read_opt_form_id(reader)?;
    Ok(())
}

fn write_being_ridden(state: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.bool(state.being_ridden);
}

fn read_being_ridden(
    state: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    state.being_ridden = reader.bool()?;
    Ok(())
}

/// Sections read and written once per reference, at the end of every
/// reference record.
#[rustfmt::skip]
pub(crate) const REF_SECTIONS: &[Section<RefState>] = &[
    super::destruction::SECTION,
    Section { name: "HST-16", write: write_reference_translation, read: read_reference_translation },
    Section { name: "SAV-15", write: write_reference_persistence, read: read_reference_persistence },
    Section { name: "MOD-03", write: write_reference_flags, read: read_reference_flags },
    super::scr_40::SECTION,
];

fn write_vampire_lord_state(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::magic::write_vampire_lord_state(world, writer);
}

fn read_vampire_lord_state(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::magic::read_vampire_lord_state(world, reader)
}

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

fn write_werewolf_transform(state: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::magic::write_werewolf_transform(state, writer);
}

fn read_werewolf_transform(
    state: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    super::magic::read_werewolf_transform(state, reader)
}

fn write_trespassing(state: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.bool(state.trespassing);
}

fn read_trespassing(
    state: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    state.trespassing = reader.bool()?;
    Ok(())
}

fn write_known_magic_effects(state: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.len(state.known_forms.len());
    for form in &state.known_forms {
        super::save::write_form_id(writer, *form);
    }
}

fn read_known_magic_effects(
    state: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let count = reader.len()?;
    if count > reader.remaining() / 6 {
        return Err(VmError::State(
            "AI-76 known form count exceeds remaining bytes".into(),
        ));
    }
    for _ in 0..count {
        state.known_forms.insert(super::save::read_form_id(reader)?);
    }
    Ok(())
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
                "invalid voice recovery deadline".into(),
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

fn write_follower_services(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.len(world.follower_services.len());
    for (actor, service) in &world.follower_services {
        super::save::write_form_id(writer, *actor);
        super::save::write_form_id(writer, service.package);
        super::save::write_form_id(writer, service.faction);
        writer.bool(service.previous_faction_rank.is_some());
        if let Some(rank) = service.previous_faction_rank {
            writer.i32(rank);
        }
        writer.bool(service.previous_teammate);
        writer.bool(service.previous_can_do_favors);
    }
}

fn read_follower_services(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let count = reader.len()?;
    let mut services = std::collections::BTreeMap::new();
    for _ in 0..count {
        let actor = super::save::read_form_id(reader)?;
        let package = super::save::read_form_id(reader)?;
        let faction = super::save::read_form_id(reader)?;
        let previous_faction_rank = if reader.bool()? {
            Some(reader.i32()?)
        } else {
            None
        };
        let previous_teammate = reader.bool()?;
        let previous_can_do_favors = reader.bool()?;
        let service = crate::services::FollowerServiceState {
            package,
            faction,
            previous_faction_rank,
            previous_teammate,
            previous_can_do_favors,
        };
        if world
            .record(package)
            .is_none_or(|record| record.tag != super::T(b"PACK"))
            || world
                .record(faction)
                .is_none_or(|record| record.tag != super::T(b"FACT"))
            || services.insert(actor, service).is_some()
        {
            return Err(VmError::State("invalid follower service state".into()));
        }
    }
    world.follower_services = services;
    Ok(())
}

fn write_scene_action_progress(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    let saved: Vec<_> = world
        .scene_runs
        .iter()
        .filter(|(_, run)| {
            !run.started_actions.is_empty()
                || !run.timer_deadlines.is_empty()
                || !run.prior_packages.is_empty()
        })
        .collect();
    writer.len(saved.len());
    for (scene, run) in saved {
        super::save::write_form_id(writer, *scene);
        writer.len(run.started_actions.len());
        for slot in &run.started_actions {
            writer.u32(*slot);
        }
        writer.len(run.timer_deadlines.len());
        for (slot, deadline) in &run.timer_deadlines {
            writer.u32(*slot);
            writer.f64(*deadline);
        }
        writer.len(run.prior_packages.len());
        for (actor, package) in &run.prior_packages {
            super::save::write_form_id(writer, *actor);
            super::save::write_opt_form_id(writer, *package);
            let alias = run.prior_aliases.get(actor).copied().flatten();
            writer.bool(alias.is_some());
            if let Some((quest, alias)) = alias {
                super::save::write_form_id(writer, quest);
                writer.u32(alias);
            }
        }
    }
}

fn read_scene_action_progress(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let count = reader.len()?;
    let mut seen_scenes = std::collections::BTreeSet::new();
    for _ in 0..count {
        let scene_id = super::save::read_form_id(reader)?;
        if !seen_scenes.insert(scene_id) {
            return Err(VmError::State("DLG-19 duplicate scene progress".into()));
        }
        let scene_record = world
            .record(scene_id)
            .ok_or_else(|| VmError::State("DLG-19 scene progress names a missing scene".into()))?;
        let scene = soe_esm::records::gameplay::Scene::from_record(scene_record)
            .map_err(|error| VmError::State(format!("DLG-19 invalid scene: {error}")))?;
        let run = world
            .scene_runs
            .get_mut(&scene_id)
            .ok_or_else(|| VmError::State("DLG-19 scene progress has no scene run".into()))?;
        let started_count = reader.len()?;
        for _ in 0..started_count {
            let slot = reader.u32()?;
            if scene.actions.get(slot as usize).is_none() || !run.started_actions.insert(slot) {
                return Err(VmError::State(
                    "DLG-19 invalid or duplicate started scene action".into(),
                ));
            }
        }
        let timer_count = reader.len()?;
        for _ in 0..timer_count {
            let slot = reader.u32()?;
            let deadline = reader.f64()?;
            if !deadline.is_finite()
                || !run.started_actions.contains(&slot)
                || !scene.actions.get(slot as usize).is_some_and(|action| {
                    matches!(
                        action.data,
                        soe_esm::records::gameplay::SceneActionData::Timer { .. }
                    )
                })
                || run.timer_deadlines.insert(slot, deadline).is_some()
            {
                return Err(VmError::State("DLG-19 invalid scene timer deadline".into()));
            }
        }
        let package_count = reader.len()?;
        for _ in 0..package_count {
            let actor = super::save::read_form_id(reader)?;
            let package = super::save::read_opt_form_id(reader)?;
            let alias = if reader.bool()? {
                Some((super::save::read_form_id(reader)?, reader.u32()?))
            } else {
                None
            };
            if run.prior_packages.insert(actor, package).is_some() {
                return Err(VmError::State(
                    "DLG-19 duplicate prior scene package actor".into(),
                ));
            }
            run.prior_aliases.insert(actor, alias);
        }
    }
    Ok(())
}

fn write_scene_package(state: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    super::save::write_opt_form_id(writer, state.ai.scene_package);
}

fn read_scene_package(
    state: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    state.ai.scene_package = super::save::read_opt_form_id(reader)?;
    Ok(())
}

fn write_can_do_favors(state: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.bool(state.can_do_favors);
}

fn read_can_do_favors(
    state: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    state.can_do_favors = reader.bool()?;
    Ok(())
}

fn write_force_greet(state: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    if state.ai.procedure == crate::ai::Procedure::ForceGreet {
        writer.bool(true);
    }
}

fn read_force_greet(
    state: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    if reader.bool()? {
        state.ai.procedure = crate::ai::Procedure::ForceGreet;
    }
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

fn write_rng_streams(
    world: &World,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    for state in world.rng_streams {
        writer.u64(state);
    }
}

fn read_rng_streams(
    world: &mut World,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    for state in &mut world.rng_streams {
        *state = reader.u64()?;
    }
    Ok(())
}

fn write_scene_runtime(
    world: &World,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    writer.len(world.scene_runs.len());
    for (id, run) in &world.scene_runs {
        super::save::write_form_id(writer, *id);
        writer.u32(run.phase);
        writer.len(run.completed_actions.len());
        for slot in &run.completed_actions {
            writer.u32(*slot);
        }
    }
}

fn read_scene_runtime(
    world: &mut World,
    reader: &mut super::save::FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    use std::collections::{BTreeMap, BTreeSet};

    let count = reader.len()?;
    let mut runs = BTreeMap::new();
    for _ in 0..count {
        let id = super::save::read_form_id(reader)?;
        let phase = reader.u32()?;
        let action_count = reader.len()?;
        let mut completed_actions = BTreeSet::new();
        for _ in 0..action_count {
            if !completed_actions.insert(reader.u32()?) {
                return Err(soe_papyrus::VmError::State(
                    "DLG-09 duplicate completed scene action".into(),
                ));
            }
        }
        let scene_record = world.record(id).ok_or_else(|| {
            soe_papyrus::VmError::State("DLG-09 scene runtime state names a missing form".into())
        })?;
        let scene =
            soe_esm::records::gameplay::Scene::from_record(scene_record).map_err(|error| {
                soe_papyrus::VmError::State(format!("DLG-09 invalid scene: {error}"))
            })?;
        let valid_phase = if scene.phases.is_empty() {
            phase == 0 && completed_actions.is_empty()
        } else {
            (phase as usize) < scene.phases.len()
        };
        if !world.scenes.contains(&id)
            || !valid_phase
            || completed_actions.iter().any(|slot| {
                scene
                    .actions
                    .get(*slot as usize)
                    .is_none_or(|action| action.end_phase != Some(phase))
            })
            || runs
                .insert(
                    id,
                    super::SceneRunState {
                        phase,
                        completed_actions,
                        ..Default::default()
                    },
                )
                .is_some()
        {
            return Err(soe_papyrus::VmError::State(
                "DLG-09 invalid scene runtime state".into(),
            ));
        }
    }
    if runs.len() != world.scenes.len() || world.scenes.iter().any(|id| !runs.contains_key(id)) {
        return Err(soe_papyrus::VmError::State(
            "DLG-09 scene runtime state is incomplete".into(),
        ));
    }
    world.scene_runs = runs;
    Ok(())
}

fn write_faction_attacks(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    let attacked: std::collections::BTreeSet<_> = world
        .factions
        .iter()
        .filter_map(|(id, faction)| faction.player_attacked.then_some(*id))
        .collect();
    writer.len(attacked.len());
    for id in attacked {
        super::save::write_form_id(writer, id);
    }
}

fn read_faction_attacks(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let count = reader.len()?;
    if count > reader.remaining() / 6 {
        return Err(VmError::State(
            "AI-80 faction count exceeds remaining bytes".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..count {
        let id = super::save::read_form_id(reader)?;
        if !seen.insert(id) {
            return Err(VmError::State("AI-80 duplicate faction".into()));
        }
        let faction = world
            .factions
            .get_mut(&id)
            .ok_or_else(|| VmError::State("AI-80 missing faction".into()))?;
        faction.player_attacked = true;
    }
    Ok(())
}

fn write_movement_flags(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.bool(actor.swimming);
    writer.bool(actor.running);
    writer.bool(actor.riding_mount);
}

fn read_movement_flags(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    actor.swimming = reader.bool()?;
    actor.running = reader.bool()?;
    actor.riding_mount = reader.bool()?;
    Ok(())
}

fn write_reference_bindings(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    let bindings: Vec<_> = world
        .pending
        .iter()
        .filter(|pending| !matches!(pending, super::Pending::ReferenceEvent { .. }))
        .enumerate()
        .filter_map(|(index, pending)| match pending {
            super::Pending::BindReference { object, base } => Some((index, object, base)),
            _ => None,
        })
        .collect();
    writer.len(bindings.len());
    for (index, object, base) in bindings {
        writer.len(index);
        writer.u64(object.0);
        super::save::write_form_id(writer, *base);
    }
}

fn read_reference_bindings(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let count = reader.len()?;
    if count > reader.remaining() / 18 {
        return Err(VmError::State(
            "MAG-64 binding count exceeds remaining bytes".into(),
        ));
    }
    let mut previous = None;
    for _ in 0..count {
        let index = reader.len()?;
        let object = soe_papyrus::ObjectId(reader.u64()?);
        let base = super::save::read_form_id(reader)?;
        if previous.is_some_and(|prior| index <= prior) || index > world.pending.len() {
            return Err(VmError::State("MAG-64 invalid binding position".into()));
        }
        previous = Some(index);
        world
            .pending
            .insert(index, super::Pending::BindReference { object, base });
    }
    Ok(())
}

fn write_package_tree(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    crate::ai::write_tree_state(writer, actor.ai.tree.as_ref());
}

fn read_package_tree(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    actor.ai.tree = crate::ai::read_tree_state(reader)?;
    Ok(())
}
