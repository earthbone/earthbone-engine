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
    super::magic::SAVE_SECTION,
    Section { name: "DLG-17", write: write_follower_services, read: read_follower_services },
    Section { name: "DLG-19", write: write_scene_action_progress, read: read_scene_action_progress },
];

/// Sections read and written once per actor, at the end of every actor
/// record.
#[rustfmt::skip]
pub(crate) const ACTOR_SECTIONS: &[Section<ActorState>] = &[
    super::furniture::SECTION,
    Section { name: "PRG-01", write: super::super::progression::write_skill_experience, read: super::super::progression::read_skill_experience },
    Section { name: "AI-88", write: super::furniture::write_furniture_seat, read: super::furniture::read_furniture_seat },
    Section { name: "UI-14", write: super::actors::write_ui_14, read: super::actors::read_ui_14 },
    Section { name: "DLG-14", write: write_force_greet, read: read_force_greet },
    Section { name: "DLG-17.detail", write: write_can_do_favors, read: read_can_do_favors },
    Section { name: "DLG-19.actor", write: write_scene_package, read: read_scene_package },
];

/// Sections read and written once per reference, at the end of every
/// reference record.
#[rustfmt::skip]
pub(crate) const REF_SECTIONS: &[Section<RefState>] = &[
    Section { name: "SAV-15", write: write_reference_persistence, read: read_reference_persistence },
    Section { name: "MOD-03", write: write_reference_flags, read: read_reference_flags },
    Section { name: "CMB-37", write: write_destruction_stage, read: read_destruction_stage },
];

fn write_follower_services(world: &World, writer: &mut soe_papyrus::vm::state::Writer) {
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
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
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
            return Err(soe_papyrus::VmError::State(
                "invalid follower service state".into(),
            ));
        }
    }
    world.follower_services = services;
    Ok(())
}

fn write_scene_action_progress(world: &World, writer: &mut soe_papyrus::vm::state::Writer) {
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
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
    let count = reader.len()?;
    let mut seen_scenes = std::collections::BTreeSet::new();
    for _ in 0..count {
        let scene_id = super::save::read_form_id(reader)?;
        if !seen_scenes.insert(scene_id) {
            return Err(soe_papyrus::VmError::State(
                "DLG-19 duplicate scene progress".into(),
            ));
        }
        let scene_record = world.record(scene_id).ok_or_else(|| {
            soe_papyrus::VmError::State("DLG-19 scene progress names a missing scene".into())
        })?;
        let scene =
            soe_esm::records::gameplay::Scene::from_record(scene_record).map_err(|error| {
                soe_papyrus::VmError::State(format!("DLG-19 invalid scene: {error}"))
            })?;
        let run = world.scene_runs.get_mut(&scene_id).ok_or_else(|| {
            soe_papyrus::VmError::State("DLG-19 scene progress has no scene run".into())
        })?;
        let started_count = reader.len()?;
        for _ in 0..started_count {
            let slot = reader.u32()?;
            if scene.actions.get(slot as usize).is_none() || !run.started_actions.insert(slot) {
                return Err(soe_papyrus::VmError::State(
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
                return Err(soe_papyrus::VmError::State(
                    "DLG-19 invalid scene timer deadline".into(),
                ));
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
                return Err(soe_papyrus::VmError::State(
                    "DLG-19 duplicate prior scene package actor".into(),
                ));
            }
            run.prior_aliases.insert(actor, alias);
        }
    }
    Ok(())
}

fn write_scene_package(state: &ActorState, writer: &mut soe_papyrus::vm::state::Writer) {
    super::save::write_opt_form_id(writer, state.ai.scene_package);
}

fn read_scene_package(
    state: &mut ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
    state.ai.scene_package = super::save::read_opt_form_id(reader)?;
    Ok(())
}

fn write_can_do_favors(state: &ActorState, writer: &mut soe_papyrus::vm::state::Writer) {
    writer.bool(state.can_do_favors);
}

fn read_can_do_favors(
    state: &mut ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
    state.can_do_favors = reader.bool()?;
    Ok(())
}

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

fn write_force_greet(state: &ActorState, writer: &mut soe_papyrus::vm::state::Writer) {
    if state.ai.procedure == crate::ai::Procedure::ForceGreet {
        writer.bool(true);
    }
}

fn read_force_greet(
    state: &mut ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
    if reader.bool()? {
        state.ai.procedure = crate::ai::Procedure::ForceGreet;
    }
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
