// Look parameters of the motion-reactive wisps. Field order and types mirror `WispMotionLook` in
// wisps_internal/src/looks.rs.
struct WispMotionLook {
    seed: f32,          // per-instance phase offset, decorrelates a cluster of wisps
    vigor: f32,         // measured speed / sweet-spot, 0 at rest
    heading: vec2<f32>, // unit travel direction (quad-local)
};
