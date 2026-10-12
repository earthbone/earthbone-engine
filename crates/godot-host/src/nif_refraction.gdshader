shader_type spatial;
render_mode unshaded, fog_disabled, depth_draw_never, blend_mix, $CULL$DEPTH_TEST;
uniform sampler2D distortion_texture : hint_default_white, filter_linear_mipmap, repeat_enable;
uniform sampler2D screen_texture : hint_screen_texture, repeat_disable, filter_linear;
uniform float strength = 0.0;
uniform float base_alpha = 1.0;
uniform vec2 uv_offset = vec2(0.0);
uniform vec2 uv_scale = vec2(1.0);
uniform bvec2 wrap_uv = bvec2(true);
void fragment() {
    vec2 uv = UV * uv_scale + uv_offset;
    if (!wrap_uv.x) { uv.x = clamp(uv.x, 0.000001, 0.999999); }
    if (!wrap_uv.y) { uv.y = clamp(uv.y, 0.000001, 0.999999); }
    vec4 distortion = texture(distortion_texture, uv);
    float opacity = distortion.a * base_alpha * $VERTEX_ALPHA;
    if (opacity < 0.1) { discard; }
    vec2 offset = (distortion.rg * 2.0 - 1.0) * opacity * strength * 0.14;
    ALBEDO = textureLod(screen_texture, clamp(SCREEN_UV + offset, vec2(0.0), vec2(1.0)), 0.0).rgb;
    ALPHA = 1.0;
}
