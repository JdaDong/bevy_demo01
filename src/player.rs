//! 玩家：生成、移动（FixedUpdate）、瞄准、射击（子弹对象池）、导弹、buff。
//!
//! 覆盖 API：`OnEnter` 生成、`children![]` 声明式层级、`Sprite::from_atlas_image` + `TextureAtlas`、
//! `Mesh2d` + `MeshMaterial2d`、`Single` / `Option<Single>`、`Query<.., With<Disabled>>`
//! 实体禁用做对象池、`EntityCommands::{insert, remove, try_insert}`、`RemovedComponents`、
//! `Has<T>`、`#[derive(QueryData)]` 自定义查询、`Time<Fixed>`。

use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::query::QueryData;
use bevy::prelude::*;

use crate::assets::GameAssets;
use crate::components::*;
use crate::config::GameConfig;
use crate::events::PlaySfx;
use crate::input::PlayerIntent;
use crate::math::{
    ARENA_HALF, aim_direction, clamp_to_arena, integrate_velocity, rotation_for_direction, steer_towards,
};
use crate::schedule::{GameSet, PhysicsSet};
use crate::score::RunStats;
use crate::state::AppState;

/// 玩家点击锁定的目标（导弹用）。
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LockedTarget(pub Option<Entity>);

/// 对象池统计（Debug HUD 用）。
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BulletPoolStats {
    pub spawned: u32,
    pub reused: u32,
}

/// 自定义 QueryData：把玩家常用组件打包成一个具名结构，系统签名更干净。
#[derive(QueryData)]
#[query_data(mutable)]
pub struct PlayerShip {
    pub entity: Entity,
    pub transform: &'static mut Transform,
    pub velocity: &'static mut Velocity,
    pub fire: &'static mut FireCooldown,
    pub missile: &'static mut MissileCooldown,
    pub rapid: Option<&'static RapidFire>,
}

pub fn spawn_player(mut commands: Commands, assets: Res<GameAssets>, config: Res<GameConfig>) {
    commands.spawn((
        Player,
        Health::new(config.player_max_hp),
        Collider { radius: config.player_radius },
        Sprite::from_atlas_image(assets.ship.clone(), TextureAtlas { layout: assets.ship_layout.clone(), index: 0 }),
        SpriteAnimation::new(0, 3, 12.0),
        Transform::from_xyz(0.0, 0.0, 10.0).with_scale(Vec3::splat(1.5)),
        DespawnOnExit(AppState::InGame),
        // 声明式子实体：护盾光环，默认隐藏
        children![(
            Name::new("ShieldRing"),
            Shield { charges: 0 },
            Mesh2d(assets.shield_mesh.clone()),
            MeshMaterial2d(assets.shield_material.clone()),
            Transform::from_xyz(0.0, 0.0, 1.0).with_scale(Vec3::splat(1.0 / 1.5)),
            Visibility::Hidden,
        )],
    ));
}

/// 固定步长移动：加速度 + 指数阻尼 + 夹边。
pub fn move_player(
    time: Res<Time>,
    intent: Res<PlayerIntent>,
    config: Res<GameConfig>,
    player: Option<Single<(&mut Transform, &mut Velocity), With<Player>>>,
) {
    let Some(player) = player else { return };
    let (mut tf, mut vel) = player.into_inner();
    let dt = time.delta_secs(); // FixedUpdate 中 Time = Time<Fixed>
    vel.0 = integrate_velocity(
        vel.0,
        intent.movement,
        config.player_accel,
        config.player_drag,
        config.player_max_speed,
        dt,
    );
    let next = tf.translation.truncate() + vel.0 * dt;
    let clamped = clamp_to_arena(next, ARENA_HALF, config.player_radius);
    // 撞墙时把该轴速度清零，避免“贴墙滑行”
    if clamped.x != next.x {
        vel.x = 0.0;
    }
    if clamped.y != next.y {
        vel.y = 0.0;
    }
    tf.translation = clamped.extend(tf.translation.z);
}

/// 朝鼠标/右摇杆/移动方向转向。
pub fn aim_player(intent: Res<PlayerIntent>, player: Option<Single<(&mut Transform, &Velocity), With<Player>>>) {
    let Some(player) = player else { return };
    let (mut tf, vel) = player.into_inner();
    let pos = tf.translation.truncate();
    let current = Dir2::new((tf.rotation * Vec3::Y).truncate()).unwrap_or(Dir2::Y);
    let dir = if let Some(stick) = intent.aim_stick {
        Dir2::new(stick).unwrap_or(current)
    } else if let Some(point) = intent.aim_point {
        aim_direction(pos, point, current)
    } else {
        Dir2::new(vel.0).unwrap_or(current)
    };
    tf.rotation = rotation_for_direction(dir);
}

/// 射击：优先从池里取一颗 `Disabled` 的子弹复用，没有才 spawn。
#[allow(clippy::too_many_arguments)]
pub fn fire_bullets(
    mut commands: Commands,
    time: Res<Time>,
    intent: Res<PlayerIntent>,
    config: Res<GameConfig>,
    assets: Res<GameAssets>,
    mut player: Query<PlayerShip, With<Player>>,
    // 默认查询会过滤掉 Disabled 实体；显式 With<Disabled> 才能拿到它们
    pool: Query<Entity, (With<Projectile>, With<Disabled>)>,
    mut stats: ResMut<RunStats>,
    mut pool_stats: ResMut<BulletPoolStats>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let Ok(mut ship) = player.single_mut() else { return };
    ship.fire.0.tick(time.delta());
    if !intent.fire || !ship.fire.0.is_finished() {
        return;
    }
    let cooldown = if ship.rapid.is_some() { config.fire_cooldown_secs * 0.4 } else { config.fire_cooldown_secs };
    ship.fire.0.set_duration(std::time::Duration::from_secs_f32(cooldown));
    ship.fire.0.reset();

    let forward = (ship.transform.rotation * Vec3::Y).truncate();
    let origin = ship.transform.translation.truncate() + forward * 22.0;
    let bundle = (
        Projectile { kind: ProjectileKind::Bullet, damage: config.bullet_damage, team: Team::Player },
        Transform::from_translation(origin.extend(8.0)).with_rotation(ship.transform.rotation),
        Velocity(forward * config.bullet_speed + ship.velocity.0 * 0.3),
        Lifetime::secs(config.bullet_lifetime_secs),
    );

    if let Some(reuse) = pool.iter().next() {
        // 复用：覆盖状态并移除 Disabled
        commands.entity(reuse).insert(bundle).remove::<Disabled>();
        pool_stats.reused += 1;
    } else {
        commands.spawn((
            Name::new("Bullet"),
            bundle,
            Mesh2d(assets.bullet_mesh.clone()),
            MeshMaterial2d(assets.bullet_material.clone()),
            DespawnOnExit(AppState::InGame),
        ));
        pool_stats.spawned += 1;
    }
    stats.shots += 1;
    sfx.write(PlaySfx::Shoot);
}

/// 发射追踪导弹：目标 = 点击锁定的敌人，否则最近的敌人。
#[allow(clippy::too_many_arguments)]
pub fn fire_missile(
    mut commands: Commands,
    time: Res<Time>,
    intent: Res<PlayerIntent>,
    config: Res<GameConfig>,
    assets: Res<GameAssets>,
    locked: Res<LockedTarget>,
    mut player: Query<PlayerShip, With<Player>>,
    enemies: Query<(Entity, &Transform), (With<Enemy>, Without<Player>)>,
    mut stats: ResMut<RunStats>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    let Ok(mut ship) = player.single_mut() else { return };
    ship.missile.0.tick(time.delta());
    if !intent.missile || !ship.missile.0.is_finished() {
        return;
    }
    let pos = ship.transform.translation.truncate();
    let target = locked
        .0
        .filter(|e| enemies.contains(*e))
        .or_else(|| nearest(pos, enemies.iter().map(|(e, t)| (e, t.translation.truncate()))));
    let Some(target) = target else { return };

    ship.missile.0.set_duration(std::time::Duration::from_secs_f32(config.missile_cooldown_secs));
    ship.missile.0.reset();
    let forward = (ship.transform.rotation * Vec3::Y).truncate();
    commands.spawn((
        Name::new("Missile"),
        Projectile { kind: ProjectileKind::Missile, damage: config.missile_damage, team: Team::Player },
        Homing { speed: config.missile_speed, turn_rate: config.missile_turn_rate },
        Targeting(target),
        Collider { radius: 7.0 },
        Mesh2d(assets.missile_mesh.clone()),
        MeshMaterial2d(assets.missile_material.clone()),
        Transform::from_translation((pos + forward * 20.0).extend(8.0)),
        Velocity(forward * config.missile_speed * 0.5),
        Lifetime::secs(4.0),
        DespawnOnExit(AppState::InGame),
    ));
    stats.missiles += 1;
    sfx.write(PlaySfx::Missile);
}

/// 纯函数：找最近的实体。
pub fn nearest(from: Vec2, candidates: impl Iterator<Item = (Entity, Vec2)>) -> Option<Entity> {
    candidates.map(|(e, p)| (e, p.distance_squared(from))).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(e, _)| e)
}

/// 导弹制导：沿 `Targeting` 关系追踪；目标消失（关系被自动移除）就直线飞。
pub fn steer_missiles(
    time: Res<Time>,
    mut missiles: Query<(&mut Velocity, &mut Transform, &Homing, &Targeting)>,
    targets: Query<&GlobalTransform>,
) {
    let dt = time.delta_secs();
    for (mut vel, mut tf, homing, targeting) in &mut missiles {
        let Ok(target) = targets.get(targeting.0) else { continue };
        vel.0 = steer_towards(
            vel.0,
            tf.translation.truncate(),
            target.translation().truncate(),
            homing.speed,
            homing.turn_rate,
            dt,
        );
        if let Ok(dir) = Dir2::new(vel.0) {
            tf.rotation = rotation_for_direction(dir);
        }
    }
}

/// buff 计时：无敌闪烁、连射到期。
pub fn tick_buffs(
    mut commands: Commands,
    time: Res<Time>,
    mut invuln: Query<(Entity, &mut Invulnerable, &mut Visibility), With<Player>>,
    mut rapid: Query<(Entity, &mut RapidFire)>,
) {
    for (e, mut inv, mut vis) in &mut invuln {
        inv.0.tick(time.delta());
        // 每 0.1s 闪一次
        let blink = ((inv.0.elapsed_secs() * 10.0) as u32).is_multiple_of(2);
        *vis = if blink { Visibility::Inherited } else { Visibility::Hidden };
        if inv.0.is_finished() {
            commands.entity(e).remove::<Invulnerable>();
        }
    }
    for (e, mut r) in &mut rapid {
        if r.0.tick(time.delta()).is_finished() {
            commands.entity(e).remove::<RapidFire>();
        }
    }
}

/// `RemovedComponents`：无敌结束后确保玩家可见（闪烁可能停在 Hidden 帧）。
pub fn restore_visibility(mut removed: RemovedComponents<Invulnerable>, mut q: Query<&mut Visibility, With<Player>>) {
    for e in removed.read() {
        if let Ok(mut vis) = q.get_mut(e) {
            *vis = Visibility::Inherited;
        }
    }
}

/// 护盾显隐跟随 charges（`Changed<Shield>` 过滤器，只在变化时处理）。
pub fn sync_shield_visibility(mut q: Query<(&Shield, &mut Visibility), Changed<Shield>>) {
    for (shield, mut vis) in &mut q {
        *vis = if shield.charges > 0 { Visibility::Inherited } else { Visibility::Hidden };
    }
}

pub fn clear_locked_target(mut locked: ResMut<LockedTarget>) {
    locked.0 = None;
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LockedTarget>()
            .init_resource::<BulletPoolStats>()
            .add_systems(OnEnter(AppState::InGame), (spawn_player, clear_locked_target))
            .add_systems(
                FixedUpdate,
                (
                    move_player,
                    // 先转向再积分：两者都读写导弹的速度/变换，顺序不定会破坏确定性
                    steer_missiles.before(crate::enemy::integrate_motion),
                )
                    .in_set(PhysicsSet),
            )
            .add_systems(
                Update,
                (aim_player, fire_bullets, fire_missile, tick_buffs, restore_visibility, sync_shield_visibility)
                    .chain()
                    .in_set(GameSet::Simulate),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_picks_closest() {
        let mut world = World::new();
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let c = world.spawn_empty().id();
        let pts = [(a, Vec2::new(10.0, 0.0)), (b, Vec2::new(2.0, 1.0)), (c, Vec2::new(-5.0, 0.0))];
        assert_eq!(nearest(Vec2::ZERO, pts.into_iter()), Some(b));
        assert_eq!(nearest(Vec2::ZERO, std::iter::empty()), None);
    }
}
