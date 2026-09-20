// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot physics contact adapter. PhysicsDirectSpaceState3D.intersect_ray
//! returns the first body RID, as specified in the local Godot class reference.

use std::collections::HashMap;
use std::hash::BuildHasher;

use godot::classes::{PhysicsRayQueryParameters3D, PhysicsServer3D};
use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_game::combat::Error;

/// Query one authored weapon segment in Godot world coordinates, in metres.
/// Actor body RIDs map to the existing form table. An unmapped first body is
/// an obstruction, so actors behind walls cannot receive this contact.
///
/// # Errors
/// Invalid geometry or an unavailable physics space. Call during a physics tick.
pub fn first_contact<S: BuildHasher>(
    space: Rid,
    actors: &HashMap<Rid, GlobalFormId, S>,
    attacker: GlobalFormId,
    from: Vector3,
    to: Vector3,
) -> Result<Option<GlobalFormId>, Error> {
    if !from.is_finite() || !to.is_finite() || from == to {
        return Err(Error::Number);
    }
    let mut query = PhysicsRayQueryParameters3D::new_gd();
    query.set_from(from);
    query.set_to(to);
    query.set_hit_from_inside(true);
    let excluded: Array<Rid> = actors
        .iter()
        .filter_map(|(rid, form)| (*form == attacker).then_some(*rid))
        .collect();
    query.set_exclude(&excluded);
    let mut state = PhysicsServer3D::singleton()
        .space_get_direct_state(space)
        .ok_or(Error::Unsupported("unavailable physics contact space"))?;
    let result = state.intersect_ray(&query);
    Ok(result
        .get("rid")
        .and_then(|value| value.try_to::<Rid>().ok())
        .and_then(|rid| actors.get(&rid).copied()))
}
