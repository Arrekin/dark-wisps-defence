use bevy::prelude::*;

use game_core::prelude::{MapBound, ZDepth};
use visuals::prelude::BuilderWispAttackEffect;

use crate::common::AnimationController;

pub struct WispAttackEffectPlugin;
impl Plugin for WispAttackEffectPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<WispAttackEffectAtlas>()
            .add_systems(Update, (
                remove_effects_system,
            ))
            .add_observer(on_builder_add_spawn_wisp_attack_effect);
    }
}

#[derive(Resource)]
pub(crate) struct WispAttackEffectAtlas {
    pub atlas_handle: Handle<TextureAtlasLayout>,
    pub texture_handle: Handle<Image>,
}
impl FromWorld for WispAttackEffectAtlas {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        let texture_handle = asset_server.load("effects/wisp_attack.png");

        let texture_atlas = TextureAtlasLayout::from_grid(
            UVec2::new(192, 192),
            5,
            2,
            None,
            None,
        );
        let mut texture_atlases = world.resource_mut::<Assets<TextureAtlasLayout>>();
        let texture_atlas_handle = texture_atlases.add(texture_atlas);
        WispAttackEffectAtlas {
            atlas_handle: texture_atlas_handle,
            texture_handle,
        }
    }
}

#[derive(Component)]
#[require(MapBound, ZDepth::GROUND_EFFECT)]
pub(crate) struct WispAttackEffect;

fn on_builder_add_spawn_wisp_attack_effect(
    trigger: On<Add, BuilderWispAttackEffect>,
    mut commands: Commands,
    explosion_atlas: Res<WispAttackEffectAtlas>,
    builders: Query<&BuilderWispAttackEffect>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderWispAttackEffect>()
        .insert((
            Sprite {
                image: explosion_atlas.texture_handle.clone(),
                texture_atlas: Some(TextureAtlas {
                    layout: explosion_atlas.atlas_handle.clone(),
                    index: 0,
                    ..default()
                }),
                ..default()
            },
            Transform {
                translation: builder.0.extend(0.),
                scale: Vec3::new(0.5, 0.5, 1.0),
                ..Default::default()
            },
            AnimationController::new(0, 9, 0.025, false),
            WispAttackEffect,
        ));
}

fn remove_effects_system(
    mut commands: Commands,
    explosions: Query<(Entity, &AnimationController), With<WispAttackEffect>>,
) {
    for (explosion_entity, animation_controller) in &explosions {
        if animation_controller.has_finished {
            commands.entity(explosion_entity).despawn();
        }
    }
}


