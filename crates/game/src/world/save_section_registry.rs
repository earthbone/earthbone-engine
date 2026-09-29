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
    Section { name: "ANM-58", write: super::events::write_animation_waits, read: super::events::read_animation_waits },
];

/// Sections read and written once per actor, at the end of every actor
/// record.
#[rustfmt::skip]
pub(crate) const ACTOR_SECTIONS: &[Section<ActorState>] = &[
    Section { name: "CMB-38", write: super::knockback::write_ragdoll_override, read: super::knockback::read_ragdoll_override },
];

/// Sections read and written once per reference, at the end of every
/// reference record.
#[rustfmt::skip]
pub(crate) const REF_SECTIONS: &[Section<RefState>] = &[
    super::destruction::SECTION,
    Section { name: "HST-16", write: super::reference_translation::write_section, read: super::reference_translation::read_section },
];
