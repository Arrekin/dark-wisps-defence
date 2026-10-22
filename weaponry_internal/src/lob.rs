//! # Lob flight and presentation
//!
//! Updates lobbed projectiles from the game clock, keeping gameplay positions on the ground.
//! Projectiles with a future launch time wait hidden at their origin with `LobAwaitingLaunch`.
//! At launch, they become visible and switch to `LobInFlight`, enabling movement and visual updates.
//!
//! Each projectile spawns two visual children, which this module updates:
//! - `LobBody` grows with height and shifts along the flight line away from its midpoint to simulate
//!   perspective. Its screen path stays straight, with slower motion near launch and landing.
//! - `LobShadow` falls away from the sun, spreading and fading with height.
//!
//! Focused towers with `LobPreview` show dotted body and shadow paths using their `LobProfile`.
//! The preview targets the latest shot in flight, or the target wisp's next cell. Target
//! changes and dot spacing are smoothed; perspective packs dots near launch and landing. Body
//! dots grow and brighten with height, while shadow dots spread and fade. Each tower reuses a
//! fixed pool of dots, hidden when the preview has no target. Losing focus clears the preview.
//!
//! `SaveLobExtension` and `LoadLobExtension` store flight state in the shared `lobs` table,
//! keyed by projectile entity.

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use buildings::prelude::TowerWispTarget;
use game_core::prelude::{CELL_SIZE, ZDepth};
use hud::prelude::FocusedMapObject;
use persistence::{prelude::*, rusqlite};
use session::GameClock;
use states::prelude::GameState;
use visuals::map_light::{MAP_SUN_DIRECTION, MAP_SUN_GROUND_DIRECTION};
use weaponry::{lob::{Lob, LobAwaitingLaunch, LobInFlight, LobPreview, LobProfile}, prelude::FiredProjectiles};
use wisps::prelude::{Wisp, WispHeading};

pub(crate) struct LobPlugin;
impl Plugin for LobPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<LobDiscImage>()
            .add_systems(Update, (
                (launch_awaiting_lobs, advance_lobs).chain().run_if(in_state(GameState::Running)),
                update_lob_visuals,
            ).chain())
            .add_systems(Update, (update_lob_preview_targets, update_lob_preview_dots).chain())
            .add_observer(on_add_lob_insert_launch_phase)
            .add_observer(on_add_lob_preview_spawn_dots)
            .add_observer(on_remove_focused_map_object_clear_lob_preview_target);
    }
}

// Perspective
/// Simulated camera height in world units. Lower values exaggerate projectile growth.
const CAMERA_HEIGHT: f32 = 320.;
/// Scales the perspective offset along the flight line, measured from the flight's midpoint.
/// Excessive stretch makes projectiles appear to reverse near launch and landing.
const PARALLAX_STRETCH: f32 = 0.5;
/// Visual height limit as a fraction of `CAMERA_HEIGHT`. Keeps perspective scaling finite.
const MAX_VISUAL_HEIGHT: f32 = 0.85;

// Shadow
/// Shared by projectile shadows and preview shadow dots.
const SHADOW_COLOR: Color = Color::srgba(0., 0., 0., 0.35); // black
/// Height in world units at which shadow spread and fading reach their limits.
const SHADOW_SPREAD_HEIGHT: f32 = 4. * CELL_SIZE;
/// Maximum size increase as a fraction of the ground-level shadow size.
const SHADOW_SPREAD: f32 = 0.5;
/// Maximum opacity reduction as a fraction of `SHADOW_COLOR`'s alpha.
const SHADOW_FADE: f32 = 0.5;

// Disc texture
/// Width and height of the shared shadow and preview texture, in pixels.
const DISC_TEXTURE_SIZE: u32 = 64;

// Preview trace
/// Exponential smoothing rate for target position and dot spacing, in inverse seconds.
const PREVIEW_SETTLE_RATE: f32 = 8.;
/// Desired ground spacing in world units. Increases when the dot pool cannot cover the flight.
const PREVIEW_DOT_SPACING: f32 = 12.;
/// Dots per line in each preview's pool.
const PREVIEW_MAX_DOTS: u32 = 48;
const PREVIEW_BODY_DOT_DIAMETER: f32 = 4.;
const PREVIEW_SHADOW_DOT_DIAMETER: f32 = 6.;
/// Target spacing near launch and landing as a fraction of uniform ground spacing.
/// Controls preview parallax: smaller values pack dots closer together at the ends.
const PREVIEW_END_GAP: f32 = 0.15;
/// Body dot opacity at ground level.
const PREVIEW_BODY_ALPHA_LOW: f32 = 0.35;
/// Body dot opacity at twice the ground-level size or larger.
const PREVIEW_BODY_ALPHA_HIGH: f32 = 0.8;
/// Fraction of flight used to fade dots in at launch and out at landing.
const PREVIEW_END_FADE: f32 = 0.04;
const PREVIEW_BODY_COLOR: Color = Color::srgb(1., 0.7, 0.4); // warm orange

/// Smoothed landing point for a `LobPreview`. `None` hides the preview.
#[derive(Component, Default, PartialEq)]
struct LobPreviewTarget(Option<Vec2>);
impl LobPreviewTarget {
    fn point(&self) -> Option<Vec2> {
        self.0
    }

    /// Interpolates toward `point` by `settle`, or uses it directly if no target is set.
    fn ease_toward(&mut self, point: Vec2, settle: f32) {
        self.0 = Some(self.0.map_or(point, |shown| shown.lerp(point, settle)));
    }
}

/// One dot of a `LobPreview` trace. Child of the previewing entity.
#[derive(Component)]
struct LobPreviewDot {
    /// Position in the line, from launch toward landing.
    index: u32,
    /// Smoothed flight progress (0 = launch, 1 = landing). Reset to `None` when the preview hides.
    progress: Option<f32>,
    kind: LobPreviewDotKind,
}
impl LobPreviewDot {
    fn new(index: u32, kind: LobPreviewDotKind) -> Self {
        Self { index, progress: None, kind }
    }

    fn reset_progress(&mut self) {
        self.progress = None;
    }

    /// Interpolates progress toward `min(index / gaps, 1)` and returns it.
    /// `settle` controls interpolation; a reset dot starts directly at its assigned position.
    fn settle_toward_slot(&mut self, gaps: u32, settle: f32) -> f32 {
        let slot = (self.index as f32 / gaps as f32).min(1.);
        let progress = self.progress.map_or(slot, |progress| progress.lerp(slot, settle));
        self.progress = Some(progress);
        progress
    }
}

enum LobPreviewDotKind {
    Body,
    Shadow,
}

/// Body and shadow offsets, scales, and shadow opacity at a point in the flight.
/// Offsets are relative to the projectile's ground position.
struct LobView {
    body_offset: Vec2,
    body_scale: f32,
    shadow_offset: Vec2,
    shadow_scale: f32,
    /// Opacity multiplier relative to the ground-level shadow.
    shadow_opacity: f32,
}
impl LobView {
    /// `parallax_stretch` is the drift along the flight line, as in `PARALLAX_STRETCH`.
    fn at(lob: &Lob, progress: f32, parallax_stretch: f32) -> Self {
        let sample = lob.sample(progress);
        let height = sample.height.min(CAMERA_HEIGHT * MAX_VISUAL_HEIGHT);
        let body_scale = CAMERA_HEIGHT / (CAMERA_HEIGHT - height);
        let body_offset = (sample.ground_position - lob.sample(0.5).ground_position) * parallax_stretch * (body_scale - 1.);
        let shadow_length_per_height = MAP_SUN_DIRECTION.truncate().length() / MAP_SUN_DIRECTION.z;
        let shadow_spread = (sample.height / SHADOW_SPREAD_HEIGHT).min(1.);
        Self {
            body_offset,
            body_scale,
            shadow_offset: -MAP_SUN_GROUND_DIRECTION * sample.height * shadow_length_per_height,
            shadow_scale: 1. + SHADOW_SPREAD * shadow_spread,
            shadow_opacity: 1. - SHADOW_FADE * shadow_spread,
        }
    }
}

/// Visible projectile body. Child of a `Lob` entity.
#[derive(Component)]
#[require(ZDepth::Projectile)]
pub(crate) struct LobBody;

/// The projectile's shadow on the ground. Child of a `Lob` entity.
#[derive(Component)]
#[require(ZDepth::GroundEffect)]
pub(crate) struct LobShadow;

/// Soft round texture shared by lob shadows and preview dots.
#[derive(Resource)]
pub(crate) struct LobDiscImage(Handle<Image>);
impl FromWorld for LobDiscImage {
    fn from_world(world: &mut World) -> Self {
        let size = DISC_TEXTURE_SIZE;
        let mut data = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let offset = (Vec2::new(x as f32, y as f32) + 0.5) / size as f32 * 2. - 1.;
                let falloff = 1. - ((offset.length() - 0.3) / 0.7).clamp(0., 1.);
                let alpha = falloff * falloff * (3. - 2. * falloff);
                data.extend_from_slice(&[255, 255, 255, (alpha * 255.) as u8]);
            }
        }
        let image = Image::new(
            Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        Self(world.resource_mut::<Assets<Image>>().add(image))
    }
}
impl LobDiscImage {
    /// Shadow child for a projectile whose ground-level shadow is `diameter` world units across.
    pub fn make_shadow(&self, diameter: f32) -> impl Bundle {
        (
            LobShadow,
            Sprite {
                image: self.0.clone(),
                color: SHADOW_COLOR,
                custom_size: Some(Vec2::splat(diameter)),
                ..default()
            },
        )
    }
}

fn on_add_lob_insert_launch_phase(
    trigger: On<Add<Lob>>,
    mut commands: Commands,
    clock: Res<GameClock>,
    mut lobs: Query<(&Lob, &mut Transform)>,
) {
    let entity = trigger.entity;
    let Ok((lob, mut transform)) = lobs.get_mut(entity) else { return; };
    transform.translation = lob.current(clock.now()).ground_position.extend(transform.translation.z);
    if lob.is_awaiting_launch(clock.now()) {
        commands.entity(entity).insert((LobAwaitingLaunch, Visibility::Hidden));
    } else {
        commands.entity(entity).insert((LobInFlight, Visibility::Inherited));
    }
}

fn launch_awaiting_lobs(
    mut commands: Commands,
    clock: Res<GameClock>,
    lobs: Query<(Entity, &Lob), With<LobAwaitingLaunch>>,
) {
    for (entity, lob) in lobs.iter() {
        if lob.is_awaiting_launch(clock.now()) { continue; }
        commands.entity(entity).remove::<LobAwaitingLaunch>().insert((LobInFlight, Visibility::Inherited));
    }
}

pub(crate) fn advance_lobs(
    clock: Res<GameClock>,
    mut lobs: Query<(&Lob, &mut Transform), With<LobInFlight>>,
) {
    for (lob, mut transform) in lobs.iter_mut() {
        transform.translation = lob.current(clock.now()).ground_position.extend(transform.translation.z);
    }
}

fn update_lob_visuals(
    clock: Res<GameClock>,
    lobs: Query<(&Lob, &Children), With<LobInFlight>>,
    mut bodies: Query<&mut Transform, With<LobBody>>,
    mut shadows: Query<(&mut Transform, &mut Sprite), (With<LobShadow>, Without<LobBody>)>,
) {
    for (lob, children) in lobs.iter() {
        let view = LobView::at(lob, lob.progress(clock.now()), PARALLAX_STRETCH);
        for child in children.iter() {
            if let Ok(mut transform) = bodies.get_mut(child) {
                transform.translation = view.body_offset.extend(transform.translation.z);
                transform.scale = Vec3::splat(view.body_scale);
            } else if let Ok((mut transform, mut sprite)) = shadows.get_mut(child) {
                transform.translation = view.shadow_offset.extend(transform.translation.z);
                transform.scale = Vec3::splat(view.shadow_scale);
                sprite.color.set_alpha(SHADOW_COLOR.alpha() * view.shadow_opacity);
            }
        }
    }
}

fn on_add_lob_preview_spawn_dots(
    trigger: On<Add<LobPreview>>,
    mut commands: Commands,
    disc_image: Res<LobDiscImage>,
) {
    commands.entity(trigger.entity).insert(LobPreviewTarget::default()).with_children(|parent| {
        for index in 0..PREVIEW_MAX_DOTS {
            for (kind, color, diameter, z_depth) in [
                (LobPreviewDotKind::Shadow, SHADOW_COLOR, PREVIEW_SHADOW_DOT_DIAMETER, ZDepth::GroundEffect),
                (LobPreviewDotKind::Body, PREVIEW_BODY_COLOR, PREVIEW_BODY_DOT_DIAMETER, ZDepth::ProjectileUnder),
            ] {
                parent.spawn((
                    LobPreviewDot::new(index, kind),
                    Sprite {
                        image: disc_image.0.clone(),
                        color,
                        custom_size: Some(Vec2::splat(diameter)),
                        ..default()
                    },
                    Visibility::Hidden,
                    z_depth,
                ));
            }
        }
    });
}

fn update_lob_preview_targets(
    time: Res<Time>,
    mut previews: Query<(&TowerWispTarget, Option<&FiredProjectiles>, &mut LobPreviewTarget), (With<LobPreview>, With<FocusedMapObject>)>,
    lobs: Query<&Lob, With<LobInFlight>>,
    wisps: Query<WispHeading, With<Wisp>>,
) {
    let settle = 1. - (-PREVIEW_SETTLE_RATE * time.delta_secs()).exp();
    for (target, fired_projectiles, mut preview_target) in previews.iter_mut() {
        let in_flight_target = fired_projectiles.into_iter().flat_map(FiredProjectiles::newest_first).find_map(|projectile| lobs.get(projectile).ok()).map(Lob::target);
        let shot_point = in_flight_target.or_else(|| {
            let TowerWispTarget::Wisp(target_wisp) = *target else { return None; };
            wisps.get(target_wisp).ok().map(|wisp_heading| wisp_heading.next_cell_center())
        });
        match shot_point {
            Some(shot_point) => preview_target.ease_toward(shot_point, settle),
            None if matches!(*target, TowerWispTarget::NoValidTargets(_) | TowerWispTarget::Suspended) => {
                preview_target.set_if_neq(LobPreviewTarget::default());
            }
            None => {} // Between targets: hold the last point.
        }
    }
}

fn on_remove_focused_map_object_clear_lob_preview_target(
    trigger: On<Remove<FocusedMapObject>>,
    mut preview_targets: Query<&mut LobPreviewTarget>,
) {
    let Ok(mut preview_target) = preview_targets.get_mut(trigger.entity) else { return; };
    preview_target.set_if_neq(LobPreviewTarget::default());
}

fn update_lob_preview_dots(
    time: Res<Time>,
    previews: Query<(&LobProfile, Ref<LobPreviewTarget>, &GlobalTransform, &Children), With<LobPreview>>,
    mut dots: Query<(&mut LobPreviewDot, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    let settle = 1. - (-PREVIEW_SETTLE_RATE * time.delta_secs()).exp();
    for (lob_profile, preview_target, owner_transform, children) in previews.iter() {
        let Some(target) = preview_target.point() else {
            if preview_target.is_changed() {
                for child in children.iter() {
                    let Ok((mut dot, _, _, mut visibility)) = dots.get_mut(child) else { continue; };
                    dot.reset_progress();
                    visibility.set_if_neq(Visibility::Hidden);
                }
            }
            continue;
        };
        // The preview samples the flight by progress, so its launch time is irrelevant.
        let lob = lob_profile.lob(owner_transform.translation().xy(), target, 0.);
        let flight_distance = lob.origin().distance(lob.target()).max(1.);
        // Endpoint screen speed / ground speed = 1 - 2 * stretch * apex / CAMERA_HEIGHT.
        let visual_apex = lob.apex().clamp(1., CAMERA_HEIGHT * MAX_VISUAL_HEIGHT);
        let parallax_stretch = (1. - PREVIEW_END_GAP) * CAMERA_HEIGHT / (2. * visual_apex);
        let world_to_owner = owner_transform.affine().inverse();
        let gaps = (flight_distance / PREVIEW_DOT_SPACING).ceil().min((PREVIEW_MAX_DOTS - 1) as f32) as u32;

        for child in children.iter() {
            let Ok((mut dot, mut transform, mut sprite, mut visibility)) = dots.get_mut(child) else { continue; };
            let progress = dot.settle_toward_slot(gaps, settle);

            let end_fade = (progress / PREVIEW_END_FADE).min((1. - progress) / PREVIEW_END_FADE).clamp(0., 1.);
            let view = LobView::at(&lob, progress, parallax_stretch);
            let (offset, size, alpha) = match dot.kind {
                LobPreviewDotKind::Body => (
                    view.body_offset,
                    view.body_scale,
                    PREVIEW_BODY_ALPHA_LOW.lerp(PREVIEW_BODY_ALPHA_HIGH, (view.body_scale - 1.).min(1.)),
                ),
                LobPreviewDotKind::Shadow => (
                    view.shadow_offset,
                    view.shadow_scale,
                    SHADOW_COLOR.alpha() * view.shadow_opacity,
                ),
            };
            let world_position = (lob.sample(progress).ground_position + offset).extend(0.);
            let local_position = world_to_owner.transform_point3(world_position);
            transform.translation = local_position.truncate().extend(transform.translation.z);
            transform.scale = Vec3::splat(size);
            sprite.color.set_alpha(alpha * end_fade);
            visibility.set_if_neq(Visibility::Inherited);
        }
    }
}

pub(crate) trait SaveLobExtension {
    /// Writes `lob` to the shared `lobs` table. The caller registers the entity.
    fn save_lob(&self, entity_id: u32, lob: &Lob) -> rusqlite::Result<usize>;
}
impl SaveLobExtension for SaveContext<'_> {
    fn save_lob(&self, entity_id: u32, lob: &Lob) -> rusqlite::Result<usize> {
        self.tx.prepare_cached("INSERT OR REPLACE INTO lobs (entity_id, origin_x, origin_y, target_x, target_y, duration, apex, launch_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?
            .execute(rusqlite::params![entity_id, lob.origin().x, lob.origin().y, lob.target().x, lob.target().y, lob.duration(), lob.apex(), lob.launch_at()])
    }
}

pub(crate) trait LoadLobExtension {
    fn lob(&self, old_id: u32) -> LoadResult<Lob>;
}
impl LoadLobExtension for LoadContext<'_> {
    fn lob(&self, old_id: u32) -> LoadResult<Lob> {
        self.conn.prepare_cached("SELECT origin_x, origin_y, target_x, target_y, duration, apex, launch_at FROM lobs WHERE entity_id = ?1")
            .and_then(|mut statement| statement.query_row([old_id], |row| Ok(Lob::restore(
                Vec2::new(row.get(0)?, row.get(1)?),
                Vec2::new(row.get(2)?, row.get(3)?),
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))))
            .map_err(LoadError::table_read("lobs"))
    }
}
