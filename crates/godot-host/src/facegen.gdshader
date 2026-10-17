// SPDX-License-Identifier: GPL-3.0-or-later
// CONTEXT: FaceGen blend and X,Z,Y normal packing follow the local
// skyrim-community-shaders/package/Shaders/Lighting.hlsl.
shader_type spatial;
render_mode CULL_MODE;

uniform sampler2D diffuse_map : hint_default_white;
uniform sampler2D tint_map : hint_default_white;
uniform sampler2D detail_map;
uniform sampler2D normal_map;
uniform sampler2D specular_map : hint_default_black;
uniform bool has_specular_map = false;
uniform vec3 specular_color = vec3(0.0);
uniform float specular_strength = 0.0;
uniform float glossiness = 1.0;
varying vec2 face_uv;
uniform bool has_detail = false;
uniform bool has_normal = false;
uniform bool model_space_normals = false;
uniform mat3 skin_normals[BONE_COUNT];
varying mat3 face_normal_to_view;

void vertex() {
    face_uv = UV;
    mat3 skin = skin_normals[BONE_INDICES.x] * BONE_WEIGHTS.x
        + skin_normals[BONE_INDICES.y] * BONE_WEIGHTS.y
        + skin_normals[BONE_INDICES.z] * BONE_WEIGHTS.z
        + skin_normals[BONE_INDICES.w] * BONE_WEIGHTS.w;
    face_normal_to_view = MODELVIEW_NORMAL_MATRIX * skin;
}

void fragment() {
    vec3 base = texture(diffuse_map, UV).rgb;
    vec3 tint = texture(tint_map, UV).rgb;
    vec3 detail = has_detail
        ? 3.984375 * (texture(detail_map, UV).rgb + vec3(1.0 / 255.0, 0.0, 1.0 / 255.0))
        : vec3(1.0);
    vec3 color = (base * base + 2.0 * tint * base * (1.0 - base)) * detail;
    ALBEDO = pow(max(color, vec3(0.0)), vec3(2.2));
    if (has_normal) {
        vec3 packed = texture(normal_map, UV).rgb;
        if (model_space_normals) {
            vec3 model_normal = vec3(packed.r * 2.0 - 1.0, packed.g * 2.0 - 1.0, 1.0 - packed.b * 2.0);
            NORMAL = normalize(face_normal_to_view * model_normal);
        } else {
            NORMAL_MAP = packed;
        }
    }
    // CONTEXT: The NIF mask controls highlights. Unmasked IBL turns dim skin grey.
#if FACE_SPECULAR_ENABLED
    SPECULAR = 0.0;
#endif
    ROUGHNESS = 0.8;
}

#if FACE_SPECULAR_ENABLED
void light() {
    DIFFUSE_LIGHT += LIGHT_COLOR * ATTENUATION * max(dot(NORMAL, LIGHT), 0.0) / PI;
    float mask = model_space_normals
        ? (has_specular_map ? texture(specular_map, face_uv).r : 0.0)
        : (has_normal ? texture(normal_map, face_uv).a : 0.0);
    vec3 half_direction = normalize(LIGHT + VIEW);
    float highlight = pow(max(dot(NORMAL, half_direction), 0.0), max(glossiness, 0.001));
    SPECULAR_LIGHT += LIGHT_COLOR * ATTENUATION * specular_color * specular_strength * mask * highlight;
}

#endif
