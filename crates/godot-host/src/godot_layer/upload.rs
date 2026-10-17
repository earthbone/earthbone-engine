// SPDX-License-Identifier: GPL-3.0-or-later

//! Resumable RID publication, following godotwind's budgeted pocket attach.
//! CPU batching retains the synchronous path's material and reference groups.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use super::{BatchTable, BatchTableMember, CellStreamerNode, StreamTotals};
use crate::batch::{batch_surfaces, instancing_candidates, InstanceGroup};
use crate::data_source::{DataSource, LoadedCell, LoadedSurface, SurfaceTextures};
use crate::mesh::{MaterialDesc, SurfaceArrays};
use godot::prelude::*;
use soe_esm::GlobalFormId;

pub(crate) const SLICE_BUDGET: Duration = Duration::from_millis(8);
pub(super) const LOD_SLICE_BUDGET: Duration = Duration::from_millis(2);

#[derive(Default)]
pub(crate) struct FrameBudget {
    frame: Option<u64>,
    spent: Duration,
}

impl FrameBudget {
    pub(super) fn remaining(&mut self, frame: u64, limit: Duration) -> Duration {
        if self.frame != Some(frame) {
            self.frame = Some(frame);
            self.spent = Duration::ZERO;
        }
        limit.saturating_sub(self.spent)
    }

    pub(super) fn charge(&mut self, elapsed: Duration) {
        self.spent = self.spent.saturating_add(elapsed);
    }
}

enum Unit {
    Particle(Box<crate::data_source::LoadedParticle>),
    Door(GlobalFormId, Vec<LoadedSurface>),
    Animated(Box<LoadedSurface>),
    Multi(Box<InstanceGroup>),
    Mesh(Box<MeshUpload>),
}

struct MeshUpload {
    arrays: SurfaceArrays,
    textures: SurfaceTextures,
    members: Vec<BatchTableMember>,
}

pub(crate) struct Plan {
    units: VecDeque<Unit>,
    pub(crate) largest_batch: usize,
}

pub(crate) fn base_tag(source: Option<&DataSource>, id: GlobalFormId) -> Option<soe_esm::Tag> {
    let source = source?;
    let owner = source.order().winner_ref(id)?;
    let record = source.order().record(owner);
    if record.tag != soe_esm::tag::REFR {
        return Some(record.tag);
    }
    let reference = soe_esm::records::refr::Refr::from_record(record).ok()?;
    source
        .order()
        .winner(source.order().resolve(owner.file, reference.base))
        .map(|base| base.tag)
}

pub(super) fn render_kind(
    source: Option<&DataSource>,
    id: GlobalFormId,
    material: &MaterialDesc,
) -> usize {
    if material.water.is_some() {
        return 3;
    }
    match base_tag(source, id).map(|tag| *tag.as_bytes()) {
        Some([b'G', b'R', b'A', b'S']) => 1,
        Some([b'T', b'R', b'E', b'E']) => 2,
        _ => 0,
    }
}

/// A rejected instance must carry its placement into the ordinary mesh path.
fn placed_fallback(surface: &LoadedSurface) -> LoadedSurface {
    let mut placed = surface.clone();
    let Some(transform) = placed.instance_transform.take() else {
        return placed;
    };
    placed.mesh_identity = None;
    let rotate = |v: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|r| (0..3).map(|c| transform.basis[r][c] * v[c]).sum())
    };
    let direction = |v| {
        let value = rotate(v);
        let length = value.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length > 0.0 {
            value.map(|v| v / length)
        } else {
            value
        }
    };
    let origin = [transform.origin.x, transform.origin.y, transform.origin.z];
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for point in &mut placed.arrays.positions {
        let rotated = rotate(*point);
        *point = std::array::from_fn(|i| rotated[i] + origin[i]);
        for i in 0..3 {
            min[i] = min[i].min(point[i]);
            max[i] = max[i].max(point[i]);
        }
    }
    if !placed.arrays.positions.is_empty() {
        placed.arrays.bounds.min = crate::units::GodotPos::new(min[0], min[1], min[2]);
        placed.arrays.bounds.max = crate::units::GodotPos::new(max[0], max[1], max[2]);
    }
    for normal in &mut placed.arrays.normals {
        *normal = direction(*normal);
    }
    for tangent in placed.arrays.tangents.as_chunks_mut::<4>().0 {
        let rotated = direction([tangent[0], tangent[1], tangent[2]]);
        tangent[..3].copy_from_slice(&rotated);
    }
    placed
}

impl Plan {
    pub(crate) fn new(
        source: Option<&DataSource>,
        cell: GlobalFormId,
        surfaces: &[LoadedSurface],
        batch: bool,
        particles: &[crate::data_source::LoadedParticle],
    ) -> Self {
        let _span = soe_trace::Span::new("host.prepare_upload", "host");
        let mut units: VecDeque<_> = particles
            .iter()
            .cloned()
            .map(|p| Unit::Particle(Box::new(p)))
            .collect();
        let mut doors = BTreeMap::<GlobalFormId, Vec<LoadedSurface>>::new();
        let mut groups = BTreeMap::<usize, Vec<LoadedSurface>>::new();
        for surface in surfaces {
            if base_tag(source, surface.reference) == Some(soe_esm::Tag::new(b"DOOR")) {
                doors
                    .entry(surface.reference)
                    .or_default()
                    .push(surface.clone());
            } else {
                groups
                    .entry(render_kind(
                        source,
                        surface.reference,
                        &surface.arrays.material,
                    ))
                    .or_default()
                    .push(surface.clone());
            }
        }
        for (id, surfaces) in doors {
            units.push_back(Unit::Door(id, surfaces));
        }
        let persistent =
            source.is_some_and(|source| source.cell_index().persistent_cell_world(cell).is_some());
        let mut largest_batch = 0;
        for ordinary in groups.values() {
            for surface in ordinary
                .iter()
                .filter(|surface| surface.animation.is_some())
            {
                units.push_back(Unit::Animated(Box::new(surface.clone())));
            }
            let (instances, remainder) = if persistent {
                crate::batch::worldspace_instancing_candidates(ordinary)
            } else {
                instancing_candidates(ordinary)
            };
            units.extend(
                instances
                    .into_iter()
                    .map(|group| Unit::Multi(Box::new(group))),
            );
            let remainder: Vec<_> = remainder
                .into_iter()
                .filter(|i| ordinary[*i].animation.is_none())
                .map(|i| placed_fallback(&ordinary[i]))
                .collect();
            if batch {
                let (batches, stats) = batch_surfaces(&remainder);
                largest_batch = largest_batch.max(stats.largest_batch_surfaces);
                for batch in batches {
                    let members = batch
                        .members
                        .iter()
                        .map(|member| BatchTableMember {
                            reference: remainder[member.surface].reference,
                            member: *member,
                            origin: remainder[member.surface].placement,
                            bounds: remainder[member.surface].arrays.bounds,
                        })
                        .collect();
                    units.push_back(Unit::Mesh(Box::new(MeshUpload {
                        arrays: batch.arrays,
                        textures: batch.textures,
                        members,
                    })));
                }
            } else {
                largest_batch = largest_batch.max(usize::from(!remainder.is_empty()));
                for surface in remainder {
                    let member = BatchTableMember {
                        reference: surface.reference,
                        member: crate::batch::BatchMember {
                            surface: 0,
                            first_vertex: 0,
                            vertex_count: surface.arrays.positions.len(),
                            first_triangle: 0,
                            triangle_count: surface.arrays.triangle_count(),
                        },
                        origin: surface.placement,
                        bounds: surface.arrays.bounds,
                    };
                    units.push_back(Unit::Mesh(Box::new(MeshUpload {
                        arrays: surface.arrays,
                        textures: surface.textures,
                        members: vec![member],
                    })));
                }
            }
        }
        Self {
            units,
            largest_batch,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}

impl CellStreamerNode {
    pub(crate) fn upload_plan_step(
        &mut self,
        cell: GlobalFormId,
        plan: &mut Plan,
        totals: &mut StreamTotals,
        budget: Duration,
    ) -> bool {
        if plan.is_empty() {
            return true;
        }
        let Some(scenario) = self.scenario() else {
            return false;
        };
        let remaining = if budget == Duration::MAX {
            budget
        } else {
            let frame = godot::classes::Engine::singleton().get_process_frames();
            self.upload_clock.remaining(frame, budget)
        };
        if remaining.is_zero() {
            return true;
        }
        let _system = soe_trace::SystemScope::new(soe_trace::System::Render);
        let _span = soe_trace::Span::new("host.upload_slice", "host");
        let _load = (budget != Duration::MAX)
            .then(|| soe_trace::LoadScope::operation("cell_upload", cell.to_string(), budget));
        let started = Instant::now();
        let mut count = 0;
        while count == 0 || started.elapsed() < remaining {
            let Some(unit) = plan.units.pop_front() else {
                break;
            };
            match unit {
                Unit::Particle(particle) => {
                    self.add_nif_particle(cell, &particle, scenario);
                    totals.instances += 1;
                }
                Unit::Door(id, surfaces) => {
                    self.hide_resident_distant_references(cell, [id]);
                    self.add_door(cell, id, &surfaces, scenario);
                    totals.instances += surfaces.len();
                }
                Unit::Animated(surface) => {
                    self.hide_resident_distant_references(cell, [surface.reference]);
                    self.add_animated_surface(cell, &surface, scenario);
                    totals.instances += 1;
                }
                Unit::Multi(group) => {
                    self.hide_resident_distant_references(
                        cell,
                        group.members.iter().map(|member| member.reference),
                    );
                    self.add_multimesh(cell, &group, scenario);
                    totals.instances += 1;
                }
                Unit::Mesh(mesh) => {
                    self.hide_resident_distant_references(
                        cell,
                        mesh.members.iter().map(|member| member.reference),
                    );
                    let material = super::material_for(
                        &mut self.materials,
                        &mesh.arrays.material,
                        &mesh.textures,
                    );
                    self.add_instance(
                        cell,
                        &mesh.arrays,
                        &material,
                        scenario,
                        BatchTable::Triangles(mesh.members),
                    );
                    totals.instances += 1;
                }
            }
            count += 1;
        }
        if budget != Duration::MAX {
            self.upload_clock.charge(started.elapsed());
            self.upload_slices += 1;
            self.upload_worst_ms = self
                .upload_worst_ms
                .max(started.elapsed().as_secs_f64() * 1000.0);
        }
        totals.largest_batch = totals.largest_batch.max(plan.largest_batch);
        true
    }
}

#[godot_api(secondary)]
impl CellStreamerNode {
    /// Publication slices and retained custom shader variants for frame probes.
    #[func]
    pub fn upload_progress(&self) -> VarDictionary {
        vdict! {
            "slices" => self.upload_slices as i64,
            "worst_ms" => self.upload_worst_ms,
            "budget_ms" => SLICE_BUDGET.as_secs_f64() * 1000.0,
            "shader_variants" => self.materials.shaders.borrow().len() as i64,
        }
    }
}

impl CellStreamerNode {
    pub(crate) fn batch_uploads(&self) -> bool {
        self.batch_surfaces
    }

    pub(crate) fn prepare_upload(&self, cell: GlobalFormId, loaded: &LoadedCell) -> Plan {
        Plan::new(
            self.data_source.as_deref(),
            cell,
            &loaded.surfaces,
            self.batch_surfaces,
            &loaded.particles,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_source::SurfaceTextures;
    use crate::mesh::{build_surface, MeshData, MeshOptions};

    fn surface(object: u32) -> LoadedSurface {
        LoadedSurface {
            reference: GlobalFormId { file: 0, object },
            arrays: build_surface(
                &MeshData {
                    positions: vec![[0.0, 0.0, 0.0], [70.0, 0.0, 0.0], [0.0, 70.0, 0.0]],
                    indices: vec![0, 1, 2],
                    ..MeshData::default()
                },
                MeshOptions::default(),
            )
            .unwrap(),
            textures: SurfaceTextures::default(),
            placement: soe_physics::Iso::IDENTITY,
            animation: None,
            instance_transform: None,
            mesh_identity: None,
        }
    }

    #[test]
    fn prf_40_near_and_distant_share_the_render_frame_budget() {
        let mut budget = FrameBudget::default();
        assert_eq!(budget.remaining(12, SLICE_BUDGET), SLICE_BUDGET);
        budget.charge(Duration::from_millis(5));
        assert_eq!(budget.remaining(12, SLICE_BUDGET), Duration::from_millis(3));
        budget.charge(Duration::from_millis(4));
        assert_eq!(budget.remaining(12, SLICE_BUDGET), Duration::ZERO);
        assert_eq!(budget.remaining(13, SLICE_BUDGET), SLICE_BUDGET);
    }

    #[test]
    fn prf_40_an_indivisible_overrun_prevents_more_uploads_in_the_frame() {
        let mut budget = FrameBudget::default();
        assert_eq!(budget.remaining(1, SLICE_BUDGET), SLICE_BUDGET);
        budget.charge(Duration::from_millis(40));
        for _ in 0..8 {
            assert_eq!(budget.remaining(1, SLICE_BUDGET), Duration::ZERO);
        }
        assert_eq!(budget.remaining(2, SLICE_BUDGET), SLICE_BUDGET);
    }

    #[test]
    fn prf_40_resumable_batches_preserve_every_triangle_owner() {
        let surfaces: Vec<_> = (1..=8).map(surface).collect();
        let plan = Plan::new(
            None,
            GlobalFormId {
                file: 0,
                object: 100,
            },
            &surfaces,
            true,
            &[],
        );
        assert_eq!(plan.units.len(), 1);
        assert_eq!(plan.largest_batch, surfaces.len());
        let Some(Unit::Mesh(mesh)) = plan.units.front() else {
            panic!("ordinary surfaces must batch")
        };
        assert_eq!(mesh.arrays.triangle_count(), surfaces.len());
        assert_eq!(mesh.members.len(), surfaces.len());
        for (index, member) in mesh.members.iter().enumerate() {
            assert_eq!(member.reference, surfaces[index].reference);
            assert_eq!(member.member.first_triangle, index);
            assert_eq!(member.member.triangle_count, 1);
            assert_eq!(member.origin, surfaces[index].placement);
        }
    }

    #[test]
    fn prf_40_upload_plans_retain_instancing_and_the_unbatched_path() {
        let mut surfaces: Vec<_> = (1..=8).map(surface).collect();
        for surface in &mut surfaces {
            surface.mesh_identity = Some(std::sync::Arc::from("rock:0"));
            surface.instance_transform = Some(crate::units::GodotTransform {
                basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                origin: crate::units::GodotPos::default(),
            });
        }
        let id = GlobalFormId {
            file: 0,
            object: 100,
        };
        let plan = Plan::new(None, id, &surfaces, true, &[]);
        assert_eq!(plan.units.len(), 1);
        let Some(Unit::Multi(group)) = plan.units.front() else {
            panic!("repeated models must instance")
        };
        assert_eq!(group.members.len(), 8);
        for (surface, member) in surfaces.iter().zip(&group.members) {
            assert_eq!(surface.reference, member.reference);
        }
        let surfaces: Vec<_> = (1..=8).map(surface).collect();
        let plan = Plan::new(None, id, &surfaces, false, &[]);
        assert_eq!(plan.units.len(), surfaces.len());
        assert!(plan.units.iter().all(|unit| matches!(unit, Unit::Mesh(_))));
    }
    #[test]
    fn r4_tree_fallback_upload_keeps_rotation_scale_and_world_position() {
        for skinned in [false, true] {
            for batch in [false, true] {
                let mut input = surface(1);
                input.mesh_identity = Some(std::sync::Arc::from("tree:0"));
                input.instance_transform = Some(crate::units::GodotTransform {
                    basis: [[0.0, 0.0, 2.0], [0.0, 2.0, 0.0], [-2.0, 0.0, 0.0]],
                    origin: crate::units::GodotPos::new(100.0, -50.0, 200.0),
                });
                input.arrays.normals = vec![[1.0, 0.0, 0.0]; 3];
                input.arrays.tangents = [1.0, 0.0, 0.0, -1.0].repeat(3);
                if skinned {
                    input.arrays.bones = vec![0; 12];
                    input.arrays.weights = [1.0, 0.0, 0.0, 0.0].repeat(3);
                }
                let expected: Vec<_> = input
                    .arrays
                    .positions
                    .iter()
                    .map(|p| [100.0 + 2.0 * p[2], -50.0 + 2.0 * p[1], 200.0 - 2.0 * p[0]])
                    .collect();
                let plan = Plan::new(
                    None,
                    GlobalFormId {
                        file: 0,
                        object: 100,
                    },
                    &[input],
                    batch,
                    &[],
                );
                let Some(Unit::Mesh(mesh)) = plan.units.front() else {
                    panic!("single tree must use mesh fallback")
                };
                assert_eq!(
                    mesh.arrays.positions, expected,
                    "skinned={skinned} batch={batch}"
                );
                assert!(mesh.arrays.normals.iter().all(|n| *n == [0.0, 0.0, -1.0]));
                assert!(mesh
                    .arrays
                    .tangents
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|t| *t == [0.0, 0.0, -1.0, -1.0]));
                assert_eq!(
                    mesh.arrays.bounds.min.x,
                    100.0 - 2.0 * crate::units::METRES_PER_UNIT * 70.0
                );
                assert_eq!(mesh.arrays.bounds.max.z, 200.0);
                assert_eq!(
                    mesh.members[0].reference,
                    GlobalFormId { file: 0, object: 1 }
                );
            }
        }
    }
}
