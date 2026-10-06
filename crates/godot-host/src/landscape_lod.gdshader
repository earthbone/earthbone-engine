// SPDX-License-Identifier: GPL-3.0-or-later
// CONTEXT: Community Shaders Lighting.hlsl MODELSPACENORMALS/LODLANDSCAPE
// decodes RGB as X, Z, Y. Godot fragment NORMAL is in view space.
// LODLANDNOISE uses GetLodLandBlendParameter/Multiplier and a tiled mask.
shader_type spatial;
render_mode diffuse_burley, specular_disabled;

uniform sampler2D diffuse_texture : source_color, filter_linear_mipmap_anisotropic, repeat_disable;
uniform sampler2D normal_texture : filter_linear_mipmap_anisotropic, repeat_disable;
uniform bool has_normal = false;
uniform sampler2D noise_texture : filter_linear_mipmap_anisotropic, repeat_enable;
uniform bool has_noise = false;

void fragment() {
    ALBEDO = texture(diffuse_texture, UV).rgb;
    if (has_noise) {
        float parameter = clamp(1.6666666 * (dot(ALBEDO, vec3(0.55)) - 0.4), 0.0, 1.0);
        parameter = parameter * parameter * (3.0 - 2.0 * parameter) * 0.8;
        float mask = texture(noise_texture, UV * 3.0).r;
        ALBEDO *= 0.8333333 * (parameter * (0.37 - mask) + mask) + 0.37;
    }
    ROUGHNESS = 1.0;
    if (has_normal) {
        vec3 encoded = texture(normal_texture, UV).rgb * 2.0 - 1.0;
        vec3 godot_normal = vec3(encoded.r, encoded.g, -encoded.b);
        NORMAL = normalize(mat3(VIEW_MATRIX) * MODEL_NORMAL_MATRIX * godot_normal);
    }
}
