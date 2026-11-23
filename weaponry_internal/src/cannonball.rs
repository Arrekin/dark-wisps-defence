//! # Cannonball
//!
//! Cannon tower projectile. On landing, detonates a 1-step (3x3) blast via [`BlastHitManager`],
//! spawning an explosion and dealing full damage in every hit cell.

use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{CELL_SIZE, DamageMessage, GridCoords, OptionalCommands, Property};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use states::prelude::MapLoadingStage;
use visuals::{drop_shadow::DropShadow, prelude::BuilderExplosion};
use weaponry::{lob::{Lob, LobArc, LobArcProgress, LobLanded}, prelude::*};

use crate::{blast::BlastHitManager, lob::{LoadLobExtension, LobBody, SaveLobExtension}};

pub(crate) struct CannonballPlugin;
impl Plugin for CannonballPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(on_builder_add_spawn_cannonball)
            .add_observer(on_lob_landed_detonate_cannonball)
            .add_systems(CollectSave, collect_cannonballs)
            .register_loader(MapLoadingStage::SpawnMapElements, "cannonballs", load_cannonballs);
    }
}

const CANNONBALL_BASE_IMAGE: &str = "projectiles/cannonball.png";
/// Body and ground-level shadow diameter, in world units.
const CANNONBALL_SIZE: f32 = CELL_SIZE / 2.;

// Blast area, in steps from the landing cell
const BLAST_REACH: u8 = 1;

#[log_tags(Tag::GameSave)]
fn collect_cannonballs(
    cannonballs: Query<(Entity, &LobArc, &LobArcProgress, &AttackDamage), With<Cannonball>>,
    mut save: SaveWriter,
) {
    if cannonballs.is_empty() { return; }

    #[debug_dev("Saving {} cannonballs", rows.len())]
    let rows: Vec<(u32, f32, LobArc, LobArcProgress)> = cannonballs
        .iter()
        .map(|(entity, arc, progress, damage)| (entity.index_u32(), damage.get(), *arc, *progress))
        .collect();
    save.submit(move |ctx| {
        for (id, damage, arc, progress) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO cannonballs (id, damage) VALUES (?1, ?2)",
                rusqlite::params![id, damage],
            )?;
            ctx.save_lob(id, arc, progress)?;
        }
        Ok(())
    });
}

fn load_cannonballs(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, damage FROM cannonballs", |ctx, old_id, entity, row| {
        let damage: f32 = row.get(1)?;
        let (arc, progress) = ctx.lob(old_id)?;
        let builder = BuilderCannonball::new(arc, AttackDamage::new(damage)).with_progress(progress);
        ctx.insert(entity, builder);
        Ok(())
    })
}

fn on_builder_add_spawn_cannonball(
    trigger: On<Add<BuilderCannonball>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    builders: Query<&BuilderCannonball>,
) {
    let entity = trigger.entity;
    let Ok(builder) = builders.get(entity) else { return; };

    commands.entity(entity)
        .remove::<BuilderCannonball>()
        .insert_some(builder.shooter.map(ProjectileOf))
        .insert((
            Cannonball,
            builder.arc,
            builder.progress,
            Lob,
            builder.damage,
            children![
                (
                    LobBody,
                    Sprite {
                        image: asset_server.load(CANNONBALL_BASE_IMAGE),
                        custom_size: Some(Vec2::splat(CANNONBALL_SIZE)),
                        ..default()
                    },
                ),
                DropShadow::new(CANNONBALL_SIZE, builder.arc.target),
            ],
        ));
}

fn on_lob_landed_detonate_cannonball(
    trigger: On<LobLanded>,
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    blast_hit_manager: BlastHitManager,
    cannonballs: Query<(&LobArc, &AttackDamage), With<Cannonball>>,
) {
    let entity = trigger.0;
    let Ok((arc, attack_damage)) = cannonballs.get(entity) else { return; };
    commands.entity(entity).despawn();

    let Some(blast) = blast_hit_manager.go_off(GridCoords::from_world_position(arc.target), BLAST_REACH) else { return; };
    for hit in blast.hits() {
        commands.spawn(BuilderExplosion(hit.position));
        for &wisp in hit.wisps {
            damage_messages.write(DamageMessage { target: wisp, amount: attack_damage.get() });
        }
    }
}
