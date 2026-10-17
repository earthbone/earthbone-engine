// SPDX-License-Identifier: GPL-3.0-or-later
// CONTEXT: OpenMW terrain/material.cpp separates native tiles from LAND weights.
shader_type spatial;

uniform vec2 quadrant;
LAYER_UNIFORMS

void fragment() {
    vec2 uv = vec2(UV.x, 1.0 - UV.y) * 2.0 - quadrant;
    if (any(lessThan(uv, vec2(0.0))) || any(greaterThanEqual(uv, vec2(1.0)))) {
        discard;
    }
    vec2 weights_uv = (uv * (WEIGHT_SIDE - 1.0) + 0.5) / WEIGHT_SIDE;
    vec3 color = texture(layer_0, uv * 4.0).rgb;
    LAYER_BLEND
    ALBEDO = color;
    ROUGHNESS = 1.0;
}
