// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot sky inputs from shared World forms, using the Godot sky shader API.
//! The preview owns a World only when no gameplay PlayerNode is bound.

use std::collections::HashMap;
use std::sync::Arc;

use godot::classes::{IRefCounted, ImageTexture, RefCounted};
use godot::prelude::*;
use soe_game::World;

use crate::data_source::DataSource;
use crate::godot_layer::CellStreamerNode;
use crate::godot_player::PlayerNode;

#[derive(GodotClass)]
#[class(base = RefCounted)]
struct SkyRecords {
    base: Base<RefCounted>,
    source: Option<Arc<DataSource>>,
    preview: Option<World>,
    player: Option<Gd<PlayerNode>>,
    textures: HashMap<String, Gd<ImageTexture>>,
}

#[godot_api]
impl IRefCounted for SkyRecords {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            base,
            source: None,
            preview: None,
            player: None,
            textures: HashMap::new(),
        }
    }
}

#[godot_api]
#[allow(clippy::needless_pass_by_value)]
impl SkyRecords {
    #[func]
    fn use_streamer(&mut self, streamer: Gd<CellStreamerNode>) -> bool {
        let Some(source) = streamer.bind().shared_source() else {
            return false;
        };
        self.preview = Some(World::from_shared_order(source.shared_order()));
        self.source = Some(source);
        self.player = None;
        self.textures.clear();
        true
    }

    #[func]
    fn use_player(&mut self, player: Gd<PlayerNode>) -> bool {
        let Some(source) = player.bind().sky_source() else {
            return false;
        };
        self.source = Some(source);
        self.player = Some(player);
        self.preview = None;
        self.textures.clear();
        true
    }

    #[func]
    fn set_time(&mut self, hour: f64, days: f64) -> bool {
        if !hour.is_finite() || !days.is_finite() || days < 0.0 {
            return false;
        }
        let Some(world) = &mut self.preview else {
            return false;
        };
        world.clock.game_days = days.floor() + hour.rem_euclid(24.0) / 24.0;
        if let Some(id) = world.form("GameHour") {
            world.set_global(id, hour as f32);
        }
        if let Some(id) = world.form("GameDaysPassed") {
            world.set_global(id, world.clock.game_days as f32);
        }
        true
    }

    #[func]
    fn set_weather(&mut self, editor_id: GString, accelerate: bool) -> bool {
        let Some(world) = &mut self.preview else {
            return false;
        };
        let Some(id) = world.form(&editor_id.to_string()) else {
            return false;
        };
        world.set_active_weather(id, true, accelerate).is_ok()
    }

    #[func]
    fn advance(&mut self, seconds: f64) {
        if seconds.is_finite() && seconds >= 0.0 {
            if let Some(world) = &mut self.preview {
                world.advance_clock(seconds);
            }
        }
    }

    #[func]
    fn frame(&mut self, cell_name: GString) -> VarDictionary {
        let result = if let Some(player) = &mut self.player {
            player.bind_mut().sky_frame_for_host()
        } else if let Some(world) = &mut self.preview {
            let cell = world.cell_by_editor_id(&cell_name.to_string());
            cell.map_or_else(
                || Err(soe_papyrus::VmError::State("missing sky cell".into())),
                |cell| world.sky_frame(cell),
            )
        } else {
            return VarDictionary::new();
        };
        let mut out = VarDictionary::new();
        match result {
            Ok(Some(frame)) => {
                out.set("weather_sky", true);
                out.set("sky_visible", frame.visible);
                out.set("weather", frame.weather.to_string());
                for (key, index) in [
                    ("upper", 0),
                    ("fog_color", 1),
                    ("fog_far_color", 12),
                    ("ambient", 3),
                    ("sunlight", 4),
                    ("sun_color", 5),
                    ("stars_color", 6),
                    ("lower", 7),
                    ("horizon", 8),
                ] {
                    let c = frame.colors[index];
                    out.set(key, Color::from_rgb(c[0], c[1], c[2]));
                }
                out.set(
                    "fog",
                    Vector4::new(frame.fog[0], frame.fog[1], frame.fog[2], frame.fog[3]),
                );
                out.set("sun_direction", direction(frame.sun_direction));
                out.set("sun_texture", frame.sun_texture);
                out.set("glare_texture", frame.glare_texture);
                out.set("sun_glare", frame.sun_glare);
                out.set("night_alpha", frame.night_alpha);
                out.set("night_model", frame.night_model);
                out.set("masser", frame.moons[0]);
                out.set("secunda", frame.moons[1]);
                out.set("phase", i32::from(frame.moon_phase));
                out.set("phase_days", i32::from(frame.phase_days));
                out.set("masser_direction", direction(frame.moon_directions[0]));
                out.set("secunda_direction", direction(frame.moon_directions[1]));
                out.set("transition", frame.transition);
                let mut clouds = Array::<VarDictionary>::new();
                for cloud in frame.clouds {
                    let mut c = VarDictionary::new();
                    c.set("texture", cloud.texture);
                    c.set(
                        "color",
                        Color::from_rgb(cloud.color[0], cloud.color[1], cloud.color[2]),
                    );
                    c.set("alpha", cloud.alpha);
                    c.set("speed", Vector2::new(cloud.speed[0], cloud.speed[1]));
                    clouds.push(&c);
                }
                out.set("clouds", &clouds);
                if frame.interior.is_some() {
                    if let Some(light) = self.interior_lighting(&cell_name.to_string()) {
                        interior(&mut out, &light);
                    } else {
                        out.set("error", "missing or malformed interior lighting");
                    }
                }
            }
            Ok(None) => {
                out.set("weather_sky", false);
                let lighting = self.interior_lighting(&cell_name.to_string());
                if let Some(light) = lighting {
                    interior(&mut out, &light);
                }
            }
            Err(error) => {
                out.set("error", error.to_string());
            }
        }
        out
    }

    #[func]
    fn texture(&mut self, path: GString) -> Variant {
        let path = path.to_string();
        if let Some(texture) = self.textures.get(&path) {
            return texture.to_variant();
        }
        let Some(source) = &self.source else {
            return Variant::nil();
        };
        let normalized = path.replace('\\', "/");
        let normalized = if normalized.to_ascii_lowercase().starts_with("textures/") {
            normalized
        } else {
            format!("textures/{normalized}")
        };
        let Some(data) = source.effect_shader_texture(&normalized) else {
            return Variant::nil();
        };
        let Some(texture) = crate::godot_layer::image_texture_from(&data) else {
            return Variant::nil();
        };
        let variant = texture.to_variant();
        self.textures.insert(path, texture);
        variant
    }

    #[func]
    fn night_texture(&mut self, model: GString) -> GString {
        let Some(source) = &self.source else {
            return GString::new();
        };
        let model = model.to_string().replace('\\', "/");
        if model.is_empty() {
            return GString::new();
        }
        let model = if model.to_ascii_lowercase().starts_with("meshes/") {
            model
        } else {
            format!("meshes/{model}")
        };
        let Ok(bytes) = source.vfs().read(&model) else {
            return GString::new();
        };
        let Ok(file) = soe_nif::parse(&bytes) else {
            return GString::new();
        };
        for block in &file.blocks {
            if let soe_nif::blocks::Block::TextureSet(set) = block {
                if let Some(path) = set.textures.first() {
                    return GString::from(path);
                }
            }
            if let soe_nif::blocks::Block::EffectShader(shader) = block {
                return GString::from(&shader.source_texture);
            }
            if let soe_nif::blocks::Block::Unknown { type_name, data } = block {
                if type_name == "BSSkyShaderProperty"
                    && file.header.user_version == 12
                    && file.header.bs_version == 100
                {
                    if let Some(path) = sky_shader_texture(data) {
                        return GString::from(path);
                    }
                }
            }
        }
        GString::new()
    }
}

impl SkyRecords {
    fn interior_lighting(&self, cell_name: &str) -> Option<crate::cell_lighting::CellLighting> {
        if let Some(player) = &self.player {
            player.bind().sky_interior_for_host()
        } else {
            let source = self.source.as_ref()?;
            crate::cell_lighting::cell_lighting(
                &source.shared_order(),
                source.resolve_cell(cell_name)?,
            )
        }
    }
}

fn interior(out: &mut VarDictionary, light: &crate::cell_lighting::CellLighting) {
    let color = |rgb: [u8; 3]| {
        Color::from_rgb(
            f32::from(rgb[0]) / 255.0,
            f32::from(rgb[1]) / 255.0,
            f32::from(rgb[2]) / 255.0,
        )
    };
    for (key, rgb) in [
        ("ambient", light.ambient),
        ("sunlight", light.directional),
        ("fog_color", light.fog_near_color),
        ("fog_far_color", light.fog_far_color),
    ] {
        out.set(key, color(rgb));
    }
    out.set(
        "fog",
        Vector4::new(
            light.fog_near,
            light.fog_far,
            light.fog_power,
            light.fog_max,
        ),
    );
    out.set(
        "rotation",
        Vector2::new(light.rotation_xy as f32, light.rotation_z as f32),
    );
    out.set("fade", light.directional_fade);
    let mut ambient_directions = Array::<Color>::new();
    for rgb in light.ambient_directions {
        ambient_directions.push(color(rgb));
    }
    out.set("ambient_directions", &ambient_directions);
    out.set("fog_clip_distance", light.fog_clip_distance);
    out.set(
        "light_fade",
        Vector2::new(light.light_fade_begin, light.light_fade_end),
    );
}

fn direction(v: [f32; 3]) -> Vector3 {
    Vector3::new(v[0], v[2], -v[1])
}

/// SSE BSSkyShaderProperty baseTexture, from nifly Shaders.cpp and Objects.cpp.
fn sky_shader_texture(data: &[u8]) -> Option<&str> {
    let count = u32::from_le_bytes(data.get(4..8)?.try_into().ok()?) as usize;
    let offset = count.checked_mul(4)?.checked_add(36)?;
    let len =
        u32::from_le_bytes(data.get(offset..offset.checked_add(4)?)?.try_into().ok()?) as usize;
    let begin = offset.checked_add(4)?;
    std::str::from_utf8(data.get(begin..begin.checked_add(len)?)?)
        .ok()
        .map(|v| v.trim_end_matches('\0'))
}

#[cfg(test)]
mod tests {
    use super::sky_shader_texture;

    #[test]
    fn rnd_18_night_sky_texture_reads_sse_shader_with_extra_data() {
        let mut bytes = vec![0; 44];
        bytes[4..8].copy_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&14_u32.to_le_bytes());
        bytes.extend_from_slice(b"sky/stars.dds\0");
        assert_eq!(sky_shader_texture(&bytes), Some("sky/stars.dds"));
        bytes.truncate(46);
        assert_eq!(sky_shader_texture(&bytes), None);
    }
}
