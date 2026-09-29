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
    Section { name: "SAV-48", write: World::write_pack_navmesh_section, read: World::read_pack_navmesh_section },
    Section { name: "SAV-53", write: World::write_base_object_section, read: World::read_base_object_section },
    Section { name: "SAV-42", write: World::write_static_object_section, read: World::read_static_object_section },
    Section { name: "SAV-44", write: World::write_appa_idlm_section, read: World::read_appa_idlm_section },
    Section { name: "SAV-50", write: World::write_woop_mgef_section, read: World::read_woop_mgef_section },
];

/// Sections read and written once per actor, at the end of every actor
/// record.
#[rustfmt::skip]
pub(crate) const ACTOR_SECTIONS: &[Section<ActorState>] = &[
];

/// Sections read and written once per reference, at the end of every
/// reference record.
#[rustfmt::skip]
pub(crate) const REF_SECTIONS: &[Section<RefState>] = &[
];
