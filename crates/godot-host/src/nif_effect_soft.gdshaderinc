if (soft_depth > 0.0) {
    float scene_depth = texture(depth_texture, SCREEN_UV).r;
    vec4 scene = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, scene_depth, 1.0);
    float separation = VERTEX.z - scene.z / scene.w;
    float fade = 0.845 * pow(clamp(separation / (soft_depth * 0.33), 0.0, 1.0), 1.3);
    float facing = abs(dot(normalize(NORMAL), VIEW));
    float distance_ratio = clamp(length(VERTEX) / soft_depth, 0.0, 1.0);
    fade *= facing * (1.0 - pow(1.0 - distance_ratio * distance_ratio, 2.0)) * (1.0 - pow(1.0 - facing, 1.3));
    opacity *= fade;
}
