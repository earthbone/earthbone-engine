shader_type spatial;
render_mode $MODES;
uniform sampler2D source_texture : $SOURCE_HINThint_default_white, filter_linear_mipmap, repeat_enable;
uniform sampler2D palette_texture : source_color, hint_default_white, filter_linear, repeat_disable;
$DEPTH_UNIFORM
$SCREEN_UNIFORM
uniform vec4 base_color = vec4(1.0);
uniform float base_scale = 1.0;
uniform vec4 falloff = vec4(1.0);
uniform float soft_depth = 0.0;
uniform float lighting_influence = 0.0;
uniform float alpha_threshold = 0.0;
uniform vec2 uv_offset = vec2(0.0);
uniform vec2 uv_scale = vec2(1.0);
uniform float uv_rotation = 0.0;
uniform bvec2 wrap_uv = bvec2(true);
varying float angle_opacity;
void vertex() {
    angle_opacity = 1.0;
    if ($FALLOFF) {
        vec3 position = (MODELVIEW_MATRIX * vec4(VERTEX, 1.0)).xyz;
        vec3 normal = normalize(MODELVIEW_NORMAL_MATRIX * NORMAL);
        float cosine = abs(dot(normal, normalize(position)));
        float interval = falloff.y - falloff.x;
        float t = abs(interval) < 0.000001 ? step(falloff.x, cosine) : clamp((cosine - falloff.x) / interval, 0.0, 1.0);
        t = t * t * (3.0 - 2.0 * t);
        angle_opacity = mix(clamp(falloff.z, 0.0, 1.0), clamp(falloff.w, 0.0, 1.0), t);
    }
}
void fragment() {
    vec2 uv = UV * uv_scale;
    float c = cos(uv_rotation);
    float s = sin(uv_rotation);
    uv = mat2(vec2(c, s), vec2(-s, c)) * (uv - vec2(0.5)) + vec2(0.5) + uv_offset;
    if (!wrap_uv.x) { uv.x = clamp(uv.x, 0.000001, 0.999999); }
    if (!wrap_uv.y) { uv.y = clamp(uv.y, 0.000001, 0.999999); }
    vec4 texel = texture(source_texture, uv);
    vec4 tint = base_color * vec4($VERTEX_COLOR, $VERTEX_ALPHA);
    float opacity = tint.a * angle_opacity * ($PALETTE_ALPHA ? 1.0 : texel.a);
    $SOFT
    if ($PALETTE_ALPHA) { opacity = texture(palette_texture, vec2(texel.a, opacity)).a; }
    vec3 source_rgb = texel.rgb;
    if ($PALETTE_ALPHA && !$PALETTE_COLOR) {
        vec3 low = source_rgb / 12.92;
        vec3 high = pow((source_rgb + 0.055) / 1.055, vec3(2.4));
        source_rgb = mix(low, high, step(vec3(0.04045), source_rgb));
    }
    vec3 radiance = source_rgb * tint.rgb * base_scale;
    if ($PALETTE_COLOR) { radiance = texture(palette_texture, vec2(texel.g, tint.r)).rgb * base_scale; }
    $ALPHA_TEST
    ALBEDO = radiance * lighting_influence;
    EMISSION = radiance * (1.0 - lighting_influence);
    ROUGHNESS = 1.0;
    $OUTPUT
}
