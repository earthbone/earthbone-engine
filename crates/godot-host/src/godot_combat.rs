// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot physics contact adapter. PhysicsDirectSpaceState3D.intersect_ray
//! returns the first body RID, as specified in the local Godot class reference.

use std::collections::HashMap;
use std::hash::BuildHasher;

use godot::classes::{PhysicsRayQueryParameters3D, PhysicsServer3D, Skeleton3D};
use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_game::combat::{Error, FlightSegment, Hand};

use crate::combat::{weapon_bones, weapon_segment};
use crate::units::{GodotPos, SkyrimPos, METRES_PER_UNIT};

/// The first object met by an authored weapon segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactHit {
    /// The segment crossed empty space.
    Empty,
    /// A resident actor owns the first body.
    Actor(GlobalFormId),
    /// A world body blocks the actor behind it.
    Obstruction(Rid),
}

/// Query one authored weapon segment in Godot world coordinates, in metres.
/// Actor body RIDs map to the existing form table through `owners`, which
/// holds the host-registered bodies and the World-synchronized ones. An
/// unmapped first body is an obstruction, so actors behind walls cannot
/// receive this contact. Every body the attacker owns is excluded.
///
/// # Errors
/// Invalid geometry or an unavailable physics space. Call during a physics tick.
pub fn first_contact<S: BuildHasher>(
    space: Rid,
    owners: &[&HashMap<Rid, GlobalFormId, S>],
    attacker: GlobalFormId,
    from: Vector3,
    to: Vector3,
) -> Result<ContactHit, Error> {
    if !from.is_finite() || !to.is_finite() || from == to {
        return Err(Error::Number);
    }
    let mut query = PhysicsRayQueryParameters3D::new_gd();
    query.set_from(from);
    query.set_to(to);
    query.set_hit_from_inside(true);
    let excluded: Array<Rid> = owners
        .iter()
        .flat_map(|map| map.iter())
        .filter_map(|(rid, form)| (*form == attacker).then_some(*rid))
        .collect();
    query.set_exclude(&excluded);
    let mut state = PhysicsServer3D::singleton()
        .space_get_direct_state(space)
        .ok_or(Error::Unsupported("unavailable physics contact space"))?;
    soe_trace::count(soe_trace::Operation::Raycasts, 1);
    let result = soe_trace::measure_physics_query(soe_trace::PhysicsQueryKind::Raycast, || {
        state.intersect_ray(&query)
    });
    let Some(rid) = result
        .get("rid")
        .and_then(|value| value.try_to::<Rid>().ok())
    else {
        return Ok(ContactHit::Empty);
    };
    Ok(match owners.iter().find_map(|map| map.get(&rid).copied()) {
        Some(actor) => ContactHit::Actor(actor),
        None => ContactHit::Obstruction(rid),
    })
}

/// Query one projectile segment and return its first obstruction or resident actor.
/// The attacker is excluded from every mapped actor body.
///
/// # Errors
/// Invalid segment geometry or an unavailable physics space.
pub fn first_projectile_contact<S: BuildHasher>(
    space: Rid,
    owners: &[&HashMap<Rid, GlobalFormId, S>],
    attacker: GlobalFormId,
    segment: FlightSegment,
) -> Result<Option<crate::combat::ProjectileContact>, Error> {
    let from = SkyrimPos::new(segment.from[0], segment.from[1], segment.from[2]).to_godot();
    let to = SkyrimPos::new(segment.to[0], segment.to[1], segment.to[2]).to_godot();
    let from = Vector3::new(from.x, from.y, from.z);
    let to = Vector3::new(to.x, to.y, to.z);
    if !from.is_finite() || !to.is_finite() || from == to {
        return Err(Error::Number);
    }
    let mut query = PhysicsRayQueryParameters3D::new_gd();
    query.set_from(from);
    query.set_to(to);
    query.set_hit_from_inside(true);
    let excluded: Array<Rid> = owners
        .iter()
        .flat_map(|map| map.iter())
        .filter_map(|(rid, form)| (*form == attacker).then_some(*rid))
        .collect();
    query.set_exclude(&excluded);
    let mut state = PhysicsServer3D::singleton()
        .space_get_direct_state(space)
        .ok_or(Error::Unsupported("unavailable projectile contact space"))?;
    soe_trace::count(soe_trace::Operation::Raycasts, 1);
    let result = soe_trace::measure_physics_query(soe_trace::PhysicsQueryKind::Raycast, || {
        state.intersect_ray(&query)
    });
    let Some(rid) = result
        .get("rid")
        .and_then(|value| value.try_to::<Rid>().ok())
    else {
        return Ok(None);
    };
    let hit = match owners.iter().find_map(|map| map.get(&rid).copied()) {
        Some(actor) => ContactHit::Actor(actor),
        None => ContactHit::Obstruction(rid),
    };
    let point = result
        .get("position")
        .and_then(|value| value.try_to::<Vector3>().ok())
        .ok_or(Error::Unsupported("projectile contact has no position"))?;
    let point = GodotPos::new(point.x, point.y, point.z).to_skyrim();
    Ok(Some(crate::combat::ProjectileContact {
        hit,
        position: [point.x, point.y, point.z],
    }))
}

/// World-space pose of the swing hand's weapon node in `skeleton`, trying
/// the [`weapon_bones`] candidates in order.
///
/// # Errors
/// A skeleton without any candidate bone.
pub fn weapon_bone_pose(skeleton: &Gd<Skeleton3D>, hand: Hand) -> Result<Transform3D, Error> {
    let bone = weapon_bones(hand)
        .iter()
        .map(|name| skeleton.find_bone(*name))
        .find(|index| *index >= 0)
        .ok_or(Error::Unsupported(
            "skeleton has no weapon bone for the swing hand",
        ))?;
    Ok(skeleton.get_global_transform() * skeleton.get_bone_global_pose(bone))
}

/// The weapon segment a bone pose sweeps: from the node's origin along its
/// blade axis for `reach_units` game units. The blade is the node's Skyrim
/// local +Y, which the host's axis map (`units.rs`) carries to Godot local
/// -Z, so the axis is the pose's negated Z basis column.
///
/// # Errors
/// A degenerate pose or reach, see [`weapon_segment`].
pub fn pose_segment(pose: Transform3D, reach_units: f32) -> Result<(Vector3, Vector3), Error> {
    let origin = pose.origin;
    let axis = -pose.basis.col_c();
    let (from, to) = weapon_segment(
        [origin.x, origin.y, origin.z],
        [axis.x, axis.y, axis.z],
        reach_units * METRES_PER_UNIT,
    )?;
    Ok((
        Vector3::new(from[0], from[1], from[2]),
        Vector3::new(to[0], to[1], to[2]),
    ))
}
