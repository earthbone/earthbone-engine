// SPDX-License-Identifier: GPL-3.0-or-later

//! Every named save section, one list per level. A row that adds saved
//! state appends one entry to the list for the level that owns the
//! state, and nothing else lives in this file: keep it that way so two
//! branches adding different entries merge cleanly (`.gitattributes`
//! marks this path `merge=union`). Keep each entry on a single line.

use super::save_sections::Section;
use super::{ActorState, RefState, World};

/// Sections read and written once, at the end of [`World::save_state`]
/// and [`World::load_state`].
pub(crate) const WORLD_SECTIONS: &[Section<World>] = &[];

/// Sections read and written once per actor, at the end of every actor
/// record.
pub(crate) const ACTOR_SECTIONS: &[Section<ActorState>] = &[];

/// Sections read and written once per reference, at the end of every
/// reference record.
pub(crate) const REF_SECTIONS: &[Section<RefState>] = &[];
