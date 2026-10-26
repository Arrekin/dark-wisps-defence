//! Camera and mouse systems.
//!
//! - Main game camera with zoom and movement controls
//! - `CameraOf` / `OwnedCameras` relationship for automatic camera lifecycle management
//! - `CameraAutoFollowEntity` for automatically following an entity
//! - `MouseInfo` resource tracking screen/world/grid cursor position

use bevy::{
    asset::RenderAssetUsages,
    camera::RenderTarget,
    input::mouse::MouseWheel,
    picking::{PickingSystems, hover::PickingInteraction},
    post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter},
    prelude::*,
    render::{
        render_resource::{TextureDimension, TextureFormat, TextureUsages},
        view::DebandDither,
    },
    ui::ComputedNode,
    window::PrimaryWindow,
};
use bevy_egui::PrimaryEguiContext;

use game_core::prelude::{CELL_SIZE, GridCoords, OptionalCommands};
use visuals::render_schedule::BandedCamera;

const ZOOM_MIN: f32 = 1.;
const ZOOM_MAX: f32 = 4.;
const ZOOM_SPEED: f32 = 20.;
const SLIDE_SPEED: f32 = CELL_SIZE * 30.;

pub struct ViewportPlugin;
impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<MouseInfo>()
            .add_systems(Startup, spawn_main_camera)
            .add_systems(PreUpdate, update_mouse_info_system.after(PickingSystems::Hover))
            .add_systems(Update, (
                camera_zoom,
                camera_movement,
            ))
            .add_systems(PostUpdate, CameraAutoFollowEntity::update)
            .add_observer(BuilderPreviewCamera::on_builder_add_spawn_preview_camera);
    }
}

#[derive(Component)]
pub struct MainCamera;

fn spawn_main_camera(mut commands: Commands) {
    commands.spawn((
        BandedCamera,
        Transform::from_xyz(500., 500., 0.),
        // Only colour above 1.0 blooms, added on top of the untouched image.
        Bloom {
            prefilter: BloomPrefilter { threshold: 1.0, threshold_softness: 0.0 },
            composite_mode: BloomCompositeMode::Additive,
            intensity: 0.1,
            ..Bloom::OLD_SCHOOL
        },
        // Breaks up banding in dark gradients when the HDR image is written to the 8-bit output.
        DebandDither::Enabled,
        MainCamera,
        // Pin egui's primary context to the main window camera. Requires
        // `EguiGlobalSettings::auto_create_primary_context = false` (set in EditorPlugin).
        // `PrimaryEguiContext` requires `EguiContext` and, via its on_insert hook, wires up
        // the `EguiPrimaryContextPass` multipass schedule for this entity automatically.
        PrimaryEguiContext,
    ));
}

fn camera_zoom(
    mut mouse_wheel_events: MessageReader<MouseWheel>,
    mouse_info: Res<MouseInfo>,
    time: Res<Time>,
    camera: Single<&mut Projection, With<MainCamera>>,
) {
    if mouse_info.is_over_ui { return; }
    let mut scroll = 0.0;
    for event in mouse_wheel_events.read() {
        scroll += event.y;
    }

    let mut projection = camera.into_inner();
    match &mut *projection {
        Projection::Orthographic(orthographic) => {
            let mut log_scale = orthographic.scale.ln();
            log_scale -= scroll * ZOOM_SPEED * time.delta_secs();
            orthographic.scale = log_scale.exp().clamp(ZOOM_MIN, ZOOM_MAX);
        }
        _ => panic!("Only orthographic projections are supported for zooming"),
    }
}

fn camera_movement(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    camera: Single<&mut Transform, With<MainCamera>>,
) {
    let mut translation = Vec3::ZERO;
    if keyboard_input.pressed(KeyCode::KeyI) {
        translation.y += 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyK) {
        translation.y -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyJ) {
        translation.x -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyL) {
        translation.x += 1.0;
    }
    let mut transform = camera.into_inner();
    transform.translation += SLIDE_SPEED * time.delta_secs() * translation;
}

////////////////////////////////////////////
//     Camera Ownership Relationship
////////////////////////////////////////////

/// Relationship component: marks a camera as belonging to another entity.
///
/// When an entity with `OwnedCameras` is despawned, all cameras with `CameraOf`
/// pointing to it are automatically despawned via Bevy's `linked_spawn` feature.
///
/// # Usage
///
/// ```rust,ignore
/// // Spawn a camera owned by a UI node
/// let camera = commands.spawn((
///     Camera2d::default(),
///     Camera { target: RenderTarget::Image(image.into()), ..default() },
///     CameraOf(ui_node_entity),
/// )).id();
///
/// // Or use the builder for the common preview camera setup:
/// let camera = commands.spawn(BuilderPreviewCamera::new(ui_node_entity, position, scale)).id();
/// ```
#[derive(Component)]
#[relationship(relationship_target = OwnedCameras)]
pub struct CameraOf(pub Entity);

/// Cameras owned by this entity, despawned with it.
#[derive(Component, Default)]
#[relationship_target(relationship = CameraOf, linked_spawn)]
pub struct OwnedCameras(Vec<Entity>);

/// Builder for spawning preview cameras with automatic lifecycle management.
///
/// Preview cameras render to an off-screen image that can be displayed in UI
/// via `ViewportNode`, for picture-in-picture views of the map.
///
/// # Example
///
/// ```rust,ignore
/// // Spawn a preview camera owned by a tooltip
/// let camera = commands.spawn(BuilderPreviewCamera::new(
///     tooltip_entity,
///     world_position,
///     2.5, // zoom level
/// )).id();
///
/// // Connect it to a UI node for display
/// commands.entity(tooltip_entity).insert(ViewportNode::new(camera));
/// ```
#[derive(Component)]
pub struct BuilderPreviewCamera {
    /// The entity that owns this camera. When the owner is despawned, the camera is too.
    pub owner: Entity,
    /// World position the camera should look at.
    pub position: Vec2,
    /// Orthographic scale (zoom level). Higher values = more zoomed out.
    pub scale: f32,
    /// Entity the camera follows, if any.
    pub auto_follow_entity: Option<CameraAutoFollowEntity>,
}
impl BuilderPreviewCamera {
    pub fn new(owner: Entity, position: Vec2, scale: f32) -> Self {
        Self { owner, position, scale, auto_follow_entity: None }
    }

    pub fn with_auto_follow_entity(mut self, entity: Entity) -> Self {
        self.auto_follow_entity = Some(CameraAutoFollowEntity(entity));
        self
    }

    fn on_builder_add_spawn_preview_camera(
        trigger: On<Add<BuilderPreviewCamera>>,
        mut commands: Commands,
        mut images: ResMut<Assets<Image>>,
        builders: Query<&BuilderPreviewCamera>,
    ) {
        let Ok(builder) = builders.get(trigger.entity) else { return; };

        // Create render target image - size is determined by the ViewportNode
        let mut image = Image::new_uninit(
            default(),
            TextureDimension::D2,
            TextureFormat::Bgra8UnormSrgb,
            RenderAssetUsages::all(),
        );
        image.texture_descriptor.usage =
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        let image_handle = images.add(image);

        commands.entity(trigger.entity)
            .remove::<BuilderPreviewCamera>()
            .insert((
                BandedCamera,
                Camera {
                    order: -1, // Render before main camera
                    ..default()
                },
                RenderTarget::Image(image_handle.into()),
                DebandDither::Enabled,
                Projection::Orthographic(OrthographicProjection {
                    scale: builder.scale,
                    ..OrthographicProjection::default_2d()
                }),
                Transform::from_xyz(builder.position.x, builder.position.y, 0.),
                CameraOf(builder.owner),
            ))
            .insert_some(builder.auto_follow_entity);
    }
}

/// Keeps a camera centred on an entity's position.
#[derive(Component, Clone, Copy)]
pub struct CameraAutoFollowEntity(pub Entity);
impl CameraAutoFollowEntity {
    fn update(
        mut cameras: Query<(&CameraAutoFollowEntity, &mut Transform)>,
        targets: Query<&Transform, Without<CameraAutoFollowEntity>>,
    ) {
        for (auto_follow, mut camera_transform) in cameras.iter_mut() {
            if let Ok(target_transform) = targets.get(auto_follow.0) {
                camera_transform.translation.x = target_transform.translation.x;
                camera_transform.translation.y = target_transform.translation.y;
            }
        }
    }
}

////////////////////////////////////////////
//              Mouse
////////////////////////////////////////////

#[derive(Resource, Default)]
pub struct MouseInfo {
    pub screen_position: Vec2,
    pub world_position: Vec2,
    /// Not guaranteed to be within the map bounds.
    pub grid_coords: GridCoords,
    pub is_over_ui: bool,
}

fn update_mouse_info_system(
    mut mouse_info: ResMut<MouseInfo>,
    ui_nodes: Query<&PickingInteraction, With<ComputedNode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    let (camera, camera_transform) = camera.into_inner();

    if let Some(screen_position) = window.into_inner().cursor_position() {
        let world_position = camera.viewport_to_world_2d(camera_transform, screen_position).unwrap();
        let grid_coords = GridCoords::from_world_position(world_position);
        mouse_info.screen_position = screen_position;
        mouse_info.world_position = world_position;
        mouse_info.grid_coords = grid_coords;
    }
    if !ui_nodes.is_empty() {
        mouse_info.is_over_ui = ui_nodes.iter().any(|interaction| !matches!(interaction, PickingInteraction::None));
    }
}
