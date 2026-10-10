shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, fog_disabled;

uniform sampler2D night_texture : source_color, filter_linear_mipmap, repeat_enable;
uniform vec3 stars_color : source_color;
uniform float night_alpha;
uniform sampler2DArray cloud_textures : source_color, filter_linear_mipmap, repeat_enable;
uniform vec4 cloud_colors[256] : source_color;
uniform vec2 cloud_speeds[256];
uniform float cloud_layers[256];
uniform int cloud_count;
uniform float elapsed;
uniform sampler2D sun_texture : source_color, filter_linear;
uniform sampler2D masser_texture : source_color, filter_linear;
uniform sampler2D secunda_texture : source_color, filter_linear;
uniform bool sun_enabled;
uniform bool masser_enabled;
uniform bool secunda_enabled;
uniform vec3 sun_direction;
uniform vec3 masser_direction;
uniform vec3 secunda_direction;
varying vec3 sky_direction;

float disc_alpha(sampler2D tex, vec3 direction, float radius, vec3 eye) {
	if (direction.y <= 0.0 || dot(eye, direction) < cos(radius * 1.5)) { return 0.0; }
	vec3 axis = abs(direction.y) > 0.99 ? vec3(0.0, 0.0, -1.0) : vec3(0.0, 1.0, 0.0);
	vec3 right = normalize(cross(axis, direction));
	vec3 up = cross(direction, right);
	vec2 uv = vec2(dot(eye, right), -dot(eye, up)) / (radius * 2.0) + 0.5;
	if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) { return 0.0; }
	return texture(tex, uv).a;
}

void vertex() {
	sky_direction = VERTEX;
	// CONTEXT: Direction-only projection follows the camera, at reversed-Z background depth.
	vec3 direction = (VIEW_MATRIX * vec4(VERTEX, 0.0)).xyz;
	POSITION = PROJECTION_MATRIX * vec4(direction, 1.0);
	POSITION.z = 0.0;
}

void fragment() {
	vec4 texel = texture(night_texture, UV);
	vec3 eye = normalize(sky_direction);
	float transmission = 1.0;
	if (sun_enabled) { transmission *= 1.0 - disc_alpha(sun_texture, sun_direction, 0.025, eye); }
	if (masser_enabled) { transmission *= 1.0 - disc_alpha(masser_texture, masser_direction, 0.055, eye); }
	if (secunda_enabled) { transmission *= 1.0 - disc_alpha(secunda_texture, secunda_direction, 0.035, eye); }
	if (eye.y > 0.0) {
		vec2 uv = eye.xz / (1.0 + eye.y) * 0.25 + 0.5;
		float horizon_alpha = smoothstep(0.0, 0.16, eye.y);
		// CONTEXT: Clouds draw in the background pass before these far-depth meshes.
		for (int i = 0; i < cloud_count; i++) {
			float alpha = texture(cloud_textures, vec3(uv + cloud_speeds[i] * elapsed, cloud_layers[i])).a;
			transmission *= 1.0 - alpha * cloud_colors[i].a * horizon_alpha;
		}
	}
	// CONTEXT: Sky vertex RGB stores blend weights, rather than a literal vertex tint.
	float color_weight = clamp(dot(COLOR.rgb, vec3(1.0)), 0.0, 1.0);
	ALBEDO = texel.rgb * color_weight * stars_color;
	ALPHA = texel.a * COLOR.a * night_alpha * transmission;
}
