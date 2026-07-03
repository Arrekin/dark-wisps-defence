/// Compute the shortest angular difference between two angles, normalized to [-π, π].
/// Returns `target_angle - current_angle` wrapped to the shortest rotation direction.
pub fn angle_difference(target_angle: f32, current_angle: f32) -> f32 {
    (target_angle - current_angle + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
