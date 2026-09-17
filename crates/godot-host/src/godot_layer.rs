// SPDX-License-Identifier: GPL-3.0-or-later

//! The Godot-facing layer. Nothing here decides anything: the node forwards
//! a focus position to `streaming::CellStreamer` every frame and turns the
//! plan into signals, and the two functions copy already converted arrays
//! into Godot's packed types. Keeping it this thin is what lets the rest of
//! the crate test without an engine.

use std::path::Path;

use godot::classes::base_material_3d::{CullMode, Feature, TextureParam, Transparency};
use godot::classes::image::Format;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::{
    ArrayMesh, INode3D, Image, ImageTexture, MeshInstance3D, Node3D, StandardMaterial3D,
};
use godot::prelude::*;
use soe_dds::Image as DdsImage;

use crate::data_source::{DataSource, LoadedSurface, SurfaceTextures};
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
    streamer: Option<CellStreamer>,
    /// Set by `load_data`. `stream_cell` needs this before it can read a
    /// real cell, the synthetic streaming above never touches it.
    data_source: Option<DataSource>,
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
            streamer: None,
            data_source: None,
        }
    }

    fn ready(&mut self) {
        self.reconfigure();
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

    /// Resolves `editor_id` against the data loaded by `load_data`, builds a
    /// `MeshInstance3D` for every static reference's surface, adds each as a
    /// child of this node, and returns how many were added. Returns -1 if
    /// `load_data` has not opened a data source, 0 if the cell resolved but
    /// nothing built (either it is empty or every mesh failed, check the
    /// Godot error log).
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
        let mut materials: std::collections::HashMap<TextureSetKey, Gd<StandardMaterial3D>> =
            std::collections::HashMap::new();
        let mut built = 0;
        for surface in &loaded.surfaces {
            let mesh = array_mesh_from(&surface.arrays);
            let mut instance = MeshInstance3D::new_alloc();
            instance.set_mesh(&mesh);
            let material = material_for(&mut materials, surface);
            instance.set_surface_override_material(0, &material);
            self.base_mut().add_child(&instance);
            built += 1;
        }
        built
    }
}

/// A distinct texture set: the material's diffuse and normal paths,
/// lowercased. Two materials that name the same textures share one
/// `StandardMaterial3D` rather than each getting their own `ImageTexture`
/// upload.
type TextureSetKey = (Option<String>, Option<String>);

fn texture_set_key(material: &MaterialDesc) -> TextureSetKey {
    (
        material.diffuse.as_deref().map(str::to_lowercase),
        material.normal.as_deref().map(str::to_lowercase),
    )
}

/// Looks up or builds the `StandardMaterial3D` for `surface`'s texture set,
/// cloning the cached `Gd` handle (cheap, Godot objects are reference
/// counted) rather than re-uploading the same textures for every reference
/// that shares them.
fn material_for(
    cache: &mut std::collections::HashMap<TextureSetKey, Gd<StandardMaterial3D>>,
    surface: &LoadedSurface,
) -> Gd<StandardMaterial3D> {
    let key = texture_set_key(&surface.arrays.material);
    if let Some(existing) = cache.get(&key) {
        return existing.clone();
    }
    let material = standard_material_from(&surface.arrays.material, &surface.textures);
    cache.insert(key, material.clone());
    material
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
    let mut mat = StandardMaterial3D::new_gd();

    if let Some(diffuse) = &textures.diffuse {
        if let Some(tex) = image_texture_from(diffuse) {
            mat.set_texture(TextureParam::ALBEDO, &tex);
        }
    }
    if let Some(normal) = &textures.normal {
        if let Some(tex) = image_texture_from(normal) {
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

/// Uploads a decoded RGBA8 texture as an `ImageTexture`. `None` only if
/// Godot's own `Image::create_from_data` refuses the buffer, which needs a
/// length mismatch against width times height times four and should not
/// happen for a `soe_dds` decode this crate produced itself.
fn image_texture_from(image: &DdsImage) -> Option<Gd<ImageTexture>> {
    let bytes: PackedByteArray = image.rgba.iter().copied().collect();
    let godot_image = Image::create_from_data(
        image.width as i32,
        image.height as i32,
        false,
        Format::RGBA8,
        &bytes,
    )?;
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
