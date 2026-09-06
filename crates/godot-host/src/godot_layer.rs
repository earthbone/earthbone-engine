// SPDX-License-Identifier: GPL-3.0-or-later

//! The Godot-facing layer. Nothing here decides anything: the node forwards
//! a focus position to `streaming::CellStreamer` every frame and turns the
//! plan into signals, and the two functions copy already converted arrays
//! into Godot's packed types. Keeping it this thin is what lets the rest of
//! the crate test without an engine.

use godot::classes::image::Format;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::{ArrayMesh, INode3D, Image, Node3D};
use godot::prelude::*;

use crate::mesh::SurfaceArrays;
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
        self.streamer.as_ref().map_or(0, |s| s.resident_count() as i32)
    }

    #[func]
    pub fn is_resident(&self, cell_x: i32, cell_y: i32) -> bool {
        self.streamer
            .as_ref()
            .is_some_and(|s| s.is_resident(CellCoord::new(cell_x, cell_y)))
    }
}

/// An `ArrayMesh` with one triangle surface from converted arrays. The
/// material is not applied here, the caller maps `surface.material` onto
/// whatever material system it uses.
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
    Image::create_from_data(region.size as i32, region.size as i32, false, Format::RF, &bytes)
}
