//! The style picker shown while placing walls: a row of swatches, one per style,
//! framed when selected. Lives only for the length of a wall placement session.

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    ui_render::ui_material::UiMaterial,
};

use game_core::prelude::CELL_SIZE;
use grids::placement::{BeginPlacing, GridObjectPlacer, GridPlacerOverridePropertyRequest, PlacementStyle, StopPlacing};
use map_objects::prelude::Wall;
use map_objects::wall_style::{WallStyle, WallStyleKey, WallStyles};
use visuals::prelude::ShaderLibraryAppExt;

pub(crate) struct WallEditorUiPlugin;
impl Plugin for WallEditorUiPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_shader_library("shaders/wall_style.wgsl")
            .add_plugins(UiMaterialPlugin::<WallSwatchMaterial>::default())
            .add_observer(WallSwatch::on_add_construct_wall_swatch)
            .add_observer(GridPlacerUiForWall::on_add_construct_grid_placer_ui)
            .add_observer(GridPlacerUiForWall::on_begin_placing_spawn_grid_placer_ui)
            .add_observer(GridPlacerUiForWall::on_stop_placing_despawn_grid_placer_ui)
            ;
    }
}

////////////////////////////////////////////
//        Swatch
////////////////////////////////////////////

/// The swatch shader gets its layers from `dwd::wall_style`, which reads no bindings, so the
/// only thing bound here is the style being previewed.
#[derive(Asset, AsBindGroup, TypePath, Clone, Copy, Debug)]
struct WallSwatchMaterial {
    #[uniform(0)]
    style: WallStyle,
}
impl UiMaterial for WallSwatchMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wall_swatch.wgsl".into()
    }
}

/// Draws one style as a single wall cell, at the size that cell has on the map.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct WallSwatch(pub WallStyleKey);
impl WallSwatch {
    /// World pixels per cell. Matches `CELL_SIZE` in assets/shaders/wall_swatch.wgsl.
    const SIZE: f32 = CELL_SIZE;

    fn on_add_construct_wall_swatch(
        trigger: On<Add, WallSwatch>,
        mut commands: Commands,
        swatches: Query<&WallSwatch>,
        styles: Res<WallStyles>,
        mut materials: ResMut<Assets<WallSwatchMaterial>>,
    ) {
        let entity = trigger.entity;
        let Ok(swatch) = swatches.get(entity) else { return; };
        let Some(entry) = styles.entries.get(swatch.0.0 as usize) else { return; };

        commands.entity(entity).insert((
            Node {
                width: Val::Px(Self::SIZE),
                height: Val::Px(Self::SIZE),
                ..default()
            },
            MaterialNode(materials.add(WallSwatchMaterial { style: entry.style })),
        ));
    }
}

////////////////////////////////////////////
//        Style Picker
////////////////////////////////////////////

/// Frame around a swatch, marking which style the placer will build with.
const SWATCH_FRAME_SELECTED: Color = Color::srgb(0.16, 0.78, 1.00);
const SWATCH_FRAME_IDLE: Color = Color::srgb(0.15, 0.15, 0.18);

fn swatch_frame_color(selected: bool) -> Color {
    if selected { SWATCH_FRAME_SELECTED } else { SWATCH_FRAME_IDLE }
}

/// Style picker, alive for the length of a wall placement session. It stores no selection:
/// the placer's [`PlacementStyle`] holds it, and the frames are drawn from it.
#[derive(Component)]
pub(crate) struct GridPlacerUiForWall;
impl GridPlacerUiForWall {
    fn on_add_construct_grid_placer_ui(
        trigger: On<Add, GridPlacerUiForWall>,
        mut commands: Commands,
        styles: Res<WallStyles>,
        placer: Single<&PlacementStyle, With<GridObjectPlacer>>,
    ) {
        let selected = WallStyleKey::from(*placer.into_inner());
        commands.entity(trigger.entity)
            .insert(Node {
                position_type: PositionType::Absolute,
                top: Val::Px(5.0),
                left: Val::Percent(50.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|parent| {
                for (index, entry) in styles.entries.iter().enumerate() {
                    let key = WallStyleKey(index as u32);
                    parent.spawn((
                        WallStyleButton(key),
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(2.0),
                            padding: UiRect::all(Val::Px(2.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor::from(swatch_frame_color(key == selected)),
                        BackgroundColor(Color::BLACK),
                        children![
                            WallSwatch(key),
                            (Text::new(entry.name.clone()), TextFont::default().with_font_size(10.0)),
                        ],
                    ))
                    .observe(WallStyleButton::on_click_select_style);
                }
            });
    }

    fn on_begin_placing_spawn_grid_placer_ui(
        _trigger: On<BeginPlacing<Wall>>,
        mut commands: Commands,
    ) {
        commands.spawn(GridPlacerUiForWall);
    }

    fn on_stop_placing_despawn_grid_placer_ui(
        _trigger: On<StopPlacing>,
        mut commands: Commands,
        existing_ui: Single<Entity, With<GridPlacerUiForWall>>,
    ) {
        commands.entity(existing_ui.into_inner()).despawn();
    }
}

/// One selectable style: a swatch and its name, framed when selected. The frame is on this
/// node so it does not cover the cell being previewed.
#[derive(Component, Clone, Copy)]
#[require(Button, Pickable)]
pub(crate) struct WallStyleButton(pub WallStyleKey);
impl WallStyleButton {
    fn on_click_select_style(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        mut buttons: Query<(&WallStyleButton, &mut BorderColor)>,
    ) {
        let Some(selected) = buttons.get(trigger.entity).ok().map(|(button, _)| button.0) else { return; };
        for (button, mut frame) in buttons.iter_mut() {
            *frame = BorderColor::from(swatch_frame_color(button.0 == selected));
        }
        commands.trigger(GridPlacerOverridePropertyRequest::OverrideStyle(selected.0));
    }
}
