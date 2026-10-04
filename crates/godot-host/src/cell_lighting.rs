// SPDX-License-Identifier: GPL-3.0-or-later

//! Resolve CELL lighting and its inherited LGTM fields for host rendering.

use godot::prelude::{Array, Color, VarDictionary};
use soe_esm::{records::cell::Lighting, GlobalFormId, LoadOrder, Record};

const AMBIENT: u32 = 0x001;
const DIRECTIONAL: u32 = 0x002;
const FOG_COLOR: u32 = 0x004;
const FOG_NEAR: u32 = 0x008;
const FOG_FAR: u32 = 0x010;
const DIRECTIONAL_ROTATION: u32 = 0x020;
const DIRECTIONAL_FADE: u32 = 0x040;
const CLIP_DISTANCE: u32 = 0x080;
const FOG_POWER: u32 = 0x100;
const FOG_MAX: u32 = 0x200;
const LIGHT_FADE: u32 = 0x400;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CellLighting {
    pub(crate) ambient: [u8; 3],
    pub(crate) directional: [u8; 3],
    pub(crate) ambient_directions: [[u8; 3]; 6],
    pub(crate) fog_near_color: [u8; 3],
    pub(crate) fog_far_color: [u8; 3],
    pub(crate) fog_near: f32,
    pub(crate) fog_far: f32,
    pub(crate) rotation_xy: i32,
    pub(crate) rotation_z: i32,
    pub(crate) directional_fade: f32,
    pub(crate) fog_clip_distance: f32,
    pub(crate) fog_power: f32,
    pub(crate) fog_max: f32,
    pub(crate) light_fade_begin: f32,
    pub(crate) light_fade_end: f32,
}

fn rgb(color: [u8; 4]) -> [u8; 3] {
    [color[0], color[1], color[2]]
}

fn template_lighting(record: &Record) -> Option<Lighting> {
    let data = record.find(soe_esm::Tag::new(b"DATA"))?;
    if data.len() < 84 {
        return None;
    }
    let dalc = record.find(soe_esm::Tag::new(b"DALC"));
    let color = |bytes: &[u8]| [bytes[0], bytes[1], bytes[2], 0];
    let ambient_colors = dalc
        .filter(|bytes| bytes.len() >= 24)
        .map(|bytes| std::array::from_fn(|i| color(&bytes[i * 4..i * 4 + 4])))
        .unwrap_or([[0; 4]; 6]);
    let read_u32 = |offset: usize| {
        u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ])
    };
    let mut xcll = Vec::with_capacity(92);
    xcll.extend_from_slice(&data[..12]);
    for offset in [12, 16, 20, 24, 28, 32, 36] {
        xcll.extend_from_slice(&read_u32(offset).to_le_bytes());
    }
    for color in ambient_colors {
        xcll.extend_from_slice(&color);
    }
    if let Some(bytes) = dalc.filter(|bytes| bytes.len() >= 32) {
        xcll.extend_from_slice(&bytes[24..32]);
    } else {
        xcll.extend_from_slice(&[0; 8]);
    }
    xcll.extend_from_slice(&data[68..84]);
    xcll.extend_from_slice(&[0; 4]);
    Lighting::decode(&xcll).ok()
}

fn cell_lighting(order: &LoadOrder, id: GlobalFormId) -> Option<CellLighting> {
    let reference = order.winner_ref(id)?;
    let cell_record = order.record(reference);
    let cell = soe_esm::records::cell::Cell::from_record(cell_record, false).ok()?;
    if cell.flags & soe_esm::records::cell::flags::INTERIOR == 0 {
        return None;
    }
    let source = cell.lighting.as_ref();
    let inherited = cell
        .lighting_template
        .map(|raw| order.resolve(reference.file, raw))
        .and_then(|template| order.winner(template))
        .and_then(template_lighting);
    let template = inherited.as_ref();
    let flags = source
        .and_then(|value| value.inheritance_flags)
        .unwrap_or(0);
    let choose = |flag: u32| (flags & flag != 0).then_some(template).flatten();
    let own = source.or(template)?;
    let ambient = if let Some(t) = choose(AMBIENT) {
        &t.ambient_color
    } else {
        &own.ambient_color
    };
    let directional = if let Some(t) = choose(DIRECTIONAL) {
        &t.directional_color
    } else {
        &own.directional_color
    };
    let ambient_directions = choose(AMBIENT)
        .map_or(own.ambient_colors, |t| t.ambient_colors)
        .map(|color| rgb(color.0));
    let fog_near_color = if let Some(t) = choose(FOG_COLOR) {
        &t.fog_near_color
    } else {
        &own.fog_near_color
    };
    let fog_far_color = if let Some(t) = choose(FOG_COLOR) {
        t.fog_far_color.as_ref().unwrap_or(&t.fog_near_color)
    } else {
        own.fog_far_color.as_ref().unwrap_or(&own.fog_near_color)
    };
    Some(CellLighting {
        ambient: rgb(ambient.0),
        directional: rgb(directional.0),
        ambient_directions,
        fog_near_color: rgb(fog_near_color.0),
        fog_far_color: rgb(fog_far_color.0),
        fog_near: choose(FOG_NEAR).map_or(own.fog_near, |t| t.fog_near),
        fog_far: choose(FOG_FAR).map_or(own.fog_far, |t| t.fog_far),
        rotation_xy: choose(DIRECTIONAL_ROTATION)
            .map_or(own.directional_rotation_xy, |t| t.directional_rotation_xy),
        rotation_z: choose(DIRECTIONAL_ROTATION)
            .map_or(own.directional_rotation_z, |t| t.directional_rotation_z),
        directional_fade: choose(DIRECTIONAL_FADE)
            .map_or(own.directional_fade, |t| t.directional_fade),
        fog_clip_distance: choose(CLIP_DISTANCE)
            .map_or(own.fog_clip_distance, |t| t.fog_clip_distance),
        fog_power: choose(FOG_POWER).map_or(own.fog_power, |t| t.fog_power),
        fog_max: choose(FOG_MAX)
            .and_then(|t| t.fog_max)
            .or(own.fog_max)
            .unwrap_or(1.0),
        light_fade_begin: choose(LIGHT_FADE)
            .and_then(|t| t.light_fade_begin)
            .or(own.light_fade_begin)
            .unwrap_or(0.0),
        light_fade_end: choose(LIGHT_FADE)
            .and_then(|t| t.light_fade_end)
            .or(own.light_fade_end)
            .unwrap_or(0.0),
    })
}

pub(crate) fn dictionary(order: &LoadOrder, id: GlobalFormId) -> Option<VarDictionary> {
    let value = cell_lighting(order, id)?;
    let mut result = VarDictionary::new();
    let color = |rgb: [u8; 3]| {
        Color::from_rgb(
            f32::from(rgb[0]) / 255.0,
            f32::from(rgb[1]) / 255.0,
            f32::from(rgb[2]) / 255.0,
        )
    };
    result.set("interior", true);
    result.set("ambient", color(value.ambient));
    result.set("directional", color(value.directional));
    let mut ambient_directions = Array::<Color>::new();
    for direction in value.ambient_directions {
        ambient_directions.push(color(direction));
    }
    result.set("ambient_directions", &ambient_directions);
    result.set("fog_near_color", color(value.fog_near_color));
    result.set("fog_far_color", color(value.fog_far_color));
    result.set("fog_near", value.fog_near);
    result.set("fog_far", value.fog_far);
    result.set("rotation_xy", value.rotation_xy);
    result.set("rotation_z", value.rotation_z);
    result.set("directional_fade", value.directional_fade);
    result.set("fog_clip_distance", value.fog_clip_distance);
    result.set("fog_power", value.fog_power);
    result.set("fog_max", value.fog_max);
    result.set("light_fade_begin", value.light_fade_begin);
    result.set("light_fade_end", value.light_fade_end);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, Plugin, Tag};

    fn lighting(color: u8, inheritance_flags: u32) -> Lighting {
        Lighting {
            ambient_color: soe_esm::records::cell::LightingColor([color, color, color, 0]),
            directional_color: soe_esm::records::cell::LightingColor([color, 2, 3, 0]),
            fog_near_color: soe_esm::records::cell::LightingColor([color, 4, 5, 0]),
            fog_near: f32::from(color),
            fog_far: f32::from(color) + 10.0,
            directional_rotation_xy: i32::from(color),
            directional_rotation_z: i32::from(color) + 1,
            directional_fade: f32::from(color) / 10.0,
            fog_clip_distance: f32::from(color) + 20.0,
            fog_power: f32::from(color) / 100.0,
            ambient_colors: [soe_esm::records::cell::LightingColor([color, 0, 0, 0]); 6],
            specular_color: Some(soe_esm::records::cell::LightingColor([color, 0, 0, 0])),
            fresnel_power: Some(0.5),
            fog_far_color: Some(soe_esm::records::cell::LightingColor([color, 6, 7, 0])),
            fog_max: Some(f32::from(color) + 30.0),
            light_fade_begin: Some(f32::from(color) + 40.0),
            light_fade_end: Some(f32::from(color) + 50.0),
            inheritance_flags: Some(inheritance_flags),
        }
    }

    fn template_data(value: &Lighting) -> (Vec<u8>, Vec<u8>) {
        let encoded = value.encode();
        let mut data = encoded[..40].to_vec();
        data.extend_from_slice(&[0; 28]);
        data.extend_from_slice(&encoded[72..88]);
        let mut dalc = Vec::new();
        for color in value.ambient_colors {
            dalc.extend_from_slice(&color.0);
        }
        dalc.extend_from_slice(&value.specular_color.unwrap().0);
        dalc.extend_from_slice(&value.fresnel_power.unwrap().to_le_bytes());
        (data, dalc)
    }

    fn order_with_lighting(flags: u32) -> LoadOrder {
        let own = lighting(10, flags);
        let template = lighting(200, 0);
        let (template_data, dalc) = template_data(&template);
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(Tag::new(b"CELL"), 1)
                .sub(Tag::new(b"DATA"), 1_u16.to_le_bytes())
                .sub(Tag::new(b"XCLL"), own.encode())
                .sub(Tag::new(b"LTMP"), 2_u32.to_le_bytes()),
        );
        plugin.push_top(
            Record::new(Tag::new(b"LGTM"), 2)
                .sub(Tag::new(b"DATA"), template_data)
                .sub(Tag::new(b"DALC"), dalc),
        );
        let mut order = LoadOrder::new();
        order.add("Lighting.esm", plugin).unwrap();
        order
    }

    #[test]
    fn rnd_21_cell_light_applies_lgtm_inheritance_and_keeps_uninherited_fields() {
        let all = order_with_lighting(0x7ff);
        let id = GlobalFormId { file: 0, object: 1 };
        let result = cell_lighting(&all, id).unwrap();
        assert_eq!(result.ambient, [200; 3]);
        assert_eq!(result.directional, [200, 2, 3]);
        assert_eq!(result.ambient_directions, [[200, 0, 0]; 6]);
        assert_eq!(result.fog_near_color, [200, 4, 5]);
        assert_eq!(result.fog_far_color, [200, 6, 7]);
        assert_eq!(result.fog_near, 200.0);
        assert_eq!(result.fog_far, 210.0);
        assert_eq!(result.rotation_xy, 200);
        assert_eq!(result.rotation_z, 201);
        assert_eq!(result.directional_fade, 20.0);
        assert_eq!(result.fog_clip_distance, 220.0);
        assert_eq!(result.fog_power, 2.0);
        assert_eq!(result.fog_max, 230.0);
        assert_eq!(result.light_fade_begin, 240.0);
        assert_eq!(result.light_fade_end, 250.0);

        let none = order_with_lighting(0);
        let result = cell_lighting(&none, id).unwrap();
        assert_eq!(result.ambient, [10; 3]);
        assert_eq!(result.directional, [10, 2, 3]);
        assert_eq!(result.ambient_directions, [[10, 0, 0]; 6]);
        assert_eq!(result.fog_far_color, [10, 6, 7]);
        assert_eq!(result.fog_near, 10.0);
        assert_eq!(result.rotation_xy, 10);
        assert_eq!(result.light_fade_end, 60.0);
    }

    #[test]
    fn rnd_21_exterior_cells_do_not_receive_interior_lighting() {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(Tag::new(b"CELL"), 1)
                .sub(Tag::new(b"DATA"), 0_u16.to_le_bytes())
                .sub(Tag::new(b"XCLL"), lighting(100, 0).encode()),
        );
        let mut order = LoadOrder::new();
        order.add("Exterior.esm", plugin).unwrap();
        assert!(cell_lighting(&order, GlobalFormId { file: 0, object: 1 }).is_none());
    }

    #[test]
    fn rnd_21_lgtm_supplies_lighting_when_xcll_is_absent() {
        let template = lighting(170, 0);
        let (data, dalc) = template_data(&template);
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(Tag::new(b"CELL"), 1)
                .sub(Tag::new(b"DATA"), 1_u16.to_le_bytes())
                .sub(Tag::new(b"LTMP"), 2_u32.to_le_bytes()),
        );
        plugin.push_top(
            Record::new(Tag::new(b"LGTM"), 2)
                .sub(Tag::new(b"DATA"), data)
                .sub(Tag::new(b"DALC"), dalc),
        );
        let mut order = LoadOrder::new();
        order.add("TemplateOnly.esm", plugin).unwrap();
        let result = cell_lighting(&order, GlobalFormId { file: 0, object: 1 }).unwrap();
        assert_eq!(result.ambient, [170; 3]);
        assert_eq!(result.directional, [170, 2, 3]);
        assert_eq!(result.ambient_directions, [[170, 0, 0]; 6]);
        assert_eq!(result.fog_near, 170.0);
        assert_eq!(result.rotation_z, 171);
        assert_eq!(result.light_fade_end, 220.0);
    }
}
