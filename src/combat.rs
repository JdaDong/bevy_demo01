//! 战斗结算：碰撞检测 → `Damage` 事件 → 观察者结算血量 → 死亡/爆炸/掉落。
//!
//! 覆盖 API：`GlobalTransform`（子实体世界坐标）、`commands.trigger(EntityEvent)` 冒泡、
//! 全局观察者 `On<Damage>`、`Has<T>`、`Or<..>` 过滤器、`Disabled` 回收、
//! `Query<.., Allow<Disabled>>`（显式包含被禁用实体）、`MessageWriter`、
//! `NextState`、`try_despawn`（幂等销毁）。

use bevy::ecs::entity_disabling::Disabled;
use bevy::prelude::*;

use crate::assets::GameAssets;
use crate::components::*;
use crate::config::GameConfig;
use crate::events::*;
use crate::math::{ARENA_HALF, circles_overlap, is_outside_arena};
use crate::rng::GameRng;
use crate::schedule::GameSet;
use crate::score::RunStats;
use crate::state::AppState;

/// 玩家投射物 vs 敌人（含装甲子实体）。只产出 `Damage`，不直接改血量。
pub fn projectile_hits(
    mut commands: Commands,
    projectiles: Query<(Entity, &Projectile, &GlobalTransform, &Collider)>,
    targets: Query<(Entity, &GlobalTransform, &Collider, Has<ArmorPlate>), Or<(With<Enemy>, With<ArmorPlate>)>>,
) {
    for (p_entity, projectile, p_tf, p_col) in &projectiles {
        if projectile.team != Team::Player {
            continue;
        }
        let p_pos = p_tf.translation().truncate();
        // 先查装甲（在外圈），再查本体：同一颗子弹只命中一个
        let mut best: Option<(Entity, bool, f32)> = None;
        for (t_entity, t_tf, t_col, is_plate) in &targets {
            let t_pos = t_tf.translation().truncate();
            if circles_overlap(p_pos, p_col.radius, t_pos, t_col.radius) {
                let d = p_pos.distance_squared(t_pos);
                let better = match best {
                    None => true,
                    Some((_, best_plate, best_d)) => {
                        (is_plate && !best_plate) || (is_plate == best_plate && d < best_d)
                    }
                };
                if better {
                    best = Some((t_entity, is_plate, d));
                }
            }
        }
        if let Some((hit, _, _)) = best {
            commands.trigger(Damage { entity: hit, amount: projectile.damage });
            recycle_projectile(&mut commands, p_entity, projectile.kind);
        }
    }
}

/// 子弹 → 回池（Disabled）；导弹 → 销毁。
pub fn recycle_projectile(commands: &mut Commands, entity: Entity, kind: ProjectileKind) {
    match kind {
        ProjectileKind::Bullet => {
            commands.entity(entity).insert(Disabled);
        }
        ProjectileKind::Missile => {
            commands.entity(entity).try_despawn();
        }
    }
}

/// 投射物超时/出界回收。
pub fn recycle_expired_projectiles(mut commands: Commands, q: Query<(Entity, &Projectile, &Lifetime, &Transform)>) {
    for (e, p, life, tf) in &q {
        if life.0.is_finished() || is_outside_arena(tf.translation.truncate(), ARENA_HALF, 40.0) {
            recycle_projectile(&mut commands, e, p.kind);
        }
    }
}

/// 全局观察者：对带 `Health` 的实体结算伤害（冒泡链上每一跳都会调用一次）。
pub fn apply_damage(
    damage: On<Damage>,
    mut commands: Commands,
    mut q: Query<(&mut Health, &GlobalTransform, Option<&Enemy>, Has<Player>, Has<Dying>)>,
) {
    let Ok((mut health, tf, enemy, is_player, already_dying)) = q.get_mut(damage.entity) else {
        return;
    };
    if is_player || already_dying {
        // 玩家伤害走 player_contact 的专用流程（无敌帧/护盾）
        return;
    }
    health.apply(damage.amount);
    if health.is_dead()
        && let Some(enemy) = enemy
    {
        commands.trigger(Explosion {
            position: tf.translation().truncate(),
            radius: enemy.kind.radius() * 1.5,
            color: match enemy.kind {
                EnemyKind::Meteor => Color::srgb(2.0, 1.4, 0.8),
                EnemyKind::Hunter => Color::srgb(2.5, 0.6, 0.9),
                EnemyKind::Carrier => Color::srgb(1.8, 0.8, 3.0),
            },
        });
        commands.entity(damage.entity).insert(Dying);
    }
}

/// 标记：本帧死亡、等待统一结算（避免观察者里重复发消息）。
#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Dying;

/// 结算死亡：发消息、掉落、销毁（连同子实体装甲）。
#[allow(clippy::too_many_arguments)]
pub fn resolve_deaths(
    mut commands: Commands,
    dying: Query<(Entity, &Enemy, &GlobalTransform), With<Dying>>,
    config: Res<GameConfig>,
    assets: Res<GameAssets>,
    mut rng: ResMut<GameRng>,
    mut destroyed: MessageWriter<EnemyDestroyed>,
) {
    for (e, enemy, tf) in &dying {
        let position = tf.translation().truncate();
        destroyed.write(EnemyDestroyed { kind: enemy.kind, position });
        let chance = if enemy.kind == EnemyKind::Carrier { 1.0 } else { config.powerup_drop_chance };
        if rng.chance(chance) {
            let kind = *rng.pick(&PowerUpKind::ALL).unwrap_or(&PowerUpKind::Heal);
            spawn_powerup(&mut commands, &assets, kind, position);
        }
        commands.entity(e).despawn();
    }
}

pub fn spawn_powerup(commands: &mut Commands, assets: &GameAssets, kind: PowerUpKind, position: Vec2) -> Entity {
    commands
        .spawn((
            Name::new(format!("PowerUp::{kind:?}")),
            PowerUp(kind),
            Sprite {
                image: assets.powerup.clone(),
                color: kind.color(),
                custom_size: Some(Vec2::splat(26.0)),
                ..default()
            },
            Transform::from_translation(position.extend(3.0)),
            DespawnOnExit(AppState::InGame),
        ))
        .id()
}

/// 玩家与敌人接触：受伤（护盾优先、无敌帧保护）。
#[allow(clippy::too_many_arguments)]
pub fn player_contact(
    mut commands: Commands,
    config: Res<GameConfig>,
    player: Option<
        Single<(Entity, &GlobalTransform, &Collider, &mut Health, Option<&Children>, Has<Invulnerable>), With<Player>>,
    >,
    mut shields: Query<&mut Shield>,
    enemies: Query<(Entity, &GlobalTransform, &Collider), (With<Enemy>, Without<Player>)>,
    mut damaged: MessageWriter<PlayerDamaged>,
    mut sfx: MessageWriter<PlaySfx>,
    mut stats: ResMut<RunStats>,
) {
    let Some(player) = player else { return };
    let (p_entity, p_tf, p_col, mut health, children, invulnerable) = player.into_inner();
    if invulnerable {
        return;
    }
    let p_pos = p_tf.translation().truncate();
    let Some((hit_enemy, e_pos)) = enemies.iter().find_map(|(e, tf, col)| {
        let pos = tf.translation().truncate();
        circles_overlap(p_pos, p_col.radius, pos, col.radius).then_some((e, pos))
    }) else {
        return;
    };

    // 撞到的敌人承受大量伤害（陨石直接碎）
    commands.trigger(Damage { entity: hit_enemy, amount: 3 });

    // 护盾（子实体）优先吸收
    let mut absorbed = false;
    for child in children.into_iter().flat_map(|c| c.iter()) {
        if let Ok(mut shield) = shields.get_mut(child)
            && shield.charges > 0
        {
            shield.charges -= 1;
            absorbed = true;
            break;
        }
    }

    commands.entity(p_entity).insert(Invulnerable(Timer::from_seconds(config.invulnerable_secs, TimerMode::Once)));
    commands.trigger(Explosion { position: (p_pos + e_pos) * 0.5, radius: 18.0, color: Color::srgb(0.6, 1.5, 3.0) });
    if absorbed {
        sfx.write(PlaySfx::PowerUp);
        return;
    }
    health.apply(1);
    stats.damage_taken += 1;
    sfx.write(PlaySfx::Hurt);
    damaged.write(PlayerDamaged { remaining: health.current });
    if health.is_dead() {
        commands.trigger(PlayerDied { position: p_pos });
    }
}

/// 玩家死亡观察者：大爆炸 + 切到 GameOver。
pub fn on_player_died(died: On<PlayerDied>, mut commands: Commands, mut next: ResMut<NextState<AppState>>) {
    commands.trigger(Explosion { position: died.position, radius: 70.0, color: Color::srgb(3.0, 2.0, 1.0) });
    next.set(AppState::GameOver);
}

/// 调试/HUD：统计池中子弹（包括被禁用的）。`Allow<Disabled>` 让默认过滤失效。
pub fn count_pooled_bullets(q: Query<(&Projectile, Has<Disabled>), Allow<Disabled>>) -> (usize, usize) {
    let mut active = 0;
    let mut pooled = 0;
    for (p, disabled) in &q {
        if p.kind == ProjectileKind::Bullet {
            if disabled {
                pooled += 1;
            } else {
                active += 1;
            }
        }
    }
    (active, pooled)
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(apply_damage)
            .add_observer(on_player_died)
            .add_systems(
                Update,
                (projectile_hits, player_contact, recycle_expired_projectiles).chain().in_set(GameSet::Collide),
            )
            .add_systems(Update, resolve_deaths.in_set(GameSet::Resolve));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn pooled_bullet_counter_sees_disabled() {
        let mut world = World::new();
        let bullet = Projectile { kind: ProjectileKind::Bullet, damage: 1, team: Team::Player };
        world.spawn(bullet);
        world.spawn((bullet, Disabled));
        world.spawn((bullet, Disabled));
        world.spawn(Projectile { kind: ProjectileKind::Missile, ..bullet });
        let (active, pooled) = world.run_system_once(count_pooled_bullets).unwrap();
        assert_eq!((active, pooled), (1, 2));

        // 对照：默认查询看不到 Disabled
        let visible = world.run_system_once(|q: Query<&Projectile>| q.iter().count()).unwrap();
        assert_eq!(visible, 2);
    }
}
