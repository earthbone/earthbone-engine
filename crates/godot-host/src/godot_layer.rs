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

use godot::classes::base_material_3d::{CullMode, Feature, TextureParam, Transparency};
use godot::classes::image::Format;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::{
    ArrayMesh, Engine, INode3D, Image, ImageTexture, Node3D, RenderingServer, SceneTree,
    StandardMaterial3D, Viewport,
};
use godot::prelude::*;
use soe_esm::GlobalFormId;

use crate::batch::{batch_surfaces, BatchMember};
use crate::data_source::{BlockFormat, DataSource, LoadedCell, SurfaceTextures, TextureData};
use crate::mesh::{AlphaMode, MaterialDesc, SurfaceArrays};
use crate::streaming::{CellCoord, CellStreamer, StreamConfig};
use crate::terrain::RegionHeightmap;
use crate::units::GodotPos;

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
    data_source: Option<DataSource>,
    /// One entry per `RenderingServer` instance `stream_cell` or
    /// `stream_exterior` created, in creation order. The scene tree never
    /// sees them: a batch is a mesh RID and an instance RID in this node's
    /// scenario, freed in `exit_tree`, so a cell costs no `Node3D` at all.
    instances: Vec<Instance>,
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
}

/// One drawn batch: its instance and mesh RIDs and the side table that maps
/// a triangle back to the form that placed it.
struct Instance {
    rid: Rid,
    mesh: Gd<ArrayMesh>,
    table: BatchTable,
}

/// The side table for one instance: which form owns each triangle range.
struct BatchTable {
    members: Vec<(GlobalFormId, BatchMember)>,
}

impl BatchTable {
    fn reference_of_triangle(&self, triangle: usize) -> Option<GlobalFormId> {
        let at = self
            .members
            .partition_point(|(_, m)| m.first_triangle + m.triangle_count <= triangle);
        self.members
            .get(at)
            .filter(|(_, m)| triangle >= m.first_triangle)
            .map(|(id, _)| *id)
    }
}

/// Running totals over the cells one `stream_cell` or `stream_exterior`
/// call added, reported through `last_stats`.
#[derive(Default)]
struct StreamTotals {
    cells: usize,
    references: usize,
    statics: usize,
    meshes_parsed: usize,
    meshes_failed: usize,
    triangles: usize,
    surfaces: usize,
    instances: usize,
    largest_batch: usize,
    land_cells: usize,
    world: String,
    cell_list: Vec<String>,
}

impl StreamTotals {
    fn add_cell(&mut self, loaded: &LoadedCell) {
        self.cells += 1;
        self.references += loaded.stats.references;
        self.statics += loaded.stats.statics_with_model;
        self.meshes_parsed += loaded.stats.meshes_parsed;
        self.meshes_failed += loaded.stats.meshes_failed;
        self.triangles += loaded.stats.triangles;
        self.surfaces += loaded.surfaces.len();
    }

    fn to_dictionary(&self, materials: usize) -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("cells", self.cells as i64);
        d.set("references", self.references as i64);
        d.set("statics", self.statics as i64);
        d.set("meshes_parsed", self.meshes_parsed as i64);
        d.set("meshes_failed", self.meshes_failed as i64);
        d.set("triangles", self.triangles as i64);
        d.set("surfaces", self.surfaces as i64);
        d.set("instances", self.instances as i64);
        d.set("largest_batch", self.largest_batch as i64);
        d.set("materials", materials as i64);
        d.set("land_cells", self.land_cells as i64);
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
            instances: Vec::new(),
            materials: MaterialMap::new(),
            bounds: None,
            last_stats: VarDictionary::new(),
            collision: crate::godot_physics::CellBodies::default(),
            last_collision_stats: VarDictionary::new(),
        }
    }

    fn ready(&mut self) {
        self.reconfigure();
    }

    fn exit_tree(&mut self) {
        self.free_instances();
        self.collision.free();
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
        match DataSource::open(Path::new(&path.to_string())) {
            Ok(source) => {
                self.data_source = Some(source);
                true
            }
            Err(e) => {
                godot_error!("CellStreamerNode::load_data: {e}");
                self.data_source = None;
                false
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
        let loaded = match source.stream_cell(&editor_id.to_string()) {
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
        self.add_cell(&loaded, scenario, &mut totals);
        self.last_stats = totals.to_dictionary(self.materials.len() - materials_before);
        totals.instances as i32
    }

    /// Streams the square of exterior cells within `radius` of the cell
    /// named `editor_id` (2 is the game's `uGridsToLoad=5`), each cell's
    /// statics and its terrain, batched per cell so Godot still culls per
    /// cell. Returns the instances created, -1 on failure, and fills
    /// `last_stats` with the block's totals and the cell list.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn stream_exterior(&mut self, editor_id: GString, radius: i32) -> i32 {
        let Some(source) = self.data_source.as_ref() else {
            godot_error!("CellStreamerNode::stream_exterior: load_data was not called");
            return -1;
        };
        let loaded = match source.stream_exterior(&editor_id.to_string(), radius) {
            Ok(loaded) => loaded,
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
            land_cells: loaded.land_cells,
            world: loaded.block.world_editor_id.clone().unwrap_or_default(),
            ..StreamTotals::default()
        };
        for (cell, loaded_cell) in &loaded.cells {
            totals.cell_list.push(format!("({},{})", cell.x, cell.y));
            self.add_cell(loaded_cell, scenario, &mut totals);
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

    /// Instances alive in this node's scenario.
    #[func]
    pub fn instance_count(&self) -> i32 {
        self.instances.len() as i32
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
        let (colliders, read) = match source.cell_colliders(cell_id) {
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
        self.collision = crate::godot_physics::add_statics(space, &colliders);
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
        self.last_collision_stats.clone()
    }

    /// Frees every instance and mesh this node created and forgets the
    /// bounds. Materials stay cached.
    #[func]
    pub fn clear(&mut self) {
        self.collision.free();
        self.free_instances();
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
        let own = self
            .base()
            .get_viewport()
            .and_then(|viewport| viewport.find_world_3d());
        let world = own.or_else(|| {
            let tree = Engine::singleton()
                .get_main_loop()?
                .try_cast::<SceneTree>()
                .ok()?;
            tree.get_root()?.upcast::<Viewport>().find_world_3d()
        });
        if world.is_none() {
            godot_error!("CellStreamerNode: no World3D to draw into");
        }
        world.map(|w| w.get_scenario())
    }

    /// Draws one loaded cell's geometry as instances, batched or not.
    fn add_cell(&mut self, loaded: &LoadedCell, scenario: Rid, totals: &mut StreamTotals) {
        let _span = soe_trace::Span::new("host.add_cell", "host");
        totals.add_cell(loaded);
        if self.batch_surfaces {
            let (batches, stats) = batch_surfaces(&loaded.surfaces);
            totals.largest_batch = totals.largest_batch.max(stats.largest_batch_surfaces);
            for batch in &batches {
                let first = &loaded.surfaces[batch.members[0].surface];
                let material =
                    material_for(&mut self.materials, &first.arrays.material, &batch.textures);
                let table = BatchTable {
                    members: batch
                        .members
                        .iter()
                        .map(|m| (loaded.surfaces[m.surface].reference, *m))
                        .collect(),
                };
                self.add_instance(&batch.arrays, &material, scenario, table);
                totals.instances += 1;
            }
        } else {
            totals.largest_batch = totals.largest_batch.max(1);
            for surface in &loaded.surfaces {
                let material = material_for(
                    &mut self.materials,
                    &surface.arrays.material,
                    &surface.textures,
                );
                let table = BatchTable {
                    members: vec![(
                        surface.reference,
                        BatchMember {
                            surface: 0,
                            first_vertex: 0,
                            vertex_count: surface.arrays.positions.len(),
                            first_triangle: 0,
                            triangle_count: surface.arrays.triangle_count(),
                        },
                    )],
                };
                self.add_instance(&surface.arrays, &material, scenario, table);
                totals.instances += 1;
            }
        }
    }

    /// One `ArrayMesh` and one `RenderingServer` instance for it, at
    /// identity since the placement is already in the vertices.
    fn add_instance(
        &mut self,
        arrays: &SurfaceArrays,
        material: &Gd<StandardMaterial3D>,
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
        self.instances.push(Instance { rid, mesh, table });
    }

    fn free_instances(&mut self) {
        let mut server = RenderingServer::singleton();
        for instance in self.instances.drain(..) {
            server.free_rid(instance.rid);
            drop(instance.mesh);
        }
    }
}

#[derive(Default)]
struct MaterialMap {
    materials: std::collections::HashMap<crate::mesh::MaterialKey, Gd<StandardMaterial3D>>,
    textures: std::collections::HashMap<String, Gd<ImageTexture>>,
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
) -> Gd<StandardMaterial3D> {
    let key = crate::mesh::MaterialKey::from(desc);
    if let Some(existing) = cache.materials.get(&key) {
        return existing.clone();
    }
    let material = material_with_textures(desc, textures, &mut cache.textures);
    cache.materials.insert(key, material.clone());
    material
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
    let _span = soe_trace::Span::with_payload(
        "host.upload_texture",
        "host",
        u64::from(texture.width() * texture.height()),
    );
    let godot_image = match texture {
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
    ImageTexture::create_from_image(&godot_image)
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
