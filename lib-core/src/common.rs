use crate::lib_prelude::*;

pub mod common_prelude {
    pub use super::*;
}

pub struct CommonPlugin;
impl Plugin for CommonPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_db_loader::<MapInfo>(MapLoadingStage::LoadMapInfo)
            .register_db_saver(MapInfo::on_game_save)
            .add_systems(Update, (
                ColorPulsation::pulsate_sprites_system,
            ))
            .add_observer(ZDepth::on_insert)
            .add_observer(ColorPulsation::on_remove)
            ;
    }
}

pub trait SSS: Send + Sync + 'static {}

// Simple property trait for single value objects. Useful in generic contexts.
pub trait Property {
    fn get(&self) -> f32;
    fn set(&mut self, value: f32);
    fn new(value: f32) -> Self;
}

/// Global identifier for all placeable objects on the map.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MapObject {
    Building(BuildingType),
    Wall,
    DarkOre,
    QuantumField,
    Wisp(WispType),
}

// Event that carries string-identified or constant game events
#[derive(Event)]
pub struct DynamicGameEvent(pub String);
impl DynamicGameEvent {
    pub fn game_started() -> Self { DynamicGameEvent("game-started".to_string()) }
}

// Component for entities that are bound to the map and shall be removed on its change
#[derive(Component, Default)]
pub struct MapBound;

#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32, // A helper, source of truth is in MaxHealth component
}
impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }
    pub fn get_current(&self) -> f32 {
        self.current
    }
    pub fn get_max(&self) -> f32 {
        self.max
    }
    pub fn get_percent(&self) -> f32 {
        self.current / self.max
    }
    pub fn decrease(&mut self, amount: f32) {
        self.current = (self.current - amount).max(0.);
    }
    pub fn is_dead(&self) -> bool {
        self.current <= 0.
    }
}
impl Default for Health {
    fn default() -> Self {
        Self { current: f32::MAX, max: f32::MAX }
    }
}


#[derive(Component, Default)]
pub struct ColorPulsation {
    min_brightness: f32,
    max_brightness: f32,
    duration: f32,
    is_increasing: bool,
    delta_change: f32,
}
impl ColorPulsation {
    pub fn new(min_brightness: f32, max_brightness: f32, duration: f32) -> Self {
        let mut color_pulsation = ColorPulsation::default();
        color_pulsation.update_parameters(min_brightness, max_brightness, duration);
        color_pulsation
    }
    pub fn update_parameters(&mut self, min_brightness: f32, max_brightness: f32, duration: f32) {
        assert!(min_brightness < max_brightness, "min_brightness must be less than max_brightness");
        self.min_brightness = min_brightness;
        self.max_brightness = max_brightness;
        self.duration = duration;
        self.delta_change = (max_brightness - min_brightness) / duration;
    }

    fn on_remove(
        trigger: On<Remove, ColorPulsation>,
        mut sprites: Query<&mut Sprite>,
    ) {
        let entity = trigger.entity;
        let Ok(mut sprite) = sprites.get_mut(entity) else { return; };
        match &mut sprite.color {
            Color::Hsla(Hsla {lightness, .. }) => {
                *lightness = 1.0;
            }
            _ => {}
        }
    }

    fn pulsate_sprites_system(
        time: Res<Time>,
        mut sprites: Query<(&mut Sprite, &mut ColorPulsation)>,
    ) {
        for (mut sprite, mut color_pulsation) in sprites.iter_mut() {
            let delta_time = time.delta_secs();
            match &mut sprite.color {
                Color::Hsla(Hsla {lightness, .. }) => {
                    if color_pulsation.is_increasing && *lightness > color_pulsation.max_brightness {
                        color_pulsation.is_increasing = false;
                    } else if !color_pulsation.is_increasing && *lightness < color_pulsation.min_brightness {
                        color_pulsation.is_increasing = true;
                    }
                    *lightness += delta_time * color_pulsation.delta_change * if color_pulsation.is_increasing { 1. } else { -1. }
                }
                _ => {}
            }
        }
    }
}

#[derive(Component)]
#[component(immutable)]
#[require(Transform)]
pub struct ZDepth(pub f32);
impl ZDepth {
    fn on_insert(
        trigger: On<Insert, ZDepth>,
        mut transforms: Query<(&mut Transform, &ZDepth)>,
    ) {
        let entity = trigger.entity;
        let Ok((mut transform, z_depth)) = transforms.get_mut(entity) else { return; };
        transform.translation.z = z_depth.0;
    }
}
impl From<f32> for ZDepth {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

#[derive(Resource, Default, Clone, SSS)]
pub struct MapInfo {
    pub grid_width: i32,
    pub grid_height: i32,
    pub world_width: f32,
    pub world_height: f32,
    pub name: String,
}
impl Saveable for MapInfo {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        tx.execute(
            "INSERT OR REPLACE INTO map_info (id, width, height, name) VALUES (1, ?1, ?2, ?3)",
            (self.grid_width, self.grid_height, &self.name),
        )?;
        Ok(())
    }
}
impl Loadable for MapInfo {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT width, height, name FROM map_info WHERE id = 1")?;
        let result = stmt.query_row([], |row| {
            let width: i32 = row.get(0)?;
            let height: i32 = row.get(1)?;
            let name: String = row.get(2)?;
            Ok((width, height, name))
        });

        let (width, height, name) = result?;
        let map_info = MapInfo {
            grid_width: width,
            grid_height: height,
            world_width: width as f32 * CELL_SIZE,
            world_height: height as f32 * CELL_SIZE,
            name,
        };

        ctx.commands.insert_resource(map_info);
        Ok(LoadResult::Finished)
    }
}
impl MapInfo {
    fn on_game_save(
        mut commands: Commands,
        map_info: Res<MapInfo>,
    ) {
        commands.queue(SaveableBatchCommand::from_single(map_info.clone()));
    }
}

/// Links a unit (e.g., ExpeditionDrone) to its owning building.
/// Used for: refueling location, operational checks (is home powered?), UI grouping.
#[derive(Component)]
#[relationship(relationship_target = HomeBaseLinkedObjects)]
pub struct HomeBase(pub Entity);

/// Inverse of HomeBase - auto-populated by Bevy's relationship system.
/// Query this on buildings to find all units that consider it home.
#[derive(Component)]
#[relationship_target(relationship = HomeBase)]
pub struct HomeBaseLinkedObjects(Vec<Entity>);




macro_rules! define_z_indexes {
    // Internal macro to handle incrementing the counter
    (@internal $counter:expr, $name:ident) => {
        pub const $name: f32 = $counter;
    };
    (@internal $counter:expr, $name:ident, $($rest:ident),+) => {
        pub const $name: f32 = $counter;
        define_z_indexes!(@internal $counter + 0.001, $($rest),+);
    };
    // Public-facing macro interface
    ($($name:ident),+) => {
        define_z_indexes!(@internal 0.001, $($name),+);
    };
}

define_z_indexes!(
    Z_OBSTACLE,
    Z_OVERLAY_ENERGY_SUPPLY,
    Z_BUILDING,
    Z_WISP,
    Z_GROUND_EFFECT,
    Z_TOWER_TOP,
    Z_MAP_UI,
    Z_AERIAL_UNIT,
    Z_PROJECTILE_UNDER,
    Z_PROJECTILE
);