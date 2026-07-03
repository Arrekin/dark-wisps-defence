use bevy::prelude::*;

use game_core::prelude::{GridImprint, MapBound, Z_GROUND_EFFECT};
use visuals::prelude::BuilderExplosion;

use crate::common::AnimationController;

pub struct ExplosionPlugin;
impl Plugin for ExplosionPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ExplosionAtlas>()
            .add_systems(Update, (
                remove_explosions_system,
            ))
            .add_observer(on_builder_add_spawn_explosion);
    }
}

#[derive(Resource)]
pub(crate) struct ExplosionAtlas {
    pub atlas_handle: Handle<TextureAtlasLayout>,
    pub texture_handle: Handle<Image>,
}
impl FromWorld for ExplosionAtlas {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        let texture_handle = asset_server.load("effects/explosion.png");

        let texture_atlas = TextureAtlasLayout::from_grid(
            UVec2::new(16, 18),
            4,
            1,
            None,
            None,
        );
        let mut texture_atlases = world.resource_mut::<Assets<TextureAtlasLayout>>();
        let texture_atlas_handle = texture_atlases.add(texture_atlas);
        ExplosionAtlas {
            atlas_handle: texture_atlas_handle,
            texture_handle,
        }
    }
}

#[derive(Component)]
#[require(MapBound)]
pub(crate) struct Explosion;

fn on_builder_add_spawn_explosion(
    trigger: On<Add, BuilderExplosion>,
    mut commands: Commands,
    explosion_atlas: Res<ExplosionAtlas>,
    builders: Query<&BuilderExplosion>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderExplosion>()
        .insert((
            Sprite {
                image: explosion_atlas.texture_handle.clone(),
                texture_atlas: Some(TextureAtlas {
                    layout: explosion_atlas.atlas_handle.clone(),
                    index: 0,
                    ..Default::default()
                }),
                custom_size: Some(GridImprint::default().world_size()), // Adjust to CELL_SIZE
                ..default()
            },
            Transform {
                translation: builder.0.to_world_position_centered(GridImprint::Rectangle { width: 1, height: 1 }).extend(Z_GROUND_EFFECT),
                ..default()
            },
            AnimationController::new(0, 3, 0.1, false),
            Explosion,
        ));
}

fn remove_explosions_system(
    mut commands: Commands,
    explosions: Query<(Entity, &AnimationController), With<Explosion>>,
) {
    for (explosion_entity, animation_controller) in &explosions {
        if animation_controller.has_finished {
            commands.entity(explosion_entity).despawn();
        }
    }
}
