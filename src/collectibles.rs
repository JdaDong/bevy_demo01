//! 收集物：星星（计分）、道具（护盾 / 连射 / 回血）。
//!
//! 覆盖 API：`ParamSet`（同一系统内对同一组件的冲突访问）、`Local<Timer>`、
//! `Or<(With<A>, With<B>)>`、`run_if(resource_exists::<T>)`、`Children` 遍历、
//! `EntityCommands::insert` 追加 buff 组件。

use bevy::prelude::*;

use crate::assets::GameAssets;
use crate::components::*;
use crate::config::GameConfig;
use crate::events::{PlaySfx, PowerUpCollected, SpawnPopup, StarCollected};
use crate::math::{ARENA_HALF, circles_overlap};
use crate::rng::GameRng;
use crate::schedule::GameSet;
use crate::state::AppState;

/// 星星生成计时（Local 状态：系统私有，不污染全局资源）。
pub fn spawn_stars(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<GameConfig>,
    assets: Res<GameAssets>,
    mut rng: ResMut<GameRng>,
    stars: Query<(), With<Star>>,
    mut timer: Local<Option<Timer>>,
) {
    let timer = timer.get_or_insert_with(|| Timer::from_seconds(config.star_spawn_secs, TimerMode::Repeating));
    if !timer.tick(time.delta()).just_finished() {
        return;
    }
    if stars.iter().count() as u32 >= config.star_max {
        return;
    }
    let pos = rng.point_in_rect(ARENA_HALF - Vec2::splat(60.0));
    spawn_star(&mut commands, &assets, config.star_value, pos, rng.range_f32(0.0, std::f32::consts::TAU));
}

pub fn spawn_star(commands: &mut Commands, assets: &GameAssets, value: u32, position: Vec2, phase: f32) -> Entity {
    commands
        .spawn((
            Star { value, phase },
            Sprite {
                image: assets.star.clone(),
                color: Color::srgb(2.2, 1.9, 0.6), // HDR 发光
                custom_size: Some(Vec2::splat(22.0)),
                ..default()
            },
            Transform::from_translation(position.extend(2.0)),
            DespawnOnExit(AppState::InGame),
        ))
        .id()
}

/// 星星呼吸动画。
pub fn pulse_stars(time: Res<Time>, mut q: Query<(&Star, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (star, mut tf) in &mut q {
        let s = 1.0 + 0.15 * ops::sin(t * 4.0 + star.phase);
        tf.scale = Vec3::new(s, s, 1.0);
        tf.rotation = Quat::from_rotation_z(t * 0.8 + star.phase);
    }
}

/// 磁铁：靠近玩家的星星被吸过去。
/// 玩家 Transform 只读、星星 Transform 可写 → 同一组件冲突，用 `ParamSet` 分时借用。
pub fn magnet_stars(
    time: Res<Time>,
    config: Res<GameConfig>,
    mut set: ParamSet<(Query<&Transform, With<Player>>, Query<&mut Transform, Or<(With<Star>, With<PowerUp>)>>)>,
) {
    let Ok(player_pos) = set.p0().single().map(|t| t.translation.truncate()) else {
        return;
    };
    let radius = config.magnet_radius;
    let dt = time.delta_secs();
    for mut tf in set.p1().iter_mut() {
        let pos = tf.translation.truncate();
        let to = player_pos - pos;
        let d = to.length();
        if d > 1.0 && d < radius {
            let pull = (1.0 - d / radius) * 600.0 * dt;
            tf.translation += (to / d * pull.min(d)).extend(0.0);
        }
    }
}

/// 拾取星星与道具。
#[allow(clippy::too_many_arguments)]
pub fn pickup(
    mut commands: Commands,
    player: Option<Single<(Entity, &GlobalTransform, &Collider, &mut Health, Option<&Children>), With<Player>>>,
    mut shields: Query<&mut Shield>,
    stars: Query<(Entity, &GlobalTransform, &Collider, &Star)>,
    powerups: Query<(Entity, &GlobalTransform, &Collider, &PowerUp)>,
    mut collected: MessageWriter<StarCollected>,
    mut powered: MessageWriter<PowerUpCollected>,
    mut popups: MessageWriter<SpawnPopup>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let Some(player) = player else { return };
    let (p_entity, p_tf, p_col, mut health, children) = player.into_inner();
    let p_pos = p_tf.translation().truncate();

    for (e, tf, col, star) in &stars {
        let pos = tf.translation().truncate();
        if circles_overlap(p_pos, p_col.radius, pos, col.radius) {
            collected.write(StarCollected { value: star.value, position: pos });
            commands.entity(e).despawn();
        }
    }

    for (e, tf, col, powerup) in &powerups {
        let pos = tf.translation().truncate();
        if !circles_overlap(p_pos, p_col.radius, pos, col.radius) {
            continue;
        }
        match powerup.0 {
            PowerUpKind::Shield => {
                for child in children.into_iter().flat_map(|c| c.iter()) {
                    if let Ok(mut s) = shields.get_mut(child) {
                        s.charges = (s.charges + 1).min(3);
                    }
                }
            }
            PowerUpKind::RapidFire => {
                commands.entity(p_entity).insert(RapidFire(Timer::from_seconds(6.0, TimerMode::Once)));
            }
            PowerUpKind::Heal => health.heal(1),
        }
        powered.write(PowerUpCollected(powerup.0));
        popups.write(SpawnPopup { position: pos, text: powerup.0.label().to_string(), color: powerup.0.color() });
        sfx.write(PlaySfx::PowerUp);
        commands.entity(e).despawn();
    }
}

pub struct CollectiblesPlugin;

impl Plugin for CollectiblesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                // 与 spawn_enemies 都写 GameRng：必须显式排序，否则执行器可能每次顺序不同 → 不可复现
                spawn_stars.run_if(resource_exists::<GameAssets>).after(crate::enemy::spawn_enemies),
                pulse_stars,
                magnet_stars.after(pulse_stars),
            )
                .in_set(GameSet::Simulate),
        )
        .add_systems(Update, pickup.in_set(GameSet::Collide));
    }
}
