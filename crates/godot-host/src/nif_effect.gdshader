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
uniform bvec2 wrap_uv = bvec2(true);
uniform float animation_time = -1.0;
$ANIMATION_FUNCTIONS
varying float angle_opacity;
void vertex() {
    vec4 animated_falloff = falloff;
    $ANIMATION_VERTEX
    angle_opacity = 1.0;
    if ($FALLOFF) {
        vec3 position = (MODELVIEW_MATRIX * vec4(VERTEX, 1.0)).xyz;
        vec3 normal = normalize(MODELVIEW_NORMAL_MATRIX * NORMAL);
        float cosine = abs(dot(normal, normalize(position)));
        float interval = animated_falloff.y - animated_falloff.x;
        float t = abs(interval) < 0.000001 ? step(animated_falloff.x, cosine) : clamp((cosine - animated_falloff.x) / interval, 0.0, 1.0);
        t = t * t * (3.0 - 2.0 * t);
        angle_opacity = mix(clamp(animated_falloff.z, 0.0, 1.0), clamp(animated_falloff.w, 0.0, 1.0), t);
    }
}
void fragment() {
    vec2 animated_offset = uv_offset;
    vec2 animated_scale = uv_scale;
    vec4 animated_color = base_color;
    float animated_multiple = base_scale;
    $ANIMATION_FRAGMENT
    vec2 uv = UV * animated_scale + animated_offset;
    if (!wrap_uv.x) { uv.x = clamp(uv.x, 0.000001, 0.999999); }
    if (!wrap_uv.y) { uv.y = clamp(uv.y, 0.000001, 0.999999); }
    vec4 texel = texture(source_texture, uv);
    vec4 tint = animated_color * vec4($VERTEX_COLOR, $VERTEX_ALPHA);
    float opacity = tint.a * angle_opacity * ($PALETTE_ALPHA ? 1.0 : texel.a);
    $SOFT
    if ($PALETTE_ALPHA) { opacity = texture(palette_texture, vec2(texel.a, opacity)).a; }
    vec3 source_rgb = texel.rgb;
    if ($PALETTE_ALPHA && !$PALETTE_COLOR) {
        vec3 low = source_rgb / 12.92;
        vec3 high = pow((source_rgb + 0.055) / 1.055, vec3(2.4));
        source_rgb = mix(low, high, step(vec3(0.04045), source_rgb));
    }
    vec3 radiance = source_rgb * tint.rgb * animated_multiple;
    if ($PALETTE_COLOR) { radiance = texture(palette_texture, vec2(texel.g, tint.r)).rgb * animated_multiple; }
    $ALPHA_TEST
    ALBEDO = radiance * lighting_influence;
    EMISSION = radiance * (1.0 - lighting_influence);
    ROUGHNESS = 1.0;
    $OUTPUT
}
