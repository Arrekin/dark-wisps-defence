//! Prepares the shared GPU bind group for the band, cut, and UI passes.
//!
//! The default bind group uses Bevy's [`FallbackImage`] for [`SharedGpuBinding::BeneathTexture`]
//! and for any noise texture that has not finished loading. It is rebuilt whenever a shared buffer
//! or noise texture changes its GPU allocation. Passes with a captured scene snapshot build a
//! per-snapshot bind group via [`SharedGpuBindGroup::with_beneath`].

use bevy::{
    image::ImageLoaderSettings,
    platform::collections::HashMap,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_phase::TrackedRenderPass,
        render_resource::{AddressMode, BindGroup, BindGroupEntry, BindGroupLayout, BindingResource, Buffer, FilterMode, PipelineCache, Sampler, SamplerDescriptor, TextureView},
        renderer::RenderDevice,
        storage::{GpuShaderBuffer, RenderChangedShaderBuffers},
        texture::{FallbackImage, GpuImage},
    },
};
use strum::IntoEnumIterator;

use visuals::shared_gpu::{SHARED_GPU_GROUP, SharedGpuBinding, SharedGpuBindingKind, SharedGpuBuffers, gap_layout, shared_gpu_layout};

pub(crate) struct SharedGpuPlugin;
impl Plugin for SharedGpuPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<SharedGpuBuffers>()
            .init_resource::<NoiseTextures>()
            .add_plugins((ExtractResourcePlugin::<SharedGpuBuffers>::default(), ExtractResourcePlugin::<NoiseTextures>::default()));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app.add_systems(Render, prepare_shared_gpu_bind_group.in_set(RenderSystems::PrepareBindGroups));
    }
}

/// Static noise textures read through `shaders::common::noise`, one per noise texture binding.
#[derive(Resource, ExtractResource, Clone)]
#[extract_app(RenderApp)]
struct NoiseTextures {
    handles: HashMap<SharedGpuBinding, Handle<Image>>,
}
impl FromWorld for NoiseTextures {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        let handles = [
            (SharedGpuBinding::NoiseValue3d, "textures/noise/value_3d.ktx2"),
            (SharedGpuBinding::NoiseValue2d, "textures/noise/value_2d.ktx2"),
            (SharedGpuBinding::NoiseGradient2d, "textures/noise/gradient_2d.ktx2"),
        ]
        .into_iter()
        // Load as linear data (`is_srgb = false`) so the sampler does not apply sRGB-to-linear conversion to lattice values.
        .map(|(binding, path)| (binding, asset_server.load_builder().with_settings(|settings: &mut ImageLoaderSettings| settings.is_srgb = false).load(path)))
        .collect();
        Self { handles }
    }
}
impl NoiseTextures {
    /// Yields each noise texture binding paired with its loaded [`TextureView`], or the matching [`FallbackImage`] view while loading.
    fn views<'a>(&'a self, gpu_images: &'a RenderAssets<GpuImage>, fallback_image: &'a FallbackImage) -> impl Iterator<Item = (SharedGpuBinding, &'a TextureView)> {
        self.handles.iter().map(|(&binding, handle)| {
            let fallback = match binding.kind() {
                SharedGpuBindingKind::Texture3d => &fallback_image.d3,
                _ => &fallback_image.d2,
            };
            (binding, gpu_images.get(handle).map_or(&fallback.texture_view, |image| &image.texture_view))
        })
    }
}

/// GPU resources shared across both the default bind group and per-snapshot bind groups.
struct SharedGpuResources {
    buffers: HashMap<SharedGpuBinding, Buffer>,
    noise_textures: HashMap<SharedGpuBinding, TextureView>,
    beneath_sampler: Sampler,
    noise_sampler: Sampler,
}

/// Shared resources and an empty group for unused binding slots.
/// Created once every buffer [`SharedGpuBinding`] has a GPU buffer.
#[derive(Resource)]
pub(crate) struct SharedGpuBindGroup {
    resources: SharedGpuResources,
    layout: BindGroupLayout,
    /// Default bind group with [`FallbackImage`] bound at [`SharedGpuBinding::BeneathTexture`].
    shared: BindGroup,
    gap: BindGroup,
}
impl SharedGpuBindGroup {
    /// Creates a bind group with `snapshot` bound at [`SharedGpuBinding::BeneathTexture`].
    pub(crate) fn with_beneath(&self, render_device: &RenderDevice, snapshot: &TextureView) -> BindGroup {
        Self::create(render_device, &self.layout, &self.resources, snapshot)
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

    fn create(render_device: &RenderDevice, layout: &BindGroupLayout, resources: &SharedGpuResources, snapshot: &TextureView) -> BindGroup {
        let entries: Vec<BindGroupEntry> = SharedGpuBinding::iter()
            .map(|binding| {
                let resource = match binding {
                    SharedGpuBinding::BeneathTexture => BindingResource::TextureView(snapshot),
                    SharedGpuBinding::BeneathSampler => BindingResource::Sampler(&resources.beneath_sampler),
                    SharedGpuBinding::NoiseSampler => BindingResource::Sampler(&resources.noise_sampler),
                    buffer if buffer.kind() == SharedGpuBindingKind::Buffer => resources.buffers[&buffer].as_entire_binding(),
                    noise => BindingResource::TextureView(&resources.noise_textures[&noise]),
                };
                BindGroupEntry { binding: binding.index(), resource }
            })
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
    gpu_images: Res<RenderAssets<GpuImage>>,
    changed_shader_buffers: Res<RenderChangedShaderBuffers>,
    shared_gpu_buffers: Res<SharedGpuBuffers>,
    noise_textures: Res<NoiseTextures>,
    bind_group: Option<Res<SharedGpuBindGroup>>,
) {
    let noise_textures_changed = bind_group.as_ref().is_none_or(|bind_group| {
        noise_textures.views(&gpu_images, &fallback_image).any(|(binding, view)| bind_group.resources.noise_textures[&binding].id() != view.id())
    });
    let is_stale = noise_textures_changed
        || shared_gpu_buffers.is_changed()
        || SharedGpuBinding::buffers().any(|binding| changed_shader_buffers.0.contains(&shared_gpu_buffers.get(binding).id()));
    if !is_stale { return; }

    let buffers: Option<HashMap<SharedGpuBinding, Buffer>> = SharedGpuBinding::buffers()
        .map(|binding| gpu_buffers.get(shared_gpu_buffers.get(binding)).map(|gpu_buffer| (binding, gpu_buffer.buffer.clone())))
        .collect();
    let Some(buffers) = buffers else { return; };
    let noise_textures = noise_textures.views(&gpu_images, &fallback_image).map(|(binding, view)| (binding, view.clone())).collect();
    let layout = pipeline_cache.get_bind_group_layout(&shared_gpu_layout());
    let beneath_sampler = render_device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let noise_sampler = render_device.create_sampler(&SamplerDescriptor {
        address_mode_u: AddressMode::Repeat,
        address_mode_v: AddressMode::Repeat,
        address_mode_w: AddressMode::Repeat,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let resources = SharedGpuResources { buffers, noise_textures, beneath_sampler, noise_sampler };
    let shared = SharedGpuBindGroup::create(&render_device, &layout, &resources, &fallback_image.d2.texture_view);
    let gap = render_device.create_bind_group("shared_gpu_gap_bind_group", &pipeline_cache.get_bind_group_layout(&gap_layout()), &[]);
    commands.insert_resource(SharedGpuBindGroup { resources, layout, shared, gap });
}
