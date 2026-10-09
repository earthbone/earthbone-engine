shader_type sky;
render_mode disable_fog;

uniform vec3 upper : source_color;
uniform vec3 lower : source_color;
uniform vec3 horizon : source_color;
uniform vec3 sun_color : source_color;
uniform vec3 stars_color : source_color;
uniform vec3 sun_direction;
uniform sampler2D sun_texture : source_color, filter_linear;
uniform sampler2D glare_texture : source_color, filter_linear;
uniform bool sun_enabled;
uniform float sun_glare;
uniform sampler2D stars_texture : source_color, filter_linear, repeat_enable;
uniform bool stars_enabled;
uniform float night_alpha;
uniform sampler2D masser_texture : source_color, filter_linear;
uniform sampler2D secunda_texture : source_color, filter_linear;
uniform bool masser_enabled;
uniform bool secunda_enabled;
uniform vec3 masser_direction;
uniform vec3 secunda_direction;
uniform sampler2DArray cloud_textures : source_color, filter_linear_mipmap, repeat_enable;
uniform vec4 cloud_colors[256] : source_color;
uniform vec2 cloud_speeds[256];
uniform float cloud_layers[256];
uniform int cloud_count;
uniform float elapsed;

vec4 disc(sampler2D tex, vec3 direction, float radius, vec3 eye) {
	if (dot(eye, direction) < cos(radius * 1.5)) { return vec4(0.0); }
	vec3 axis = abs(direction.y) > 0.99 ? vec3(0.0, 0.0, -1.0) : vec3(0.0, 1.0, 0.0);
	vec3 right = normalize(cross(axis, direction));
	vec3 up = cross(direction, right);
	vec2 uv = vec2(dot(eye, right), -dot(eye, up)) / (radius * 2.0) + 0.5;
	if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) { return vec4(0.0); }
	return texture(tex, uv);
}

void sky() {
	COLOR = EYEDIR.y >= 0.0 ? mix(horizon, upper, smoothstep(0.0, 0.55, EYEDIR.y)) : mix(horizon, lower, -EYEDIR.y);
	if (!AT_CUBEMAP_PASS) {
		if (stars_enabled && night_alpha > 0.0) {
			vec4 stars = texture(stars_texture, SKY_COORDS);
			COLOR += stars.rgb * stars_color * night_alpha;
		}
		if (sun_enabled && sun_direction.y > 0.0) {
			vec4 glare = disc(glare_texture, sun_direction, 0.15, EYEDIR);
			COLOR += glare.rgb * glare.a * sun_color * sun_glare;
			vec4 sun = disc(sun_texture, sun_direction, 0.025, EYEDIR);
			COLOR = mix(COLOR, sun.rgb * sun_color, sun.a);
		}
		if (masser_enabled && masser_direction.y > 0.0) {
			vec4 moon = disc(masser_texture, masser_direction, 0.055, EYEDIR);
			COLOR = mix(COLOR, moon.rgb, moon.a);
		}
		if (secunda_enabled && secunda_direction.y > 0.0) {
			vec4 moon = disc(secunda_texture, secunda_direction, 0.035, EYEDIR);
			COLOR = mix(COLOR, moon.rgb, moon.a);
		}
		if (EYEDIR.y > 0.0) {
			// CONTEXT: Stereographic hemisphere coordinates have no cube faces or horizon clamp.
			vec2 uv = EYEDIR.xz / (1.0 + EYEDIR.y) * 0.25 + 0.5;
			float horizon_alpha = smoothstep(0.0, 0.16, EYEDIR.y);
			for (int i = 0; i < cloud_count; i++) {
				vec4 cloud = texture(cloud_textures, vec3(uv + cloud_speeds[i] * elapsed, cloud_layers[i]));
				COLOR = mix(COLOR, cloud.rgb * cloud_colors[i].rgb, cloud.a * cloud_colors[i].a * horizon_alpha);
			}
		}
	}
}
