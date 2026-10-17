// SPDX-License-Identifier: GPL-3.0-or-later

//! Water type shading data, read straight from a WATR record's raw
//! subrecords. No `esm` crate module exists for WATR yet, so this follows
//! the generic `Record::find` pattern `models::model_for` already uses for
//! FURN and DOOR, instead of adding a dedicated record view for one field
//! set.
//!
//! Layout is TES5Edit's `wbDefinitionsTES5.pas` (WATR, lines 10304 to
//! 10429) and the UESP WATR page: ANAM is a single opacity byte, and DNAM
//! is a 228 (base game) or 232 (Special Edition) byte block whose Shallow,
//! Deep and Reflection colors sit at byte offsets 40, 44 and 48, each a
//! four byte R, G, B, unused `wbByteColors` struct. TNAM (the water's MATT
//! material) is outside this shader adapter. NAM2 to NAM4 carry noise
//! normals, NAM5 carries flow normals and FNAM enables flow mapping.
//! xEdit supplies the DNAM scalar offsets used below.

use soe_esm::cursor::{zstring, Fields};
use soe_esm::records::wrld::World;
use soe_esm::{GlobalFormId, LoadOrder, Record, Tag};

use crate::mesh::{WaterMaterial, WaterShading};

impl super::DataSource {
    /// Resolve the same CELL and WRLD water used to draw the surface.
    pub(crate) fn cell_water(&self, cell: GlobalFormId) -> Option<(f32, Option<GlobalFormId>)> {
        let owner = self.order.winner_ref(cell)?;
        let localized = self.order.file(owner.file).plugin.header.is_localized();
        let view =
            soe_esm::records::cell::Cell::from_record(self.order.record(owner), localized).ok()?;
        if !view.has_water() {
            return None;
        }
        let fallback = match (view.grid, view.world) {
            (Some(_), Some(raw)) => world_water(&self.order, self.order.resolve(owner.file, raw)),
            _ => WorldWater::default(),
        };
        let height = view
            .water_height
            .filter(|height| is_recorded_height(*height))
            .or(fallback.height)?;
        let water = view
            .water
            .map(|raw| self.order.resolve(owner.file, raw))
            .or(fallback.water);
        Some((height, water))
    }

    /// Exterior movement samples the physical grid cell at the actor's XY.
    #[cfg_attr(not(feature = "godot"), allow(dead_code))]
    pub(crate) fn water_at(
        &self,
        cell: GlobalFormId,
        position: [f32; 3],
    ) -> Option<(f32, Option<GlobalFormId>)> {
        let cell = if let Some(grid) = self.cell_index().grid_of(cell) {
            let x = (position[0] / crate::units::CELL_UNITS).floor() as i32;
            let y = (position[1] / crate::units::CELL_UNITS).floor() as i32;
            self.cell_index().exterior_cell(grid.world, x, y)?
        } else {
            cell
        };
        self.cell_water(cell)
    }
}

/// `WRLD` `PNAM` bit 3, "Use Water Data" in TES5Edit's WRLD definition:
/// the worldspace takes its water height and type from its `WNAM` parent.
const USE_PARENT_WATER: u16 = 0x0008;

/// Parent hops followed before giving up. Vanilla chains are one hop deep
/// (`WhiterunWorld` to `Tamriel`), the cap only stops a cyclic plugin.
const MAX_PARENT_HOPS: usize = 4;

/// Magnitude past which an `XCLW` value is a marker, not a height. UESP's
/// CELL page reserves `0x7F7FFFFF` (the largest finite float), xEdit writes
/// it as "Default" meaning the worldspace height applies, and notes
/// `0xFF7FFFFF` in shipped data too. No real water sits a trillion units up.
const XCLW_MARKER_MAGNITUDE: f32 = 1.0e30;

/// Whether `XCLW` holds a real height rather than the "use the worldspace
/// default" marker.
pub(crate) fn is_recorded_height(height: f32) -> bool {
    height.is_finite() && height.abs() < XCLW_MARKER_MAGNITUDE
}

/// The water an exterior cell falls back to when it records none of its
/// own: the worldspace's `DNAM` default water height and its `NAM2` water
/// type, each taken from the parent when `PNAM` says so.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct WorldWater {
    pub height: Option<f32>,
    pub water: Option<GlobalFormId>,
}

/// Reads `world`'s default water, following `WNAM` while "Use Water Data"
/// is set. A worldspace that does not resolve yields no default, so the
/// cell draws no plane rather than one at an invented height.
pub(crate) fn world_water(order: &LoadOrder, world: GlobalFormId) -> WorldWater {
    let mut current = world;
    for _ in 0..=MAX_PARENT_HOPS {
        let Some(rref) = order.winner_ref(current) else {
            break;
        };
        let localized = order.file(rref.file).plugin.header.is_localized();
        let Ok(view) = World::from_record(order.record(rref), localized) else {
            break;
        };
        let inherits = view.parent_flags.is_some_and(|f| f & USE_PARENT_WATER != 0);
        match view.parent.filter(|_| inherits) {
            Some(parent) => current = order.resolve(rref.file, parent),
            None => {
                return WorldWater {
                    height: view.default_water_height.filter(|h| is_recorded_height(*h)),
                    water: view.water.map(|raw| order.resolve(rref.file, raw)),
                }
            }
        }
    }
    WorldWater::default()
}

/// Read the retained raw property with the same NiObjectNET layout as soe-nif.
/// nif.xml and OpenMW property.cpp supply the inherited fields and flags.
pub(super) fn mesh_inputs(
    file: &soe_nif::NifFile,
    shape: usize,
) -> soe_nif::Result<Option<crate::mesh::WaterMesh>> {
    use soe_nif::blocks::Block;
    use soe_nif::stream::Stream;
    let shader = match file.blocks.get(shape) {
        Some(Block::BsTriShape(s)) => s.shader,
        Some(Block::TriShape(s)) => s.shader,
        _ => return Ok(None),
    };
    let Some(Block::Unknown { type_name, data }) = file.get(shader) else {
        return Ok(None);
    };
    if type_name != "BSWaterShaderProperty" {
        return Ok(None);
    }
    let mut stream = Stream::new(data);
    stream.u32()?;
    let extras = stream.u32()?;
    let count = stream.count(extras, 4)?;
    stream.take(count * 4)?;
    stream.block_ref()?;
    let flags1 = stream.u32()?;
    let flags2 = stream.u32()?;
    let uv_offset = stream.vec2()?;
    let uv_scale = stream.vec2()?;
    let water_flags = stream.u32()?;
    if stream.remaining() != 0 || !uv_offset.into_iter().chain(uv_scale).all(f32::is_finite) {
        return Err(soe_nif::Error::InvalidData {
            block: shader.get().unwrap_or(shape),
            reason: "invalid BSWaterShaderProperty UVs or trailing fields".into(),
        });
    }
    Ok(Some(crate::mesh::WaterMesh {
        flags1,
        flags2,
        water_flags,
        uv_offset,
        uv_scale,
    }))
}

const SHALLOW_COLOR_OFFSET: usize = 40;
const REFLECTION_COLOR_OFFSET: usize = 48;

/// Reads opacity, colors, normal paths and DNAM constants from WATR.
/// Always returns a value, missing or malformed
/// subrecords fall back to full opacity and black colors rather than
/// failing the cell's water plane.
pub(crate) fn water_material_from(record: &Record) -> WaterMaterial {
    let opacity = record
        .find(Tag::new(b"ANAM"))
        .and_then(|d| d.first().copied())
        .unwrap_or(255);
    let (shallow_color, deep_color, reflection_color) = record
        .find(Tag::new(b"DNAM"))
        .and_then(read_dnam_colors)
        .unwrap_or_default();
    let path = |tag: &[u8; 4]| {
        record
            .find(Tag::new(tag))
            .map(zstring)
            .filter(|s| !s.is_empty())
    };
    let old: Vec<_> = record
        .subrecords
        .iter()
        .filter(|sub| sub.tag == Tag::new(b"NNAM"))
        .take(3)
        .map(|sub| zstring(&sub.data))
        .collect();
    let noise_layers = [b"NAM2", b"NAM3", b"NAM4"].map(path);
    let noise_layers = std::array::from_fn(|i| {
        noise_layers[i]
            .clone()
            .or_else(|| old.get(i).filter(|s| !s.is_empty()).cloned())
    });
    WaterMaterial {
        opacity,
        shallow_color,
        deep_color,
        reflection_color,
        noise_texture: noise_layers[0].clone(),
        noise_layers,
        flow_texture: path(b"NAM5"),
        shading: record
            .find(Tag::new(b"DNAM"))
            .map_or_else(WaterShading::default, read_shading),
        flags: record
            .find(Tag::new(b"FNAM"))
            .and_then(|d| d.first())
            .copied()
            .unwrap_or(0),
        mesh: None,
    }
}

fn read_shading(data: &[u8]) -> WaterShading {
    let defaults = WaterShading::default();
    let scalar = |offset: usize, fallback| {
        data.get(offset..offset + 4)
            .and_then(|b| <[u8; 4]>::try_from(b).ok())
            .map(f32::from_le_bytes)
            .filter(|f| f.is_finite())
            .unwrap_or(fallback)
    };
    let layers =
        |offset, fallback: [f32; 3]| std::array::from_fn(|i| scalar(offset + 4 * i, fallback[i]));
    WaterShading {
        sun_specular_power: scalar(16, defaults.sun_specular_power),
        reflectivity: scalar(20, defaults.reflectivity),
        fresnel: scalar(24, defaults.fresnel),
        fog_near: scalar(32, defaults.fog_near),
        fog_far: scalar(36, defaults.fog_far),
        fog_amount: scalar(132, defaults.fog_amount),
        wind_direction: layers(100, defaults.wind_direction),
        wind_speed: layers(112, defaults.wind_speed),
        uv_scale: layers(172, defaults.uv_scale),
        amplitude_scale: layers(184, defaults.amplitude_scale),
        reflection_magnitude: scalar(196, defaults.reflection_magnitude),
        sparkle_magnitude: scalar(200, defaults.sparkle_magnitude),
        sun_specular_magnitude: scalar(204, defaults.sun_specular_magnitude),
        sparkle_power: scalar(224, defaults.sparkle_power),
        flowmap_scale: scalar(228, defaults.flowmap_scale),
    }
}

type Colors = ([f32; 3], [f32; 3], [f32; 3]);

/// The Shallow, Deep and Reflection colors from DNAM, each a byte RGB
/// triple normalised to 0.0 to 1.0. `None` for a DNAM shorter than the
/// reflection color's offset, which no vanilla or DLC record produces but
/// a malformed plugin could.
fn read_dnam_colors(dnam: &[u8]) -> Option<Colors> {
    if dnam.len() < REFLECTION_COLOR_OFFSET + 4 {
        return None;
    }
    let mut f = Fields::new(Tag::new(b"WATR"), Tag::new(b"DNAM"), dnam);
    f.skip(SHALLOW_COLOR_OFFSET).ok()?;
    let shallow = read_color(&mut f)?;
    let deep = read_color(&mut f)?;
    let reflection = read_color(&mut f)?;
    Some((shallow, deep, reflection))
}

/// One `wbByteColors`: R, G, B, then one unused byte.
fn read_color(f: &mut Fields<'_>) -> Option<[f32; 3]> {
    let r = f.u8().ok()?;
    let g = f.u8().ok()?;
    let b = f.u8().ok()?;
    f.skip(1).ok()?;
    Some([
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rnd_02_the_xclw_default_markers_are_not_heights() {
        assert!(is_recorded_height(-14000.0));
        assert!(is_recorded_height(0.0));
        assert!(!is_recorded_height(f32::from_bits(0x7F7F_FFFF)));
        assert!(!is_recorded_height(f32::from_bits(0xFF7F_FFFF)));
        assert!(!is_recorded_height(f32::NAN));
    }

    #[test]
    fn water_shader_reads_dnam_scalars_layers_colors_and_texture_paths() {
        let mut dnam = [0_u8; 232];
        let fields = [
            (16, 63.0_f32),
            (20, 0.6),
            (24, 0.03),
            (32, 4.0),
            (36, 800.0),
            (100, 0.0),
            (104, 90.0),
            (108, 180.0),
            (112, 5.0),
            (116, 8.0),
            (120, 12.0),
            (132, 0.75),
            (172, 50.0),
            (176, 100.0),
            (180, 200.0),
            (184, 0.2),
            (188, 0.4),
            (192, 0.8),
            (196, 1.2),
            (200, 0.5),
            (204, 2.0),
            (224, 32.0),
            (228, 3.0),
        ];
        for (offset, value) in fields {
            dnam[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        dnam[40..52].copy_from_slice(&[10, 20, 30, 255, 40, 50, 60, 128, 70, 80, 90, 0]);
        let record = Record::new(Tag::new(b"WATR"), 0x800)
            .sub(b"ANAM", [75])
            .sub(b"FNAM", [24])
            .sub(b"DNAM", dnam)
            .zsub(b"NAM2", "water/a.dds")
            .zsub(b"NAM3", "water/b.dds")
            .zsub(b"NAM4", "water/c.dds")
            .zsub(b"NAM5", "water/flow.dds");
        let material = water_material_from(&record);
        assert_eq!(material.opacity, 75);
        assert_eq!(material.flags, 24);
        assert_eq!(
            material.shallow_color,
            [10.0, 20.0, 30.0].map(|v| v / 255.0)
        );
        assert_eq!(material.deep_color, [40.0, 50.0, 60.0].map(|v| v / 255.0));
        assert_eq!(
            material.reflection_color,
            [70.0, 80.0, 90.0].map(|v| v / 255.0)
        );
        assert_eq!(
            material.noise_layers,
            [
                Some("water/a.dds".into()),
                Some("water/b.dds".into()),
                Some("water/c.dds".into())
            ]
        );
        assert_eq!(material.flow_texture.as_deref(), Some("water/flow.dds"));
        assert_eq!(
            material.shading,
            WaterShading {
                sun_specular_power: 63.0,
                reflectivity: 0.6,
                fresnel: 0.03,
                fog_near: 4.0,
                fog_far: 800.0,
                fog_amount: 0.75,
                wind_direction: [0.0, 90.0, 180.0],
                wind_speed: [5.0, 8.0, 12.0],
                uv_scale: [50.0, 100.0, 200.0],
                amplitude_scale: [0.2, 0.4, 0.8],
                reflection_magnitude: 1.2,
                sparkle_magnitude: 0.5,
                sun_specular_magnitude: 2.0,
                sparkle_power: 32.0,
                flowmap_scale: 3.0,
            }
        );
        let old =
            water_material_from(&Record::new(Tag::new(b"WATR"), 0x801).sub(b"DNAM", &dnam[..228]));
        assert_eq!(
            old.shading.flowmap_scale, 1.0,
            "LE's DNAM stops before flowmap scale"
        );
    }

    #[test]
    fn water_shader_short_missing_and_nonfinite_fields_keep_fallbacks() {
        let empty = water_material_from(&Record::new(Tag::new(b"WATR"), 0x800));
        assert_eq!(empty.opacity, 255);
        assert_eq!(empty.shading, WaterShading::default());
        assert_eq!(empty.noise_layers, [None, None, None]);
        for size in 0..232 {
            let data = vec![0_u8; size];
            let material =
                water_material_from(&Record::new(Tag::new(b"WATR"), 0x800).sub(b"DNAM", data));
            assert!(material.shading.fog_far.is_finite());
            if size < 52 {
                assert_eq!(material.shallow_color, [0.0; 3]);
            }
        }
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let data = invalid.to_le_bytes().repeat(58);
            let material =
                water_material_from(&Record::new(Tag::new(b"WATR"), 0x800).sub(b"DNAM", data));
            assert_eq!(material.shading, WaterShading::default());
        }
    }

    #[test]
    fn water_shader_named_paths_override_legacy_nnam_by_layer() {
        let record = Record::new(Tag::new(b"WATR"), 0x800)
            .zsub(b"NNAM", "old/a.dds")
            .zsub(b"NNAM", "old/b.dds")
            .zsub(b"NNAM", "old/c.dds")
            .zsub(b"NAM2", "new/a.dds")
            .zsub(b"NAM4", "");
        let material = water_material_from(&record);
        assert_eq!(
            material.noise_layers,
            [
                Some("new/a.dds".into()),
                Some("old/b.dds".into()),
                Some("old/c.dds".into())
            ]
        );
    }

    fn water_nif(property: &[u8]) -> soe_nif::NifFile {
        let mut writer = crate::data_source::fixture::NifWriter::new(100);
        let shader = writer.add("BSWaterShaderProperty", property.to_vec());
        let (vertices, triangles) = crate::data_source::fixture::quad();
        let shape = writer.bs_tri_shape(
            "Water",
            [0.0; 3],
            -1,
            shader,
            -1,
            soe_nif::vertex::attr::VERTEX,
            &vertices,
            &triangles,
            true,
        );
        let shape = writer.add("BSTriShape", shape);
        writer.roots = vec![shape];
        soe_nif::parse(&writer.finish()).unwrap()
    }

    #[test]
    fn water_shader_reads_nif_flags_uv_transform_and_extra_data() {
        use crate::data_source::fixture::Buf;
        let mut bytes = Buf::default();
        bytes
            .i32(-1)
            .u32(2)
            .i32(-1)
            .i32(-1)
            .i32(-1)
            .u32(0x80000008)
            .u32(0x21)
            .floats(&[0.25, -0.5, 2.0, 3.0])
            .u32(0x3C4);
        let file = water_nif(&bytes.0);
        let shape = file.meshes().unwrap()[0].block;
        assert_eq!(
            mesh_inputs(&file, shape).unwrap(),
            Some(crate::mesh::WaterMesh {
                flags1: 0x80000008,
                flags2: 0x21,
                water_flags: 0x3C4,
                uv_offset: [0.25, -0.5],
                uv_scale: [2.0, 3.0],
            })
        );
    }

    #[test]
    fn water_shader_rejects_truncated_nonfinite_and_trailing_nif_inputs() {
        use crate::data_source::fixture::Buf;
        let mut bytes = Buf::default();
        bytes
            .i32(-1)
            .u32(0)
            .i32(-1)
            .u32(8)
            .u32(0x21)
            .floats(&[0.0, 0.0, 1.0, 1.0])
            .u32(0x1C4);
        for size in 0..bytes.0.len() {
            let mut file = water_nif(&bytes.0);
            let shape = file.meshes().unwrap()[0].block;
            let soe_nif::blocks::Block::Unknown { data, .. } = &mut file.blocks[0] else {
                panic!("fixture must retain the water property");
            };
            data.truncate(size);
            assert!(mesh_inputs(&file, shape).is_err(), "truncation at {size}");
        }
        bytes.0[20..24].copy_from_slice(&f32::NAN.to_le_bytes());
        let file = water_nif(&bytes.0);
        assert!(mesh_inputs(&file, file.meshes().unwrap()[0].block).is_err());
        bytes.0[20..24].copy_from_slice(&0.0_f32.to_le_bytes());
        bytes.0.push(0);
        let file = water_nif(&bytes.0);
        assert!(mesh_inputs(&file, file.meshes().unwrap()[0].block).is_err());
    }
}
