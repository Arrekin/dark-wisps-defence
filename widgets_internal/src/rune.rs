//! Rune construction and flight.
//!
//! The shader derives fading from rune age, so the material is created only at spawn.

use bevy::prelude::*;

use widgets::prelude::{BuilderRune, Rune, RuneMaterial};

pub(crate) struct RunePlugin;
impl Plugin for RunePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(UiMaterialPlugin::<RuneMaterial>::default())
            .add_observer(on_builder_add_spawn_rune)
            .add_systems(Update, advance_runes);
    }
}

fn on_builder_add_spawn_rune(
    trigger: On<Add, BuilderRune>,
    mut commands: Commands,
    time: Res<Time>,
    builders: Query<&BuilderRune>,
    mut materials: ResMut<Assets<RuneMaterial>>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return };

    let started_at = time.elapsed_secs();
    let material = materials.add(RuneMaterial {
        color: builder.color.to_linear(),
        params: builder.params(started_at),
        life: builder.life(),
    });
    // A flight is described between centres, but `left`/`top` place a node's corner.
    let corner = builder.flight.from - Vec2::splat(builder.size * 0.5);

    commands.entity(entity)
        .remove::<BuilderRune>()
        .insert((
            MaterialNode(material),
            Rune { flight: builder.flight, started_at, size: builder.size },
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(corner.x),
                top: Val::Px(corner.y),
                width: Val::Px(builder.size),
                height: Val::Px(builder.size),
                ..default()
            },
            // A rune crossing a panel must not intercept clicks meant for what is under it.
            Pickable::IGNORE,
        ));
}

/// Moves each rune and despawns it on arrival.
fn advance_runes(
    mut commands: Commands,
    time: Res<Time>,
    mut runes: Query<(Entity, &Rune, &mut Node)>,
) {
    let now = time.elapsed_secs();
    for (entity, rune, mut node) in runes.iter_mut() {
        if rune.progress(now) >= 1.0 {
            commands.entity(entity).try_despawn();
            continue;
        }

        let corner = rune.position(now) - Vec2::splat(rune.size * 0.5);
        node.left = Val::Px(corner.x);
        node.top = Val::Px(corner.y);
    }
}
