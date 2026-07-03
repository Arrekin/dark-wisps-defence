use bevy::{
    app::{App, Plugin},
    core_pipeline::{Core2dSystems, schedule::Core2d, tonemapping::tonemapping},
    ecs::schedule::IntoScheduleConfigs,
    render::RenderApp,
};

use visuals::prelude::{ForceFieldPostProcessSet, QuantumFieldPostProcessSet, RipplePostProcessSet};

/// Pins the screen-space post-process passes into one intentional order.
///
/// The passes form a ping-pong chain (each samples the previous one's output), so their order
/// decides layering wherever effects overlap on screen. The chosen order is:
///
/// `Tonemapping → Ripple → ForceField → QuantumField → Upscaling`
///
/// i.e. ground ripple at the bottom, the force-field dome above it, and the quantum anomaly
/// composited on top, so the anomaly's reality-warp distorts everything beneath it.
/// Upscaling runs after all post-processing via Bevy's built-in `Core2d` schedule.
///
/// **Must be added after** the three effect plugins: the system sets must already be
/// configured by each effect plugin before the ordering constraints are applied here.
pub struct PostProcessOrderingPlugin;
impl Plugin for PostProcessOrderingPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.configure_sets(
            Core2d,
            (
                RipplePostProcessSet
                    .in_set(Core2dSystems::PostProcess)
                    .after(tonemapping),
                ForceFieldPostProcessSet
                    .in_set(Core2dSystems::PostProcess)
                    .after(RipplePostProcessSet),
                QuantumFieldPostProcessSet
                    .in_set(Core2dSystems::PostProcess)
                    .after(ForceFieldPostProcessSet),
            ),
        );
    }
}
