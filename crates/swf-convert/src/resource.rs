// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot 4.4 text resources, matching ArrayMesh::_set_surfaces and FontFile::_set.
//! Canvas shaders retain SWF paint transforms instead of baking one fill color.

use std::fmt::Write;
use std::path::Path;

use serde_json::Value;

use crate::bitmap::Bitmap;
use crate::{Error, Font, Mesh, Movie, Paint};

fn quoted(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

fn bytes(data: &[u8]) -> String {
    let mut result = String::from("PackedByteArray(");
    for (index, byte) in data.iter().enumerate() {
        if index != 0 {
            result.push(',');
        }
        let _ = write!(result, "{byte}");
    }
    result.push(')');
    result
}

fn image_body(bitmap: &Bitmap, id: &str) -> String {
    format!("[sub_resource type=\"Image\" id={} ]\ndata = {{\"data\": {}, \"format\": \"RGBA8\", \"height\": {}, \"width\": {}, \"mipmaps\": false}}\n\n",
        quoted(id), bytes(&bitmap.rgba), bitmap.height, bitmap.width)
}

fn bitmap(bitmap: &Bitmap, path: &Path) -> Result<(), Error> {
    let result = format!("[gd_resource type=\"ImageTexture\" load_steps=2 format=3]\n\n{}[resource]\nimage = SubResource(\"pixels\")\nmetadata/swf_id = {}\n",
        image_body(bitmap, "pixels"), bitmap.id);
    std::fs::write(path, result)?;
    Ok(())
}

fn font(font: &Font, path: &Path) -> Result<(), Error> {
    let mut result = format!(
        "[gd_resource type=\"FontFile\" load_steps={} format=3]\n\n",
        font.atlases.len() + 1
    );
    for (index, atlas) in font.atlases.iter().enumerate() {
        result.push_str(&image_body(atlas, &format!("atlas_{index}")));
    }
    let _ = write!(result, "[resource]\nfont_name = {}\nfont_style = {}\nfixed_size = {}\nfixed_size_scale_mode = 1\nfont_weight = {}\nfont_italic = {}\n",
        quoted(&font.name), quoted(if font.bold { "Bold" } else { "Regular" }), font.size, if font.bold { 700 } else { 400 }, font.italic);
    let prefix = format!("cache/0/{}/0", font.size);
    let _ = writeln!(
        result,
        "{prefix}/ascent = {}\n{prefix}/descent = {}\n{prefix}/scale = 1.0",
        font.metrics[0], font.metrics[1]
    );
    for index in 0..font.atlases.len() {
        let _ = writeln!(
            result,
            "{prefix}/textures/{index}/image = SubResource(\"atlas_{index}\")"
        );
    }
    for glyph in &font.glyphs {
        let [x, y, w, h] = glyph.atlas_rect;
        let code = glyph.code;
        let _ = writeln!(result, "{prefix}/glyphs/{code}/advance = Vector2({}, 0)\n{prefix}/glyphs/{code}/offset = Vector2({}, {})\n{prefix}/glyphs/{code}/size = Vector2({w}, {h})\n{prefix}/glyphs/{code}/uv_rect = Rect2({x}, {y}, {w}, {h})\n{prefix}/glyphs/{code}/texture_idx = {}", glyph.advance, glyph.offset[0], glyph.offset[1], glyph.texture_index);
    }
    for (left, right, adjustment) in &font.kerning {
        let _ = writeln!(
            result,
            "{prefix}/kerning_overrides/{left}/{right} = Vector2({adjustment}, 0)"
        );
    }
    let _ = writeln!(result, "metadata/swf_id = {}\nmetadata/glyph_count = {}\nmetadata/swf_version = {}\nmetadata/leading = {}", font.id, font.glyphs.len(), font.version, font.metrics[2]);
    std::fs::write(path, result)?;
    Ok(())
}

fn color(value: [u8; 4]) -> String {
    format!(
        "vec4({:.8},{:.8},{:.8},{:.8})",
        value[0] as f64 / 255.0,
        value[1] as f64 / 255.0,
        value[2] as f64 / 255.0,
        value[3] as f64 / 255.0
    )
}

fn local(matrix: [f64; 6]) -> Result<String, Error> {
    let [a, b, c, d, tx, ty] = matrix;
    let determinant = a * d - b * c;
    if determinant.abs() < 1e-12 {
        return Err(Error::Invalid("paint matrix is singular".into()));
    }
    Ok(format!("vec2 q=local_position*20.0-vec2({tx:.12},{ty:.12}); vec2 p=vec2({:.12}*q.x+{:.12}*q.y,{:.12}*q.x+{:.12}*q.y);", d/determinant, -c/determinant, -b/determinant, a/determinant))
}

fn shader(paint: &Paint) -> Result<String, Error> {
    let mut result = String::from("shader_type canvas_item; varying vec2 local_position; void vertex(){local_position=VERTEX;} ");
    match paint {
        Paint::Solid(rgba) => {
            let _ = write!(result, "void fragment(){{COLOR *= {};}}", color(*rgba));
        }
        Paint::Bitmap {
            matrix,
            smoothed,
            repeating,
            ..
        } => {
            let _ = write!(result, "uniform sampler2D bitmap: {}, {}; void fragment(){{{} vec2 uv=p/vec2(textureSize(bitmap,0)); ",
                if *smoothed { "filter_linear" } else { "filter_nearest" }, if *repeating { "repeat_enable" } else { "repeat_disable" }, local(*matrix)?);
            if !repeating {
                result.push_str("if(any(lessThan(uv,vec2(0.0)))||any(greaterThan(uv,vec2(1.0)))){COLOR=vec4(0.0);}else{");
            }
            result.push_str("COLOR *= texture(bitmap,uv);");
            if !repeating {
                result.push('}');
            }
            result.push('}');
        }
        Paint::Gradient {
            matrix,
            kind,
            focal,
            spread,
            linear_rgb,
            stops,
        } => {
            let _ = write!(result, "void fragment(){{{} ", local(*matrix)?);
            match kind {
                0 => result.push_str("float t=p.x/32768.0+0.5;"),
                1 => result.push_str("float t=length(p)/16384.0;"),
                _ => {
                    let f = focal.clamp(-0.999, 0.999);
                    let _ = write!(result, "vec2 v=p/16384.0-vec2({f:.12},0.0); float t=(v.x*{f:.12}+sqrt(max(0.0,v.x*v.x+v.y*v.y*(1.0-{f:.12}*{f:.12}))))/(1.0-{f:.12}*{f:.12});");
                }
            }
            result.push_str(match spread {
                1 => "t=1.0-abs(mod(t,2.0)-1.0);",
                2 => "t=fract(t);",
                _ => "t=clamp(t,0.0,1.0);",
            });
            let transform = |c: [u8; 4]| {
                if *linear_rgb {
                    format!("vec4(pow({}.rgb,vec3(2.2)),{}.a)", color(c), color(c))
                } else {
                    color(c)
                }
            };
            let initial = stops.first().map_or([0, 0, 0, 0], |s| s.1);
            let _ = write!(result, "vec4 c={};", transform(initial));
            for pair in stops.windows(2) {
                let (left, lc) = pair[0];
                let (right, rc) = pair[1];
                if right > left {
                    let _ = write!(
                        result,
                        "c=mix(c,{},clamp((t-{:.12})/{:.12},0.0,1.0));",
                        transform(rc),
                        left as f64 / 255.0,
                        (right - left) as f64 / 255.0
                    );
                } else {
                    let _ = write!(
                        result,
                        "if(t>={:.12}){{c={};}}",
                        right as f64 / 255.0,
                        transform(rc)
                    );
                }
                let _ = lc;
            }
            if *linear_rgb {
                result.push_str("c.rgb=pow(c.rgb,vec3(1.0/2.2));");
            }
            result.push_str("COLOR *= c;}");
        }
    }
    Ok(result)
}

fn mesh(mesh: &Mesh, path: &Path) -> Result<(), Error> {
    let surfaces: Vec<_> = mesh
        .surfaces
        .iter()
        .filter(|s| !s.indices.is_empty())
        .collect();
    let mut result = format!(
        "[gd_resource type=\"ArrayMesh\" load_steps={} format=3]\n\n",
        1 + surfaces.len() * 2
            + surfaces
                .iter()
                .filter(|s| matches!(s.paint, Paint::Bitmap { .. }))
                .count()
    );
    for (index, surface) in surfaces.iter().enumerate() {
        if let Paint::Bitmap { id, .. } = &surface.paint {
            let _ = writeln!(result, "[ext_resource type=\"Texture2D\" path=\"bitmap-{id}.tres\" id=\"texture_{index}\"]");
        }
    }
    result.push('\n');
    for (index, surface) in surfaces.iter().enumerate() {
        let _ = writeln!(result, "[sub_resource type=\"Shader\" id=\"shader_{index}\"]\ncode = {}\n\n[sub_resource type=\"ShaderMaterial\" id=\"paint_{index}\"]\nshader = SubResource(\"shader_{index}\")", quoted(&shader(&surface.paint)?));
        if matches!(surface.paint, Paint::Bitmap { .. }) {
            let _ = writeln!(
                result,
                "shader_parameter/bitmap = ExtResource(\"texture_{index}\")"
            );
        }
        result.push('\n');
    }
    let mut native = Vec::new();
    for (index, surface) in surfaces.iter().enumerate() {
        let mut vertex_data = Vec::new();
        let mut min = [f32::MAX; 2];
        let mut max = [f32::MIN; 2];
        for vertex_index in &surface.indices {
            let vertex = surface
                .vertices
                .get(*vertex_index as usize)
                .ok_or_else(|| Error::Invalid("mesh vertex index exceeds its table".into()))?;
            for component in 0..2 {
                min[component] = min[component].min(vertex[component]);
                max[component] = max[component].max(vertex[component]);
            }
            vertex_data.extend_from_slice(&vertex[0].to_le_bytes());
            vertex_data.extend_from_slice(&vertex[1].to_le_bytes());
            vertex_data.extend_from_slice(&0_f32.to_le_bytes());
        }
        // CONTEXT: Godot 4.4's current surface format is version 2 at bit 35, with VERTEX at bit 0.
        native.push(format!("{{\"format\": 34359738369, \"primitive\": 3, \"vertex_data\": {}, \"vertex_count\": {}, \"aabb\": AABB({}, {}, 0, {}, {}, 0.00001), \"material\": SubResource(\"paint_{index}\"), \"name\": \"surface_{index}\"}}",
            bytes(&vertex_data), surface.indices.len(), min[0], min[1], (max[0]-min[0]).max(0.00001), (max[1]-min[1]).max(0.00001)));
    }
    let _ = writeln!(result, "[resource]\n_surfaces = [{}]\nmetadata/swf_id = {}\nmetadata/swf_bounds = Rect2({}, {}, {}, {})", native.join(",\n"), mesh.id, mesh.bounds[0],mesh.bounds[1],mesh.bounds[2],mesh.bounds[3]);
    std::fs::write(path, result)?;
    Ok(())
}

fn variant(value: &Value) -> String {
    match value {
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(variant).collect::<Vec<_>>().join(",")
        ),
        Value::Object(items) => format!(
            "{{{}}}",
            items
                .iter()
                .map(|(key, value)| format!("{}:{}", quoted(key), variant(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => value.to_string(),
    }
}

/// Write only mesh and bitmap resources from the supplied art dictionary.
///
/// # Errors
/// Returns resource encoding or filesystem errors. No scripts, fonts, text,
/// linkage names, buttons or timelines are written.
pub fn write_art(movie: &Movie, directory: &Path) -> Result<(), Error> {
    std::fs::create_dir_all(directory)?;
    for (id, bitmap_data) in &movie.bitmaps {
        bitmap(bitmap_data, &directory.join(format!("bitmap-{id}.tres")))?;
    }
    for (id, mesh_data) in &movie.meshes {
        mesh(mesh_data, &directory.join(format!("shape-{id}.tres")))?;
    }
    Ok(())
}

pub(crate) fn write(movie: &Movie, directory: &Path) -> Result<(), Error> {
    write_art(movie, directory)?;
    for (id, font_data) in &movie.fonts {
        font(font_data, &directory.join(format!("font-{id}.tres")))?;
        for (index, glyph) in font_data.glyphs.iter().enumerate() {
            mesh(
                &Mesh {
                    id: *id,
                    bounds: [0.0; 4],
                    surfaces: glyph.surfaces.clone(),
                },
                &directory.join(format!("font-{id}-glyph-{index}.tres")),
            )?;
        }
    }
    let fonts: Value = movie.fonts.iter().map(|(id,font)| (id.to_string(), serde_json::json!({"name":font.name,"glyph_count":font.glyphs.len(),"version":font.version,"codes":font.glyphs.iter().map(|g|g.code).collect::<Vec<_>>(),"metrics":font.metrics}))).collect();
    let metadata = serde_json::json!({"version":movie.version,"stage":movie.stage,"frame_rate":movie.frame_rate,"symbols":movie.symbols,"imports":movie.imports,"fonts":fonts,
        "text_fields":movie.text_fields,"static_text":movie.static_text,"timelines":movie.timelines,"buttons":movie.buttons,"action_blocks":movie.action_blocks,"unsupported_tags":movie.unsupported_tags});
    std::fs::write(directory.join("movie.json"), serde_json::to_vec(&metadata)?)?;
    let assets: Vec<_> = movie
        .meshes
        .keys()
        .map(|id| (format!("shape_{id}"), format!("shape-{id}.tres")))
        .chain(
            movie
                .fonts
                .keys()
                .map(|id| (format!("font_{id}"), format!("font-{id}.tres"))),
        )
        .chain(
            movie
                .bitmaps
                .keys()
                .map(|id| (format!("bitmap_{id}"), format!("bitmap-{id}.tres"))),
        )
        .collect();
    let mut result = format!(
        "[gd_resource type=\"Resource\" load_steps={} format=3]\n\n",
        assets.len() + 1
    );
    for (index, (_, file)) in assets.iter().enumerate() {
        let _ = writeln!(
            result,
            "[ext_resource type=\"Resource\" path={} id=\"asset_{index}\"]",
            quoted(file)
        );
    }
    let _ = writeln!(
        result,
        "\n[resource]\nmetadata/movie = {}",
        variant(&metadata)
    );
    for (index, (name, _)) in assets.iter().enumerate() {
        let _ = writeln!(result, "metadata/{name} = ExtResource(\"asset_{index}\")");
    }
    std::fs::write(directory.join("movie.tres"), result)?;
    Ok(())
}
