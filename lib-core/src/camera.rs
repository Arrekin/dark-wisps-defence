//! Camera systems and utilities.
//!
//! This module provides:
//! - Main game camera with zoom and movement controls
//! - `CameraOf` / `OwnedCameras` relationship for automatic camera lifecycle management
//! - `CameraAutoFollowEntity` for automatically following an entity

use bevy::{post_process::bloom::Bloom, input::mouse::MouseWheel};
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::render::render_resource::{TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::Hdr;
use crate::lib_prelude::*;

const ZOOM_MIN: f32 = 1.;
const ZOOM_MAX: f32 = 4.;
const ZOOM_SPEED: f32 = 20.;
const SLIDE_SPEED: f32 = CELL_SIZE * 30.;

pub struct CameraPlugin;
impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Startup, startup)
            .add_systems(Update, (
                camera_zoom,
                camera_movement,
            ))
            .add_systems(PostUpdate, CameraAutoFollowEntity::update)
            .add_observer(BuilderPreviewCamera::on_add)
            ;
    }
}

#[derive(Component)]
pub struct MainCamera;

// Post process effect auto-hook themselves to cameras with this component
#[derive(Component)]
pub struct PostProcessCamera;

fn startup(mut commands: Commands) {
    commands.spawn((
        Camera2d::default(),
        Transform::from_xyz(500., 500., 0.),
        Bloom { high_pass_frequency: 0.5, ..default() },
        MainCamera,
        PostProcessCamera,
    ));
}

fn camera_zoom(
    time: Res<Time>,
    mouse_info: Res<MouseInfo>,
    mut mouse_wheel_events: MessageReader<MouseWheel>,
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

    // 'i' moves the camera up
    if keyboard_input.pressed(KeyCode::KeyI) {
        translation.y += 1.0;
    }

    // 'k' moves the camera down
    if keyboard_input.pressed(KeyCode::KeyK) {
        translation.y -= 1.0;
    }

    // 'j' moves the camera to the left
    if keyboard_input.pressed(KeyCode::KeyJ) {
        translation.x -= 1.0;
    }

    // 'l' moves the camera to the right
    if keyboard_input.pressed(KeyCode::KeyL) {
        translation.x += 1.0;
    }

    // Apply the camera movement
    let mut transform = camera.into_inner();
    transform.translation += SLIDE_SPEED * time.delta_secs() * translation;
}

////////////////////////////////////////////
////     Camera Ownership Relationship  ////
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
/// // Or use the builder for common preview camera setup:
/// let camera = PreviewCamera::spawn(&mut commands, &mut images, ui_node_entity, position, scale);
/// ```
#[derive(Component)]
#[relationship(relationship_target = OwnedCameras)]
pub struct CameraOf(pub Entity);

/// Relationship target: automatically tracks all cameras owned by this entity.
///
/// This component is automatically added when a `CameraOf(this_entity)` is spawned.
/// The `linked_spawn` attribute ensures that when this entity is despawned,
/// all related cameras are automatically despawned too.
#[derive(Component, Default)]
#[relationship_target(relationship = CameraOf, linked_spawn)]
pub struct OwnedCameras(Vec<Entity>);

/// Builder for spawning preview cameras with automatic lifecycle management.
///
/// Preview cameras render to an off-screen image that can be displayed in UI
/// via `ViewportNode`. They are commonly used for:
/// - Drone slot tooltips (showing drone on map)
/// - Target selection previews
/// - Any "picture-in-picture" style preview
///
/// # Lifecycle
///
/// The camera is automatically despawned when its owner entity is despawned,
/// thanks to the `CameraOf` / `OwnedCameras` relationship.
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
    /// If Entity is provided, adds CameraAutoFollowEntity component to the camera.
    pub auto_follow_entity: Option<Entity>,
}
impl BuilderPreviewCamera {
    /// Creates a new preview camera builder.
    ///
    /// # Arguments
    ///
    /// * `owner` - Entity that owns this camera (camera despawns when owner does)
    /// * `position` - World position the camera should look at
    /// * `scale` - Orthographic scale (zoom level, higher = zoomed out)
    pub fn new(owner: Entity, position: Vec2, scale: f32) -> Self {
        Self { owner, position, scale, auto_follow_entity: None }
    }

    /// Adds an entity to follow with the camera.
    /// 
    /// # Arguments
    ///
    /// * `entity` - Entity to follow with the camera
    pub fn with_auto_follow_entity(mut self, entity: Entity) -> Self {
        self.auto_follow_entity = Some(entity);
        self
    }

    /// Observer that builds the camera when `BuilderPreviewCamera` is added.
    ///
    /// Creates a render target image and configures the camera with:
    /// - Off-screen rendering to an image texture
    /// - Orthographic projection at the specified scale
    /// - Automatic lifecycle via `CameraOf` relationship
    /// - If auto_follow_entity is provided, adds CameraAutoFollowEntity component to the camera
    fn on_add(
        trigger: On<Add, BuilderPreviewCamera>,
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

        let mut entity_commands = commands.entity(trigger.entity);

        // Spawn camera with linked ownership
        entity_commands
            .remove::<BuilderPreviewCamera>()
            .insert((
                Camera2d::default(),
                Camera {
                    order: -1, // Render before main camera
                    ..default()
                },
                PostProcessCamera,
                RenderTarget::Image(image_handle.into()),
                Hdr,
                Projection::Orthographic(OrthographicProjection {
                    near: -1000.,
                    far: 1000.,
                    scale: builder.scale,
                    ..OrthographicProjection::default_2d()
                }),
                Transform::from_xyz(builder.position.x, builder.position.y, 0.),
                CameraOf(builder.owner),
            ));

        if let Some(entity) = builder.auto_follow_entity {
            entity_commands.insert(CameraAutoFollowEntity(entity));
        }
    }
}

/// Camera that follows a drone for tooltip preview
#[derive(Component)]
pub struct CameraAutoFollowEntity(pub Entity);
impl CameraAutoFollowEntity {
    fn update(
        mut cameras: Query<(&CameraAutoFollowEntity, &mut Transform)>,
        targets: Query<&Transform, Without<CameraAutoFollowEntity>>,
    ) {
        for (auto_follow, mut cam_transform) in cameras.iter_mut() {
            if let Ok(drone_transform) = targets.get(auto_follow.0) {
                cam_transform.translation.x = drone_transform.translation.x;
                cam_transform.translation.y = drone_transform.translation.y;
            }
        }
    }
}