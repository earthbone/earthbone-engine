// SPDX-License-Identifier: GPL-3.0-or-later

//! Refine door picks against submitted geometry. OpenMW renderingmanager.cpp
//! uses rendered triangles for interaction. Godot Geometry3D supplies the
//! intersection routine. Physics convex proxies remain movement geometry.

use super::*;
use godot::classes::Geometry3D;

pub(crate) struct PickMesh {
    positions: Vec<[f32; 3]>,
    indices: Vec<i32>,
    bounds: crate::mesh::Aabb,
}

impl PickMesh {
    pub(crate) fn new(arrays: &SurfaceArrays) -> Self {
        Self {
            positions: arrays.positions.clone(),
            indices: arrays.indices.clone(),
            bounds: arrays.bounds,
        }
    }

    fn hit(&self, from: Vector3, to: Vector3, transform: Transform3D) -> Option<Vector3> {
        mesh_hit(
            from,
            to,
            &self.positions,
            &self.indices,
            self.bounds,
            transform,
        )
    }
}

fn vector(point: [f32; 3]) -> Vector3 {
    Vector3::from_array(point)
}

fn bounds_box(bounds: crate::mesh::Aabb) -> Aabb {
    let min = Vector3::new(bounds.min.x, bounds.min.y, bounds.min.z);
    let max = Vector3::new(bounds.max.x, bounds.max.y, bounds.max.z);
    Aabb::new(min, max - min)
}

fn mesh_hit(
    from: Vector3,
    to: Vector3,
    positions: &[[f32; 3]],
    indices: &[i32],
    bounds: crate::mesh::Aabb,
    transform: Transform3D,
) -> Option<Vector3> {
    if transform.basis.determinant().abs() < f32::EPSILON {
        return None;
    }
    let inverse = transform.affine_inverse();
    let local_from = inverse * from;
    let local_to = inverse * to;
    if !bounds_box(bounds)
        .grow(0.0001)
        .intersects_segment(local_from, local_to)
    {
        return None;
    }
    let mut geometry = Geometry3D::singleton();
    let mut nearest: Option<Vector3> = None;
    for triangle in indices.as_chunks::<3>().0 {
        let points = triangle.map(|index| positions.get(index as usize).copied().map(vector));
        let [Some(a), Some(b), Some(c)] = points else {
            continue;
        };
        let triangle_bounds = Aabb::new(a, Vector3::ZERO).expand(b).expand(c).grow(0.0001);
        if !triangle_bounds.intersects_segment(local_from, local_to) {
            continue;
        }
        let Ok(point) = geometry
            .segment_intersects_triangle(local_from, local_to, a, b, c)
            .try_to::<Vector3>()
        else {
            continue;
        };
        let point = transform * point;
        if nearest.is_none_or(|old| from.distance_squared_to(point) < from.distance_squared_to(old))
        {
            nearest = Some(point);
        }
    }
    nearest
}

fn nearer(
    from: Vector3,
    best: &mut Option<(GlobalFormId, Vector3)>,
    reference: GlobalFormId,
    point: Vector3,
) {
    if best.is_none_or(|(old_id, old)| {
        from.distance_squared_to(point)
            .total_cmp(&from.distance_squared_to(old))
            .then(reference.cmp(&old_id))
            .is_lt()
    }) {
        *best = Some((reference, point));
    }
}

impl CellStreamerNode {
    /// Refine a physics pick only when a drawn door intersects the ray.
    /// Other drawn surfaces still occlude that door, including static walls.
    pub(crate) fn rendered_door_ray(
        &self,
        from: Vector3,
        to: Vector3,
    ) -> Option<(GlobalFormId, Vector3)> {
        let _span = crate::godot_ui_trace::scope(|| "host.pick.rendered_door".into());
        let mut best = None;
        for (reference, door) in &self.doors {
            if !door.visible {
                continue;
            }
            for surface in &door.surfaces {
                if let Some(point) = surface.pick_mesh.hit(from, to, surface.submitted) {
                    nearer(from, &mut best, *reference, point);
                }
            }
        }
        let (_, door_point) = best?;
        for instance in &self.instances {
            if !self.console_visibility[instance.render_kind] {
                continue;
            }
            match &instance.table {
                BatchTable::Triangles(members) => {
                    for member in members {
                        let (visible, target, _) = self
                            .ref_cache
                            .get(&member.reference)
                            .cloned()
                            .unwrap_or((true, member.origin, None));
                        if !visible {
                            continue;
                        }
                        let range = member.member.first_triangle * 3
                            ..(member.member.first_triangle + member.member.triangle_count) * 3;
                        let indices = &instance.arrays.indices[range];
                        let animated_positions;
                        let (positions, bounds, transform) = if let Some(animation) =
                            &instance.animation
                        {
                            let transform = animation
                                .last_transforms
                                .first()
                                .copied()
                                .unwrap_or(Transform3D::IDENTITY);
                            if instance.arrays.is_skinned() {
                                animated_positions =
                                    skin_positions(&instance.arrays, &animation.last_transforms);
                                let Some(bounds) = position_bounds(&animated_positions) else {
                                    continue;
                                };
                                (animated_positions.as_slice(), bounds, transform)
                            } else {
                                (
                                    instance.arrays.positions.as_slice(),
                                    member.bounds,
                                    transform,
                                )
                            }
                        } else {
                            let delta = target.then(&member.origin.inverse());
                            (
                                instance.arrays.positions.as_slice(),
                                member.bounds,
                                crate::godot_door::pose_transform(delta),
                            )
                        };
                        if let Some(point) =
                            mesh_hit(from, door_point, positions, indices, bounds, transform)
                        {
                            nearer(from, &mut best, member.reference, point);
                        }
                    }
                }
                BatchTable::Instances(references) => {
                    for reference in references {
                        let Some((true, pose, _)) = self.ref_cache.get(reference) else {
                            continue;
                        };
                        if let Some(point) = mesh_hit(
                            from,
                            door_point,
                            &instance.arrays.positions,
                            &instance.arrays.indices,
                            instance.arrays.bounds,
                            crate::godot_door::pose_transform(*pose),
                        ) {
                            nearer(from, &mut best, *reference, point);
                        }
                    }
                }
            }
        }
        best
    }
}

#[godot_api(secondary)]
impl CellStreamerNode {
    /// Read-only picking witness for the free-walk regression.
    #[func]
    pub fn rendered_door_pick(&self, from: Vector3, to: Vector3) -> VarDictionary {
        let mut result = VarDictionary::new();
        if let Some((reference, point)) = self.rendered_door_ray(from, to) {
            result.set("file", reference.file as i32);
            result.set("object", reference.object as i64);
            result.set("position", point);
        }
        result
    }
}

fn skin_positions(arrays: &SurfaceArrays, transforms: &[Transform3D]) -> Vec<[f32; 3]> {
    arrays
        .positions
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let mut skinned = Vector3::ZERO;
            for slot in index * 4..index * 4 + 4 {
                let (Some(bone), Some(weight)) = (arrays.bones.get(slot), arrays.weights.get(slot))
                else {
                    continue;
                };
                let Some(transform) = transforms.get(*bone as usize + 1) else {
                    continue;
                };
                skinned += (*transform * vector(*point)) * *weight;
            }
            skinned.to_array()
        })
        .collect()
}

fn position_bounds(positions: &[[f32; 3]]) -> Option<crate::mesh::Aabb> {
    let first = *positions.first()?;
    let bounds = positions
        .iter()
        .fold(Aabb::new(vector(first), Vector3::ZERO), |bounds, point| {
            bounds.expand(vector(*point))
        });
    let min = bounds.position;
    let max = bounds.end();
    Some(crate::mesh::Aabb {
        min: GodotPos::new(min.x, min.y, min.z),
        max: GodotPos::new(max.x, max.y, max.z),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_distance_picks_use_form_order_and_nearer_walls_win() {
        let low = GlobalFormId { file: 0, object: 1 };
        let high = GlobalFormId { file: 0, object: 2 };
        for order in [[low, high], [high, low]] {
            let mut best = None;
            for reference in order {
                nearer(Vector3::ZERO, &mut best, reference, Vector3::BACK * 2.0);
            }
            assert_eq!(best.map(|hit| hit.0), Some(low));
            nearer(Vector3::ZERO, &mut best, high, Vector3::BACK);
            assert_eq!(best.map(|hit| hit.0), Some(high));
        }
    }

    #[test]
    fn zero_scale_geometry_is_not_pickable() {
        let transform = Transform3D::new(Basis::from_scale(Vector3::ZERO), Vector3::ZERO);
        let bounds = crate::mesh::Aabb {
            min: GodotPos::new(-1.0, -1.0, -1.0),
            max: GodotPos::new(1.0, 1.0, 1.0),
        };
        assert_eq!(
            mesh_hit(Vector3::ZERO, Vector3::BACK, &[], &[], bounds, transform),
            None
        );
    }
}
