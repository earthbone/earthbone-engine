// SPDX-License-Identifier: GPL-3.0-or-later

//! The Godot-facing layer. Nothing here decides anything: the node forwards
//! a focus position to `streaming::CellStreamer` every frame and turns the
//! plan into signals, and the two functions copy already converted arrays
//! into Godot's packed types. Keeping it this thin is what lets the rest of
//! the crate test without an engine.
//!
//! Streamed geometry bypasses the scene tree. Each batch becomes an
//! `ArrayMesh` resource and a `RenderingServer` instance in the node's
//! scenario, so a cell adds no `Node3D`, no per-node process or transform
//! notification, and a 3 by 3 exterior block is two hundred RIDs rather than
//! two hundred nodes. The node keeps the RIDs and frees them on `exit_tree`.

use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;

use godot::classes::base_material_3d::{
    CullMode, EmissionOperator, Feature, TextureParam, Transparency,
};
use godot::classes::image::Format;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::multi_mesh::TransformFormat;
use godot::classes::{
    ArrayMesh, Cubemap, INode3D, Image, ImageTexture, Material, MultiMesh, Node3D, RenderingServer,
    Shader, ShaderMaterial, SphereMesh, StandardMaterial3D,
};
use godot::prelude::*;
use soe_esm::GlobalFormId;

use crate::batch::{batch_surfaces, instancing_candidates, BatchMember, InstanceGroup};
use crate::data_source::{
    BlockFormat, DataSource, LoadedCell, LoadedSurface, SurfaceTextures, TextureData,
};
use crate::mesh::{AlphaMode, MaterialDesc, SurfaceArrays};
use crate::streaming::{CellCoord, CellStreamer, StreamConfig};
use crate::terrain::RegionHeightmap;
use crate::terrain_lod::{LodBlockCoord, LodLevel, TerrainLodConfig, TerrainLodStreamer};
use crate::units::{
    direction_to_godot, direction_to_skyrim, placement_to_godot, GodotPos, GodotTransform,
    SkyrimPos,
};
use soe_physics::{Iso, Vec3};

fn tether_form_id(encoded: i64) -> Option<GlobalFormId> {
    let encoded = u64::try_from(encoded).ok()?;
    let file = u16::try_from(encoded >> 32).ok()?;
    let object = u32::try_from(encoded & 0xffff_ffff).ok()?;
    (object <= 0x00ff_ffff).then_some(GlobalFormId { file, object })
}

/// Keeps a square of exterior cells requested around `focus`. Connect
/// `cell_load_requested` and `cell_unload_requested` to whatever builds
/// and frees cell scenes. A load that fails must be reported back with
/// `mark_unloaded` or the node assumes the cell is resident.
#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct CellStreamerNode {
    base: Base<Node3D>,
    /// Cells each side of the focus cell to keep loaded. 2 is Skyrim's
    /// `uGridsToLoad=5`.
    #[export]
    load_radius: i32,
    /// Cells each side beyond which a cell is released. At least the load
    /// radius, the gap is the hysteresis band.
    #[export]
    unload_radius: i32,
    #[export]
    max_loads_per_tick: i32,
    #[export]
    max_unloads_per_tick: i32,
    /// The node whose global position drives streaming, normally the player.
    #[export]
    focus: Option<Gd<Node3D>>,
    /// Merge a cell's surfaces per material into one mesh node each. Off
    /// gives one node per NIF shape, which is the unbatched baseline the
    /// benchmark compares against.
    #[export]
    batch_surfaces: bool,
    streamer: Option<CellStreamer>,
    /// Set by `load_data`. `stream_cell` needs this before it can read a
    /// real cell, the synthetic streaming above never touches it.
    data_source: Option<Arc<DataSource>>,
    data_load: Option<JoinHandle<Result<DataSource, crate::data_source::DataSourceError>>>,
    /// One entry per `RenderingServer` instance `stream_cell` or
    /// `stream_exterior` created, in creation order. The scene tree never
    /// sees them: a batch is a mesh RID and an instance RID in this node's
    /// scenario, freed in `exit_tree`, so a cell costs no `Node3D` at all.
    instances: Vec<Instance>,
    /// Temporary shader visuals keyed by their authored shader and target.
    effect_shaders: std::collections::HashMap<(GlobalFormId, GlobalFormId), EffectShaderInstance>,
    /// The `CELL` each streamed exterior grid position resolved to, so
    /// `release_exterior_cell` can find that cell's instances by the grid
    /// coordinate `cell_unload_requested` carries.
    exterior_cells: std::collections::HashMap<CellCoord, GlobalFormId>,
    /// `StandardMaterial3D` per texture set, kept across stream calls so a
    /// texture uploaded for one cell serves every later cell that names it.
    materials: MaterialMap,
    /// Merged bounds of everything streamed so far, in Godot metres, for a
    /// camera to frame the scene without walking mesh nodes.
    bounds: Option<crate::mesh::Aabb>,
    last_stats: VarDictionary,
    /// Static bodies this node put into the physics space, freed with the
    /// instances. Empty until `stream_cell_collision` is called, since a
    /// scene that only draws needs no collision.
    collision: crate::godot_physics::CellBodies,
    last_collision_stats: VarDictionary,
    /// The physics space `stream_cell_collision` last resolved, kept so
    /// `sync_reference_changes` can push moves and hides on the tick after,
    /// without re-resolving `get_world_3d` itself.
    collision_space: Option<Rid>,
    collision_cell: Option<GlobalFormId>,
    /// Host joints between linked references, released with either endpoint.
    tethers: crate::godot_tether::TetherConstraints,
    /// Every reference's last observed visibility and placement, gating the
    /// rendering rebake so an untouched cell costs one map lookup per
    /// reference per tick rather than a mesh rebuild. Collision has no such
    /// cache, see `CellBodies::sync_placements` for why.
    ref_cache: std::collections::HashMap<GlobalFormId, (bool, Iso)>,
    /// Distant-terrain LOD blocks standing in for full resolution terrain
    /// beyond the streamed cells. `None` until `stream_terrain_lod`.
    terrain_lod: Option<TerrainLodState>,
    /// Distant-object LOD blocks. Kept separately so terrain and object LOD
    /// can remain resident and visible at the same time.
    object_lod: Option<TerrainLodState>,
}

/// The node's distant-terrain LOD: the streamer that decides which blocks
/// are resident and records their bytes, the worldspace the blocks are read
/// from, and each resident block's drawn instances, freed when the streamer
/// releases the block. Kept apart from `instances` so `cell_instances`
/// keeps reporting full resolution cells only.
struct TerrainLodState {
    streamer: TerrainLodStreamer,
    world: GlobalFormId,
    world_editor_id: String,
    blocks: std::collections::HashMap<LodBlockCoord, Vec<LodInstance>>,
    last_step: LodStepCounts,
}

/// One drawn LOD surface. The mesh is held so the instance's mesh RID stays
/// valid while it draws.
struct LodInstance {
    rid: Rid,
    mesh: Gd<ArrayMesh>,
    triangles: usize,
}

/// What the last `stream_terrain_lod` or `update_terrain_lod` call did.
#[derive(Default)]
struct LodStepCounts {
    loaded: usize,
    missing: usize,
    failed: usize,
    released: usize,
}

/// One drawn batch: its instance and mesh RIDs and the side table that maps
/// a triangle back to the form that placed it.
struct Instance {
    cell: GlobalFormId,
    bounds: crate::mesh::Aabb,
    rid: Rid,
    mesh: InstanceMesh,
    table: BatchTable,
    /// The batch's arrays as first uploaded, kept so
    /// `CellStreamerNode::sync_reference_changes` recomputes a moved or
    /// hidden member's vertex range from this baseline every time, instead
    /// of composing a delta onto an already patched copy. Same reasoning as
    /// `StaticEntry::source` in `godot_physics`.
    arrays: SurfaceArrays,
}

/// The mesh resource an [`Instance`] drew: a merged or unbatched `ArrayMesh`
/// with the placement already baked into its vertices, or a `MultiMesh`
/// sharing one local-space `ArrayMesh` across a `Transform3D` per placement.
/// Both keep the `RenderingServer` instance's own transform at identity, so
/// freeing either only needs the enum, never a second lookup.
///
/// Neither payload is read again after construction: each variant exists to
/// hold the strong `Gd` reference the underlying `RefCounted` resource needs
/// to stay alive, since the `RenderingServer` instance's mesh RID does not
/// itself keep the resource from being freed. `free_instances` drops the
/// whole enum, which is enough, so `#[allow(dead_code)]` here documents an
/// RAII field rather than hiding an unused one.
#[allow(dead_code)]
enum InstanceMesh {
    Array(Gd<ArrayMesh>),
    Multi(Gd<MultiMesh>),
}

/// One transient RenderingServer visual and the resources it references.
#[allow(dead_code)]
struct EffectShaderInstance {
    rid: Rid,
    mesh: Gd<SphereMesh>,
    material: Gd<StandardMaterial3D>,
    remaining: Option<f32>,
    cell: Option<GlobalFormId>,
}

impl Drop for CellStreamerNode {
    fn drop(&mut self) {
        // The worker executes extension code. Join before the node and library
        // can unload, including an Escape exit during loading.
        if let Some(worker) = self.data_load.take() {
            let _ = worker.join();
        }
    }
}

/// One source surface's place in a batch, plus its load-time placement so a
/// runtime move or hide can be diffed against a known baseline. `origin` is
/// `LoadedSurface::placement`, the reference's transform at load time, in
/// Skyrim space: LAND terrain carries `Iso::IDENTITY` since it has no
/// separate placement.
struct BatchTableMember {
    reference: GlobalFormId,
    member: BatchMember,
    origin: Iso,
}

/// The side table for one instance: which form owns each triangle range and
/// where each member started, for `CellStreamerNode::sync_reference_changes`,
/// or, for a `MultiMesh` instance, which forms placed it (by placement index,
/// since a raycast hit resolves to a triangle in the shared local geometry,
/// not to a placement). `reference_at` cannot answer a `MultiMesh` triangle
/// today, see the module doc and `docs/godot-host.md`: `Instances` carries
/// the reference list for a picking path that would resolve a placement
/// index some other way (a per-instance collider, most likely), which is why
/// `#[allow(dead_code)]` sits on it rather than dropping data `reference_at`
/// cannot use yet. `sync_reference_changes` updates both table variants.
#[allow(dead_code)]
enum BatchTable {
    Triangles(Vec<BatchTableMember>),
    Instances(Vec<GlobalFormId>),
}

impl BatchTable {
    fn reference_of_triangle(&self, triangle: usize) -> Option<GlobalFormId> {
        let Self::Triangles(members) = self else {
            return None;
        };
        let at = members
            .partition_point(|m| m.member.first_triangle + m.member.triangle_count <= triangle);
        members
            .get(at)
            .filter(|m| triangle >= m.member.first_triangle)
            .map(|m| m.reference)
    }
}

/// Running totals over the cells one `stream_cell` or `stream_exterior`
/// call added, reported through `last_stats`.
#[derive(Default)]
struct StreamTotals {
    cells: usize,
    references: usize,
    statics: usize,
    furniture: usize,
    containers: usize,
    meshes_parsed: usize,
    meshes_failed: usize,
    triangles: usize,
    surfaces: usize,
    instances: usize,
    largest_batch: usize,
    land_cells: usize,
    water_planes: usize,
    grass_placements: usize,
    world: String,
    cell_list: Vec<String>,
}

impl StreamTotals {
    fn add_cell(&mut self, loaded: &LoadedCell) {
        self.cells += 1;
        self.water_planes += loaded.stats.water_plane_count;
        self.references += loaded.stats.references;
        self.statics += loaded.stats.statics_with_model;
        self.furniture += loaded.stats.furniture_with_model;
        self.containers += loaded.stats.containers_with_model;
        self.meshes_parsed += loaded.stats.meshes_parsed;
        self.meshes_failed += loaded.stats.meshes_failed;
        self.triangles += loaded.stats.triangles;
        self.surfaces += loaded.surfaces.len();
        self.grass_placements += loaded.stats.grass_placements;
    }

    fn to_dictionary(&self, materials: usize) -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("cells", self.cells as i64);
        d.set("references", self.references as i64);
        d.set("statics", self.statics as i64);
        d.set("furniture", self.furniture as i64);
        d.set("containers", self.containers as i64);
        d.set("meshes_parsed", self.meshes_parsed as i64);
        d.set("meshes_failed", self.meshes_failed as i64);
        d.set("triangles", self.triangles as i64);
        d.set("surfaces", self.surfaces as i64);
        d.set("instances", self.instances as i64);
        d.set("largest_batch", self.largest_batch as i64);
        d.set("materials", materials as i64);
        d.set("land_cells", self.land_cells as i64);
        d.set("water_planes", self.water_planes as i64);
        d.set("grass_placements", self.grass_placements as i64);
        d.set("world", self.world.as_str());
        d.set("cell_list", self.cell_list.join(" ").as_str());
        d
    }
}

#[godot_api]
impl INode3D for CellStreamerNode {
    fn init(base: Base<Node3D>) -> Self {
        let d = StreamConfig::default();
        Self {
            base,
            load_radius: d.load_radius,
            unload_radius: d.unload_radius,
            max_loads_per_tick: d.max_loads_per_tick as i32,
            max_unloads_per_tick: d.max_unloads_per_tick as i32,
            focus: None,
            batch_surfaces: true,
            streamer: None,
            data_source: None,
            data_load: None,
            instances: Vec::new(),
            effect_shaders: std::collections::HashMap::new(),
            exterior_cells: std::collections::HashMap::new(),
            materials: MaterialMap::new(),
            bounds: None,
            last_stats: VarDictionary::new(),
            collision: crate::godot_physics::CellBodies::default(),
            last_collision_stats: VarDictionary::new(),
            collision_space: None,
            collision_cell: None,
            tethers: crate::godot_tether::TetherConstraints::default(),
            ref_cache: std::collections::HashMap::new(),
            terrain_lod: None,
            object_lod: None,
        }
    }

    fn ready(&mut self) {
        self.reconfigure();
    }

    fn exit_tree(&mut self) {
        self.free_instances();
        self.free_effect_shaders();
        self.clear_terrain_lod();
        self.collision.free();
        self.tethers.free();
        self.collision_space = None;
        self.collision_cell = None;
        self.ref_cache.clear();
    }

    fn process(&mut self, _delta: f64) {
        let Some(focus) = self.focus.as_ref() else {
            return;
        };
        let p = focus.get_global_position();
        let sky = GodotPos::new(p.x, p.y, p.z).to_skyrim();
        let Some(streamer) = self.streamer.as_mut() else {
            return;
        };
        let plan = streamer.update(sky);
        for c in plan.load {
            self.signals().cell_load_requested().emit(c.x, c.y);
        }
        for c in plan.unload {
            self.signals().cell_unload_requested().emit(c.x, c.y);
        }
    }
}

impl CellStreamerNode {
    /// Apply queued game requests and advance finite shader durations.
    pub(crate) fn sync_effect_shaders(
        &mut self,
        world: &soe_game::World,
        requests: Vec<soe_game::world::EffectShaderRequest>,
        delta: f32,
    ) {
        let mut server = RenderingServer::singleton();
        for request in requests {
            let key = (request.shader, request.target);
            if !request.playing {
                if let Some(effect) = self.effect_shaders.remove(&key) {
                    server.free_rid(effect.rid);
                }
                continue;
            }
            let Some(placement) = world.ref_placement(request.target) else {
                continue;
            };
            let Some(scenario) = self.scenario() else {
                continue;
            };
            let position = SkyrimPos::new(
                placement.position[0],
                placement.position[1],
                placement.position[2],
            )
            .to_godot();
            let transform = Transform3D::new(
                Basis::IDENTITY.scaled(Vector3::splat(placement.scale.max(0.01))),
                Vector3::new(position.x, position.y, position.z),
            );
            if let Some(effect) = self.effect_shaders.get_mut(&key) {
                effect.remaining = (request.duration >= 0.0).then_some(request.duration);
                effect.cell = world
                    .reference_ref(request.target)
                    .and_then(|reference| reference.cell);
                server.instance_set_transform(effect.rid, transform);
                continue;
            }
            let mut mesh = SphereMesh::new_gd();
            mesh.set_radius(0.75);
            mesh.set_height(1.5);
            let mut material = StandardMaterial3D::new_gd();
            material.set_shading_mode(godot::classes::base_material_3d::ShadingMode::UNSHADED);
            material.set_transparency(Transparency::ALPHA);
            material.set_albedo(Color::from_rgba(0.35, 0.65, 1.0, 0.45));
            material.set_feature(Feature::EMISSION, true);
            material.set_emission(Color::from_rgb(0.25, 0.55, 1.0));
            let rid = server.instance_create2(mesh.get_rid(), scenario);
            server.instance_set_transform(rid, transform);
            server.instance_geometry_set_material_override(rid, material.get_rid());
            self.effect_shaders.insert(
                key,
                EffectShaderInstance {
                    rid,
                    mesh,
                    material,
                    remaining: (request.duration >= 0.0).then_some(request.duration),
                    cell: world
                        .reference_ref(request.target)
                        .and_then(|reference| reference.cell),
                },
            );
        }
        self.effect_shaders.retain(|(_, target), effect| {
            let Some(placement) = world.ref_placement(*target).filter(|p| p.visible) else {
                server.free_rid(effect.rid);
                return false;
            };
            let position = SkyrimPos::new(
                placement.position[0],
                placement.position[1],
                placement.position[2],
            )
            .to_godot();
            let transform = Transform3D::new(
                Basis::IDENTITY.scaled(Vector3::splat(placement.scale.max(0.01))),
                Vector3::new(position.x, position.y, position.z),
            );
            server.instance_set_transform(effect.rid, transform);
            effect.cell = world
                .reference_ref(*target)
                .and_then(|reference| reference.cell);
            let Some(remaining) = effect.remaining.as_mut() else {
                return true;
            };
            *remaining -= delta.max(0.0);
            if *remaining <= 0.0 {
                server.free_rid(effect.rid);
                false
            } else {
                true
            }
        });
    }

    fn free_effect_shaders(&mut self) {
        let mut server = RenderingServer::singleton();
        for (_, effect) in self.effect_shaders.drain() {
            server.free_rid(effect.rid);
        }
    }

    pub(crate) fn shared_source(&self) -> Option<Arc<DataSource>> {
        self.data_source.clone()
    }

    /// Install new cells before releasing departed cells. Retained instances keep their RIDs.
    pub(crate) fn apply_cell_delta(
        &mut self,
        entered: &[crate::data_source::PreparedCell],
        departed: &[GlobalFormId],
    ) -> bool {
        let Some(scenario) = self.scenario() else {
            return false;
        };
        let before = self.materials.len();
        let mut totals = StreamTotals::default();
        for cell in entered {
            totals.cell_list.push(cell.id.to_string());
            self.add_cell(cell.id, &cell.loaded, scenario, &mut totals);
        }
        let mut server = RenderingServer::singleton();
        self.effect_shaders.retain(|_, effect| {
            if effect.cell.is_some_and(|cell| departed.contains(&cell)) {
                server.free_rid(effect.rid);
                false
            } else {
                true
            }
        });
        self.instances.retain(|instance| {
            if departed.contains(&instance.cell) {
                server.free_rid(instance.rid);
                false
            } else {
                true
            }
        });
        self.bounds = self
            .instances
            .iter()
            .map(|i| i.bounds)
            .reduce(crate::batch::merge_bounds);
        self.last_stats = totals.to_dictionary(self.materials.len() - before);
        true
    }

    /// Replace drawn resources after CPU preparation succeeds. No per-reference nodes.
    pub(crate) fn replace_location(
        &mut self,
        location: &crate::data_source::LoadedLocation,
    ) -> bool {
        let Some(scenario) = self.scenario() else {
            return false;
        };
        self.clear();
        let before = self.materials.len();
        let mut totals = StreamTotals::default();
        for (id, cell) in &location.cells {
            totals.cell_list.push(id.to_string());
            self.add_cell(*id, cell, scenario, &mut totals);
        }
        self.last_stats = totals.to_dictionary(self.materials.len() - before);
        true
    }

    /// Applies game-side changes to collision, copies simulated rigid-body
    /// poses into World, then updates rendering from that shared state.
    ///
    /// Rendering diffs each reference against `ref_cache` before visiting
    /// batches, so every surface of a changed reference updates this tick.
    ///
    /// A disabled reference is collapsed to a degenerate, zero-area patch
    /// rather than dropped from the batch: the batch is one shared
    /// `RenderingServer` instance and index buffer, and removing vertices
    /// would renumber every triangle after it. A moved reference gets a
    /// fresh vertex range computed from `Instance::arrays`, the pristine
    /// upload, never from an already patched copy, matching
    /// `StaticEntry::source` on the collision side.
    pub(crate) fn sync_reference_changes(&mut self, world: &mut soe_game::World) {
        let stale_tethers: Vec<_> = self
            .tether_endpoints()
            .into_iter()
            .filter(|id| {
                world
                    .ref_placement(*id)
                    .is_none_or(|placement| !placement.visible)
            })
            .collect();
        for id in stale_tethers {
            self.tethers.release_reference(id);
        }
        if let Some(space) = self.collision_space {
            self.collision.ensure_references(world);
            self.collision.sync_placements(world, space);
            self.collision.poll_movable(world);
        }
        let mut changed = std::collections::HashSet::new();
        let mut track = |reference: GlobalFormId| {
            let Some(placement) = world.ref_placement(reference) else {
                return;
            };
            let iso = Iso::placement(placement.position, placement.rotation, placement.scale);
            let key = (placement.visible, iso);
            if self.ref_cache.insert(reference, key) != Some(key) {
                changed.insert(reference);
            }
        };
        for instance in &self.instances {
            match &instance.table {
                BatchTable::Triangles(members) => {
                    for member in members {
                        track(member.reference);
                    }
                }
                BatchTable::Instances(members) => {
                    for reference in members {
                        track(*reference);
                    }
                }
            }
        }
        let mut server = RenderingServer::singleton();
        for instance in &mut self.instances {
            let BatchTable::Triangles(members) = &instance.table else {
                if let (BatchTable::Instances(members), InstanceMesh::Multi(multi)) =
                    (&instance.table, &mut instance.mesh)
                {
                    for (index, reference) in members.iter().enumerate() {
                        let Some(placement) = world.ref_placement(*reference) else {
                            continue;
                        };
                        let target =
                            Iso::placement(placement.position, placement.rotation, placement.scale);
                        if !changed.contains(reference) {
                            continue;
                        }
                        let scale = if placement.visible {
                            placement.scale
                        } else {
                            0.0
                        };
                        let transform =
                            placement_to_godot(target.rotation.rows, placement.position, scale);
                        multi.set_instance_transform(index as i32, transform3d_from(transform));
                    }
                }
                continue;
            };
            let mut touched = false;
            let mut targets: Vec<(usize, bool, Iso)> = Vec::new();
            for (index, member) in members.iter().enumerate() {
                let Some(placement) = world.ref_placement(member.reference) else {
                    continue;
                };
                let target_iso =
                    Iso::placement(placement.position, placement.rotation, placement.scale);
                if changed.contains(&member.reference) {
                    touched = true;
                }
                targets.push((index, placement.visible, target_iso));
            }
            if !touched {
                continue;
            }
            // Rebuilt fresh from `instance.arrays`, the pristine upload,
            // every time: reapplying every live member's current placement
            // rather than composing this tick's delta onto the previous
            // rebuild's output keeps a reference that moves every tick from
            // drifting off its true position.
            let mut patched = instance.arrays.clone();
            for (index, visible, target_iso) in targets {
                apply_member_patch(&mut patched, &members[index], visible, &target_iso);
            }
            let new_mesh = array_mesh_from(&patched);
            server.instance_set_base(instance.rid, new_mesh.get_rid());
            instance.mesh = InstanceMesh::Array(new_mesh);
        }
    }

    fn tether_endpoints(&self) -> Vec<GlobalFormId> {
        self.tethers.endpoints()
    }
}

#[godot_api]
impl CellStreamerNode {
    #[signal]
    fn cell_load_requested(cell_x: i32, cell_y: i32);

    #[signal]
    fn cell_unload_requested(cell_x: i32, cell_y: i32);

    /// Rebuild the streamer from the exported settings. Resident cells are
    /// forgotten, so call it before any cell is loaded or expect every cell
    /// to be requested again.
    #[func]
    pub fn reconfigure(&mut self) {
        let config = StreamConfig {
            load_radius: self.load_radius,
            unload_radius: self.unload_radius,
            max_loads_per_tick: self.max_loads_per_tick.max(0) as usize,
            max_unloads_per_tick: self.max_unloads_per_tick.max(0) as usize,
        };
        match CellStreamer::new(config) {
            Ok(s) => self.streamer = Some(s),
            Err(e) => {
                godot_error!("CellStreamer: {e}");
                self.streamer = None;
            }
        }
    }

    #[func]
    pub fn mark_unloaded(&mut self, cell_x: i32, cell_y: i32) {
        if let Some(s) = self.streamer.as_mut() {
            s.mark_unloaded(CellCoord::new(cell_x, cell_y));
        }
    }

    /// Creates the cart-to-horse host pin joint once both rigid bodies are
    /// resident. This node owns the joint RID and frees it with either form.
    #[func]
    pub fn tether_bodies(
        &mut self,
        cart_form: i64,
        horse_form: i64,
        cart_body: Rid,
        horse_body: Rid,
        cart_pivot: Vector3,
        horse_pivot: Vector3,
    ) -> bool {
        let Some(cart) = tether_form_id(cart_form) else {
            return false;
        };
        let Some(horse) = tether_form_id(horse_form) else {
            return false;
        };
        self.tethers
            .tether(cart, horse, cart_body, horse_body, cart_pivot, horse_pivot)
    }

    /// Explicitly removes every tether touching a reference.
    #[func]
    pub fn untether_reference(&mut self, file: i32, object: i64) -> i32 {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return 0;
        };
        self.tethers
            .release_reference(GlobalFormId { file, object }) as i32
    }

    #[func]
    pub fn tether_count(&self) -> i32 {
        self.tethers.link_count() as i32
    }

    #[func]
    pub fn resident_count(&self) -> i32 {
        self.streamer
            .as_ref()
            .map_or(0, |s| s.resident_count() as i32)
    }

    #[func]
    pub fn is_resident(&self, cell_x: i32, cell_y: i32) -> bool {
        self.streamer
            .as_ref()
            .is_some_and(|s| s.is_resident(CellCoord::new(cell_x, cell_y)))
    }

    /// Opens `path` as a Skyrim SE `Data` folder: the five base masters and
    /// their archives, plus `path` itself as a loose overlay. Returns
    /// whether it opened, the reason goes to Godot's error log. Call this
    /// once before `stream_cell`.
    // gdext's `#[func]` marshals a GDScript argument into an owned value, so
    // there is no borrowed form to take instead.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn load_data(&mut self, path: GString) -> bool {
        if self.data_load.is_some() {
            godot_error!("CellStreamerNode::load_data: a background load is pending");
            return false;
        }
        match DataSource::open(Path::new(&path.to_string())) {
            Ok(source) => {
                self.data_source = Some(Arc::new(source));
                true
            }
            Err(e) => {
                godot_error!("CellStreamerNode::load_data: {e}");
                self.data_source = None;
                false
            }
        }
    }

    /// Start CPU-only data loading. Poll from the main thread before streaming.
    /// A second request is rejected until the first result has been collected.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn begin_load_data(&mut self, path: GString) -> bool {
        if self.data_load.is_some() {
            return false;
        }
        let path = std::path::PathBuf::from(path.to_string());
        match std::thread::Builder::new()
            .name("soe-data-load".into())
            .spawn(move || DataSource::open(&path))
        {
            Ok(worker) => {
                self.data_source = None;
                self.data_load = Some(worker);
                true
            }
            Err(e) => {
                godot_error!("CellStreamerNode::begin_load_data: {e}");
                false
            }
        }
    }

    /// Zero while loading, one when ready, minus one on failure or no request.
    #[func]
    pub fn poll_load_data(&mut self) -> i32 {
        let Some(worker) = self.data_load.as_ref() else {
            return if self.data_source.is_some() { 1 } else { -1 };
        };
        if !worker.is_finished() {
            return 0;
        }
        let Some(worker) = self.data_load.take() else {
            return -1;
        };
        match worker.join() {
            Ok(Ok(source)) => {
                self.data_source = Some(Arc::new(source));
                1
            }
            Ok(Err(e)) => {
                godot_error!("CellStreamerNode::poll_load_data: {e}");
                -1
            }
            Err(_) => {
                godot_error!("CellStreamerNode::poll_load_data: worker panicked");
                -1
            }
        }
    }

    /// Resolves `editor_id` against the data loaded by `load_data`, draws
    /// the cell's surfaces as `RenderingServer` instances in this node's
    /// scenario (one per material set when `batch_surfaces` is on, one per
    /// surface otherwise), and returns how many were created. Returns -1 if
    /// `load_data` has not opened a data source or the node is outside the
    /// tree, 0 if the cell resolved but nothing built (either it is empty or
    /// every mesh failed, check the Godot error log).
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn stream_cell(&mut self, editor_id: GString) -> i32 {
        let Some(source) = self.data_source.as_ref() else {
            godot_error!("CellStreamerNode::stream_cell: load_data was not called");
            return -1;
        };
        let Some(cell_id) = source.resolve_cell(&editor_id.to_string()) else {
            return -1;
        };
        let loaded = match source.stream_cell_id(cell_id) {
            Ok(loaded) => loaded,
            Err(e) => {
                godot_error!("CellStreamerNode::stream_cell: {e}");
                return -1;
            }
        };
        let Some(scenario) = self.scenario() else {
            return -1;
        };
        let materials_before = self.materials.len();
        let mut totals = StreamTotals::default();
        totals.cell_list.push(editor_id.to_string());
        self.add_cell(cell_id, &loaded, scenario, &mut totals);
        self.last_stats = totals.to_dictionary(self.materials.len() - materials_before);
        totals.instances as i32
    }

    /// Streams the square of exterior cells within `radius` of the cell
    /// named `editor_id` (2 is the game's `uGridsToLoad=5`), each cell's
    /// statics and its terrain, batched per cell so Godot still culls per
    /// cell. Returns the instances created, -1 on failure, and fills
    /// `last_stats` with the block's totals and the cell list.
    ///
    /// Streams and converts one cell at a time (`stream_exterior_each`)
    /// instead of collecting the whole block into a `Vec` first: each
    /// `LoadedCell`'s decoded meshes and textures are handed to `add_cell`
    /// for RenderingServer upload and then dropped before the next cell is
    /// read, so the block's raw CPU-side geometry never all sits in memory
    /// at once. This is the PRF-02 fix for the real exterior benchmark's
    /// startup RSS budget (docs/performance-limits.md).
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn stream_exterior(&mut self, editor_id: GString, radius: i32) -> i32 {
        let Some(source) = self.data_source.as_ref() else {
            godot_error!("CellStreamerNode::stream_exterior: load_data was not called");
            return -1;
        };
        let block = match source.resolve_exterior_block(&editor_id.to_string(), radius) {
            Ok(block) => block,
            Err(e) => {
                godot_error!("CellStreamerNode::stream_exterior: {e}");
                return -1;
            }
        };
        let Some(scenario) = self.scenario() else {
            return -1;
        };
        let materials_before = self.materials.len();
        let mut totals = StreamTotals {
            world: block.world_editor_id.clone().unwrap_or_default(),
            ..StreamTotals::default()
        };
        let source = Arc::clone(source);
        let result = source.stream_exterior_each(&block, |cell, loaded_cell, has_land| {
            if has_land {
                totals.land_cells += 1;
            }
            totals.cell_list.push(format!("({},{})", cell.x, cell.y));
            self.exterior_cells
                .insert(CellCoord::new(cell.x, cell.y), cell.id);
            self.add_cell(cell.id, &loaded_cell, scenario, &mut totals);
        });
        if let Err(e) = result {
            godot_error!("CellStreamerNode::stream_exterior: {e}");
            return -1;
        }
        self.last_stats = totals.to_dictionary(self.materials.len() - materials_before);
        totals.instances as i32
    }

    /// Totals from the last `stream_cell` or `stream_exterior` call: cells,
    /// references, statics, meshes_parsed, meshes_failed, triangles,
    /// surfaces, instances, largest_batch, materials (new in that call),
    /// land_cells, world, cell_list.
    #[func]
    pub fn last_stats(&self) -> VarDictionary {
        self.last_stats.clone()
    }

    /// One dictionary per drawn instance that carries a water plane:
    /// `cell` (the owning cell's form id as text), `rid` (the
    /// RenderingServer instance), `height` (the plane's Godot y in metres),
    /// `material` (the material set as the instance's override),
    /// `opacity` (the WATR `ANAM` byte) and `shallow_color`. Lets a scene
    /// script check what is drawn against the record it came from.
    #[func]
    pub fn water_planes(&mut self) -> Array<VarDictionary> {
        let mut out = Array::new();
        let drawn: Vec<(GlobalFormId, Rid, MaterialDesc, f32)> = self
            .instances
            .iter()
            .filter(|i| i.arrays.material.water.is_some())
            .map(|i| (i.cell, i.rid, i.arrays.material.clone(), i.bounds.min.y))
            .collect();
        for (cell, rid, desc, height) in drawn {
            let Some(water) = desc.water.as_ref() else {
                continue;
            };
            // The cache hands back the same material `add_cell` set as the
            // override, the key covers every water field it reads.
            let material = material_for(&mut self.materials, &desc, &SurfaceTextures::default());
            let [r, g, b] = water.shallow_color;
            let mut d = VarDictionary::new();
            d.set("cell", cell.to_string().as_str());
            d.set("rid", rid);
            d.set("height", height);
            d.set("material", &material);
            d.set("opacity", i64::from(water.opacity));
            d.set("shallow_color", Color::from_rgb(r, g, b));
            out.push(&d);
        }
        out
    }

    /// Instances alive in this node's scenario.
    #[func]
    pub fn instance_count(&self) -> i32 {
        self.instances.len() as i32
    }

    /// Render handles grouped by owning cell, for residency diagnostics.
    #[func]
    pub fn cell_instances(&self) -> VarDictionary {
        let mut groups = std::collections::BTreeMap::<GlobalFormId, Vec<i64>>::new();
        for instance in &self.instances {
            groups
                .entry(instance.cell)
                .or_default()
                .push(instance.rid.to_u64() as i64);
        }
        let mut result = VarDictionary::new();
        for (cell, ids) in groups {
            result.set(
                cell.to_string(),
                &ids.into_iter().collect::<PackedInt64Array>(),
            );
        }
        result
    }

    /// What the exterior cell at grid `cell_x`, `cell_y` currently draws:
    /// `instances` (every batch), `multimeshes` (instanced batches, where
    /// grass lands) and `multimesh_members` (placements across them). All
    /// zero when the cell is not resident.
    #[func]
    pub fn exterior_cell_batches(&self, cell_x: i32, cell_y: i32) -> VarDictionary {
        let owner = self
            .exterior_cells
            .get(&CellCoord::new(cell_x, cell_y))
            .copied();
        let mut instances = 0_i64;
        let mut multimeshes = 0_i64;
        let mut members = 0_i64;
        for instance in self.instances.iter().filter(|i| Some(i.cell) == owner) {
            instances += 1;
            if let BatchTable::Instances(refs) = &instance.table {
                multimeshes += 1;
                members += refs.len() as i64;
            }
        }
        let mut result = VarDictionary::new();
        result.set("instances", instances);
        result.set("multimeshes", multimeshes);
        result.set("multimesh_members", members);
        result
    }

    /// Frees every batch the exterior cell at grid `cell_x`, `cell_y` drew,
    /// grass `MultiMesh` batches included, and marks the cell unloaded in
    /// the streamer. This is the answer to `cell_unload_requested`: without
    /// it a streamed cell's batches stay in the scenario until `clear`.
    /// Returns the number of batches freed, 0 when the cell is not resident.
    #[func]
    pub fn release_exterior_cell(&mut self, cell_x: i32, cell_y: i32) -> i32 {
        let coord = CellCoord::new(cell_x, cell_y);
        if let Some(s) = self.streamer.as_mut() {
            s.mark_unloaded(coord);
        }
        let Some(owner) = self.exterior_cells.remove(&coord) else {
            return 0;
        };
        if self.collision_cell == Some(owner) {
            self.collision.free();
            self.collision_cell = None;
            self.collision_space = None;
        }
        let (released, kept): (Vec<Instance>, Vec<Instance>) = std::mem::take(&mut self.instances)
            .into_iter()
            .partition(|i| i.cell == owner);
        self.instances = kept;
        let mut server = RenderingServer::singleton();
        let count = released.len();
        for instance in released {
            server.free_rid(instance.rid);
            drop(instance.mesh);
        }
        self.bounds = self
            .instances
            .iter()
            .map(|i| i.bounds)
            .reduce(crate::batch::merge_bounds);
        count as i32
    }

    /// Merged bounds of everything streamed so far, in Godot metres. An
    /// empty `AABB` before the first stream call.
    #[func]
    pub fn scene_bounds(&self) -> Aabb {
        self.bounds.map_or_else(Aabb::default, |b| {
            Aabb::new(
                Vector3::new(b.min.x, b.min.y, b.min.z),
                Vector3::new(b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z),
            )
        })
    }

    /// Builds the cell's Havok collision into this node's physics space and
    /// returns how many static bodies were created, or -1 on failure.
    ///
    /// Godot's own physics server owns the result, and from 4.4 that server
    /// is Jolt, so the engine links no physics library of its own. The
    /// headless simulation keeps its own world, see `docs/physics.md` for why
    /// the two exist. Call `collision_stats` after this for the counts.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn stream_cell_collision(&mut self, editor_id: GString) -> i32 {
        let Some(source) = self.data_source.as_ref() else {
            godot_error!("stream_cell_collision: load_data was not called");
            return -1;
        };
        let Some(cell_id) = source.resolve_cell(&editor_id.to_string()) else {
            godot_error!("stream_cell_collision: no cell named {editor_id}");
            return -1;
        };
        let (colliders, read) = match source.placed_cell_colliders(cell_id) {
            Ok(pair) => pair,
            Err(e) => {
                godot_error!("stream_cell_collision: {e}");
                return -1;
            }
        };
        let Some(space) = self
            .base()
            .get_world_3d()
            .map(|w| w.get_space())
            .filter(|s| s.is_valid())
        else {
            godot_error!("stream_cell_collision: the node is outside a 3D world");
            return -1;
        };
        self.collision.free();
        self.collision = crate::godot_physics::add_placed_statics(space, &colliders);
        self.collision_space = Some(space);
        self.collision_cell = Some(cell_id);
        let built = &self.collision.stats;
        self.last_collision_stats = {
            let mut d = VarDictionary::new();
            d.set("references", read.references as i64);
            d.set("with_collision", read.with_collision as i64);
            d.set("colliders", read.colliders as i64);
            d.set("failed", read.failed as i64);
            d.set("bodies", built.bodies as i64);
            d.set("shapes", built.shapes as i64);
            d.set("triangles", built.triangles as i64);
            d.set("skipped", built.skipped as i64);
            d
        };
        built.bodies as i32
    }

    /// The counts from the last `stream_cell_collision` call.
    #[func]
    pub fn collision_stats(&self) -> VarDictionary {
        let mut stats = self.last_collision_stats.clone();
        stats.set("bodies", self.collision.stats.bodies as i64);
        stats.set("shapes", self.collision.stats.shapes as i64);
        stats.set("movable", self.collision.movable_count() as i64);
        stats
    }

    /// Current host body poses for movable references in Skyrim units.
    #[func]
    pub fn movable_reference_poses(&self) -> Array<VarDictionary> {
        self.collision.movable_poses()
    }

    /// Last placement applied to a drawn reference, in Skyrim units.
    #[func]
    pub fn rendered_reference_pose(&self, file: i32, object: i64) -> VarDictionary {
        let mut row = VarDictionary::new();
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return row;
        };
        let Some((visible, pose)) = self.ref_cache.get(&GlobalFormId { file, object }) else {
            return row;
        };
        row.set(
            "position",
            Vector3::new(pose.translation.x, pose.translation.y, pose.translation.z),
        );
        row.set("visible", *visible);
        row
    }

    /// Largest difference between a reference's uploaded geometry and its
    /// cached World placement, in Godot metres. Returns -1 if it is not drawn.
    #[func]
    pub fn render_alignment_error(&self, file: i32, object: i64) -> f32 {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return -1.0;
        };
        let reference = GlobalFormId { file, object };
        let Some((visible, target)) = self.ref_cache.get(&reference) else {
            return -1.0;
        };
        let mut largest: f32 = 0.0;
        let mut found = false;
        for instance in &self.instances {
            match (&instance.table, &instance.mesh) {
                (BatchTable::Triangles(members), InstanceMesh::Array(mesh)) => {
                    let actual = mesh.surface_get_arrays(0);
                    let Some(vertices) = actual
                        .get(ArrayType::VERTEX.ord() as usize)
                        .and_then(|value| value.try_to::<PackedVector3Array>().ok())
                    else {
                        continue;
                    };
                    for member in members.iter().filter(|m| m.reference == reference) {
                        found = true;
                        let mut expected = instance.arrays.clone();
                        apply_member_patch(&mut expected, member, *visible, target);
                        for index in member.member.first_vertex
                            ..member.member.first_vertex + member.member.vertex_count
                        {
                            let (Some(point), Some(want)) =
                                (vertices.get(index), expected.positions.get(index))
                            else {
                                continue;
                            };
                            largest = largest
                                .max((point - Vector3::new(want[0], want[1], want[2])).length());
                        }
                    }
                }
                (BatchTable::Instances(members), InstanceMesh::Multi(multi)) => {
                    let position = [
                        target.translation.x,
                        target.translation.y,
                        target.translation.z,
                    ];
                    let scale = if *visible { target.scale } else { 0.0 };
                    let expected =
                        transform3d_from(placement_to_godot(target.rotation.rows, position, scale));
                    for (index, member) in members.iter().enumerate() {
                        if *member != reference {
                            continue;
                        }
                        found = true;
                        let actual = multi.get_instance_transform(index as i32);
                        for axis in [Vector3::RIGHT, Vector3::UP, Vector3::BACK, Vector3::ZERO] {
                            largest = largest.max((actual * axis - expected * axis).length());
                        }
                    }
                }
                _ => {}
            }
        }
        if found {
            largest
        } else {
            -1.0
        }
    }

    /// Starts distant-terrain LOD for the worldspace named
    /// `world_editor_id` around cell (`cell_x`, `cell_y`) and settles it:
    /// every BTR block whose nearest cell lies beyond `full_res_radius` and
    /// within `lod_radius` (ring distance in cells) is read, drawn as
    /// `RenderingServer` instances in this node's scenario, and its built
    /// bytes recorded against `budget_bytes`, 0 for no cap. Over the cap the
    /// farthest blocks are released until the recorded usage fits.
    ///
    /// Pass the radius `stream_exterior` drew full resolution cells at as
    /// `full_res_radius`. A block is planned only when all of its cells lie
    /// beyond that radius, so LOD takes over where full resolution LAND
    /// stops and never draws over it. The block grid and level come from the
    /// worldspace's `lodsettings` file, a grid anchored at cell (0, 0) at
    /// level 4 when there is none. Replaces any earlier LOD state. Returns
    /// the resident block count, or -1 on failure with the reason in the
    /// Godot error log.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn stream_terrain_lod(
        &mut self,
        world_editor_id: GString,
        cell_x: i32,
        cell_y: i32,
        full_res_radius: i32,
        lod_radius: i32,
        budget_bytes: i64,
    ) -> i32 {
        self.clear_terrain_lod();
        let Some(source) = self.data_source.clone() else {
            godot_error!("CellStreamerNode::stream_terrain_lod: load_data was not called");
            return -1;
        };
        let Ok(budget_bytes) = u64::try_from(budget_bytes) else {
            godot_error!("CellStreamerNode::stream_terrain_lod: budget {budget_bytes} is negative");
            return -1;
        };
        let Some((world, world_editor_id)) = source.resolve_world(&world_editor_id.to_string())
        else {
            godot_error!("CellStreamerNode::stream_terrain_lod: no worldspace {world_editor_id}");
            return -1;
        };
        let settings = match source.lod_settings(&world_editor_id) {
            Ok(settings) => settings,
            Err(e) => {
                godot_error!("CellStreamerNode::stream_terrain_lod: {e}");
                return -1;
            }
        };
        let config = TerrainLodConfig {
            level: settings
                .and_then(|s| s.finest_level())
                .unwrap_or(LodLevel::L4),
            full_res_radius,
            lod_radius,
            budget_bytes,
            ..TerrainLodConfig::default()
        };
        let anchor = settings.map_or(CellCoord::new(0, 0), |s| s.sw_cell);
        let streamer = match TerrainLodStreamer::with_anchor(config, anchor) {
            Ok(streamer) => streamer,
            Err(e) => {
                godot_error!("CellStreamerNode::stream_terrain_lod: {e}");
                return -1;
            }
        };
        self.terrain_lod = Some(TerrainLodState {
            streamer,
            world,
            world_editor_id,
            blocks: std::collections::HashMap::new(),
            last_step: LodStepCounts::default(),
        });
        let focus = CellCoord::new(cell_x, cell_y);
        if self
            .lod_cycle(focus, true, "stream_terrain_lod", false)
            .is_none()
        {
            return -1;
        }
        self.terrain_lod_resident_count()
    }

    /// Starts distant object LOD for the named worldspace. BTO blocks use
    /// the same block grid and distance band as terrain LOD, and are only
    /// requested beyond `full_res_radius`, where full resolution references
    /// have left the streamed cell set. Returns the resident block count,
    /// or -1 on failure.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn stream_object_lod(
        &mut self,
        world_editor_id: GString,
        cell_x: i32,
        cell_y: i32,
        full_res_radius: i32,
        lod_radius: i32,
        budget_bytes: i64,
    ) -> i32 {
        self.clear_object_lod();
        let Some(source) = self.data_source.clone() else {
            godot_error!("CellStreamerNode::stream_object_lod: load_data was not called");
            return -1;
        };
        let Ok(budget_bytes) = u64::try_from(budget_bytes) else {
            godot_error!("CellStreamerNode::stream_object_lod: budget {budget_bytes} is negative");
            return -1;
        };
        let Some((world, world_editor_id)) = source.resolve_world(&world_editor_id.to_string())
        else {
            godot_error!("CellStreamerNode::stream_object_lod: no worldspace {world_editor_id}");
            return -1;
        };
        let settings = match source.lod_settings(&world_editor_id) {
            Ok(settings) => settings,
            Err(e) => {
                godot_error!("CellStreamerNode::stream_object_lod: {e}");
                return -1;
            }
        };
        let config = TerrainLodConfig {
            level: settings
                .and_then(|s| s.finest_level())
                .unwrap_or(LodLevel::L4),
            full_res_radius,
            lod_radius,
            budget_bytes,
            ..TerrainLodConfig::default()
        };
        let anchor = settings.map_or(CellCoord::new(0, 0), |s| s.sw_cell);
        let streamer = match TerrainLodStreamer::with_anchor(config, anchor) {
            Ok(streamer) => streamer,
            Err(e) => {
                godot_error!("CellStreamerNode::stream_object_lod: {e}");
                return -1;
            }
        };
        self.object_lod = Some(TerrainLodState {
            streamer,
            world,
            world_editor_id,
            blocks: std::collections::HashMap::new(),
            last_step: LodStepCounts::default(),
        });
        if self
            .lod_cycle(
                CellCoord::new(cell_x, cell_y),
                true,
                "stream_object_lod",
                true,
            )
            .is_none()
        {
            return -1;
        }
        self.object_lod
            .as_ref()
            .map_or(0, |lod| lod.streamer.resident_count() as i32)
    }

    /// One incremental LOD step for a focus at cell (`cell_x`, `cell_y`),
    /// with the settings the last `stream_terrain_lod` call gave: at most
    /// one block read and two released, then the budget enforced. Call it
    /// when the focus changes cell, alongside the cell streamer, to keep
    /// the LOD ring following the player without a long frame. Returns the
    /// blocks drawn by this call, or -1 when `stream_terrain_lod` has not
    /// run or the node is outside a world.
    #[func]
    pub fn update_terrain_lod(&mut self, cell_x: i32, cell_y: i32) -> i32 {
        self.lod_cycle(
            CellCoord::new(cell_x, cell_y),
            false,
            "update_terrain_lod",
            false,
        )
        .map_or(-1, |drawn| drawn as i32)
    }

    /// One incremental object LOD step at cell (`cell_x`, `cell_y`). Call
    /// when the focus changes cell. Returns the blocks drawn by this call,
    /// or -1 when `stream_object_lod` has not run or the node is outside a
    /// world.
    #[func]
    pub fn update_object_lod(&mut self, cell_x: i32, cell_y: i32) -> i32 {
        self.lod_cycle(
            CellCoord::new(cell_x, cell_y),
            false,
            "update_object_lod",
            true,
        )
        .map_or(-1, |drawn| drawn as i32)
    }

    /// LOD blocks resident now, each drawn in this node's scenario.
    #[func]
    pub fn terrain_lod_resident_count(&self) -> i32 {
        self.terrain_lod
            .as_ref()
            .map_or(0, |lod| lod.streamer.resident_count() as i32)
    }

    /// Recorded bytes across resident LOD blocks, the residency budget's
    /// current reading.
    #[func]
    pub fn terrain_lod_usage_bytes(&self) -> i64 {
        self.terrain_lod
            .as_ref()
            .map_or(0, |lod| lod.streamer.usage_bytes() as i64)
    }

    /// The LOD state as a dictionary: world, level, anchor_x, anchor_y,
    /// full_res_radius, lod_radius, budget_bytes, usage_bytes, over_budget,
    /// resident, absent, instances, triangles, and from the last call
    /// loaded, missing, failed and released. Empty before
    /// `stream_terrain_lod`.
    #[func]
    pub fn terrain_lod_stats(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(lod) = self.terrain_lod.as_ref() else {
            return d;
        };
        let config = lod.streamer.config();
        let anchor = lod.streamer.anchor();
        let drawn = lod.blocks.values().flatten();
        d.set("world", lod.world_editor_id.as_str());
        d.set("level", i64::from(config.level.cells()));
        d.set("anchor_x", i64::from(anchor.x));
        d.set("anchor_y", i64::from(anchor.y));
        d.set("full_res_radius", i64::from(config.full_res_radius));
        d.set("lod_radius", i64::from(config.lod_radius));
        d.set("budget_bytes", config.budget_bytes as i64);
        d.set("usage_bytes", lod.streamer.usage_bytes() as i64);
        d.set("over_budget", lod.streamer.over_budget());
        d.set("resident", lod.streamer.resident_count() as i64);
        d.set("absent", lod.streamer.absent_count() as i64);
        d.set("instances", drawn.clone().count() as i64);
        d.set(
            "triangles",
            drawn.map(|i| i.triangles).sum::<usize>() as i64,
        );
        d.set("loaded", lod.last_step.loaded as i64);
        d.set("missing", lod.last_step.missing as i64);
        d.set("failed", lod.last_step.failed as i64);
        d.set("released", lod.last_step.released as i64);
        d
    }

    /// Frees every LOD block this node drew and forgets the LOD settings.
    #[func]
    pub fn clear_terrain_lod(&mut self) {
        let Some(lod) = self.terrain_lod.take() else {
            return;
        };
        let mut server = RenderingServer::singleton();
        for instance in lod.blocks.into_values().flatten() {
            server.free_rid(instance.rid);
            drop(instance.mesh);
        }
    }

    /// Frees every object LOD block this node drew and forgets its settings.
    #[func]
    pub fn clear_object_lod(&mut self) {
        let Some(lod) = self.object_lod.take() else {
            return;
        };
        let mut server = RenderingServer::singleton();
        for instance in lod.blocks.into_values().flatten() {
            server.free_rid(instance.rid);
            drop(instance.mesh);
        }
    }

    /// Frees every instance and mesh this node created, terrain LOD
    /// included, and forgets the bounds. Materials stay cached.
    #[func]
    pub fn clear(&mut self) {
        self.collision.free();
        self.tethers.free();
        self.collision_space = None;
        self.collision_cell = None;
        self.free_instances();
        self.free_effect_shaders();
        self.exterior_cells.clear();
        self.ref_cache.clear();
        self.clear_terrain_lod();
        self.clear_object_lod();
        self.bounds = None;
    }

    /// The form that owns `triangle` of the `instance`th instance this node
    /// created (creation order), as `file:object`, or empty when out of
    /// range. This is the picking path: a raycast's face index against the
    /// batch's collision, once that exists, resolves to a reference here.
    #[func]
    pub fn reference_at(&self, instance: i32, triangle: i32) -> GString {
        usize::try_from(instance)
            .ok()
            .and_then(|n| self.instances.get(n))
            .zip(usize::try_from(triangle).ok())
            .and_then(|(instance, t)| instance.table.reference_of_triangle(t))
            .map(|id| GString::from(id.to_string().as_str()))
            .unwrap_or_default()
    }

    /// The scenario instances draw into: this node's viewport's `World3D`
    /// when the node is in a tree, otherwise the main scene tree's root
    /// viewport, created on demand. The fallback matters because a
    /// `SceneTree` script's `_initialize` adds nodes before the tree marks
    /// them inside it, and the benchmark streams from there.
    fn scenario(&self) -> Option<Rid> {
        crate::godot_world::scenario(&self.base(), "CellStreamerNode")
    }

    /// Draws one loaded cell's geometry as instances, batched or not.
    /// `instancing_candidates` runs first and unconditionally: a repeated
    /// model draws as a `MultiMesh` whether or not `batch_surfaces` is on,
    /// since instancing and merging answer different questions (one draw
    /// call sharing a geometry buffer, versus one draw call sharing a merged
    /// buffer) and `batch_surfaces` off is meant to stay the one-node-per-
    /// shape baseline for the surfaces instancing did not already take.
    fn add_cell(
        &mut self,
        cell: GlobalFormId,
        loaded: &LoadedCell,
        scenario: Rid,
        totals: &mut StreamTotals,
    ) {
        let _span = soe_trace::Span::new("host.add_cell", "host");
        totals.add_cell(loaded);
        let (groups, remainder) = instancing_candidates(&loaded.surfaces);
        for group in &groups {
            self.add_multimesh(cell, group, scenario);
            totals.instances += 1;
        }
        let remainder: Vec<LoadedSurface> = remainder
            .into_iter()
            .map(|i| loaded.surfaces[i].clone())
            .collect();
        if self.batch_surfaces {
            let (batches, stats) = batch_surfaces(&remainder);
            totals.largest_batch = totals.largest_batch.max(stats.largest_batch_surfaces);
            for batch in &batches {
                let first = &remainder[batch.members[0].surface];
                let material =
                    material_for(&mut self.materials, &first.arrays.material, &batch.textures);
                let table = BatchTable::Triangles(
                    batch
                        .members
                        .iter()
                        .map(|m| BatchTableMember {
                            reference: remainder[m.surface].reference,
                            member: *m,
                            origin: remainder[m.surface].placement,
                        })
                        .collect(),
                );
                self.add_instance(cell, &batch.arrays, &material, scenario, table);
                totals.instances += 1;
            }
        } else {
            totals.largest_batch = totals.largest_batch.max(1);
            for surface in &remainder {
                let material = material_for(
                    &mut self.materials,
                    &surface.arrays.material,
                    &surface.textures,
                );
                let table = BatchTable::Triangles(vec![BatchTableMember {
                    reference: surface.reference,
                    member: BatchMember {
                        surface: 0,
                        first_vertex: 0,
                        vertex_count: surface.arrays.positions.len(),
                        first_triangle: 0,
                        triangle_count: surface.arrays.triangle_count(),
                    },
                    origin: surface.placement,
                }]);
                self.add_instance(cell, &surface.arrays, &material, scenario, table);
                totals.instances += 1;
            }
        }
    }

    /// One `ArrayMesh` and one `RenderingServer` instance for it, at
    /// identity since the placement is already in the vertices.
    fn add_instance(
        &mut self,
        cell: GlobalFormId,
        arrays: &SurfaceArrays,
        material: &Gd<Material>,
        scenario: Rid,
        table: BatchTable,
    ) {
        let _span = soe_trace::Span::with_payload(
            "host.upload_mesh",
            "host",
            arrays.positions.len() as u64,
        );
        let mesh = array_mesh_from(arrays);
        let mut server = RenderingServer::singleton();
        let rid = server.instance_create2(mesh.get_rid(), scenario);
        server.instance_set_transform(rid, Transform3D::IDENTITY);
        server.instance_geometry_set_material_override(rid, material.get_rid());
        self.bounds = Some(match self.bounds {
            Some(b) => crate::batch::merge_bounds(b, arrays.bounds),
            None => arrays.bounds,
        });
        self.instances.push(Instance {
            cell,
            bounds: arrays.bounds,
            rid,
            mesh: InstanceMesh::Array(mesh),
            table,
            arrays: arrays.clone(),
        });
    }

    /// One `MultiMesh` and one `RenderingServer` instance for a mesh
    /// identity repeated often enough in this cell: one local-space geometry
    /// buffer, one `Transform3D` per placement, set on the `MultiMesh`
    /// itself rather than baked into a copy of the vertex data.
    fn add_multimesh(&mut self, cell: GlobalFormId, group: &InstanceGroup, scenario: Rid) {
        let _span = soe_trace::Span::with_payload(
            "host.upload_multimesh",
            "host",
            group.arrays.positions.len() as u64,
        );
        let material = material_for(&mut self.materials, &group.arrays.material, &group.textures);
        let multi = multimesh_from_group(group);
        let mut server = RenderingServer::singleton();
        let rid = server.instance_create2(multi.get_rid(), scenario);
        server.instance_set_transform(rid, Transform3D::IDENTITY);
        server.instance_geometry_set_material_override(rid, material.get_rid());
        let bounds = group_bounds(group);
        self.bounds = Some(match self.bounds {
            Some(b) => crate::batch::merge_bounds(b, bounds),
            None => bounds,
        });
        let table = BatchTable::Instances(group.members.iter().map(|m| m.reference).collect());
        self.instances.push(Instance {
            cell,
            bounds,
            rid,
            mesh: InstanceMesh::Multi(multi),
            table,
            arrays: group.arrays.clone(),
        });
    }

    fn free_instances(&mut self) {
        let mut server = RenderingServer::singleton();
        for instance in self.instances.drain(..) {
            server.free_rid(instance.rid);
            drop(instance.mesh);
        }
    }

    /// One `TerrainLodStreamer::step` against the loaded data: reads each
    /// planned block through `DataSource::load_lod_block`, records its
    /// built bytes, frees every block the step released, then draws the
    /// blocks that survived the budget. A block loaded and evicted in the
    /// same step is never uploaded. Returns the blocks drawn, `None` when
    /// the call cannot run, with the reason in the Godot error log.
    fn lod_cycle(
        &mut self,
        focus: CellCoord,
        settle: bool,
        caller: &str,
        object_lod: bool,
    ) -> Option<usize> {
        let Some(source) = self.data_source.clone() else {
            godot_error!("CellStreamerNode::{caller}: load_data was not called");
            return None;
        };
        if (object_lod && self.object_lod.is_none()) || (!object_lod && self.terrain_lod.is_none())
        {
            let start = if object_lod {
                "stream_object_lod"
            } else {
                "stream_terrain_lod"
            };
            godot_error!("CellStreamerNode::{caller}: {start} was not called");
            return None;
        }
        let scenario = self.scenario()?;
        let mut lod = if object_lod {
            self.object_lod.take()?
        } else {
            self.terrain_lod.take()?
        };
        let mut built = std::collections::HashMap::new();
        let world = lod.world;
        let world_editor_id = lod.world_editor_id.as_str();
        let step = lod.streamer.step(focus, settle, |block| {
            let loaded = if object_lod {
                source.load_object_lod_block(world, world_editor_id, block)
            } else {
                source.load_lod_block(world, world_editor_id, block)
            };
            loaded.map(|found| {
                found.map(|loaded| {
                    let bytes = loaded.approx_bytes;
                    built.insert(block, loaded);
                    bytes
                })
            })
        });
        let mut server = RenderingServer::singleton();
        for block in step.released() {
            built.remove(&block);
            for instance in lod.blocks.remove(&block).into_iter().flatten() {
                server.free_rid(instance.rid);
                drop(instance.mesh);
            }
        }
        for (block, e) in &step.failed {
            godot_error!(
                "CellStreamerNode::{caller}: LOD block {} {} {}: {e}",
                block.level.cells(),
                block.x,
                block.y
            );
        }
        let drawn = built.len();
        for (block, loaded) in built {
            let instances = loaded
                .surfaces
                .iter()
                .map(|surface| self.lod_instance(surface, scenario))
                .collect();
            lod.blocks.insert(block, instances);
        }
        lod.last_step = LodStepCounts {
            loaded: drawn,
            missing: step.missing.len(),
            failed: step.failed.len(),
            released: step.unloaded.len() + step.evicted.len(),
        };
        if object_lod {
            self.object_lod = Some(lod);
        } else {
            self.terrain_lod = Some(lod);
        }
        Some(drawn)
    }

    /// One LOD surface as an `ArrayMesh` and a `RenderingServer` instance
    /// at identity, since `load_lod_block` bakes the block's placement into
    /// the vertices the same way a reference's placement is baked.
    fn lod_instance(&mut self, surface: &LoadedSurface, scenario: Rid) -> LodInstance {
        let material = material_for(
            &mut self.materials,
            &surface.arrays.material,
            &surface.textures,
        );
        let mesh = array_mesh_from(&surface.arrays);
        let mut server = RenderingServer::singleton();
        let rid = server.instance_create2(mesh.get_rid(), scenario);
        server.instance_set_transform(rid, Transform3D::IDENTITY);
        server.instance_geometry_set_material_override(rid, material.get_rid());
        LodInstance {
            rid,
            mesh,
            triangles: surface.arrays.triangle_count(),
        }
    }
}

#[derive(Default)]
struct MaterialMap {
    materials: std::collections::HashMap<crate::mesh::MaterialKey, Gd<Material>>,
    textures: std::collections::HashMap<String, Gd<ImageTexture>>,
    cubemaps: std::collections::HashMap<String, Gd<Cubemap>>,
}

impl MaterialMap {
    fn new() -> Self {
        Self::default()
    }
    fn len(&self) -> usize {
        self.materials.len()
    }
}

fn material_for(
    cache: &mut MaterialMap,
    desc: &MaterialDesc,
    textures: &SurfaceTextures,
) -> Gd<Material> {
    let key = crate::mesh::MaterialKey::from(desc);
    if let Some(existing) = cache.materials.get(&key) {
        return existing.clone();
    }
    let material: Gd<Material> = if let Some(water) = &desc.water {
        water_material(water).upcast()
    } else if desc.decal_depth().is_some() || desc.environment_texture().is_some() {
        decal_material(desc, textures, &mut cache.textures, &mut cache.cubemaps).upcast()
    } else {
        material_with_textures(desc, textures, &mut cache.textures).upcast()
    };
    cache.materials.insert(key, material.clone());
    material
}

/// A water plane's material from its `WATR` shading data: the shallow
/// color as albedo, `ANAM` opacity as its alpha, blended when below full,
/// and visible from below as well as above since the camera can swim under
/// it. Deep and reflection colors, the noise layers and the fog ranges wait
/// for a water shader, `docs/godot-host.md` lists them.
pub fn water_material(water: &crate::mesh::WaterMaterial) -> Gd<StandardMaterial3D> {
    let mut mat = StandardMaterial3D::new_gd();
    let [r, g, b] = water.shallow_color;
    mat.set_albedo(Color::from_rgba(r, g, b, water.alpha()));
    if water.opacity < u8::MAX {
        mat.set_transparency(Transparency::ALPHA);
    }
    mat.set_cull_mode(CullMode::DISABLED);
    mat.set_roughness(WATER_ROUGHNESS);
    mat
}

/// Low roughness keeps a sharp highlight, closer to calm water than the
/// default matte finish.
const WATER_ROUGHNESS: f32 = 0.05;

/// Shader materials add authored cubemap reflection or mesh decal depth state.
/// Community Shaders supplies the cubemap, red mask and normal alpha contract.
/// OpenMW's handleDecal is the depth reference. Positive reverse-Z bias brings
/// a decal forward without moving its vertices.
fn decal_material(
    desc: &MaterialDesc,
    textures: &SurfaceTextures,
    cache: &mut std::collections::HashMap<String, Gd<ImageTexture>>,
    cubemaps: &mut std::collections::HashMap<String, Gd<Cubemap>>,
) -> Gd<ShaderMaterial> {
    let mut shader = Shader::new_gd();
    shader.set_code(&decal_shader(desc));
    let mut material = ShaderMaterial::new_gd();
    material.set_shader(&shader);
    if desc.decal_depth().is_some() {
        material.set_render_priority(1);
    }
    if let Some(data) = &textures.diffuse {
        if let Some(texture) = cached_texture(cache, desc.diffuse.as_deref(), data) {
            material.set_shader_parameter("albedo_texture", &texture.to_variant());
        }
    }
    if let Some(data) = &textures.normal {
        if let Some(texture) = cached_texture(cache, desc.normal.as_deref(), data) {
            material.set_shader_parameter("normal_texture", &texture.to_variant());
            material.set_shader_parameter("has_normal", &true.to_variant());
        }
    }
    let color = Color::from_rgb(
        desc.emissive_color[0],
        desc.emissive_color[1],
        desc.emissive_color[2],
    );
    material.set_shader_parameter("emission_color", &color.to_variant());
    material.set_shader_parameter("emission_strength", &desc.emissive_multiple.to_variant());
    if let Some(data) = &textures.glow {
        if let Some(texture) = cached_texture(cache, desc.glow_texture(), data) {
            material.set_shader_parameter("glow_texture", &texture.to_variant());
        }
    }
    if let (Some(path), Some(data)) = (desc.environment_texture(), &textures.environment) {
        let key = soe_bsa::normalize(path);
        let cube = cubemaps.get(&key).cloned().or_else(|| {
            let cube = cubemap_from(data)?;
            cubemaps.insert(key, cube.clone());
            Some(cube)
        });
        if let Some(cube) = cube {
            material.set_shader_parameter("environment_texture", &cube.to_variant());
            material
                .set_shader_parameter("environment_scale", &desc.environment_scale.to_variant());
            material.set_shader_parameter("has_environment", &true.to_variant());
        }
    }
    if let Some(data) = &textures.environment_mask {
        if let Some(texture) = cached_texture(cache, desc.environment_mask.as_deref(), data) {
            material.set_shader_parameter("environment_mask", &texture.to_variant());
            material.set_shader_parameter("has_environment_mask", &true.to_variant());
        }
    }
    material
}

fn decal_shader(desc: &MaterialDesc) -> String {
    let (test, write) = desc.decal_depth().unwrap_or((true, false));
    let depth = if desc.decal_depth().is_none() {
        "depth_draw_opaque"
    } else if write {
        "depth_draw_always"
    } else {
        "depth_draw_never"
    };
    let cull = if desc.double_sided {
        "cull_disabled"
    } else {
        "cull_back"
    };
    let test = if test { "" } else { ", depth_test_disabled" };
    let alpha = match desc.alpha {
        AlphaMode::Opaque => String::new(),
        AlphaMode::Blend => "ALPHA = color.a;".into(),
        // Debug keeps the decimal point on whole values. Display prints a
        // threshold of 255 as `1`, an int literal the shader compiler rejects
        // against the float alpha.
        AlphaMode::Cutout(t) => format!("if (color.a < {:?}) {{ discard; }}", f32::from(t) / 255.0),
    };
    let offset = if desc.decal_depth().is_some() {
        "float slope = max(abs(dFdx(FRAGCOORD.z)), abs(dFdy(FRAGCOORD.z)));
    DEPTH = min(1.0, FRAGCOORD.z + 0.65 * slope + 0.0000001);"
    } else {
        ""
    };
    format!(
        "shader_type spatial;
render_mode {depth}, {cull}{test};
uniform sampler2D albedo_texture : source_color, hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D normal_texture : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform bool has_normal = false;
uniform sampler2D glow_texture : source_color, hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
uniform vec4 emission_color : source_color = vec4(0.0, 0.0, 0.0, 1.0);
uniform float emission_strength = 1.0;
uniform samplerCube environment_texture : source_color, hint_default_black, filter_linear_mipmap;
uniform sampler2D environment_mask : hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
uniform float environment_scale = 1.0;
uniform bool has_environment = false;
uniform bool has_environment_mask = false;
void fragment() {{
    vec4 color = texture(albedo_texture, UV);
    ALBEDO = color.rgb;
    ROUGHNESS = 1.0;
    vec4 normal_sample = texture(normal_texture, UV);
    vec3 reflection_normal = NORMAL;
    if (has_normal) {{
        NORMAL_MAP = normal_sample.rgb;
        vec2 tangent_xy = normal_sample.xy * 2.0 - 1.0;
        vec3 tangent_normal = vec3(tangent_xy, sqrt(max(0.0, 1.0 - dot(tangent_xy, tangent_xy))));
        reflection_normal = normalize(mat3(TANGENT, BINORMAL, NORMAL) * tangent_normal);
    }}
    EMISSION = emission_color.rgb * emission_strength * texture(glow_texture, UV).rgb;
    if (has_environment) {{
        float mask = has_environment_mask ? texture(environment_mask, UV).r : normal_sample.a;
        vec3 world_reflection = mat3(INV_VIEW_MATRIX) * reflect(-VIEW, reflection_normal);
        vec3 skyrim_reflection = vec3(world_reflection.x, -world_reflection.z, world_reflection.y);
        EMISSION += texture(environment_texture, skyrim_reflection).rgb * environment_scale * mask;
    }}
    {alpha}
    {offset}
}}"
    )
}

fn cached_texture(
    cache: &mut std::collections::HashMap<String, Gd<ImageTexture>>,
    path: Option<&str>,
    data: &TextureData,
) -> Option<Gd<ImageTexture>> {
    let key = path.map(str::to_lowercase);
    if let Some(existing) = key.as_ref().and_then(|key| cache.get(key)) {
        return Some(existing.clone());
    }
    let texture = image_texture_from(data)?;
    if let Some(key) = key {
        cache.insert(key, texture.clone());
    }
    Some(texture)
}

/// Builds a `StandardMaterial3D` from a NIF material description and its
/// decoded textures. `normal_texture` gets only the RGB channels: Skyrim
/// stores specular strength in the normal map's alpha channel, not a
/// transparency mask, and Godot's normal mapping already ignores alpha, so
/// nothing further is needed to drop it. Alpha handling follows the shader's
/// own flags, cutout for `NiAlphaProperty`'s test, blend for its blend mode,
/// opaque otherwise, and `cull_mode` follows the double-sided flag.
pub fn standard_material_from(
    material: &MaterialDesc,
    textures: &SurfaceTextures,
) -> Gd<StandardMaterial3D> {
    material_with_textures(material, textures, &mut std::collections::HashMap::new())
}

fn material_with_textures(
    material: &MaterialDesc,
    textures: &SurfaceTextures,
    cache: &mut std::collections::HashMap<String, Gd<ImageTexture>>,
) -> Gd<StandardMaterial3D> {
    let mut mat = StandardMaterial3D::new_gd();

    if let Some(diffuse) = &textures.diffuse {
        if let Some(tex) = cached_texture(cache, material.diffuse.as_deref(), diffuse) {
            mat.set_texture(TextureParam::ALBEDO, &tex);
        }
    }
    if let Some(normal) = &textures.normal {
        if let Some(tex) = cached_texture(cache, material.normal.as_deref(), normal) {
            mat.set_feature(Feature::NORMAL_MAPPING, true);
            mat.set_texture(TextureParam::NORMAL, &tex);
        }
    }

    if material.emits_light() {
        mat.set_feature(Feature::EMISSION, true);
        mat.set_emission(Color::from_rgb(
            material.emissive_color[0],
            material.emissive_color[1],
            material.emissive_color[2],
        ));
        mat.set_emission_energy_multiplier(material.emissive_multiple);
        if let Some(glow) = &textures.glow {
            if let Some(texture) = cached_texture(cache, material.glow_texture(), glow) {
                mat.set_emission_operator(EmissionOperator::MULTIPLY);
                mat.set_texture(TextureParam::EMISSION, &texture);
            }
        }
    }

    match material.alpha {
        AlphaMode::Opaque => {}
        AlphaMode::Cutout(threshold) => {
            mat.set_transparency(Transparency::ALPHA_SCISSOR);
            mat.set_alpha_scissor_threshold(f32::from(threshold) / 255.0);
        }
        AlphaMode::Blend => mat.set_transparency(Transparency::ALPHA),
    }

    if material.double_sided {
        mat.set_cull_mode(CullMode::DISABLED);
    }

    mat
}

/// Uploads a texture as an `ImageTexture`. A block-compressed payload goes
/// up as is with the file's mip chain (`godot/texture_check.gd` proves the
/// formats round-trip headless). If Godot rejects the chain, which happens
/// when a file's chain stops short of 1 by 1, the base level goes up alone.
/// A decoded RGBA8 texture gets a generated mip chain, since without mips
/// every minified sample of a 2048 square texture misses the texture cache.
/// `None` only if `Image::create_from_data` refuses the base level too.
fn image_texture_from(texture: &TextureData) -> Option<Gd<ImageTexture>> {
    ImageTexture::create_from_image(&image_from_texture(texture)?)
}

fn cubemap_from(texture: &TextureData) -> Option<Gd<Cubemap>> {
    let TextureData::Cubemap(faces) = texture else {
        return None;
    };
    let mut images = Array::<Gd<Image>>::new();
    for face in faces.iter() {
        images.push(&image_from_texture(face)?);
    }
    let mut cube = Cubemap::new_gd();
    (cube.create_from_images(&images) == godot::global::Error::OK).then_some(cube)
}

fn image_from_texture(texture: &TextureData) -> Option<Gd<Image>> {
    let _span = soe_trace::Span::with_payload(
        "host.upload_texture",
        "host",
        u64::from(texture.width() * texture.height()),
    );
    let godot_image = match texture {
        TextureData::Cubemap(_) => return None,
        TextureData::Compressed {
            width,
            height,
            format,
            mips,
            mip0_len,
            bytes,
        } => {
            let format = match format {
                BlockFormat::Bc1 => Format::DXT1,
                BlockFormat::Bc2 => Format::DXT3,
                BlockFormat::Bc3 => Format::DXT5,
                BlockFormat::Bc4 => Format::RGTC_R,
                BlockFormat::Bc5 => Format::RGTC_RG,
                BlockFormat::Bc7 => Format::BPTC_RGBA,
            };
            let chain: PackedByteArray = bytes.iter().copied().collect();
            let with_chain = (*mips > 1)
                .then(|| {
                    Image::create_from_data(*width as i32, *height as i32, true, format, &chain)
                })
                .flatten()
                .filter(|image| !image.is_empty());
            with_chain
                .or_else(|| {
                    let base: PackedByteArray = bytes[..*mip0_len].iter().copied().collect();
                    Image::create_from_data(*width as i32, *height as i32, false, format, &base)
                })
                .filter(|image| !image.is_empty())?
        }
        TextureData::Rgba8(image) => {
            let bytes: PackedByteArray = image.rgba.iter().copied().collect();
            let mut godot_image = Image::create_from_data(
                image.width as i32,
                image.height as i32,
                false,
                Format::RGBA8,
                &bytes,
            )?;
            godot_image.generate_mipmaps();
            godot_image
        }
    };
    Some(godot_image)
}

/// An `ArrayMesh` with one triangle surface from converted arrays. The
/// caller applies a material through `set_surface_override_material`, this
/// function only builds the geometry.
pub fn array_mesh_from(surface: &SurfaceArrays) -> Gd<ArrayMesh> {
    let mut arrays = VarArray::new();
    for _ in 0..ArrayType::MAX.ord() {
        arrays.push(&Variant::nil());
    }
    let set = |arrays: &mut VarArray, slot: ArrayType, v: Variant| {
        arrays.set(slot.ord() as usize, &v);
    };

    let positions: PackedVector3Array = surface
        .positions
        .iter()
        .map(|p| Vector3::new(p[0], p[1], p[2]))
        .collect();
    set(&mut arrays, ArrayType::VERTEX, positions.to_variant());

    if !surface.normals.is_empty() {
        let normals: PackedVector3Array = surface
            .normals
            .iter()
            .map(|n| Vector3::new(n[0], n[1], n[2]))
            .collect();
        set(&mut arrays, ArrayType::NORMAL, normals.to_variant());
    }
    if !surface.tangents.is_empty() {
        let tangents: PackedFloat32Array = surface.tangents.iter().copied().collect();
        set(&mut arrays, ArrayType::TANGENT, tangents.to_variant());
    }
    if !surface.uvs.is_empty() {
        let uvs: PackedVector2Array = surface
            .uvs
            .iter()
            .map(|u| Vector2::new(u[0], u[1]))
            .collect();
        set(&mut arrays, ArrayType::TEX_UV, uvs.to_variant());
    }
    if surface.is_skinned() {
        let bones: PackedInt32Array = surface.bones.iter().copied().collect();
        let weights: PackedFloat32Array = surface.weights.iter().copied().collect();
        set(&mut arrays, ArrayType::BONES, bones.to_variant());
        set(&mut arrays, ArrayType::WEIGHTS, weights.to_variant());
    }
    let indices: PackedInt32Array = surface.indices.iter().copied().collect();
    set(&mut arrays, ArrayType::INDEX, indices.to_variant());

    let mut mesh = ArrayMesh::new_gd();
    mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
    mesh
}

/// Patches one batch member's vertex range in `arrays` in place, for
/// `CellStreamerNode::sync_reference_changes`.
///
/// A hidden reference collapses its whole range to its first vertex, a
/// zero-area triangle fan that draws nothing while leaving the batch's
/// shared index buffer valid, since removing vertices would renumber every
/// triangle after them. A moved reference gets `target.then(&member.origin
/// .inverse())` applied to its baked position and, where present, its
/// normal and tangent, the same Skyrim-space delta construction as
/// `godot_physics::CellBodies::sync_placements` uses for collision.
fn apply_member_patch(
    arrays: &mut SurfaceArrays,
    member: &BatchTableMember,
    visible: bool,
    target: &Iso,
) {
    let range = member.member.first_vertex..member.member.first_vertex + member.member.vertex_count;
    if range.is_empty() {
        return;
    }
    if !visible {
        let anchor = arrays.positions[range.start];
        for p in &mut arrays.positions[range.clone()] {
            *p = anchor;
        }
        return;
    }
    let delta = target.then(&member.origin.inverse());
    for p in &mut arrays.positions[range.clone()] {
        *p = moved_position(*p, &delta);
    }
    if !arrays.normals.is_empty() {
        for n in &mut arrays.normals[range.clone()] {
            *n = moved_direction(*n, &delta);
        }
    }
    if !arrays.tangents.is_empty() {
        let tangent_range = (range.start * 4)..(range.end * 4);
        for chunk in arrays.tangents[tangent_range].as_chunks_mut::<4>().0 {
            let d = moved_direction([chunk[0], chunk[1], chunk[2]], &delta);
            chunk[0] = d[0];
            chunk[1] = d[1];
            chunk[2] = d[2];
        }
    }
}

/// A baked vertex position, round tripped through Skyrim space so `delta`
/// (built in Skyrim space, like every other placement in this crate) can be
/// applied to it.
fn moved_position(p: [f32; 3], delta: &Iso) -> [f32; 3] {
    let sky = GodotPos::new(p[0], p[1], p[2]).to_skyrim();
    let moved = delta.apply(Vec3::new(sky.x, sky.y, sky.z));
    let g = SkyrimPos::new(moved.x, moved.y, moved.z).to_godot();
    [g.x, g.y, g.z]
}

/// A baked normal or tangent direction, round tripped the same way as
/// [`moved_position`] but with no scale or translation, matching
/// `Iso::apply_direction`.
fn moved_direction(d: [f32; 3], delta: &Iso) -> [f32; 3] {
    let sky = direction_to_skyrim(d);
    let moved = delta.apply_direction(Vec3::new(sky[0], sky[1], sky[2]));
    direction_to_godot([moved.x, moved.y, moved.z])
}

/// A `MultiMesh` sharing `group`'s local-space geometry across a
/// `Transform3D` per member, in the order `group.members` carries them
/// (the same order `InstanceGroup::reference_of_instance` indexes). Godot
/// applies each transform at draw time, unlike `array_mesh_from`, whose
/// caller already baked one placement into the vertex data.
pub fn multimesh_from_group(group: &InstanceGroup) -> Gd<MultiMesh> {
    let mesh = array_mesh_from(&group.arrays);
    let mut multi = MultiMesh::new_gd();
    // The transform format has to be set before the instance count: Godot
    // sizes the per-instance buffer from the format in effect at that call,
    // the same ordering `use_colors` and `use_custom_data` document.
    multi.set_transform_format(TransformFormat::TRANSFORM_3D);
    multi.set_mesh(&mesh);
    multi.set_instance_count(group.members.len() as i32);
    for (index, member) in group.members.iter().enumerate() {
        multi.set_instance_transform(index as i32, transform3d_from(member.transform));
    }
    multi
}

fn transform3d_from(t: GodotTransform) -> Transform3D {
    let b = t.basis;
    Transform3D::new(
        Basis::from_rows(
            Vector3::new(b[0][0], b[0][1], b[0][2]),
            Vector3::new(b[1][0], b[1][1], b[1][2]),
            Vector3::new(b[2][0], b[2][1], b[2][2]),
        ),
        Vector3::new(t.origin.x, t.origin.y, t.origin.z),
    )
}

/// An approximate world bound for a `MultiMesh` group: each member's origin
/// expanded by the local shape's own extent. It ignores rotation, so it can
/// run a little wide for a shape that is not roughly cubic. That only
/// affects the camera-framing bound this node keeps, never anything
/// gameplay reads, unlike a batch's bounds, which are exact because they
/// come from placement-baked vertices.
fn group_bounds(group: &InstanceGroup) -> crate::mesh::Aabb {
    let local = group.arrays.bounds;
    let extent = GodotPos::new(
        (local.max.x - local.min.x).abs(),
        (local.max.y - local.min.y).abs(),
        (local.max.z - local.min.z).abs(),
    );
    group
        .members
        .iter()
        .map(|member| {
            let o = member.transform.origin;
            crate::mesh::Aabb {
                min: GodotPos::new(o.x - extent.x, o.y - extent.y, o.z - extent.z),
                max: GodotPos::new(o.x + extent.x, o.y + extent.y, o.z + extent.z),
            }
        })
        .reduce(crate::batch::merge_bounds)
        .unwrap_or(local)
}

/// A `FORMAT_RF` height image for `Terrain3DData.import_images`, or for
/// `Terrain3DRegion.set_height_map`, at the region's `godot_location`.
pub fn heightmap_image_from(region: &RegionHeightmap) -> Option<Gd<Image>> {
    let bytes: PackedByteArray = region.rf_bytes().into_iter().collect();
    Image::create_from_data(
        region.size as i32,
        region.size as i32,
        false,
        Format::RF,
        &bytes,
    )
}

#[cfg(test)]
mod decal_tests {
    use super::*;
    use soe_nif::blocks::{flags1, flags2};

    /// A threshold of 255 printed as `1` once, an int literal Godot rejects
    /// against the float alpha, and the Whiterun exterior benchmark reported
    /// a shader compilation failure for it.
    #[test]
    fn cutout_threshold_is_a_float_literal_at_every_value() {
        for (threshold, literal) in [(0, "0.0"), (128, "0.5019608"), (255, "1.0")] {
            let material = MaterialDesc {
                alpha: AlphaMode::Cutout(threshold),
                ..MaterialDesc::default()
            };
            let shader = decal_shader(&material);
            assert!(
                shader.contains(&format!("if (color.a < {literal}) {{ discard; }}")),
                "threshold {threshold} must print as {literal}"
            );
        }
    }

    #[test]
    fn export_decal_shaders_for_rendered_regression() {
        let Some(directory) = std::env::var_os("SOE_SHADER_FIXTURES") else {
            return;
        };
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        for (name, alpha) in [
            ("opaque", AlphaMode::Opaque),
            ("cutout", AlphaMode::Cutout(128)),
            ("blend", AlphaMode::Blend),
        ] {
            for test in [false, true] {
                for write in [false, true] {
                    let material = MaterialDesc {
                        alpha,
                        shader_flags_1: flags1::DECAL | if test { flags1::ZBUFFER_TEST } else { 0 },
                        shader_flags_2: if write { flags2::ZBUFFER_WRITE } else { 0 },
                        ..MaterialDesc::default()
                    };
                    std::fs::write(
                        directory.join(format!(
                            "{name}-{}-{}.gdshader",
                            u8::from(test),
                            u8::from(write)
                        )),
                        decal_shader(&material),
                    )
                    .unwrap();
                }
            }
        }
    }
}
