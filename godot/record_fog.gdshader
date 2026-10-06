shader_type spatial;
render_mode unshaded, depth_draw_never, depth_test_disabled, cull_disabled, fog_disabled;

uniform sampler2D depth_texture : hint_depth_texture, repeat_disable, filter_nearest;
uniform vec3 near_color : source_color;
uniform vec3 far_color : source_color;
uniform float fog_near;
uniform float fog_far;
uniform float fog_power;
uniform float fog_max;

void vertex() {
	POSITION = vec4(VERTEX.xy, 1.0, 1.0);
}

void fragment() {
	float depth = texture(depth_texture, SCREEN_UV).r;
	vec4 view = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, depth, 1.0);
	float distance = length(view.xyz / view.w);
	float weight = clamp((distance - fog_near) / max(0.01, fog_far - fog_near), 0.0, 1.0);
	ALBEDO = mix(near_color, far_color, weight);
	ALPHA = depth > 0.0 ? pow(weight, max(0.01, fog_power)) * fog_max : 0.0;
}
