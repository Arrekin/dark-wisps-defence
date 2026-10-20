//! Prepares the shared GPU bind group for the band, cut and UI passes.
//! The base group uses Bevy's fallback image and is rebuilt when buffer handles or GPU allocations
//! change. Each scene snapshot gets a separate group with the snapshot texture substituted.

use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::ExtractResourcePlugin,
        render_asset::RenderAssets,
        render_phase::TrackedRenderPass,
        render_resource::{BindGroup, BindGroupEntry, BindGroupLayout, BindingResource, Buffer, FilterMode, PipelineCache, Sampler, SamplerDescriptor, TextureView},
        renderer::RenderDevice,
        storage::{GpuShaderBuffer, RenderChangedShaderBuffers},
        texture::FallbackImage,
    },
};
use strum::IntoEnumIterator;

use visuals::shared_gpu::{BENEATH_SAMPLER_BINDING, BENEATH_TEXTURE_BINDING, SHARED_GPU_GROUP, SharedGpuBinding, SharedGpuBuffers, gap_layout, shared_gpu_layout};

pub(crate) struct SharedGpuPlugin;
impl Plugin for SharedGpuPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<SharedGpuBuffers>()
            .add_plugins(ExtractResourcePlugin::<SharedGpuBuffers>::default());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app.add_systems(Render, prepare_shared_gpu_bind_group.in_set(RenderSystems::PrepareBindGroups));
    }
}

/// Shared buffers, snapshot sampler and an empty group for unused binding slots.
/// Created once every [`SharedGpuBinding`] has a GPU buffer.
#[derive(Resource)]
pub(crate) struct SharedGpuBindGroup {
    buffers: Vec<(u32, Buffer)>,
    layout: BindGroupLayout,
    beneath_sampler: Sampler,
    /// The group with the fallback image as its beneath snapshot.
    shared: BindGroup,
    gap: BindGroup,
}
impl SharedGpuBindGroup {
    /// The group with `snapshot` as its beneath snapshot.
    pub(crate) fn with_beneath(&self, render_device: &RenderDevice, snapshot: &TextureView) -> BindGroup {
        Self::create(render_device, &self.layout, &self.buffers, snapshot, &self.beneath_sampler)
    }

    /// Binds shared data with the supplied snapshot group, or the fallback group.
    /// Fills indices from `first_unused_group` to `SHARED_GPU_GROUP` with empty groups.
    /// Draw commands can replace those empty groups with their own bindings.
    pub(crate) fn set_on<'pass>(&'pass self, render_pass: &mut TrackedRenderPass<'pass>, first_unused_group: usize, beneath: Option<&'pass BindGroup>) {
        for group in first_unused_group..SHARED_GPU_GROUP {
            render_pass.set_bind_group(group, &self.gap, &[]);
        }
        render_pass.set_bind_group(SHARED_GPU_GROUP, beneath.unwrap_or(&self.shared), &[]);
    }

    fn create(render_device: &RenderDevice, layout: &BindGroupLayout, buffers: &[(u32, Buffer)], snapshot: &TextureView, beneath_sampler: &Sampler) -> BindGroup {
        let entries: Vec<BindGroupEntry> = buffers
            .iter()
            .map(|(binding, buffer)| BindGroupEntry { binding: *binding, resource: buffer.as_entire_binding() })
            .chain([
                BindGroupEntry { binding: BENEATH_TEXTURE_BINDING, resource: BindingResource::TextureView(snapshot) },
                BindGroupEntry { binding: BENEATH_SAMPLER_BINDING, resource: BindingResource::Sampler(beneath_sampler) },
            ])
            .collect();
        render_device.create_bind_group("shared_gpu_bind_group", layout, &entries)
    }
}

fn prepare_shared_gpu_bind_group(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    fallback_image: Res<FallbackImage>,
    gpu_buffers: Res<RenderAssets<GpuShaderBuffer>>,
    changed_shader_buffers: Res<RenderChangedShaderBuffers>,
    shared_gpu_buffers: Res<SharedGpuBuffers>,
    bind_group: Option<Res<SharedGpuBindGroup>>,
) {
    let is_stale = bind_group.is_none()
        || shared_gpu_buffers.is_changed()
        || SharedGpuBinding::iter().any(|binding| changed_shader_buffers.0.contains(&shared_gpu_buffers.get(binding).id()));
    if !is_stale { return; }

    let buffers: Option<Vec<(u32, Buffer)>> = SharedGpuBinding::iter()
        .map(|binding| gpu_buffers.get(shared_gpu_buffers.get(binding)).map(|gpu_buffer| (binding.index(), gpu_buffer.buffer.clone())))
        .collect();
    let Some(buffers) = buffers else { return; };
    let layout = pipeline_cache.get_bind_group_layout(&shared_gpu_layout());
    // Looks sample the snapshot at arbitrary world positions, so it is read with linear filtering.
    let beneath_sampler = render_device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let shared = SharedGpuBindGroup::create(&render_device, &layout, &buffers, &fallback_image.d2.texture_view, &beneath_sampler);
    let gap = render_device.create_bind_group("shared_gpu_gap_bind_group", &pipeline_cache.get_bind_group_layout(&gap_layout()), &[]);
    commands.insert_resource(SharedGpuBindGroup { buffers, layout, beneath_sampler, shared, gap });
}
