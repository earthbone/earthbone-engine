// SPDX-License-Identifier: GPL-3.0-or-later

//! Spatial samples of the existing xEdit CELL/LGTM/LIGH detection estimate.
//! OpenMW sceneutil/clusteredlighting.hpp declares rendering light grids.
//! This CPU grid instead sums all influencing lights in FormID order, without
//! a renderer light limit, and reads shared reference state each frame.

use std::collections::{BTreeMap, HashMap};

use soe_esm::{plugin::flags, GlobalFormId};
use soe_game::World;

use super::spatial::bucket;
use super::{length, sub, T};

const GRID_WIDTH: f32 = 1024.0;
const MAX_LIGHT_BUCKETS: u64 = 512;

struct Light {
    position: [f32; 3],
    radius: f32,
    intensity: f32,
}

#[derive(Default)]
struct CellLights {
    ambient: f32,
    lights: Vec<Light>,
    grid: HashMap<[i32; 3], Vec<usize>>,
    wide: Vec<usize>,
}

/// A frame's spatial light samples derived from current shared reference
/// placement and visibility. No reference scene nodes are allocated.
pub struct LightSamples {
    cells: BTreeMap<GlobalFormId, CellLights>,
}

impl LightSamples {
    /// Build light neighborhoods once for the cells containing live actors.
    pub fn new(world: &World, cells: &[GlobalFormId]) -> Self {
        let mut result = Self {
            cells: BTreeMap::new(),
        };
        for &cell in cells {
            result.cells.entry(cell).or_insert_with(|| {
                let mut samples = CellLights {
                    ambient: ambient_level(world, cell),
                    ..CellLights::default()
                };
                for id in world.light_references_in_cell(cell) {
                    let Some(light) = read_light(world, id) else {
                        continue;
                    };
                    let i = samples.lights.len();
                    let min = bucket(light.position.map(|value| value - light.radius), GRID_WIDTH);
                    let max = bucket(light.position.map(|value| value + light.radius), GRID_WIDTH);
                    let count = (0..3)
                        .map(|axis| (i64::from(max[axis]) - i64::from(min[axis]) + 1) as u64)
                        .fold(1_u64, u64::saturating_mul);
                    if count > MAX_LIGHT_BUCKETS {
                        samples.wide.push(i);
                    } else {
                        for x in min[0]..=max[0] {
                            for y in min[1]..=max[1] {
                                for z in min[2]..=max[2] {
                                    samples.grid.entry([x, y, z]).or_default().push(i);
                                }
                            }
                        }
                    }
                    samples.lights.push(light);
                }
                samples
            });
        }
        result
    }

    /// Sum ambient and influencing light contributions in the CK 0 to 150
    /// range, preserving the full scan's accumulation order.
    pub fn level(&self, cell: GlobalFormId, point: [f32; 3]) -> f32 {
        let Some(samples) = self.cells.get(&cell) else {
            return 0.0;
        };
        let mut indices = samples.wide.clone();
        if let Some(nearby) = samples.grid.get(&bucket(point, GRID_WIDTH)) {
            indices.extend_from_slice(nearby);
        }
        indices.sort_unstable();
        let mut level = samples.ambient;
        for i in indices {
            level += contribution(&samples.lights[i], point);
        }
        level.clamp(0.0, 150.0)
    }
}

fn contribution(light: &Light, point: [f32; 3]) -> f32 {
    let distance = length(sub(light.position, point));
    if distance < light.radius {
        light.intensity * (1.0 - distance / light.radius)
    } else {
        0.0
    }
}

fn read_light(world: &World, id: GlobalFormId) -> Option<Light> {
    let runtime = world.reference_ref(id);
    let record = world.record(id);
    let base = if let Some(runtime) = runtime {
        if !runtime.enabled || runtime.deleted || runtime.destroyed {
            return None;
        }
        runtime.base?
    } else {
        let record = record?;
        if record.flags & flags::INITIALLY_DISABLED != 0 {
            return None;
        }
        let bytes = record.find(T(b"NAME")).filter(|bytes| bytes.len() == 4)?;
        world.resolve_from(
            id,
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        )?
    };
    let light = world
        .record(base)
        .filter(|record| record.tag == T(b"LIGH"))?;
    let data = light.find(T(b"DATA")).filter(|bytes| bytes.len() >= 48)?;
    let flags = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    if runtime.is_none() && flags & (1 << 5) != 0 {
        return None;
    }
    let radius = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as f32;
    if radius <= 0.0 {
        return None;
    }
    let position = if let Some(runtime) = runtime {
        runtime.position
    } else {
        let bytes = record?.find(T(b"DATA")).filter(|bytes| bytes.len() >= 12)?;
        std::array::from_fn(|i| {
            let start = i * 4;
            f32::from_le_bytes(bytes[start..start + 4].try_into().unwrap_or_default())
        })
    };
    if !position.iter().all(|value| value.is_finite()) {
        return None;
    }
    let brightness = (f32::from(data[8]) + f32::from(data[9]) + f32::from(data[10])) / 3.0;
    let sign = if flags & (1 << 2) != 0 { -1.0 } else { 1.0 };
    let fade = light
        .find(T(b"FNAM"))
        .filter(|bytes| bytes.len() == 4)
        .map_or(1.0, |bytes| {
            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).clamp(0.0, 10.0)
        });
    Some(Light {
        position,
        radius,
        intensity: sign * brightness * fade * (100.0 / 255.0),
    })
}

fn ambient_level(world: &World, cell: GlobalFormId) -> f32 {
    let cell_record = world.record(cell);
    let cell_light = cell_record.and_then(|record| record.find(T(b"XCLL")));
    let inherited = cell_light
        .filter(|bytes| bytes.len() >= 92)
        .is_some_and(|bytes| {
            u32::from_le_bytes([bytes[88], bytes[89], bytes[90], bytes[91]]) & 1 != 0
        });
    let template = cell_record
        .and_then(|record| record.find(T(b"LTMP")))
        .filter(|bytes| bytes.len() == 4)
        .and_then(|bytes| {
            world.resolve_from(
                cell,
                u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            )
        })
        .and_then(|id| world.record(id))
        .filter(|record| record.tag == T(b"LGTM"))
        .and_then(|record| record.find(T(b"DATA")));
    let ambient = (if inherited || cell_light.is_none() {
        template.filter(|bytes| bytes.len() >= 24)
    } else {
        cell_light.filter(|bytes| bytes.len() >= 92)
    })
    .map_or(0.0, |bytes| {
        (f32::from(bytes[0]) + f32::from(bytes[1]) + f32::from(bytes[2])) / 3.0
    });
    ambient * (100.0 / 255.0)
}

/// Detection light from the cell's ambient and current light references.
pub fn light_level(world: &World, cell: GlobalFormId, point: [f32; 3]) -> f32 {
    LightSamples::new(world, &[cell]).level(cell, point)
}

#[cfg(test)]
pub(super) fn full_scan_light_level(world: &World, cell: GlobalFormId, point: [f32; 3]) -> f32 {
    let mut ids = world
        .order()
        .winners_of(T(b"REFR"))
        .filter_map(|(id, record)| {
            let belongs = world.reference_ref(id).map_or_else(
                || {
                    record
                        .placement
                        .cell
                        .is_some_and(|raw| world.resolve_from(id, raw) == Some(cell))
                },
                |reference| reference.cell == Some(cell),
            );
            belongs.then_some(id)
        })
        .collect::<Vec<_>>();
    ids.extend(world.runtime_references_in_cell(cell));
    ids.sort_unstable();
    let mut level = ambient_level(world, cell);
    for id in ids {
        if let Some(light) = read_light(world, id) {
            level += contribution(&light, point);
        }
    }
    level.clamp(0.0, 150.0)
}
