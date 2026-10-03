#define_import_path dwd::map_light

// Shared map sun. Materials choose their own response colours and strengths.
// Unit ground-plane direction toward the sun, used for shadows and planar edge effects.
const MAP_SUN_GROUND_DIRECTION: vec2<f32> = vec2<f32>(-0.4472, 0.8944);

// Unit direction toward the sun, approximately 62 degrees above the ground plane.
const MAP_SUN_DIRECTION: vec3<f32> = vec3<f32>(-0.2124, 0.4248, 0.8800);

// Unit half-vector between the fixed top-down eye and `MAP_SUN_DIRECTION`.
const MAP_SUN_HALF_VECTOR: vec3<f32> = vec3<f32>(-0.1095, 0.2191, 0.9695);
