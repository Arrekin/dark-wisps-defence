//! Processes the scene at band boundaries.
//! Each camera gets a displacement texture for every band with displacement draws.
//! The banded render pass uses [`Cut`] to apply those draws, capture a scene snapshot and render
//! overlays before continuing with the band's transparent items.

use bevy::{
    core_pipeline::FullscreenShader,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        camera::ExtractedCamera,
        extract_resource::ExtractResourcePlugin,
        globals::GlobalsBuffer,
        render_phase::TrackedRenderPass,
        render_resource::{
            binding_types::{sampler, texture_2d},
            *,
        },
        renderer::{RenderContext, RenderDevice},
        texture::{CachedTexture, TextureCache},
        view::{ViewTarget, ViewUniformOffset, ViewUniforms},
    },
};
use strum::IntoEnumIterator;

use game_core::z_depth::{Band, PerBand};
use visuals::{
    cut_lane::{CutBatch, CutBatches, CutPrepareSystems, DISPLACEMENT_FORMAT, SCENE_FORMAT, cut_view_layout},
    lane::BandsReadingBeneath,
};

use crate::shared_gpu::SharedGpuBindGroup;

/// Groups a cut lane pipeline fills itself, below the shared GPU group: view, quads and instances.
const CUT_LANE_GROUPS: usize = 3;

pub(crate) struct CutPlugin;
impl Plugin for CutPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<BandsReadingBeneath>()
            .add_plugins(ExtractResourcePlugin::<BandsReadingBeneath>::default())
            .add_systems(First, clear_bands_reading_beneath);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app
            .init_resource::<CutBatches>()
            .add_systems(RenderStartup, init_cut_pipelines)
            .add_systems(Render, (
                (sort_cut_overlays, prepare_displacement_textures).in_set(RenderSystems::PrepareResources).after(CutPrepareSystems),
                clear_cut_batches.in_set(RenderSystems::Cleanup),
            ));
    }
}

/// A camera's displacement buffers for this frame, present for the bands that have displacement.
#[derive(Component)]
struct ViewDisplacementTextures(PerBand<Option<CachedTexture>>);
impl ViewDisplacementTextures {
    fn of_band(&self, band: Band) -> Option<&CachedTexture> {
        self.0[band].as_ref()
    }
}

fn prepare_displacement_textures(
    mut commands: Commands,
    mut texture_cache: ResMut<TextureCache>,
    render_device: Res<RenderDevice>,
    batches: Res<CutBatches>,
    views: Query<(Entity, &ViewTarget), With<ExtractedCamera>>,
) {
    for (view, view_target) in views.iter() {
        let size = view_target.main_texture().size();
        let mut textures = PerBand::default();
        for band in Band::iter().filter(|band| !batches.displacement_of(*band).is_empty()) {
            textures[band] = Some(texture_cache.get(&render_device, TextureDescriptor {
                label: Some("displacement_buffer"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: DISPLACEMENT_FORMAT,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }));
        }
        commands.entity(view).insert(ViewDisplacementTextures(textures));
    }
}

fn clear_bands_reading_beneath(mut bands: ResMut<BandsReadingBeneath>) {
    bands.clear();
}

fn sort_cut_overlays(mut batches: ResMut<CutBatches>) {
    batches.sort_overlays();
}

fn clear_cut_batches(mut batches: ResMut<CutBatches>) {
    batches.clear();
}

/// The displacement apply and scene copy pipelines, and the sampler every cut reads the scene with.
#[derive(Resource)]
struct CutPipelines {
    apply_layout: BindGroupLayoutDescriptor,
    apply_pipeline: CachedRenderPipelineId,
    copy_layout: BindGroupLayoutDescriptor,
    copy_pipeline: CachedRenderPipelineId,
    scene_sampler: Sampler,
}

fn init_cut_pipelines(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
    render_device: Res<RenderDevice>,
) {
    let apply_layout = BindGroupLayoutDescriptor::new(
        "displacement_apply_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                texture_2d(TextureSampleType::Float { filterable: true }),
            ),
        ),
    );
    // Offsets move the sample point by fractions of a pixel, so the scene is read with linear filtering.
    let scene_sampler = render_device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let shader = asset_server.load("shaders/visuals/cut.wesl");
    let fullscreen_pipeline = |label: &'static str, layout: &BindGroupLayoutDescriptor, entry_point: &'static str| {
        pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(label.into()),
            layout: vec![layout.clone()],
            vertex: fullscreen_shader.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: shader.clone(),
                entry_point: Some(entry_point.into()),
                targets: vec![Some(ColorTargetState { format: SCENE_FORMAT, blend: None, write_mask: ColorWrites::ALL })],
                ..default()
            }),
            ..default()
        })
    };
    let apply_pipeline = fullscreen_pipeline("displacement_apply_pipeline", &apply_layout, "apply_displacement");
    let copy_layout = BindGroupLayoutDescriptor::new(
        "scene_copy_layout",
        &BindGroupLayoutEntries::single(ShaderStages::FRAGMENT, texture_2d(TextureSampleType::Float { filterable: true })),
    );
    let copy_pipeline = fullscreen_pipeline("scene_copy_pipeline", &copy_layout, "copy_scene");
    commands.insert_resource(CutPipelines { apply_layout, apply_pipeline, copy_layout, copy_pipeline, scene_sampler });
}

/// Per-view resources for applying displacement, capturing snapshots and drawing overlays.
pub(crate) struct Cut<'a> {
    camera: &'a ExtractedCamera,
    batches: &'a CutBatches,
    readers: &'a BandsReadingBeneath,
    displacement_textures: Option<&'a ViewDisplacementTextures>,
    cut_pipelines: &'a CutPipelines,
    shared_gpu: &'a SharedGpuBindGroup,
    pipeline_cache: &'a PipelineCache,
    view_bind_group: BindGroup,
    view_uniform_offset: u32,
}
impl<'a> Cut<'a> {
    /// Prepares cut resources for a view.
    /// Returns `None` if the camera is not HDR or the view and globals uniforms are unavailable.
    /// Cut pipelines require the HDR target format, `SCENE_FORMAT`.
    pub(crate) fn new(
        world: &'a World,
        context: &RenderContext,
        view: Entity,
        camera: &'a ExtractedCamera,
        view_uniform_offset: &ViewUniformOffset,
        shared_gpu: &'a SharedGpuBindGroup,
    ) -> Option<Self> {
        if !camera.hdr { return None; }
        let view_binding = world.resource::<ViewUniforms>().uniforms.binding()?;
        let globals_binding = world.resource::<GlobalsBuffer>().buffer.binding()?;
        let pipeline_cache = world.resource::<PipelineCache>();
        let view_bind_group = context.render_device().create_bind_group(
            "cut_view_bind_group",
            &pipeline_cache.get_bind_group_layout(&cut_view_layout()),
            &BindGroupEntries::sequential((view_binding, globals_binding)),
        );
        Some(Self {
            camera,
            batches: world.resource::<CutBatches>(),
            readers: world.resource::<BandsReadingBeneath>(),
            displacement_textures: world.get::<ViewDisplacementTextures>(view),
            cut_pipelines: world.resource::<CutPipelines>(),
            shared_gpu,
            pipeline_cache,
            view_bind_group,
            view_uniform_offset: view_uniform_offset.offset,
        })
    }

    /// The bands cut this frame: those with cut lane parts or a `PAINT` kind reading beneath.
    pub(crate) fn bands(&self) -> impl Iterator<Item = Band> {
        Band::iter().filter(|band| self.batches.cuts(*band) || self.readers.reads(*band))
    }

    /// Accumulates the band's displacement draws, then applies them to the scene from lower bands.
    pub(crate) fn apply_displacement(&self, context: &mut RenderContext, target: &ViewTarget, band: Band) {
        let Some(texture) = self.displacement_textures.and_then(|textures| textures.of_band(band)) else { return; };
        // Skip displacement until asynchronous compilation of the apply pipeline finishes.
        let Some(apply_pipeline) = self.pipeline_cache.get_render_pipeline(self.cut_pipelines.apply_pipeline) else { return; };

        {
            let mut render_pass = context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("cut_displacement"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &texture.default_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations { load: LoadOp::Clear(Default::default()), store: StoreOp::Store },
                })],
                ..default()
            });
            self.set_camera_viewport(&mut render_pass);
            render_pass.set_bind_group(0, &self.view_bind_group, &[self.view_uniform_offset]);
            self.shared_gpu.set_on(&mut render_pass, CUT_LANE_GROUPS, None);
            self.draw_batches(&mut render_pass, self.batches.displacement_of(band));
        }

        let post_process = target.post_process_write();
        let bind_group = context.render_device().create_bind_group(
            "displacement_apply_bind_group",
            &self.pipeline_cache.get_bind_group_layout(&self.cut_pipelines.apply_layout),
            &BindGroupEntries::sequential((post_process.source, &self.cut_pipelines.scene_sampler, &texture.default_view)),
        );
        let mut render_pass = context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("cut_displacement_apply"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post_process.destination,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            ..default()
        });
        render_pass.set_render_pipeline(apply_pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }

    /// Captures the scene if the band has overlays or paint readers, then draws its overlays.
    /// Returns the snapshot bind group for subsequent band items, or `None` if no snapshot is
    /// needed or the copy pipeline is not ready.
    pub(crate) fn snapshot_and_draw_overlays(&self, context: &mut RenderContext, target: &ViewTarget, band: Band) -> Option<BindGroup> {
        let overlays = self.batches.overlay_of(band);
        if overlays.is_empty() && !self.readers.reads(band) { return None; }
        // Shaders compile asynchronously; until the copy pipeline is ready the band has no snapshot.
        let copy_pipeline = self.pipeline_cache.get_render_pipeline(self.cut_pipelines.copy_pipeline)?;

        // The snapshot is the scene drawn so far. Drawing continues on a copy of it, so the snapshot
        // stays as it was for the rest of the band. The copy is a full-screen draw, so the main
        // textures need no copy usage.
        let post_process = target.post_process_write();
        let copy_bind_group = context.render_device().create_bind_group(
            "scene_copy_bind_group",
            &self.pipeline_cache.get_bind_group_layout(&self.cut_pipelines.copy_layout),
            &BindGroupEntries::single(post_process.source),
        );
        let beneath = self.shared_gpu.with_beneath(context.render_device(), post_process.source);
        {
            let mut render_pass = context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("cut_snapshot_and_overlays"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: post_process.destination,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations::default(),
                })],
                ..default()
            });
            render_pass.set_render_pipeline(copy_pipeline);
            render_pass.set_bind_group(0, &copy_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
            // Set after the copy, which covers the whole target.
            self.set_camera_viewport(&mut render_pass);
            render_pass.set_bind_group(0, &self.view_bind_group, &[self.view_uniform_offset]);
            self.shared_gpu.set_on(&mut render_pass, CUT_LANE_GROUPS, Some(&beneath));
            self.draw_batches(&mut render_pass, overlays);
        }
        Some(beneath)
    }

    /// Restricts the lane quads to the camera's viewport.
    fn set_camera_viewport(&self, render_pass: &mut TrackedRenderPass) {
        if let Some(viewport) = self.camera.viewport.as_ref() {
            render_pass.set_camera_viewport(viewport);
        }
    }

    fn draw_batches<'pass>(&'pass self, render_pass: &mut TrackedRenderPass<'pass>, batches: &'pass [CutBatch]) {
        for batch in batches {
            // Shaders compile asynchronously; a kind whose pipeline is not ready yet is skipped.
            let Some(pipeline) = self.pipeline_cache.get_render_pipeline(batch.pipeline) else { continue; };
            render_pass.set_render_pipeline(pipeline);
            render_pass.set_bind_group(1, &batch.quad_bind_group, &[]);
            render_pass.set_bind_group(2, &batch.instance_bind_group, &[]);
            render_pass.draw(0..6, batch.instances.clone());
        }
    }
}
