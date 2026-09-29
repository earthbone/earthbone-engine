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
    Section { name: "SAV-48", write: super::record_save::write_pack_navmesh_section, read: super::record_save::read_pack_navmesh_section },
    Section { name: "DLG-16", write: super::training::write_training_sessions, read: super::training::read_training_sessions },
];

/// Sections read and written once per actor, at the end of every actor
/// record.
#[rustfmt::skip]
pub(crate) const ACTOR_SECTIONS: &[Section<ActorState>] = &[
    Section { name: "PRG-01", write: super::super::progression::write_skill_experience, read: super::super::progression::read_skill_experience },
    Section { name: "AI-88", write: super::furniture::write_furniture_seat, read: super::furniture::read_furniture_seat },
    Section { name: "PRG-07", write: super::super::progression::write_character_progression, read: super::super::progression::read_character_progression },
    Section { name: "PRG-13", write: super::crime::write_days_in_jail, read: super::crime::read_days_in_jail },
];

/// Sections read and written once per reference, at the end of every
/// reference record.
#[rustfmt::skip]
pub(crate) const REF_SECTIONS: &[Section<RefState>] = &[
];
