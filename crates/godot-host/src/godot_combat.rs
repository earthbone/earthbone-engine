// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot physics contact adapter. PhysicsDirectSpaceState3D.intersect_ray
//! returns the first body RID, as specified in the local Godot class reference.

use std::collections::HashMap;
use std::hash::BuildHasher;

use godot::classes::{
    PhysicsDirectSpaceState3D, PhysicsRayQueryParameters3D, PhysicsServer3D,
    PhysicsShapeQueryParameters3D, Skeleton3D,
};
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

/// A physics tick's contact rays sharing query parameters and actor exclusions.
/// Godot's PhysicsRayQueryParameters3D exclude property takes body RIDs.
/// The inverted map avoids scanning every actor body for each sight ray.
pub struct ContactRays<'a, S: BuildHasher> {
    state: Gd<PhysicsDirectSpaceState3D>,
    query: Gd<PhysicsRayQueryParameters3D>,
    owners: &'a [&'a HashMap<Rid, GlobalFormId, S>],
    excluded: HashMap<GlobalFormId, Array<Rid>>,
    observer: Option<GlobalFormId>,
    from: Option<Vector3>,
    rid_key: Variant,
}

impl<'a, S: BuildHasher> ContactRays<'a, S> {
    /// Prove endpoint regions clear of every body except their own capsule.
    pub(crate) fn sight_symmetry(
        &mut self,
        capsules: &[(GlobalFormId, [f32; 3], f32)],
        trusted: &Array<Rid>,
    ) -> SightSymmetry {
        let mut sight = SightSymmetry::default();
        if !self.query.is_hit_back_faces_enabled() {
            return sight;
        }
        let mut heads = Vec::new();
        let mut low = Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut high = -low;
        for &(actor, position, scale) in capsules {
            let soe_physics::Shape::Capsule { radius, .. } =
                soe_physics::generated_controller_shape(scale)
            else {
                continue;
            };
            if !position
                .iter()
                .all(|p| p.is_finite() && p.abs() < radius * 10000.0)
            {
                continue;
            }
            let eye =
                SkyrimPos::new(position[0], position[1], position[2] + 100.0 * scale).to_godot();
            let eye = Vector3::new(eye.x, eye.y, eye.z);
            let radius = radius * METRES_PER_UNIT * 1.2;
            if !eye.is_finite() || !radius.is_finite() {
                continue;
            }
            low = Vector3::new(
                low.x.min(eye.x - radius),
                low.y.min(eye.y - radius),
                low.z.min(eye.z - radius),
            );
            high = Vector3::new(
                high.x.max(eye.x + radius),
                high.y.max(eye.y + radius),
                high.z.max(eye.z + radius),
            );
            heads.push((actor, eye, radius));
        }
        if heads.len() < 2 {
            return sight;
        }
        let center = Vector3::new(
            low.x.midpoint(high.x),
            low.y.midpoint(high.y),
            low.z.midpoint(high.z),
        );
        let half = (high - low) * 0.5;
        let coverage = half.length() * 4.1;
        if !center.is_finite() || !half.is_finite() || !coverage.is_finite() {
            return sight;
        }
        let mut server = PhysicsServer3D::singleton();
        let shape = server.sphere_shape_create();
        let mut query = PhysicsShapeQueryParameters3D::new_gd();
        query.set_shape_rid(shape);
        query.set_collision_mask(self.query.get_collision_mask());
        query.set_exclude(trusted);
        server.shape_set_data(shape, &coverage.to_variant());
        for sign in [
            Vector3::new(1.0, 1.0, 1.0),
            Vector3::new(1.0, -1.0, -1.0),
            Vector3::new(-1.0, 1.0, -1.0),
            Vector3::new(-1.0, -1.0, 1.0),
        ] {
            let position = center + half * sign * 3.0;
            if !position.is_finite() {
                server.free_rid(shape);
                return sight;
            }
            query.set_transform(Transform3D::new(Basis::IDENTITY, position));
            soe_trace::count(soe_trace::Operation::ShapeCasts, 1);
            if !self
                .state
                .intersect_shape_ex(&query)
                .max_results(1)
                .done()
                .is_empty()
            {
                server.free_rid(shape);
                return sight;
            }
        }
        for (actor, eye, radius) in heads {
            server.shape_set_data(shape, &radius.to_variant());
            query.set_transform(Transform3D::new(Basis::IDENTITY, eye));
            query.set_exclude(self.excluded.get(&actor).unwrap_or(&Array::new()));
            soe_trace::count(soe_trace::Operation::ShapeCasts, 1);
            if self
                .state
                .intersect_shape_ex(&query)
                .max_results(1)
                .done()
                .is_empty()
            {
                let index = sight.eyes.len();
                sight.eyes.insert(actor, (eye, index));
            }
        }
        server.free_rid(shape);
        sight
            .pairs
            .resize(sight.eyes.len() * sight.eyes.len(), None);
        sight
    }

    pub(crate) fn visible(
        &mut self,
        observer: GlobalFormId,
        target: GlobalFormId,
        from: Vector3,
        to: Vector3,
    ) -> bool {
        match self.first_contact(observer, from, to) {
            Ok(ContactHit::Empty) => true,
            Ok(ContactHit::Actor(actor)) => actor == target,
            _ => false,
        }
    }

    /// Prepare rays for this physics tick and the currently synchronized bodies.
    ///
    /// # Errors
    /// An unavailable physics space.
    pub fn new(space: Rid, owners: &'a [&'a HashMap<Rid, GlobalFormId, S>]) -> Result<Self, Error> {
        let state = PhysicsServer3D::singleton()
            .space_get_direct_state(space)
            .ok_or(Error::Unsupported("unavailable physics contact space"))?;
        let mut query = PhysicsRayQueryParameters3D::new_gd();
        query.set_hit_from_inside(true);
        let mut excluded: HashMap<GlobalFormId, Array<Rid>> = HashMap::new();
        for map in owners {
            for (&rid, &actor) in *map {
                excluded.entry(actor).or_default().push(rid);
            }
        }
        Ok(Self {
            state,
            query,
            owners,
            excluded,
            observer: None,
            from: None,
            rid_key: "rid".to_variant(),
        })
    }

    /// Query the first body, excluding all bodies owned by this observer.
    ///
    /// # Errors
    /// Non-finite or degenerate ray geometry.
    pub fn first_contact(
        &mut self,
        observer: GlobalFormId,
        from: Vector3,
        to: Vector3,
    ) -> Result<ContactHit, Error> {
        if !from.is_finite() || !to.is_finite() || from == to {
            return Err(Error::Number);
        }
        if self.from != Some(from) {
            self.query.set_from(from);
            self.from = Some(from);
        }
        self.query.set_to(to);
        if self.observer != Some(observer) {
            self.query
                .set_exclude(self.excluded.get(&observer).unwrap_or(&Array::new()));
            self.observer = Some(observer);
        }
        soe_trace::count(soe_trace::Operation::Raycasts, 1);
        let result = self.state.intersect_ray(&self.query);
        let Ok(rid) = result.get_or_nil(&self.rid_key).try_to::<Rid>() else {
            return Ok(ContactHit::Empty);
        };
        Ok(
            match self.owners.iter().find_map(|map| map.get(&rid).copied()) {
                Some(actor) => ContactHit::Actor(actor),
                None => ContactHit::Obstruction(rid),
            },
        )
    }
}

/// Update-local reciprocal sight for disjoint head regions and shallow rays.
/// On-axis endpoints and the slope bound keep both capsule intersections
/// inside the overlap-tested spheres. The remaining segment sees the same
/// two-sided geometry from either direction, per Godot's ray query reference.
#[derive(Default)]
pub(crate) struct SightSymmetry {
    eyes: HashMap<GlobalFormId, (Vector3, usize)>,
    pairs: Vec<Option<bool>>,
}

#[derive(Clone, Copy)]
pub(crate) enum SightReuse {
    Fresh(Option<usize>),
    Cached(bool),
}

impl SightReuse {
    pub(crate) fn cached(self) -> Option<bool> {
        match self {
            Self::Cached(visible) => Some(visible),
            Self::Fresh(_) => None,
        }
    }
}

impl SightSymmetry {
    pub(crate) fn lookup(
        &self,
        observer: GlobalFormId,
        target: GlobalFormId,
        from: Vector3,
        to: Vector3,
    ) -> SightReuse {
        let (Some(&(eye_from, observer_index)), Some(&(eye_to, target_index))) =
            (self.eyes.get(&observer), self.eyes.get(&target))
        else {
            return SightReuse::Fresh(None);
        };
        if eye_from != from || eye_to != to {
            return SightReuse::Fresh(None);
        }
        let dx = f64::from(to.x) - f64::from(from.x);
        let dy = f64::from(to.y) - f64::from(from.y);
        let dz = f64::from(to.z) - f64::from(from.z);
        let shallow = dy.powi(2) <= (dx.powi(2) + dz.powi(2)) * 0.25;
        if !shallow {
            return SightReuse::Fresh(None);
        }
        let width = self.eyes.len();
        match self
            .pairs
            .get(observer_index * width + target_index)
            .copied()
            .flatten()
        {
            Some(visible) => SightReuse::Cached(visible),
            None => SightReuse::Fresh(Some(target_index * width + observer_index)),
        }
    }

    pub(crate) fn complete(&mut self, reuse: SightReuse, visible: bool) {
        if let SightReuse::Fresh(Some(index)) = reuse {
            if let Some(slot) = self.pairs.get_mut(index) {
                *slot = Some(visible);
            }
        }
    }
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

#[cfg(test)]
mod sight_symmetry_tests {
    use super::*;

    #[test]
    fn prf_31_reciprocal_sight_requires_both_clear_heads_and_exact_endpoints() {
        let observer = GlobalFormId { file: 0, object: 1 };
        let target = GlobalFormId { file: 0, object: 2 };
        let from = Vector3::ZERO;
        let to = Vector3::new(10.0, 0.0, 0.0);
        let mut sight = SightSymmetry::default();
        sight.eyes.insert(observer, (from, 0));
        sight.pairs.resize(1, None);
        let reuse = sight.lookup(observer, target, from, to);
        sight.complete(reuse, true);
        assert_eq!(sight.lookup(target, observer, to, from).cached(), None);
        sight.eyes.insert(target, (to, 1));
        sight.pairs.resize(4, None);
        let reuse = sight.lookup(observer, target, from, to);
        sight.complete(reuse, false);
        assert_eq!(
            sight.lookup(target, observer, to, from).cached(),
            Some(false)
        );
        assert_eq!(
            sight
                .lookup(target, observer, to, from + Vector3::UP)
                .cached(),
            None
        );
        let raised = to + Vector3::UP * 6.0;
        sight.eyes.insert(target, (raised, 1));
        let reuse = sight.lookup(observer, target, from, raised);
        sight.complete(reuse, true);
        assert_eq!(sight.lookup(target, observer, raised, from).cached(), None);
    }
}
