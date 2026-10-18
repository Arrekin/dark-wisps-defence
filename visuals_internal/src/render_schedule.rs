//! # Banded Core2d
//!
//! The game's cameras select [`BandedCore2d`] through `CameraRenderGraph`.
//! This schedule follows Bevy's `Core2d`, replacing the transparent pass with a banded pass.
//! Before a band draws, the pass can apply displacement, capture a scene snapshot and draw overlays.
//! These cuts run only for bands with cut parts or paint instances that request a snapshot.
//!
//! The band pass, cut lane draws and adapted UI pass bind shared GPU data (`visuals::shared_gpu`).
//! Bevy's unmodified `main_opaque_pass_2d` does not bind it, so opaque and alpha-masked materials
//! cannot use shared data. Supporting them requires adapting that pass as well.
//!
//! Last synced with bevy 0.20.0 (`bevy_core_pipeline` `core_2d/mod.rs`, `bevy_post_process`
//! bloom, `bevy_ui_render` `ui_pass`) and bevy_egui 0.43.1.
//! Omitted because the game does not use them: MSAA writeback, FXAA, SMAA, CAS,
//! the post-process effect stack and 2D wireframes.

use core::ops::Range;

use bevy::{
    core_pipeline::{
        Core2dSystems,
        core_2d::{Transparent2d, main_opaque_pass_2d},
        tonemapping::tonemapping,
        upscaling::upscaling,
    },
    ecs::schedule::ScheduleBuildSettings,
    post_process::bloom::bloom,
    prelude::*,
    render::{
        RenderApp,
        camera::ExtractedCamera,
        diagnostic::RecordDiagnostics,
        render_phase::{SortedPhaseItem, SortedRenderPhase, ViewSortedRenderPhases},
        render_resource::{BindGroup, RenderPassDescriptor, StoreOp},
        renderer::{RenderContext, ViewQuery},
        view::{ExtractedView, ViewDepthStencilTexture, ViewTarget, ViewUniformOffset},
    },
    ui_render::{TransparentUi, UiCameraView, UiViewTarget},
};
use bevy_egui::render::{egui_pass, prepare_egui_pass};

use game_core::z_depth::Band;
use logging::prelude::*;
use visuals::render_schedule::BandedCore2d;

use crate::{cut::Cut, shared_gpu::SharedGpuBindGroup};

// Bind groups supplied by draw commands, below the shared GPU group.
/// View, mesh and material.
const MATERIAL_2D_GROUPS: usize = 3;
/// View and material, or view and image for Bevy's own UI pipelines.
const UI_GROUPS: usize = 2;

pub(crate) struct BandedCore2dPlugin;
impl Plugin for BandedCore2dPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app.edit_schedule(BandedCore2d, |schedule| {
            schedule.set_build_settings(ScheduleBuildSettings {
                auto_insert_apply_deferred: false,
                ..default()
            });
            schedule.configure_sets(
                (Core2dSystems::Prepass, Core2dSystems::MainPass, Core2dSystems::EarlyPostProcess, Core2dSystems::PostProcess).chain_weak(),
            );
            schedule.add_systems((
                (main_opaque_pass_2d, banded_transparent_pass_2d).chain().in_set(Core2dSystems::MainPass),
                bloom.before(tonemapping).in_set(Core2dSystems::PostProcess),
                tonemapping.in_set(Core2dSystems::PostProcess),
                ui_pass_with_shared_gpu.after(Core2dSystems::PostProcess).before(upscaling),
                // bevy_egui's default order: egui above Bevy UI.
                (prepare_egui_pass, egui_pass.after(Core2dSystems::MainPass).after(ui_pass_with_shared_gpu).before(upscaling)).chain(),
                upscaling.after(Core2dSystems::PostProcess),
            ));
        });
    }
}

/// Renders the sorted transparent phase, pausing at bands that need cuts.
/// Each cut applies displacement, then captures a snapshot if overlays or paint readers need it.
/// Overlays draw first; the transparent phase resumes with the snapshot bound.
fn banded_transparent_pass_2d(
    world: &World,
    view: ViewQuery<(
        &ExtractedCamera,
        &ExtractedView,
        &ViewTarget,
        &ViewDepthStencilTexture,
        &ViewUniformOffset,
    )>,
    transparent_phases: Res<ViewSortedRenderPhases<Transparent2d>>,
    shared_gpu: Res<SharedGpuBindGroup>,
    mut context: RenderContext,
) {
    let shared_gpu = shared_gpu.into_inner();
    let view_entity = view.entity();
    let (camera, extracted_view, target, depth, view_uniform_offset) = view.into_inner();
    let Some(transparent_phase) = transparent_phases.get(&extracted_view.retained_view_entity) else { return; };
    let items = BandItems { world, view_entity, camera, target, depth, shared_gpu, transparent_phase };

    let mut drawn_until = 0;
    let mut beneath: Option<BindGroup> = None;
    if let Some(cut) = Cut::new(world, &context, view_entity, camera, view_uniform_offset, shared_gpu) {
        for band in cut.bands() {
            let cut_at = items.band_start(band).max(drawn_until);
            items.draw(&mut context, beneath.as_ref(), drawn_until..cut_at);
            cut.apply_displacement(&mut context, target, band);
            beneath = cut.snapshot_and_draw_overlays(&mut context, target, band);
            drawn_until = cut_at;
        }
    }
    items.draw(&mut context, beneath.as_ref(), drawn_until..transparent_phase.items.len());
}

/// Per-view resources for rendering ranges of the sorted transparent phase.
struct BandItems<'a> {
    world: &'a World,
    view_entity: Entity,
    camera: &'a ExtractedCamera,
    target: &'a ViewTarget,
    depth: &'a ViewDepthStencilTexture,
    shared_gpu: &'a SharedGpuBindGroup,
    transparent_phase: &'a SortedRenderPhase<Transparent2d>,
}
impl BandItems<'_> {
    /// Index of the first item at or above the band's base z.
    ///
    /// This index does not split Bevy batches. Adjacent items with the same batch key can span
    /// the boundary because depth is not part of that key. The entire batch draws with its first
    /// item, so items above the boundary can receive distortion intended only for lower bands.
    fn band_start(&self, band: Band) -> usize {
        let base_z = band.base_z();
        self.transparent_phase.items.partition_point(|_, item| item.sort_key().0 < base_z)
    }

    /// Draws a range of the phase in one render pass.
    #[log_tags(Tag::Rendering)]
    fn draw(&self, context: &mut RenderContext, beneath: Option<&BindGroup>, items: Range<usize>) {
        if items.is_empty() { return; }
        let diagnostics = context.diagnostic_recorder();
        let diagnostics = diagnostics.as_deref();

        let color_attachments = [Some(self.target.get_color_attachment())];
        // The depth buffer is loaded so opaque items still occlude, and stored as a workaround
        // for https://github.com/bevyengine/bevy/issues/3776 so wgpu does not clear it.
        let depth_stencil_attachment = Some(self.depth.get_attachment(StoreOp::Store));
        let mut render_pass = context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("banded_transparent_pass_2d"),
            color_attachments: &color_attachments,
            depth_stencil_attachment,
            ..default()
        });
        let pass_span = diagnostics.pass_span(&mut render_pass, "banded_transparent_pass_2d");
        if let Some(viewport) = self.camera.viewport.as_ref() {
            render_pass.set_camera_viewport(viewport);
        }
        // Set before any pipeline; wgpu keeps it across the pipeline switches of the phase.
        self.shared_gpu.set_on(&mut render_pass, MATERIAL_2D_GROUPS, beneath);
        let _ = self.transparent_phase.render_range(&mut render_pass, self.world, self.view_entity, items).inspect_err(|error| error_dev!("Error encountered while rendering the transparent 2D phase {error:?}"));
        pass_span.end(&mut render_pass);
    }
}

/// Bevy's `ui_pass`, with the shared GPU group set for opted-in `UiMaterial` pipelines.
#[log_tags(Tag::Rendering)]
fn ui_pass_with_shared_gpu(
    world: &World,
    view: ViewQuery<&UiCameraView>,
    transparent_render_phases: Res<ViewSortedRenderPhases<TransparentUi>>,
    shared_gpu: Res<SharedGpuBindGroup>,
    ui_views: Query<(&ExtractedView, &UiViewTarget)>,
    ui_view_targets: Query<(&ViewTarget, &ExtractedCamera)>,
    mut context: RenderContext,
) {
    let ui_view_entity = view.into_inner().0;
    let Ok((extracted_view, ui_view_target)) = ui_views.get(ui_view_entity) else { return; };
    let Ok((target, camera)) = ui_view_targets.get(ui_view_target.0) else { return; };
    let Some(transparent_phase) = transparent_render_phases.get(&extracted_view.retained_view_entity) else { return; };
    if transparent_phase.items.is_empty() { return; }

    let diagnostics = context.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let mut render_pass = context.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("ui"),
        color_attachments: &[Some(target.get_unsampled_color_attachment())],
        ..default()
    });
    let pass_span = diagnostics.pass_span(&mut render_pass, "ui");
    if let Some(viewport) = camera.viewport.as_ref() {
        render_pass.set_camera_viewport(viewport);
    }
    shared_gpu.set_on(&mut render_pass, UI_GROUPS, None);
    let _ = transparent_phase.render(&mut render_pass, world, ui_view_entity).inspect_err(|error| error_dev!("Error encountered while rendering the ui phase {error:?}"));
    pass_span.end(&mut render_pass);
}
