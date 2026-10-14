// SPDX-License-Identifier: GPL-3.0-or-later
// Sources: Community Shaders Water.hlsl UV mapping and OpenMW water.frag optics.
shader_type spatial;
render_mode blend_mix, depth_draw_never, cull_disabled;

uniform sampler2D depth_texture : hint_depth_texture, filter_nearest, repeat_disable;
uniform sampler2D noise_0 : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D noise_1 : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D noise_2 : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D flow_normals : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform bvec3 has_noise = bvec3(false);
uniform bool has_flow = false;
uniform bool blend_normals = true;
uniform bool mesh_uv = false;
uniform bool use_depth = true;
uniform bool use_reflection = true;
uniform bool vertex_alpha_depth = false;
uniform vec2 uv_offset = vec2(0.0);
uniform vec2 uv_scale = vec2(1.0);
uniform vec3 noise_scale = vec3(100.0);
uniform vec3 amplitude = vec3(1.0);
uniform vec2 velocity_0 = vec2(0.0);
uniform vec2 velocity_1 = vec2(0.0);
uniform vec2 velocity_2 = vec2(0.0);
uniform float flow_scale = 1.0;
uniform vec3 shallow_color : source_color = vec3(0.0);
uniform vec3 deep_color : source_color = vec3(0.0);
uniform vec3 reflection_color : source_color = vec3(0.0);
uniform float base_alpha = 1.0;
uniform float fog_near = 0.0;
uniform float fog_far = 14.2875;
uniform float fog_amount = 1.0;
uniform float fresnel_amount = 0.02;
uniform float reflectivity = 0.5;
uniform float reflection_magnitude = 1.0;
uniform float sun_power = 50.0;
uniform float sun_magnitude = 1.0;
uniform float sparkle_power = 1.0;
uniform float sparkle_magnitude = 1.0;
uniform float animation_time = -1.0;
varying vec3 world_position;
varying float water_fresnel;

vec3 normal_sample(sampler2D tex, vec2 coords, float strength) {
    vec2 xy = (texture(tex, coords).rg * 2.0 - 1.0) * strength;
    return vec3(xy, sqrt(max(1.0 - dot(xy, xy), 0.001)));
}

void vertex() {
    world_position = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
    float elapsed = animation_time >= 0.0 ? animation_time : TIME;
    // Mesh UV units follow Community Shaders' 0.001 scale. Planes use game units.
    vec2 coords = mesh_uv ? (UV * uv_scale + uv_offset) * 1000.0 : world_position.xz / 0.0142875 * vec2(1.0, -1.0);
    vec3 n = vec3(0.0, 0.0, 1.0);
    if (!has_flow || blend_normals) {
        if (has_noise.x) { n.xy += normal_sample(noise_0, coords / noise_scale.x - velocity_0 * elapsed, amplitude.x).xy; }
        if (has_noise.y) { n.xy += normal_sample(noise_1, coords / noise_scale.y - velocity_1 * elapsed, amplitude.y).xy; }
        if (has_noise.z) { n.xy += normal_sample(noise_2, coords / noise_scale.z - velocity_2 * elapsed, amplitude.z).xy; }
    }
    if (has_flow) { n.xy += normal_sample(flow_normals, coords / noise_scale.x / flow_scale - velocity_0 * elapsed, amplitude.x).xy; }
    n = normalize(n);
    vec3 surface_normal;
    if (mesh_uv) {
        // Authored tangents are not present on every water mesh. Derivatives recover the UV basis.
        vec3 dx = dFdx(VERTEX);
        vec3 dy = dFdy(VERTEX);
        vec2 tx = dFdx(UV);
        vec2 ty = dFdy(UV);
        float determinant = tx.x * ty.y - tx.y * ty.x;
        if (abs(determinant) > 0.0) {
            float orientation = sign(determinant);
            vec3 tangent = (dx * ty.y - dy * tx.y) * orientation;
            vec3 bitangent = (dy * tx.x - dx * ty.x) * orientation;
            tangent = normalize(tangent - NORMAL * dot(tangent, NORMAL));
            bitangent = normalize(bitangent - NORMAL * dot(bitangent, NORMAL));
            surface_normal = normalize(tangent * n.x + bitangent * n.y + NORMAL * n.z);
        } else { surface_normal = NORMAL; }
    } else {
        surface_normal = normalize((VIEW_MATRIX * vec4(n.x, n.z, -n.y, 0.0)).xyz);
    }
    NORMAL = FRONT_FACING ? surface_normal : -surface_normal;
    float depth_factor = 0.0;
    if (use_depth) {
        float raw_depth = texture(depth_texture, SCREEN_UV).r;
        vec4 scene = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, raw_depth, 1.0);
        float thickness = max(-scene.z / max(scene.w, 0.000001) + VERTEX.z, 0.0);
        // Reverse-Z zero is the sky, which has no bottom and takes the deep color.
        depth_factor = raw_depth <= 0.000001 ? 1.0 : clamp((thickness - fog_near) / max(fog_far - fog_near, 0.0001), 0.0, 1.0);
    }
    if (vertex_alpha_depth) { depth_factor *= COLOR.a; }
    depth_factor *= fog_amount;
    float cosine = clamp(abs(dot(NORMAL, VIEW)), 0.0, 1.0);
    water_fresnel = use_reflection ? clamp(fresnel_amount + (1.0 - fresnel_amount) * pow(1.0 - cosine, 5.0), 0.0, 1.0) : 0.0;
    vec3 water_color = mix(shallow_color, deep_color, depth_factor);
    ALBEDO = mix(water_color, reflection_color * reflection_magnitude, clamp(water_fresnel * reflectivity, 0.0, 1.0));
    ALPHA = clamp(mix(base_alpha, 1.0, depth_factor) + water_fresnel * (1.0 - base_alpha), 0.0, 1.0);
    ROUGHNESS = 0.05;
    SPECULAR = 0.0;
}

void light() {
    DIFFUSE_LIGHT += ALBEDO * LIGHT_COLOR * ATTENUATION * max(dot(NORMAL, LIGHT), 0.0) / PI;
    if (LIGHT_IS_DIRECTIONAL) {
        vec3 half_direction = normalize(LIGHT + VIEW);
        float highlight = pow(max(dot(NORMAL, half_direction), 0.0), sun_power) * sun_magnitude;
        float sparkle = pow(max(dot(reflect(-LIGHT, NORMAL), VIEW), 0.0), sparkle_power) * sparkle_magnitude;
        SPECULAR_LIGHT += LIGHT_COLOR * ATTENUATION * (highlight + sparkle) * water_fresnel;
    }
}
