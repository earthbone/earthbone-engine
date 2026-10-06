shader_type canvas_item;
render_mode unshaded, blend_mix;

uniform sampler2D screen_texture : hint_screen_texture, filter_linear;
uniform vec4 tint_color : source_color = vec4(0.0);
uniform float blur_radius = 0.0;
uniform float double_vision = 0.0;
uniform float bloom_threshold = 0.7;
uniform float bloom_scale = 0.0;
uniform float saturation = 1.0;
uniform float brightness = 1.0;
uniform float contrast = 1.0;

vec3 sample_blurred(vec2 uv, float radius, vec2 pixel_size) {
	vec2 step_uv = pixel_size * radius;
	vec3 color = texture(screen_texture, uv).rgb * 0.4;
	color += texture(screen_texture, uv + vec2(step_uv.x, 0.0)).rgb * 0.15;
	color += texture(screen_texture, uv - vec2(step_uv.x, 0.0)).rgb * 0.15;
	color += texture(screen_texture, uv + vec2(0.0, step_uv.y)).rgb * 0.15;
	color += texture(screen_texture, uv - vec2(0.0, step_uv.y)).rgb * 0.15;
	return color;
}

void fragment() {
	float radius = clamp(blur_radius, 0.0, 16.0);
	vec2 offset = vec2(double_vision, 0.0);
	vec3 source = sample_blurred(SCREEN_UV, radius, SCREEN_PIXEL_SIZE);
	if (double_vision > 0.0) {
		source = mix(source, (source + texture(screen_texture, SCREEN_UV + offset).rgb) * 0.5, 0.5);
	}
	vec3 glow = max(source - vec3(bloom_threshold), vec3(0.0));
	source += glow * bloom_scale;
	source = (source - vec3(0.5)) * contrast + vec3(0.5);
	source *= brightness;
	float luminance = dot(source, vec3(0.299, 0.587, 0.114));
	source = mix(vec3(luminance), source, saturation);
	source = mix(source, tint_color.rgb, tint_color.a);
	COLOR = vec4(source, 1.0);
}
