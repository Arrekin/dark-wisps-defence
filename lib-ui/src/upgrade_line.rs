use bevy::{ecs::spawn::SpawnIter, text::LineBreak};

use crate::{lib_prelude::*, prelude::CostIndicator};

pub struct CommonPlugin;
impl Plugin for CommonPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_observer(UpgradeLineBuilder::on_add)
            .add_observer(UpgradeLine::on_insert)
            .add_observer(UpgradeLine::on_upgrade_applied);
    }
}

/// Builder component that triggers creation of an upgrade line UI.
/// Takes a target entity and an upgrade type to display.
#[derive(Component)]
pub struct UpgradeLineBuilder {
    pub target_entity: Entity,
    pub upgrade_type: UpgradeType,
}
impl UpgradeLineBuilder {
    fn on_add(
        trigger: On<Add, UpgradeLineBuilder>,
        mut commands: Commands,
        // Upgrades are disabled — despawn the line immediately.
        // Original logic kept below as reference for the future migration.
        // builders: Query<&UpgradeLineBuilder>,
        // targets: Query<(&Upgrades, &ModifiersBank)>,
    ) {
        commands.entity(trigger.entity).despawn();

        // let line_entity = trigger.entity;
        // let Ok(builder) = builders.get(line_entity) else { return; };
        // let Ok((upgrades, modifiers_bank)) = targets.get(builder.target_entity) else {
        //     commands.entity(line_entity).despawn();
        //     return;
        // };
        //
        // let Some(upgrade_info) = upgrades.upgrades.get(&builder.upgrade_type) else {
        //     commands.entity(line_entity).despawn();
        //     return;
        // };
        //
        // if upgrade_info.current_level >= upgrade_info.static_info.levels.len() {
        //     commands.entity(line_entity).despawn();
        //     return;
        // }
        //
        // let UpgradeType::Modifier(modifier_type) = builder.upgrade_type;
        // let current_value = modifiers_bank.get_sum(modifier_type);
        // let next_level_info = &upgrade_info.static_info.levels[upgrade_info.current_level];
        // let next_value = current_value + next_level_info.value;
        //
        // commands.entity(line_entity)
        //     .remove::<UpgradeLineBuilder>()
        //     .insert(UpgradeLine {
        //         target_entity: builder.target_entity,
        //         upgrade_type: builder.upgrade_type,
        //         costs: next_level_info.cost.clone(),
        //         current_value,
        //         next_value,
        //     });
    }
}
#[derive(Component)]
#[require(Node)]
pub struct UpgradeLine {
    pub target_entity: Entity,
    pub upgrade_type: UpgradeType,
    pub costs: Vec<Cost>,
    pub current_value: f32,
    pub next_value: f32
}
impl UpgradeLine {
    fn on_insert(
        trigger: On<Insert, UpgradeLine>,
        mut commands: Commands,
        upgrade_lines: Query<&UpgradeLine>,
    ) {
        let upgrade_line_entity = trigger.entity;
        let Ok(upgrade_line) = upgrade_lines.get(upgrade_line_entity) else { return; };
        let UpgradeType::Modifier(modifier_type) = upgrade_line.upgrade_type;

        // Clear everything in case this is rebuild operation
        commands.entity(upgrade_line_entity).despawn_related::<Children>();

        // Spawn the full upgrade line structure
        commands.entity(upgrade_line_entity).with_children(|parent| {
            // Main container for the entire upgrade line
            parent.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Px(36.0),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::horizontal(Val::Px(8.)),
                    ..default()
                },
            )).with_children(|parent| {
                // Left: Upgrade name + current level
                parent.spawn((
                    Text::new(format!("{:?} {:.2}", modifier_type, upgrade_line.current_value)),
                    TextColor::from(Color::WHITE),
                    TextLayout::new_with_linebreak(LineBreak::NoWrap),
                    Node {
                        ..default()
                    },
                ));
                
                // Right: Composite clickable button
                parent.spawn((
                    UpgradeButton(upgrade_line_entity),
                    Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        padding: UiRect::horizontal(Val::Px(6.)),
                        border: UiRect::all(Val::Px(1.)),
                        ..default()
                    },
                    BorderColor::all(Color::WHITE),
                    BackgroundColor(Color::srgba(0.2, 0.2, 0.2, 0.8)),
                ))
                    .observe(Self::on_click)
                    .with_children(|parent| {
                        parent.spawn((
                            Text::new(format!("-> {:.2}", upgrade_line.next_value)),
                            TextColor::from(Color::WHITE),
                            TextLayout::new_with_linebreak(LineBreak::NoWrap),
                            Node {
                                margin: UiRect::right(Val::Px(8.)),
                                ..default()
                            },
                        ));
                    
                        // Cost indicators
                        parent.spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                margin: UiRect::right(Val::Px(4.)),
                                ..default()
                            },
                            Children::spawn(
                                SpawnIter(upgrade_line.costs.clone().into_iter().map(|cost| CostIndicator::from(cost))),
                            ),
                        ));
                        
                        // Upgrade text
                        parent.spawn((
                            Text::new("Upgrade"),
                            TextColor::from(Color::WHITE),
                            TextLayout::new_with_linebreak(LineBreak::NoWrap),
                        ));
                    });
            });
        });
    }

    fn on_click(
        trigger: On<Pointer<Click>>,
        mut commands: Commands,
        mut stock: ResMut<Stock>,
        upgrade_buttons: Query<&UpgradeButton>,
        upgrade_lines: Query<&UpgradeLine>,
    ) {
        let entity = trigger.entity;
        let Ok(upgrade_button) = upgrade_buttons.get(entity) else { return; };
        let Ok(upgrade_line) = upgrade_lines.get(upgrade_button.0) else { return; };
        if !stock.try_pay_costs(&upgrade_line.costs) { return; }

        commands.queue(LevelUpUpgradeMessage {
            entity: upgrade_line.target_entity,
            upgrade_type: upgrade_line.upgrade_type,
        });
    }

    /// Observer for when an upgrade is applied - rebuilds the upgrade line UI
    fn on_upgrade_applied(
        trigger: On<LevelUpUpgradeAppliedEvent>,
        mut commands: Commands,
        upgrade_lines: Query<(Entity, &UpgradeLine)>,
    ) {
        let target_entity = trigger.entity;
        let upgrade_type = trigger.upgrade_type;

        // Find and rebuild all upgrade lines that match this target and upgrade type
        for (line_entity, upgrade_line) in upgrade_lines.iter() {
            if upgrade_line.target_entity == target_entity && upgrade_line.upgrade_type == upgrade_type {
                commands.entity(line_entity).insert(UpgradeLineBuilder {
                    target_entity,
                    upgrade_type,
                });
            }
        }
    }
}


#[derive(Component)]
#[require(Button)]
pub struct UpgradeButton(Entity);