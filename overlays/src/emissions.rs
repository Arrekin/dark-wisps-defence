use bevy::{
    input::common_conditions::input_just_released,
    prelude::*,
    reflect::TypePath,
    render::{
        render_resource::{AsBindGroup, ShaderType},
        storage::ShaderBuffer,
    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};

use game_core::prelude::{Bounds, MapInfo, ZDepth};
use grids::{
    emissions::{EmissionsGrid, EmissionsType},
    prelude::GridVersion,
};
use states::prelude::MapLoadingStage;
use visuals::prelude::MapCanvasBundle;

pub struct EmissionsOverlayPlugin;
impl Plugin for EmissionsOverlayPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(Material2dPlugin::<EmissionsOverlayMaterial>::default())
            .init_state::<EmissionsOverlayState>()
            .init_resource::<EmissionsOverlayConfig>()
            .add_systems(OnEnter(MapLoadingStage::LoadResources), EmissionsOverlay::create)
            .add_systems(OnEnter(EmissionsOverlayState::Show), |visibility: Single<&mut Visibility, With<EmissionsOverlay>>| { *visibility.into_inner() = Visibility::Inherited; })
            .add_systems(OnExit(EmissionsOverlayState::Show), |visibility: Single<&mut Visibility, With<EmissionsOverlay>>| { *visibility.into_inner() = Visibility::Hidden; })
            .add_systems(Update, (
                EmissionsOverlayConfig::on_config_change_system.run_if(resource_changed::<EmissionsOverlayConfig>),
                refresh_display_system.run_if(in_state(EmissionsOverlayState::Show)),
                (|mut config: ResMut<EmissionsOverlayConfig>| { config.is_overlay_globally_enabled ^= true; }).run_if(input_just_released(KeyCode::Digit6)),
            ));
    }
}

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum EmissionsOverlayState {
    #[default]
    Hide,
    Show,
}

#[derive(Resource)]
pub struct EmissionsOverlayConfig {
    pub is_overlay_globally_enabled: bool,
    pub emissions_type: EmissionsType,
    pub grid_version: GridVersion,
}
impl Default for EmissionsOverlayConfig {
    fn default() -> Self {
        Self {
            is_overlay_globally_enabled: false,
            emissions_type: EmissionsType::Energy,
            grid_version: GridVersion::default(),
        }
    }
}
impl EmissionsOverlayConfig {
    fn on_config_change_system(
        overlay_config: Res<EmissionsOverlayConfig>,
        mut overlay_state: ResMut<NextState<EmissionsOverlayState>>,
    ) {
        let shown = overlay_config.is_overlay_globally_enabled;
        overlay_state.set(if shown { EmissionsOverlayState::Show } else { EmissionsOverlayState::Hide });
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Default)]
struct EmissionsUniformData {
    grid_width: u32,
    grid_height: u32,
    min_value: f32,
    max_value: f32,
}
impl EmissionsUniformData {
    fn new(bounds: Bounds, min_value: f32, max_value: f32) -> Self {
        let (grid_width, grid_height) = bounds.as_u32();
        Self { grid_width, grid_height, min_value, max_value }
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct EmissionsOverlayMaterial {
    #[storage(0, read_only)]
    pub cells: Handle<ShaderBuffer>,
    #[uniform(1)]
    pub uniforms: EmissionsUniformData,
}
impl Material2d for EmissionsOverlayMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/overlays/emissions_map.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct EmissionsCell {
    energy: f32,
}

#[derive(Component)]
#[require(ZDepth::OVERLAY_EMISSIONS)]
pub struct EmissionsOverlay;
impl EmissionsOverlay {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<EmissionsOverlayMaterial>>,
        mut buffers: ResMut<Assets<ShaderBuffer>>,
        overlay: Option<Single<Entity, With<EmissionsOverlay>>>,
    ) {
        if let Some(overlay_entity) = overlay {
            commands.entity(overlay_entity.into_inner()).despawn();
        }

        // Bind an initialized buffer on the material's spawn frame.
        let cells = buffers.add(ShaderBuffer::from(vec![EmissionsCell::default(); map_info.grid_bounds.area()]));
        let material = materials.add(EmissionsOverlayMaterial { cells, ..default() });
        commands.spawn((
            MapCanvasBundle::new(&mut meshes, material, &map_info),
            EmissionsOverlay,
        ));
    }
}

fn refresh_display_system(
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<EmissionsOverlayMaterial>>,
    emissions_grid: Res<EmissionsGrid>,
    mut overlay_config: ResMut<EmissionsOverlayConfig>,
    overlay: Single<&MeshMaterial2d<EmissionsOverlayMaterial>, With<EmissionsOverlay>>,
) {
    let current_version = match overlay_config.emissions_type {
        EmissionsType::Energy => emissions_grid.version.energy,
    };
    if overlay_config.grid_version == current_version { return; }
    overlay_config.grid_version = current_version;

    let mut overlay_material = materials.get_mut(overlay.into_inner()).unwrap();

    // Find min/max for GPU-side normalization
    let (mut min_value, mut max_value) = (f32::MAX, f32::MIN);
    for emissions in emissions_grid.grid.iter() {
        let value = match overlay_config.emissions_type {
            EmissionsType::Energy => emissions.energy,
        };
        if value != 0. { min_value = min_value.min(value); }
        max_value = max_value.max(value);
    }
    if min_value == f32::MAX { min_value = 0.; }

    // Update SSBO
    let mut buffer = buffers.get_mut(&overlay_material.cells).unwrap();
    buffer.clear();
    buffer.extend(emissions_grid.grid.iter().map(|emissions| {
        let energy = match overlay_config.emissions_type {
            EmissionsType::Energy => emissions.energy,
        };
        EmissionsCell { energy }
    }));

    // Update uniforms
    overlay_material.uniforms = EmissionsUniformData::new(emissions_grid.bounds, min_value, max_value);
}
