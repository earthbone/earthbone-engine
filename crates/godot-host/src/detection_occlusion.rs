// SPDX-License-Identifier: GPL-3.0-or-later

//! Conservative sight rejection using the host's synchronized static capsules.
//! Jolt Geometry/RayCapsule.h, Geometry/RayAABox.h and soe-physics's generated
//! controller shape supply the sphere, slab and capsule models. An interior
//! sphere witness before the target's outer box proves occlusion. Clear paths
//! also require a Godot overlap proof for scenery. Uncertain segments still
//! use Godot's ray query.

use soe_esm::GlobalFormId;
use soe_physics::{generated_controller_shape, Shape};

struct Capsule {
    actor: GlobalFormId,
    ends: [[f64; 3]; 2],
    inner_squared: f64,
    outer_squared: f64,
    low: [f64; 3],
    high: [f64; 3],
}

pub(crate) struct ActorOcclusion(Vec<Capsule>);

impl ActorOcclusion {
    pub(crate) fn new(actors: &[(GlobalFormId, [f32; 3], f32)]) -> Self {
        Self(
            actors
                .iter()
                .filter_map(|&(actor, position, scale)| {
                    let Shape::Capsule { a, b, radius } = generated_controller_shape(scale) else {
                        return None;
                    };
                    if !position
                        .iter()
                        .all(|p| p.is_finite() && p.abs() < radius * 10000.0)
                    {
                        return None;
                    }
                    let a = [a.x, a.y, a.z].map(f64::from);
                    let b = [b.x, b.y, b.z].map(f64::from);
                    let a = std::array::from_fn(|i| a[i] + f64::from(position[i]));
                    let b = std::array::from_fn(|i| b[i] + f64::from(position[i]));
                    let radius = f64::from(radius);
                    Some(Capsule {
                        actor,
                        ends: [a, b],
                        inner_squared: (radius * 0.99).powi(2),
                        outer_squared: (radius * 1.01).powi(2),
                        low: std::array::from_fn(|i| a[i].min(b[i]) - radius * 1.01),
                        high: std::array::from_fn(|i| a[i].max(b[i]) + radius * 1.01),
                    })
                })
                .collect(),
        )
    }

    pub(crate) fn blocked(
        &self,
        observer: GlobalFormId,
        target: GlobalFormId,
        from: [f32; 3],
        to: [f32; 3],
    ) -> bool {
        if !from.iter().chain(&to).all(|p| p.is_finite()) {
            return false;
        }
        let Some(target) = self.0.iter().find(|capsule| capsule.actor == target) else {
            return false;
        };
        let from = from.map(f64::from);
        let direction: [f64; 3] = std::array::from_fn(|i| f64::from(to[i]) - from[i]);
        let squared = direction.iter().map(|d| d * d).sum::<f64>();
        if squared == 0.0 {
            return false;
        }
        let inverse_squared = 1.0 / squared;
        let mut entry = 0.0_f64;
        let mut exit = 1.0_f64;
        for i in 0..3 {
            if direction[i] == 0.0 {
                if from[i] < target.low[i] || from[i] > target.high[i] {
                    return false;
                }
            } else {
                let low = (target.low[i] - from[i]) / direction[i];
                let high = (target.high[i] - from[i]) / direction[i];
                entry = entry.max(low.min(high));
                exit = exit.min(low.max(high));
            }
        }
        if entry <= 0.0 || entry >= exit {
            return false;
        }
        let end: [f64; 3] = std::array::from_fn(|i| from[i] + direction[i] * entry);
        let low: [f64; 3] = std::array::from_fn(|i| from[i].min(end[i]));
        let high: [f64; 3] = std::array::from_fn(|i| from[i].max(end[i]));
        self.0.iter().any(|capsule| {
            capsule.actor != observer
                && capsule.actor != target.actor
                && (0..3).all(|i| capsule.low[i] <= high[i] && capsule.high[i] >= low[i])
                && capsule.intersects(
                    from,
                    direction,
                    inverse_squared,
                    entry,
                    capsule.inner_squared,
                )
        })
    }

    /// Prove a clear capsule path once Godot has ruled out other geometry.
    /// The target endpoint must be inside its capsule. Every third capsule's
    /// enlarged capsule must miss the whole segment, including its endpoints.
    pub(crate) fn clear(
        &self,
        observer: GlobalFormId,
        target: GlobalFormId,
        from: [f32; 3],
        to: [f32; 3],
    ) -> bool {
        if !from.iter().chain(&to).all(|p| p.is_finite()) || from == to {
            return false;
        }
        let Some(target) = self.0.iter().find(|capsule| capsule.actor == target) else {
            return false;
        };
        let from = from.map(f64::from);
        let to = to.map(f64::from);
        let [a, b] = target.ends;
        let nearest = [a[0], a[1], to[2].clamp(a[2], b[2])];
        if (0..3).map(|i| (to[i] - nearest[i]).powi(2)).sum::<f64>() >= target.inner_squared {
            return false;
        }
        let direction: [f64; 3] = std::array::from_fn(|i| to[i] - from[i]);
        let inverse_squared = 1.0 / direction.iter().map(|d| d * d).sum::<f64>();
        !self.0.iter().any(|capsule| {
            if capsule.actor == observer || capsule.actor == target.actor {
                return false;
            }
            let mut entry = 0.0_f64;
            let mut exit = 1.0_f64;
            for i in 0..3 {
                if direction[i] == 0.0 {
                    if from[i] < capsule.low[i] || from[i] > capsule.high[i] {
                        return false;
                    }
                } else {
                    let low = (capsule.low[i] - from[i]) / direction[i];
                    let high = (capsule.high[i] - from[i]) / direction[i];
                    entry = entry.max(low.min(high));
                    exit = exit.min(low.max(high));
                    if entry > exit {
                        return false;
                    }
                }
            }
            capsule.intersects(from, direction, inverse_squared, 1.0, capsule.outer_squared)
        })
    }
}

impl Capsule {
    fn intersects(
        &self,
        from: [f64; 3],
        direction: [f64; 3],
        inverse_squared: f64,
        end: f64,
        radius_squared: f64,
    ) -> bool {
        if self.ends.iter().any(|center| {
            let fraction = (0..3)
                .map(|i| (center[i] - from[i]) * direction[i])
                .sum::<f64>()
                * inverse_squared;
            let fraction = fraction.clamp(0.0, end);
            (0..3)
                .map(|i| (from[i] + direction[i] * fraction - center[i]).powi(2))
                .sum::<f64>()
                < radius_squared
        }) {
            return true;
        }
        let [a, b] = self.ends;
        let mut entry = 0.0_f64;
        let mut exit = end;
        if direction[2] == 0.0 {
            if from[2] < a[2] || from[2] > b[2] {
                return false;
            }
        } else {
            let low = (a[2] - from[2]) / direction[2];
            let high = (b[2] - from[2]) / direction[2];
            entry = entry.max(low.min(high));
            exit = exit.min(low.max(high));
            if entry > exit {
                return false;
            }
        }
        let horizontal = direction[0].powi(2) + direction[1].powi(2);
        let fraction = if horizontal > 0.0 {
            ((a[0] - from[0]) * direction[0] + (a[1] - from[1]) * direction[1]) / horizontal
        } else {
            entry
        }
        .clamp(entry, exit);
        (from[0] + direction[0] * fraction - a[0]).powi(2)
            + (from[1] + direction[1] * fraction - a[1]).powi(2)
            < radius_squared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    #[test]
    fn prf_31_actor_occlusion_proves_only_a_third_body_before_the_target() {
        let actors = [(id(1), [0.0; 3], 1.0), (id(2), [300.0, 0.0, 0.0], 1.0)];
        let from = [0.0, 0.0, 100.0];
        let to = [300.0, 0.0, 100.0];
        assert!(!ActorOcclusion::new(&actors).blocked(id(1), id(2), from, to));
        let mut actors = actors.to_vec();
        actors.push((id(3), [150.0, 0.0, 0.0], 1.0));
        assert!(ActorOcclusion::new(&actors).blocked(id(1), id(2), from, to));
        actors[2].1 = [350.0, 0.0, 0.0];
        assert!(!ActorOcclusion::new(&actors).blocked(id(1), id(2), from, to));
        actors[2].1 = [150.0, 100.0, 0.0];
        assert!(!ActorOcclusion::new(&actors).blocked(id(1), id(2), from, to));
    }

    #[test]
    fn prf_31_actor_occlusion_leaves_unknown_and_overlapping_targets_to_godot() {
        let actors = [(id(2), [0.0; 3], 1.0), (id(3), [50.0, 0.0, 0.0], 1.0)];
        let occlusion = ActorOcclusion::new(&actors);
        assert!(!occlusion.blocked(id(1), id(4), [0.0; 3], [300.0, 0.0, 100.0]));
        assert!(!occlusion.blocked(id(1), id(2), [0.0, 0.0, 100.0], [10.0, 0.0, 100.0]));
        assert!(!occlusion.blocked(id(1), id(2), [f32::NAN; 3], [0.0; 3]));
    }

    #[test]
    fn prf_31_cylinder_witness_covers_gaps_between_the_interior_spheres() {
        let Shape::Capsule { a, b, radius } = generated_controller_shape(1.0) else {
            panic!("generated controller must be a capsule");
        };
        let height = a.z + (b.z - a.z) * 0.75;
        let offset = radius * 0.9;
        assert!(offset.powi(2) + ((b.z - a.z) * 0.25).powi(2) > (radius * 0.99).powi(2));
        let actors = [
            (id(1), [0.0; 3], 1.0),
            (id(2), [300.0, 0.0, 0.0], 1.0),
            (id(3), [150.0, offset, 0.0], 1.0),
        ];
        assert!(ActorOcclusion::new(&actors).blocked(
            id(1),
            id(2),
            [0.0, 0.0, height],
            [300.0, 0.0, height]
        ));
    }

    #[test]
    fn prf_31_clear_capsules_reject_box_corners_without_rejecting_the_ray() {
        let Shape::Capsule { radius, .. } = generated_controller_shape(1.0) else {
            panic!("generated controller must be a capsule");
        };
        let actors = [
            (id(1), [0.0; 3], 1.0),
            (id(2), [300.0, 300.0, 0.0], 1.0),
            (id(3), [150.0, 150.0 + radius * 1.5, 0.0], 1.0),
        ];
        assert!(ActorOcclusion::new(&actors).clear(
            id(1),
            id(2),
            [0.0, 0.0, 100.0],
            [300.0, 300.0, 100.0]
        ));
    }

    #[test]
    fn prf_31_clear_capsules_require_an_interior_target_and_no_crossing_capsule() {
        let mut actors = vec![(id(1), [0.0; 3], 1.0), (id(2), [300.0, 0.0, 0.0], 1.0)];
        let from = [0.0, 0.0, 100.0];
        let to = [300.0, 0.0, 100.0];
        let occlusion = ActorOcclusion::new(&actors);
        assert!(occlusion.clear(id(1), id(2), from, to));
        assert!(!occlusion.clear(id(1), id(3), from, to));
        assert!(!occlusion.clear(id(1), id(2), from, [300.0, 100.0, 100.0]));
        assert!(!occlusion.clear(id(1), id(2), [f32::NAN; 3], to));
        actors.push((id(3), [150.0, 0.0, 0.0], 1.0));
        assert!(!ActorOcclusion::new(&actors).clear(id(1), id(2), from, to));
        actors[2].1 = [0.0; 3];
        assert!(!ActorOcclusion::new(&actors).clear(id(1), id(2), from, to));
        actors[2].1 = [350.0, 0.0, 0.0];
        assert!(ActorOcclusion::new(&actors).clear(id(1), id(2), from, to));
    }

    #[test]
    fn prf_31_occlusion_rejections_match_nearest_capsule_raycasts() {
        use soe_physics::{Collider, Iso, Layer, MotionType, PhysicsWorld, SimpleWorld, Vec3};

        let actors: Vec<_> = (1..=36)
            .map(|object| {
                (
                    id(object),
                    [
                        20000.0 + (object % 6) as f32 * 70.0,
                        -6000.0 + (object / 6) as f32 * 70.0,
                        0.0,
                    ],
                    0.8 + (object % 3) as f32 * 0.2,
                )
            })
            .collect();
        let occlusion = ActorOcclusion::new(&actors);
        let mut rejected = 0;
        let mut clear = 0;
        for &(observer, position, scale) in &actors {
            let from = [position[0], position[1], position[2] + 100.0 * scale];
            let mut world = SimpleWorld::new();
            let mut bodies = Vec::new();
            for &(actor, position, scale) in &actors {
                if actor == observer {
                    continue;
                }
                let body = world.add_static(&Collider {
                    node: soe_nif::BlockRef(0),
                    node_name: "controller".into(),
                    layer: Layer::STATIC,
                    motion: MotionType::Fixed,
                    mass: 0.0,
                    transform: Iso::placement(position, [0.0; 3], 1.0),
                    shape: generated_controller_shape(scale),
                });
                bodies.push((actor, body));
            }
            for &(target, position, scale) in &actors {
                let to = [position[0], position[1], position[2] + 100.0 * scale];
                let blocked = occlusion.blocked(observer, target, from, to);
                let visible = occlusion.clear(observer, target, from, to);
                assert!(!(blocked && visible), "{observer} -> {target}");
                if !blocked && !visible {
                    continue;
                }
                rejected += usize::from(blocked);
                clear += usize::from(visible);
                let target_body = bodies.iter().find(|(actor, _)| *actor == target).unwrap().1;
                let hit = world.raycast(
                    Vec3::new(from[0], from[1], from[2]),
                    Vec3::new(to[0], to[1], to[2]),
                );
                assert!(
                    hit.is_some_and(|hit| (hit.body == target_body) == visible),
                    "{observer} -> {target}"
                );
            }
        }
        assert!(
            rejected >= 100,
            "dense neighborhood rejected only {rejected} rays"
        );
        assert!(
            clear >= 10,
            "dense neighborhood proved only {clear} clear rays"
        );
    }
}
