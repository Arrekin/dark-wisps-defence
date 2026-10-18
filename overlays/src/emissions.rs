use bevy::{
    input::common_conditions::input_just_released,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
};

use game_core::prelude::{MapInfo, ZDepth};
use grids::{
    emissions::{EmissionsGrid, EmissionsType},
    prelude::GridVersion,
};
use states::prelude::MapLoadingStage;
use visuals::{canvas::MapCanvasBundle, specialize_with_shared_gpu};

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
}
impl Default for EmissionsOverlayConfig {
    fn default() -> Self {
        Self {
            is_overlay_globally_enabled: false,
            emissions_type: EmissionsType::Energy,
        }
    }
}
impl EmissionsOverlayConfig {
    fn on_config_change_system(
        overlay_config: Res<EmissionsOverlayConfig>,
        mut overlay_state: ResMut<NextState<EmissionsOverlayState>>,
    ) {
        let shown = overlay_config.is_overlay_globally_enabled;
        overlay_state.set_if_different(if shown { EmissionsOverlayState::Show } else { EmissionsOverlayState::Hide });
    }
}

/// Field order and types mirror `EmissionsUniforms` in `assets/shaders/overlays/emissions_map.wesl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Default)]
struct EmissionsUniformData {
    min_value: f32,
    max_value: f32,
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
struct EmissionsOverlayMaterial {
    #[uniform(0)]
    pub uniforms: EmissionsUniformData,
}
impl Material2d for EmissionsOverlayMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/overlays/emissions_map.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    specialize_with_shared_gpu!();
}

#[derive(Component, Default)]
#[require(ZDepth::OverlayEmissions)]
pub struct EmissionsOverlay {
    /// Grid version used to calculate normalization; `None` before the first refresh.
    shown_version: Option<GridVersion>,
}
impl EmissionsOverlay {
    fn create(
        mut commands: Commands,
        map_info: Res<MapInfo>,
        overlay_state: Res<State<EmissionsOverlayState>>,
        mut materials: ResMut<Assets<EmissionsOverlayMaterial>>,
    ) {
        commands.despawn_all::<With<EmissionsOverlay>>();

        let material = materials.add(EmissionsOverlayMaterial::default());
        commands.spawn((
            MapCanvasBundle::new(material, &map_info),
            EmissionsOverlay::default(),
            if *overlay_state.get() == EmissionsOverlayState::Show { Visibility::Inherited } else { Visibility::Hidden },
        ));
    }
}

fn refresh_display_system(
    mut materials: ResMut<Assets<EmissionsOverlayMaterial>>,
    emissions_grid: Res<EmissionsGrid>,
    overlay_config: Res<EmissionsOverlayConfig>,
    overlay: Single<(&MeshMaterial2d<EmissionsOverlayMaterial>, &mut EmissionsOverlay)>,
) {
    let (overlay_material, mut overlay) = overlay.into_inner();
    let current_version = match overlay_config.emissions_type {
        EmissionsType::Energy => emissions_grid.version.energy,
    };
    if overlay.shown_version == Some(current_version) { return; }
    overlay.shown_version = Some(current_version);

    let mut overlay_material = materials.get_mut(overlay_material).unwrap();

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

    overlay_material.uniforms = EmissionsUniformData { min_value, max_value };
}
