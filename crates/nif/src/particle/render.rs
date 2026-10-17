// SPDX-License-Identifier: GPL-3.0-or-later

//! Render fields from nifly Particles.cpp and nif.xml. OpenMW's particle.cpp
//! supplies grow/fade and normalized-age colour semantics.

use super::{emitters, raw, system, Emitter};
use crate::blocks::Block;
use crate::error::{Error, Result};
use crate::mesh::Material;
use crate::stream::Stream;
use crate::types::{BlockRef, Transform};
use crate::NifFile;

/// Authored volume in emitter-node coordinates, in game units.
#[derive(Debug, Clone, PartialEq)]
pub enum Volume {
    Sphere(f32),
    Cylinder { radius: f32, height: f32 },
    Box([f32; 3]),
}

/// Timing plus the fields needed to draw an SSE volume emitter.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderEmitter {
    pub timing: Emitter,
    pub block: usize,
    pub material: Material,
    pub origin: Transform,
    pub world_space: bool,
    /// Speed, speed variation, declination, declination variation,
    /// planar angle and planar variation, in NIF order.
    pub velocity: [f32; 6],
    pub volume: Volume,
    pub radius_variation: f32,
    pub grow_time: f32,
    pub fade_time: f32,
    pub base_scale: f32,
    pub colours: Vec<(f32, [f32; 4])>,
    pub gravity: [f32; 3],
    pub simple_colours: Option<([f32; 6], [[f32; 4]; 3])>,
    pub scales: Vec<f32>,
    pub atlas: Vec<[f32; 4]>,
    pub frames: [f32; 7],
    pub aspect: f32,
    pub rotation: [f32; 4],
    pub random_rotation_sign: bool,
    pub drag: [f32; 3],
    /// BSPSysLODModifier fields in NIF order: begin distance, end distance,
    /// end emit scale and end size. Distances are fractions of the far range.
    pub lod: Option<[f32; 4]>,
    /// Unmapped modifiers are reported so a host can expose its limits.
    pub unsupported: Vec<String>,
}

fn header<'a>(file: &NifFile, bytes: &'a [u8]) -> Result<(Stream<'a>, bool, String)> {
    let mut s = Stream::new(bytes);
    let name = s.string_index(&file.header.strings)?;
    s.u32()?;
    s.block_ref()?;
    let active = s.bool()?;
    Ok((s, active, name))
}

fn invalid(block: usize, reason: &str) -> Error {
    Error::InvalidData {
        block,
        reason: reason.into(),
    }
}

/// Resolve a system's scene parent using the existing model-space node pose.
fn parent_pose(file: &NifFile, block: usize, pose: &[Transform]) -> Transform {
    file.blocks
        .iter()
        .enumerate()
        .find_map(|(index, candidate)| {
            if let Block::Node(node) = candidate {
                node.children
                    .contains(&BlockRef(block as i32))
                    .then_some(pose[index])
            } else {
                None
            }
        })
        .unwrap_or(Transform::IDENTITY)
}

fn atlas(file: &NifFile, reference: BlockRef) -> Result<Vec<[f32; 4]>> {
    let Some(data) = raw(file, reference, "NiPSysData") else {
        return Ok(Vec::new());
    };
    let mut s = Stream::new(data);
    s.i32()?;
    s.u16()?;
    s.u8()?;
    s.u8()?;
    s.bool()?;
    s.u16()?;
    s.u32()?;
    s.bool()?;
    s.bound()?;
    s.bool()?;
    s.u16()?;
    s.block_ref()?;
    s.bool()?;
    s.u16()?;
    s.bool()?;
    s.bool()?;
    s.bool()?;
    s.bool()?;
    s.bool()?;
    let count = s.counted(16)?;
    let mut atlas = Vec::with_capacity(count);
    for _ in 0..count {
        atlas.push(s.color4()?);
    }
    Ok(atlas)
}

/// Extract renderer input without changing the timing-only emitter contract.
pub fn render_emitters(file: &NifFile) -> Result<Vec<RenderEmitter>> {
    let timing = emitters(file)?;
    if timing.is_empty() {
        return Ok(Vec::new());
    }
    let pose = crate::object_animation::ObjectAnimation::from_file(file)?.rest_pose();
    let mut result = Vec::new();
    for timing in timing {
        let block = timing
            .rate
            .time
            .target
            .get()
            .ok_or_else(|| invalid(0, "missing particle system"))?;
        let Some(Block::Unknown { data, .. }) = file.blocks.get(block) else {
            continue;
        };
        let sys = system(file, data)?;
        let Some(Block::EffectShader(shader)) = file.get(sys.shader) else {
            continue;
        };
        let mut material = Material::from_effect(shader);
        if let Some(Block::AlphaProperty(alpha)) = file.get(sys.alpha) {
            material.apply_alpha(alpha);
        }
        let mut render = RenderEmitter {
            timing,
            block,
            material,
            origin: parent_pose(file, block, &pose.transforms).then(&sys.transform),
            world_space: sys.world_space,
            velocity: [0.0; 6],
            volume: Volume::Sphere(0.0),
            radius_variation: 0.0,
            grow_time: 0.0,
            fade_time: 0.0,
            base_scale: 1.0,
            colours: Vec::new(),
            gravity: [0.0; 3],
            simple_colours: None,
            scales: Vec::new(),
            atlas: atlas(file, sys.data)?,
            frames: [0.0; 7],
            aspect: 1.0,
            rotation: [0.0; 4],
            random_rotation_sign: false,
            drag: [0.0; 3],
            lod: None,
            unsupported: Vec::new(),
        };
        for reference in sys.modifiers {
            let Some(Block::Unknown { type_name, data }) = file.get(reference) else {
                continue;
            };
            let (mut s, active, name) = header(file, data)?;
            if !active {
                continue;
            }
            match type_name.as_str() {
                "NiPSysSphereEmitter" | "NiPSysCylinderEmitter" | "NiPSysBoxEmitter" => {
                    if name != render.timing.name {
                        continue;
                    }
                    for value in &mut render.velocity {
                        *value = s.f32()?;
                    }
                    s.color4()?;
                    s.f32()?;
                    render.radius_variation = s.f32()?;
                    s.f32()?;
                    s.f32()?;
                    let node = s.block_ref()?;
                    render.origin = node
                        .get()
                        .and_then(|i| pose.transforms.get(i))
                        .copied()
                        .unwrap_or(render.origin);
                    render.volume = match type_name.as_str() {
                        "NiPSysCylinderEmitter" => Volume::Cylinder {
                            radius: s.f32()?,
                            height: s.f32()?,
                        },
                        "NiPSysBoxEmitter" => Volume::Box([s.f32()?, s.f32()?, s.f32()?]),
                        _ => Volume::Sphere(s.f32()?),
                    };
                }
                "BSPSysSimpleColorModifier" => {
                    let mut percentages = [0.0; 6];
                    for value in &mut percentages {
                        *value = s.f32()?;
                    }
                    render.simple_colours =
                        Some((percentages, [s.color4()?, s.color4()?, s.color4()?]));
                }
                "BSPSysScaleModifier" => {
                    let count = s.counted(4)?;
                    for _ in 0..count {
                        render.scales.push(s.f32()?);
                    }
                }
                "BSPSysSubTexModifier" => {
                    for value in &mut render.frames {
                        *value = s.f32()?;
                    }
                }
                "NiPSysRotationModifier" => {
                    for value in &mut render.rotation {
                        *value = s.f32()?;
                    }
                    render.random_rotation_sign = s.bool()?;
                    s.bool()?;
                    s.vec3()?;
                }
                "NiPSysSpawnModifier" => {
                    if s.u16()? > 0 {
                        render
                            .unsupported
                            .push("NiPSysSpawnModifier generations".into());
                    }
                }
                "BSPSysLODModifier" => {
                    render.lod = Some([s.f32()?, s.f32()?, s.f32()?, s.f32()?]);
                }
                "NiPSysDragModifier" => {
                    let node = s.block_ref()?;
                    let axis = s.vec3()?;
                    let percentage = s.f32()?;
                    let range = s.f32()?;
                    let falloff = s.f32()?;
                    if range == f32::MAX && falloff == f32::MAX {
                        let axis = node
                            .get()
                            .and_then(|i| pose.transforms.get(i))
                            .map_or(axis, |t| t.rotation.apply(axis))
                            .to_array();
                        for (drag, axis) in render.drag.iter_mut().zip(axis) {
                            *drag += axis.abs() * percentage;
                        }
                    } else {
                        render.unsupported.push(type_name.clone());
                    }
                }
                "NiPSysGrowFadeModifier" => {
                    render.grow_time = s.f32()?;
                    s.u16()?;
                    render.fade_time = s.f32()?;
                    s.u16()?;
                    render.base_scale = s.f32()?;
                }
                "NiPSysColorModifier" => {
                    let colours = s.block_ref()?;
                    if let Some(bytes) = raw(file, colours, "NiColorData") {
                        let mut keys = Stream::new(bytes);
                        let count = keys.counted(20)?;
                        let kind = if count > 0 { keys.u32()? } else { 1 };
                        if kind != 1 {
                            render
                                .unsupported
                                .push(format!("NiColorData interpolation {kind}"));
                            continue;
                        }
                        for _ in 0..count {
                            render.colours.push((keys.f32()?, keys.color4()?));
                        }
                    }
                }
                "NiPSysGravityModifier" => {
                    let node = s.block_ref()?;
                    let axis = s.vec3()?;
                    let decay = s.f32()?;
                    let strength = s.f32()?;
                    let kind = s.u32()?;
                    let turbulence = s.f32()?;
                    s.f32()?;
                    let world_aligned = s.bool()?;
                    if kind == 0 && decay == 0.0 && turbulence == 0.0 {
                        let axis = if world_aligned {
                            axis
                        } else {
                            node.get()
                                .and_then(|i| pose.transforms.get(i))
                                .map_or(axis, |t| t.rotation.apply(axis))
                        };
                        for (g, axis) in render.gravity.iter_mut().zip(axis.to_array()) {
                            *g += axis * strength;
                        }
                    } else {
                        render.unsupported.push(type_name.clone());
                    }
                }
                "NiPSysPositionModifier"
                | "NiPSysBoundUpdateModifier"
                | "NiPSysAgeDeathModifier" => {}
                _ => render.unsupported.push(type_name.clone()),
            }
        }
        let volume = match render.volume {
            Volume::Sphere(r) => [r, 0.0, 0.0],
            Volume::Cylinder { radius, height } => [radius, height, 0.0],
            Volume::Box(s) => s,
        };
        let finite = volume
            .into_iter()
            .chain([render.timing.radius])
            .chain(render.frames)
            .chain(render.rotation)
            .chain(render.drag)
            .chain(render.scales.iter().copied())
            .chain(render.atlas.iter().flatten().copied())
            .chain(
                render
                    .simple_colours
                    .iter()
                    .flat_map(|(p, c)| p.iter().chain(c.iter().flatten()))
                    .copied(),
            )
            .chain(
                render
                    .velocity
                    .into_iter()
                    .chain(render.gravity)
                    .chain([
                        render.radius_variation,
                        render.grow_time,
                        render.fade_time,
                        render.base_scale,
                    ])
                    .chain(
                        render
                            .colours
                            .iter()
                            .flat_map(|(t, c)| std::iter::once(*t).chain(*c)),
                    ),
            )
            .all(f32::is_finite);
        if !finite {
            return Err(invalid(block, "particle render values must be finite"));
        }
        result.push(render);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{AlphaProperty, EffectShader};
    use crate::header::Header;

    fn words(bytes: &mut Vec<u8>, values: &[u32]) {
        for word in values {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    fn floats(bytes: &mut Vec<u8>, values: &[f32]) {
        words(
            bytes,
            &values.iter().map(|f| f.to_bits()).collect::<Vec<_>>(),
        );
    }
    fn opaque(name: &str, data: Vec<u8>) -> Block {
        Block::Unknown {
            type_name: name.into(),
            data,
        }
    }
    fn modifier_prefix() -> Vec<u8> {
        let mut bytes = Vec::new();
        words(&mut bytes, &[1, 1000, 0]);
        bytes.push(1);
        bytes
    }
    fn fixture() -> NifFile {
        let mut sys = Vec::new();
        words(&mut sys, &[0, 0, 2, 0]);
        floats(
            &mut sys,
            &[
                10.0, 20.0, 30.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 2.0,
            ],
        );
        words(&mut sys, &[u32::MAX]);
        floats(&mut sys, &[0.0, 0.0, 0.0, 100.0]);
        words(&mut sys, &[u32::MAX, 3, 4, 0, 0, 0, 0, 5]);
        sys.push(1);
        words(&mut sys, &[3, 1, 6, 7]);
        let mut emitter = modifier_prefix();
        floats(
            &mut emitter,
            &[
                24.0, 4.0, 0.3, 0.2, 1.0, 0.5, 1.0, 0.5, 0.25, 1.0, 4.0, 2.0, 3.0, 1.0,
            ],
        );
        words(&mut emitter, &[u32::MAX]);
        floats(&mut emitter, &[9.0, 11.0]);
        let mut ctl = Vec::new();
        words(&mut ctl, &[u32::MAX]);
        ctl.extend_from_slice(&12_u16.to_le_bytes());
        floats(&mut ctl, &[1.0, 0.0, 0.0, 10.0]);
        words(&mut ctl, &[0, 8, 1, 9]);
        let mut quota = Vec::new();
        words(&mut quota, &[0]);
        quota.extend_from_slice(&7_u16.to_le_bytes());
        quota.extend_from_slice(&[0, 0, 1]);
        quota.extend_from_slice(&0_u16.to_le_bytes());
        words(&mut quota, &[0]);
        quota.push(0);
        floats(&mut quota, &[0.0, 0.0, 0.0, 1.0]);
        quota.push(0);
        quota.extend_from_slice(&0_u16.to_le_bytes());
        words(&mut quota, &[u32::MAX]);
        quota.push(1);
        quota.extend_from_slice(&0_u16.to_le_bytes());
        quota.extend_from_slice(&[1, 0, 0, 0, 0]);
        words(&mut quota, &[0]);
        let mut grow = modifier_prefix();
        floats(&mut grow, &[0.25]);
        grow.extend_from_slice(&0_u16.to_le_bytes());
        floats(&mut grow, &[0.75]);
        grow.extend_from_slice(&0_u16.to_le_bytes());
        floats(&mut grow, &[1.5]);
        let mut colour = modifier_prefix();
        words(&mut colour, &[10]);
        let mut keys = Vec::new();
        words(&mut keys, &[2, 1]);
        floats(
            &mut keys,
            &[0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
        );
        let mut rate = Vec::new();
        floats(&mut rate, &[24.0]);
        words(&mut rate, &[u32::MAX]);
        let mut visible = vec![1];
        words(&mut visible, &[u32::MAX]);
        NifFile {
            header: Header {
                version: 0x1402_0007,
                user_version: 12,
                bs_version: 100,
                author: String::new(),
                process_script: String::new(),
                export_script: String::new(),
                max_filepath: String::new(),
                block_types: Vec::new(),
                block_type_index: vec![0; 11],
                block_sizes: Vec::new(),
                strings: vec!["Smoke".into(), "Emitter".into()],
                groups: Vec::new(),
            },
            blocks: vec![
                opaque("NiParticleSystem", sys),
                opaque("NiPSysCylinderEmitter", emitter),
                opaque("NiPSysEmitterCtlr", ctl),
                Block::EffectShader(EffectShader {
                    source_texture: "textures/smoke.dds".into(),
                    base_color: [1.0; 4],
                    ..EffectShader::default()
                }),
                Block::AlphaProperty(AlphaProperty {
                    flags: 4845,
                    ..AlphaProperty::default()
                }),
                opaque("NiPSysData", quota),
                opaque("NiPSysGrowFadeModifier", grow),
                opaque("NiPSysColorModifier", colour),
                opaque("NiFloatInterpolator", rate),
                opaque("NiBoolInterpolator", visible),
                opaque("NiColorData", keys),
            ],
            roots: vec![BlockRef(0)],
        }
    }

    #[test]
    fn particle_render_preserves_source_material_motion_volume_and_age_fields() {
        let file = fixture();
        let emitters = render_emitters(&file).unwrap();
        assert_eq!(emitters.len(), 1);
        let e = &emitters[0];
        assert_eq!(e.material.diffuse(), Some("textures/smoke.dds"));
        assert!(e.material.blend.is_some());
        assert_eq!(e.velocity, [24.0, 4.0, 0.3, 0.2, 1.0, 0.5]);
        assert_eq!(
            e.volume,
            Volume::Cylinder {
                radius: 9.0,
                height: 11.0
            }
        );
        assert_eq!(e.origin.translation.to_array(), [10.0, 20.0, 30.0]);
        assert_eq!(e.origin.scale, 2.0);
        assert_eq!(e.timing.capacity, 7);
        assert_eq!(e.radius_variation, 2.0);
        assert_eq!((e.grow_time, e.fade_time, e.base_scale), (0.25, 0.75, 1.5));
        assert_eq!(e.colours, [(0.0, [1.0, 0.0, 0.0, 1.0]), (1.0, [0.0; 4])]);
        assert!(e.unsupported.is_empty());
    }

    #[test]
    fn particle_render_rejects_nonfinite_motion_and_truncated_volume() {
        let mut file = fixture();
        if let Block::Unknown { data, .. } = &mut file.blocks[1] {
            data[13..17].copy_from_slice(&f32::NAN.to_le_bytes());
        }
        assert!(render_emitters(&file).is_err());
        let mut file = fixture();
        if let Block::Unknown { data, .. } = &mut file.blocks[1] {
            data.pop();
        }
        assert!(render_emitters(&file).is_err());
    }
    #[test]
    fn hidden_source_keeps_deaths_and_resumes_without_hidden_births() {
        let e = render_emitters(&fixture()).unwrap().remove(0);
        let mut timeline = super::super::Timeline::new(e.timing);
        timeline.advance(0.25);
        assert_eq!(timeline.emitted(), 6);
        timeline.advance_disabled(4.0);
        assert_eq!(timeline.emitted(), 6);
        assert_eq!(timeline.alive(), 0);
        timeline.advance(0.25);
        assert_eq!(timeline.emitted(), 12);
        assert_eq!(timeline.alive(), 6);
    }
    #[test]
    fn installed_particle_render_asset_survey() {
        let Some(directory) = std::env::var_os("SOE_NIF_SAMPLES") else {
            return;
        };
        let path = std::path::Path::new(&directory)
            .join("meshes/clutter/giant/giantcampfire01burning.nif");
        let file = crate::parse(&std::fs::read(path).unwrap()).unwrap();
        for (index, block) in file.blocks.iter().enumerate() {
            if let Block::Unknown { type_name, data } = block {
                if type_name.starts_with("NiPSys") || type_name.starts_with("BSPSys") {
                    eprintln!("R4 SOURCE {index} {type_name} {data:02x?}");
                }
            }
        }
        let emitters = render_emitters(&file).unwrap();
        assert_eq!(emitters.len(), 3);
        for e in &emitters {
            eprintln!("R4 RENDER {e:?}");
        }
        for e in emitters {
            assert!(
                e.unsupported.is_empty(),
                "{} {:?}",
                e.timing.system_name,
                e.unsupported
            );
        }
    }
}
