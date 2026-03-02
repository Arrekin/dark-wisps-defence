//! Ripple wave effect spawned by the emitter tower.
//!
//! Each ripple is a circle mesh on render layer 1 that samples the pre-rendered
//! scene texture and applies a radial displacement as the band passes through
//! objects. See `documentation/scene_capture_effects.md` for the rendering architecture.

use bevy::{
    camera::visibility::RenderLayers,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
};

use lib_core::camera::SceneTexture;
use lib_grid::grids::wisps::WispsGrid;

use crate::prelude::*;
use crate::wisps::components::Wisp;

pub struct RipplePlugin;
impl Plugin for RipplePlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins((
                Material2dPlugin::<RippleMaterial>::default(),
            ))
            .add_systems(Update, (
                (
                    ripple_propagate_system,
                    ripple_hit_system,
                ).run_if(in_state(GameState::Running)),
            ))
            .add_observer(BuilderRipple::on_add)
            .register_db_loader::<BuilderRipple>(MapLoadingStage::SpawnMapElements)
            .register_db_saver(BuilderRipple::on_game_save);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RippleSaveData {
    pub entity: Entity,
    pub current_radius: f32,
}

#[derive(Component, SSS)]
pub struct BuilderRipple {
    pub world_position: Vec2,
    pub radius: f32, // in world size
    pub save_data: Option<RippleSaveData>,
}
impl Saveable for BuilderRipple {
    fn save(self, tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let save_data = self.save_data.expect("BuilderRipple for saving must have save_data");
        let entity_id = save_data.entity.index_u32() as i64;

        tx.register_entity(entity_id)?;
        tx.save_world_position(entity_id, self.world_position)?;
        tx.execute(
            "INSERT OR REPLACE INTO ripples (id, max_radius, current_radius) VALUES (?1, ?2, ?3)",
            rusqlite::params![entity_id, self.radius, save_data.current_radius],
        )?;
        Ok(())
    }
}
impl Loadable for BuilderRipple {
    fn load(ctx: &mut LoadContext) -> rusqlite::Result<LoadResult> {
        let mut stmt = ctx.conn.prepare("SELECT id, max_radius, current_radius FROM ripples LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(ctx.pagination.as_params())?;
        
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let old_id: i64 = row.get(0)?;
            let max_radius: f32 = row.get(1)?;
            let current_radius: f32 = row.get(2)?;
            let world_position = ctx.conn.get_world_position(old_id)?;
            
            if let Some(new_entity) = ctx.get_new_entity_for_old(old_id) {
                let save_data = RippleSaveData { entity: new_entity, current_radius };
                ctx.commands.entity(new_entity).insert(BuilderRipple::new_for_saving(
                    world_position,
                    max_radius,
                    save_data
                ));
            }
            count += 1;
        }
        Ok(count.into())
    }
}

impl BuilderRipple {
    pub fn new(world_position: Vec2, radius: f32) -> Self {
        Self { world_position, radius, save_data: None }
    }
    pub fn new_for_saving(world_position: Vec2, radius: f32, save_data: RippleSaveData) -> Self {
        Self { world_position, radius, save_data: Some(save_data) }
    }
    
    fn on_game_save(
        mut commands: Commands,
        ripples: Query<(Entity, &Transform, &Ripple)>,
    ) {
        if ripples.is_empty() { return; }
        let batch = ripples.iter().map(|(entity, transform, ripple)| {
             let save_data = RippleSaveData {
                 entity,
                 current_radius: ripple.current_radius,
             };
             BuilderRipple::new_for_saving(
                 transform.translation.xy(),
                 ripple.max_radius,
                 save_data
             )
        }).collect::<SaveableBatchCommand<_>>();
        commands.queue(batch);
    }

    fn on_add(
        trigger: On<Add, BuilderRipple>,
        mut commands: Commands,
        mut ripple_materials: ResMut<Assets<RippleMaterial>>,
        mut meshes: ResMut<Assets<Mesh>>,
        scene_texture: Res<SceneTexture>,
        builders: Query<&BuilderRipple>,
    ) {
        let entity = trigger.entity;
        let Ok(builder) = builders.get(entity) else { return; };

        let current_radius = builder.save_data.as_ref().map_or(0., |d| d.current_radius);

        commands.entity(entity)
            .remove::<BuilderRipple>()
            .insert((
                Mesh2d(meshes.add(Circle::new(builder.radius))),
                MeshMaterial2d(ripple_materials.add(RippleMaterial {
                    scene_texture: scene_texture.0.clone(),
                    current_radius: current_radius / (builder.radius * 2.),
                    wave_width: 0.15,
                })),
                Transform::from_translation(builder.world_position.extend(Z_GROUND_EFFECT)),
                Ripple{ max_radius: builder.radius, current_radius },
                MovementSpeed(50.0),
                RenderLayers::layer(1),
            ));
    }
}

#[derive(Asset, TypePath, Debug, Clone, AsBindGroup)]
pub struct RippleMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub scene_texture: Handle<Image>,
    #[uniform(2)]
    pub current_radius: f32,
    #[uniform(2)]
    pub wave_width: f32,
}

impl Material2d for RippleMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/ripple.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Component)]
#[require(MapBound)]
pub struct Ripple {
    max_radius: f32, // Must be half the mesh size
    current_radius: f32,
}

fn ripple_propagate_system(
    mut commands: Commands,
    time: Res<Time>,
    mut wave_materials: ResMut<Assets<RippleMaterial>>,
    mut ripples: Query<(Entity, &mut Ripple, &MovementSpeed, &MeshMaterial2d<RippleMaterial>)>,
) {
    for (entity, mut ripple, speed, material_handle) in ripples.iter_mut() {
        let Some(material) = wave_materials.get_mut(material_handle) else { continue; };
        ripple.current_radius += speed.0 * time.delta_secs();
        if ripple.current_radius > ripple.max_radius {
            commands.entity(entity).despawn();
        }

        // Update the ripple radius. Shader needs a normalized value from 0.0 to 1.0 over the mesh size.
        let mesh_size = ripple.max_radius * 2.;
        material.current_radius = ripple.current_radius / mesh_size;
    }
}

pub fn ripple_hit_system(
    wisps_grid: Res<WispsGrid>,
    ripples: Query<(&Ripple, &Transform)>,
    mut wisps: Query<(&mut Health, &Transform), With<Wisp>>,
) {
    for (ripple, ripple_transform) in ripples.iter() {
        // Check all fields covered by the ripple for wisp collisions
        let starting_grid_coords = GridCoords::from_transform(&ripple_transform);
        let bounds_range = (ripple.current_radius / CELL_SIZE) as i32;
        // Make bounds +/-1 since the ripple starts from in-between the grid fields
        let lower_bound_x = std::cmp::max(0, starting_grid_coords.x - bounds_range - 1);
        let lower_bound_y = std::cmp::max(0, starting_grid_coords.y - bounds_range - 1);
        let upper_bound_x = std::cmp::min(wisps_grid.width - 1, starting_grid_coords.x + bounds_range + 1);
        let upper_bound_y = std::cmp::min(wisps_grid.height - 1, starting_grid_coords.y + bounds_range + 1);
        for x in lower_bound_x..=upper_bound_x {
            for y in lower_bound_y..=upper_bound_y {
                for wisp in &wisps_grid[GridCoords{ x, y }] {
                    let Ok((mut wisp_health, wisp_transform)) = wisps.get_mut(*wisp) else { continue; };
                    let distance = wisp_transform.translation.distance(ripple_transform.translation);
                    // Hit only wisps within 1 world unit of the leading edge
                    if distance > ripple.current_radius || distance < ripple.current_radius - 1. { continue; }
                    wisp_health.decrease(1.);
                }
            }
        }
    }
}
