//! # Cannonball
//!
//! Cannon tower projectile. On landing, detonates a 1-step (3x3) blast via [`BlastHitManager`],
//! spawning an explosion and dealing full damage in every hit cell.

use bevy::prelude::*;

use alteration::modifiers::prelude::AttackDamage;
use game_core::prelude::{CELL_SIZE, DamageMessage, GridCoords, OptionalCommands, Property};
use logging::prelude::*;
use persistence::{prelude::*, rusqlite};
use session::GameClock;
use states::prelude::{GameState, MapLoadingStage};
use visuals::{drop_shadow::DropShadow, prelude::BuilderExplosion};
use weaponry::{lob::{Lob, LobInFlight}, prelude::*};

use crate::{blast::BlastHitManager, lob::{LoadLobExtension, LobBody, SaveLobExtension, advance_lobs}};

pub(crate) struct CannonballPlugin;
impl Plugin for CannonballPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(Update, resolve_cannonball_hits.after(advance_lobs).run_if(in_state(GameState::Running)))
            .add_observer(on_builder_add_spawn_cannonball)
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
    cannonballs: Query<(Entity, &Lob, &AttackDamage), With<Cannonball>>,
    mut save: SaveWriter,
) {
    if cannonballs.is_empty() { return; }

    #[debug_dev("Saving {} cannonballs", rows.len())]
    let rows: Vec<(u32, f32, Lob)> = cannonballs
        .iter()
        .map(|(entity, lob, damage)| (entity.index_u32(), damage.get(), *lob))
        .collect();
    save.submit(move |ctx| {
        for (id, damage, lob) in rows {
            ctx.register_entity(id)?;
            ctx.tx.execute(
                "INSERT OR REPLACE INTO cannonballs (id, damage) VALUES (?1, ?2)",
                rusqlite::params![id, damage],
            )?;
            ctx.save_lob(id, &lob)?;
        }
        Ok(())
    });
}

fn load_cannonballs(ctx: &mut LoadContext) -> LoadResult {
    ctx.for_each_entity("SELECT id, damage FROM cannonballs", |ctx, old_id, entity, row| {
        let damage: f32 = row.get(1)?;
        let builder = BuilderCannonball::new(ctx.lob(old_id)?, AttackDamage::new(damage));
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
            builder.lob,
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
                DropShadow::new(CANNONBALL_SIZE, builder.lob.target()),
            ],
        ));
}

fn resolve_cannonball_hits(
    mut commands: Commands,
    mut damage_messages: MessageWriter<DamageMessage>,
    clock: Res<GameClock>,
    blast_hit_manager: BlastHitManager,
    cannonballs: Query<(Entity, &Lob, &AttackDamage), (With<Cannonball>, With<LobInFlight>)>,
) {
    for (entity, lob, attack_damage) in cannonballs.iter() {
        if !lob.is_landed(clock.now()) { continue; }
        commands.entity(entity).despawn();

        let Some(blast) = blast_hit_manager.go_off(GridCoords::from_world_position(lob.target()), BLAST_REACH) else { continue; };
        for hit in blast.hits() {
            commands.spawn(BuilderExplosion(hit.position));
            for &wisp in hit.wisps {
                damage_messages.write(DamageMessage { target: wisp, amount: attack_damage.get() });
            }
        }
    }
}
