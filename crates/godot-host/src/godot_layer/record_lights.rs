// SPDX-License-Identifier: GPL-3.0-or-later

//! Placed LIGH RIDs from xEdit wbDefinitionsTES5.pas and UESP LIGH.
//! OpenMW sceneutil/lightcontroller.cpp supplies smooth flicker targets.
//! Presentation reads shared references and never owns their enabled state.

use super::*;
use godot::classes::rendering_server::LightParam;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Parameters {
    color: [f32; 3],
    radius: f32,
    flags: u32,
    falloff: f32,
    angle: f32,
    fade: f32,
    period: f32,
    intensity: f32,
    movement: f32,
}

impl Parameters {
    fn read(record: &soe_esm::Record) -> Option<Self> {
        let base = light_render_data(record)?;
        let data = record.find(soe_esm::Tag::new(b"DATA"))?;
        let scalar = |offset| {
            let value = f32::from_le_bytes(data[offset..offset + 4].try_into().ok()?);
            value.is_finite().then_some(value)
        };
        let flags = u32::from_le_bytes(data[12..16].try_into().ok()?);
        if flags & 32 != 0 {
            return None;
        }
        let fade = record
            .find(soe_esm::Tag::new(b"FNAM"))
            .filter(|bytes| bytes.len() == 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map_or(1.0, f32::from_le_bytes);
        Some(Self {
            color: base.color,
            radius: base.radius,
            flags,
            falloff: scalar(16)?.max(0.0),
            angle: scalar(20)?.clamp(0.001, 179.9) * 0.5,
            fade: if fade.is_finite() {
                fade.clamp(0.0, 10.0)
            } else {
                return None;
            },
            period: scalar(28)?.abs().max(0.01),
            intensity: scalar(32)?.clamp(0.0, 1.0),
            movement: scalar(36)?.max(0.0) * crate::units::METRES_PER_UNIT,
        })
    }

    fn spot(self) -> bool {
        self.flags & ((1 << 9) | (1 << 10) | (1 << 11)) != 0
    }

    fn shadow(self) -> bool {
        self.flags & ((1 << 10) | (1 << 11) | (1 << 12)) != 0
    }

    fn sample(self, seconds: f64, seed: u32) -> (f32, Vector3) {
        if self.flags & (8 | 64 | 128 | 256) == 0 {
            return (self.fade, Vector3::ZERO);
        }
        let slow = if self.flags & (64 | 256) == 0 {
            1.0
        } else {
            0.5
        };
        let t = (seconds * slow / f64::from(self.period)).rem_euclid(1_000_000.0);
        let pulse = self.flags & (128 | 256) != 0;
        let value = if pulse {
            (t * std::f64::consts::TAU).sin() as f32
        } else {
            let tick = t * 15.0;
            let hash = |tick: u32| {
                let mut n = tick.wrapping_add(seed).wrapping_mul(0x9e37_79b9);
                n ^= n >> 16;
                n = n.wrapping_mul(0x85eb_ca6b);
                ((n ^ (n >> 13)) & 0xffff) as f32 / 32767.5 - 1.0
            };
            let f = tick.fract() as f32;
            let smooth = f * f * (3.0 - 2.0 * f);
            let a = hash(tick as u32);
            a + (hash(tick as u32 + 1) - a) * smooth
        };
        (
            self.fade * (1.0 + value * self.intensity).max(0.0),
            Vector3::new(value, (t * 2.3).sin() as f32, (t * 1.7).cos() as f32) * self.movement,
        )
    }
}

pub(super) struct PlacedLight {
    light: Rid,
    instance: Rid,
    cell: GlobalFormId,
    parameters: Parameters,
    transform: Transform3D,
    submitted_transform: Transform3D,
    energy: f32,
}

impl CellStreamerNode {
    pub(super) fn sync_record_lights(&mut self, world: &soe_game::World) {
        let Some(scenario) = self.scenario() else {
            return;
        };
        let mut active = BTreeMap::new();
        let summoned: std::collections::BTreeSet<_> = world.summoned_effect_references().collect();
        for &cell in &self.rendered_cells {
            for id in world.light_references_in_cell(cell) {
                if world.reference_ref(id).is_some_and(|state| state.destroyed) {
                    continue;
                }
                if summoned.contains(&id) {
                    continue;
                }
                let Some(placement) = world.ref_placement(id).filter(|p| p.visible) else {
                    continue;
                };
                let base = world
                    .reference_ref(id)
                    .and_then(|state| state.base)
                    .or_else(|| {
                        let bytes = world.record(id)?.find(soe_esm::Tag::new(b"NAME"))?;
                        world.resolve_from(id, u32::from_le_bytes(bytes.try_into().ok()?))
                    });
                let Some(parameters) = base
                    .and_then(|base| world.record(base))
                    .and_then(Parameters::read)
                else {
                    continue;
                };
                let iso = Iso::placement(placement.position, placement.rotation, 1.0);
                let transform = transform3d_from(placement_to_godot(
                    iso.rotation.rows,
                    placement.position,
                    1.0,
                ));
                active.insert(id, (cell, parameters, transform));
            }
        }
        let mut server = RenderingServer::singleton();
        self.placed_lights.retain(|id, light| {
            if active
                .get(id)
                .is_some_and(|(_, parameters, _)| *parameters == light.parameters)
            {
                return true;
            }
            server.free_rid(light.instance);
            server.free_rid(light.light);
            false
        });
        for (id, (cell, parameters, transform)) in active {
            if let Some(light) = self.placed_lights.get_mut(&id) {
                light.transform = transform;
                light.cell = cell;
                continue;
            }
            let light = if parameters.spot() {
                server.spot_light_create()
            } else {
                server.omni_light_create()
            };
            server.light_set_color(
                light,
                Color::from_rgb(
                    parameters.color[0],
                    parameters.color[1],
                    parameters.color[2],
                ),
            );
            server.light_set_param(light, LightParam::RANGE, parameters.radius);
            server.light_set_param(light, LightParam::ATTENUATION, parameters.falloff);
            server.light_set_param(light, LightParam::ENERGY, parameters.fade);
            server.light_set_negative(light, parameters.flags & 4 != 0);
            server.light_set_shadow(light, parameters.shadow());
            if parameters.spot() {
                server.light_set_param(
                    light,
                    LightParam::SPOT_ANGLE,
                    if parameters.flags & (1 << 11) != 0 {
                        90.0
                    } else {
                        parameters.angle
                    },
                );
            }
            if let Some(fade) = crate::cell_lighting::cell_lighting(world.order(), cell)
                .and_then(|lighting| lighting.distance_fade())
            {
                server.light_set_distance_fade(light, true, fade[0], fade[0], fade[1]);
            }
            let instance = server.instance_create2(light, scenario);
            server.instance_set_transform(instance, transform);
            self.placed_lights.insert(
                id,
                PlacedLight {
                    light,
                    instance,
                    cell,
                    parameters,
                    transform,
                    submitted_transform: transform,
                    energy: parameters.fade,
                },
            );
        }
    }

    pub(super) fn animate_record_lights(&mut self, delta: f64) {
        if delta.is_finite() && delta > 0.0 {
            self.light_seconds += delta;
        }
        let mut server = RenderingServer::singleton();
        for (id, light) in &mut self.placed_lights {
            let (energy, movement) = light
                .parameters
                .sample(self.light_seconds, id.object ^ u32::from(id.file));
            if light.energy != energy {
                server.light_set_param(light.light, LightParam::ENERGY, energy);
                light.energy = energy;
            }
            let mut transform = light.transform;
            transform.origin += movement;
            if light.submitted_transform != transform {
                server.instance_set_transform(light.instance, transform);
                light.submitted_transform = transform;
            }
        }
    }

    pub(super) fn free_record_lights(&mut self, cell: Option<GlobalFormId>) {
        let mut server = RenderingServer::singleton();
        self.placed_lights.retain(|_, light| {
            if cell.is_some_and(|cell| cell != light.cell) {
                return true;
            }
            server.free_rid(light.instance);
            server.free_rid(light.light);
            false
        });
    }
}

impl CellStreamerNode {
    /// Authored light flags and submitted RIDs for capture and budget checks.
    pub(super) fn record_light_rows(&self) -> Array<VarDictionary> {
        let mut rows = Array::new();
        for (id, light) in &self.placed_lights {
            let mut row = VarDictionary::new();
            row.set("reference", id.to_string().as_str());
            row.set("instance", light.instance);
            row.set("light", light.light);
            row.set("shadow", light.parameters.shadow());
            row.set("spot", light.parameters.spot());
            row.set("radius", light.parameters.radius);
            row.set("energy", light.energy);
            rows.push(&row);
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(flags: u32) -> soe_esm::Record {
        let mut data = vec![0; 48];
        data[4..8].copy_from_slice(&512_u32.to_le_bytes());
        data[8..11].copy_from_slice(&[255, 128, 32]);
        data[12..16].copy_from_slice(&flags.to_le_bytes());
        for (offset, value) in [(16, 2.0_f32), (20, 60.0), (28, 0.5), (32, 0.25), (36, 5.0)] {
            data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        soe_esm::Record::new(soe_esm::Tag::new(b"LIGH"), 1)
            .sub(b"DATA", data)
            .sub(b"FNAM", 1.5_f32.to_le_bytes())
    }

    #[test]
    fn r4_record_light_types_preserve_shadow_and_spot_flags() {
        for (flags, spot, shadow) in [
            (0, false, false),
            (1 << 9, true, false),
            (1 << 10, true, true),
            (1 << 11, true, true),
            (1 << 12, false, true),
        ] {
            let light = Parameters::read(&record(flags)).unwrap();
            assert_eq!((light.spot(), light.shadow()), (spot, shadow));
            assert_eq!(light.radius, 512.0 * crate::units::METRES_PER_UNIT);
            assert_eq!(light.angle, 30.0);
            assert_eq!(light.fade, 1.5);
        }
        assert!(Parameters::read(&record(32)).is_none());
    }

    #[test]
    fn r4_record_light_rejects_short_and_nonfinite_data() {
        let short = soe_esm::Record::new(soe_esm::Tag::new(b"LIGH"), 1).sub(b"DATA", vec![0; 47]);
        assert!(Parameters::read(&short).is_none());
        let mut invalid = record(8);
        let mut data = invalid.find(soe_esm::Tag::new(b"DATA")).unwrap().to_vec();
        data[28..32].copy_from_slice(&f32::NAN.to_le_bytes());
        invalid = soe_esm::Record::new(soe_esm::Tag::new(b"LIGH"), 1).sub(b"DATA", data);
        assert!(Parameters::read(&invalid).is_none());
    }

    #[test]
    fn r4_record_flicker_is_bounded_reproducible_and_moves() {
        let light = Parameters::read(&record(8)).unwrap();
        let a = light.sample(0.125, 7);
        assert_eq!(a, light.sample(0.125, 7));
        assert_ne!(a, light.sample(0.625, 7));
        for tick in 0..1000 {
            let (energy, movement) = light.sample(f64::from(tick) / 60.0, 7);
            assert!((1.125..=1.875).contains(&energy));
            assert!(movement.length() <= 5.0 * crate::units::METRES_PER_UNIT * 3.0_f32.sqrt());
        }
        let normal = Parameters::read(&record(0)).unwrap();
        assert_eq!(normal.sample(100.0, 7), (1.5, Vector3::ZERO));
    }
}
