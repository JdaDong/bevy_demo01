//! 敌人：波次、生成、AI（陨石卷绕 / 猎手追踪 / 母舰带装甲子实体）。
//!
//! 覆盖 API：`on_timer` 运行条件、`Timer`(Repeating)、`with_children` + `ChildSpawnerCommands`、
//! `EntityCommands::observe`（实体级观察者）+ 事件冒泡 `On::propagate`、
//! `Pointer<Over/Out/Click>` 拾取事件、`Query::iter_combinations_mut`、`Populated`、
//! `Added<T>` 过滤器、`Pickable`、`#[component(immutable)]` 组件读取。

use bevy::prelude::*;

use crate::assets::GameAssets;
use crate::components::*;
use crate::config::GameConfig;
use crate::events::{Damage, WaveStarted};
use crate::math::{
    ARENA_HALF, enemy_weights_for_wave, max_enemies_for_wave, spawn_interval_for_wave, steer_towards, weighted_index,
    wrap_position,
};
use crate::player::LockedTarget;
use crate::rng::GameRng;
use crate::schedule::{GameSet, PhysicsSet};
use crate::score::RunStats;
use crate::state::AppState;

/// 波次与刷怪计时。
#[derive(Resource, Reflect, Debug, Clone)]
#[reflect(Resource)]
pub struct WaveDirector {
    pub wave: u32,
    pub spawn_timer: Timer,
    pub wave_timer: Timer,
}

impl WaveDirector {
    pub fn new(config: &GameConfig) -> Self {
        Self {
            wave: 1,
            spawn_timer: Timer::from_seconds(config.enemy_base_spawn_secs, TimerMode::Repeating),
            wave_timer: Timer::from_seconds(config.wave_duration_secs, TimerMode::Repeating),
        }
    }
}

impl Default for WaveDirector {
    fn default() -> Self {
        Self::new(&GameConfig::default())
    }
}

/// 生成某类敌人所需的全部参数（纯数据，便于测试与作弊命令复用）。
#[derive(Debug, Clone, Copy)]
pub struct EnemySpawn {
    pub kind: EnemyKind,
    pub position: Vec2,
    pub velocity: Vec2,
    pub wave: u32,
}

pub fn reset_director(mut commands: Commands, config: Res<GameConfig>) {
    commands.insert_resource(WaveDirector::new(&config));
}

/// 波次推进。
pub fn advance_wave(
    time: Res<Time>,
    config: Res<GameConfig>,
    mut director: ResMut<WaveDirector>,
    mut stats: ResMut<RunStats>,
    mut started: MessageWriter<WaveStarted>,
) {
    if director.wave_timer.tick(time.delta()).just_finished() {
        director.wave += 1;
        let interval =
            spawn_interval_for_wave(config.enemy_base_spawn_secs, director.wave, config.enemy_min_spawn_secs);
        director.spawn_timer.set_duration(std::time::Duration::from_secs_f32(interval));
        stats.wave = director.wave;
        started.write(WaveStarted(director.wave));
    }
}

/// 按计时器和权重生成敌人。
pub fn spawn_enemies(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<GameConfig>,
    assets: Res<GameAssets>,
    census: Res<EnemyCensus>,
    mut director: ResMut<WaveDirector>,
    mut rng: ResMut<GameRng>,
    player: Option<Single<Entity, With<Player>>>,
) {
    if !director.spawn_timer.tick(time.delta()).just_finished() {
        return;
    }
    if census.total_alive() >= max_enemies_for_wave(config.enemy_base_cap, director.wave) {
        return;
    }
    let weights = enemy_weights_for_wave(director.wave);
    let total: u32 = weights.iter().sum();
    let kind = EnemyKind::ALL[weighted_index(&weights, rng.range_u32(0, total))];
    let position = rng.point_on_rect_edge(ARENA_HALF + Vec2::splat(20.0));
    let toward_center = (-position).normalize_or_zero();
    let jitter = rng.unit_vec2() * 0.4;
    let speed = match kind {
        EnemyKind::Meteor => rng.range_f32(config.meteor_speed.0, config.meteor_speed.1),
        EnemyKind::Hunter => config.hunter_speed * 0.5,
        EnemyKind::Carrier => 45.0,
    };
    let spawn = EnemySpawn {
        kind,
        position,
        velocity: (toward_center + jitter).normalize_or_zero() * speed,
        wave: director.wave,
    };
    spawn_enemy(&mut commands, &assets, &config, spawn, player.map(|p| *p));
}

/// 生成单个敌人（供系统、作弊、测试共用）。
pub fn spawn_enemy(
    commands: &mut Commands,
    assets: &GameAssets,
    config: &GameConfig,
    spawn: EnemySpawn,
    player: Option<Entity>,
) -> Entity {
    let kind = spawn.kind;
    let (image, hp, size) = match kind {
        EnemyKind::Meteor => (assets.meteor.clone(), config.meteor_hp, 48.0),
        EnemyKind::Hunter => (assets.hunter.clone(), config.hunter_hp, 34.0),
        EnemyKind::Carrier => (assets.carrier.clone(), config.carrier_hp, 72.0),
    };
    let mut ec = commands.spawn((
        Name::new(format!("{kind:?}")),
        Enemy { kind },
        Health::new(hp),
        Collider { radius: kind.radius() },
        SpawnedInWave(spawn.wave),
        Sprite { image, custom_size: Some(Vec2::splat(size)), ..default() },
        Transform::from_translation(spawn.position.extend(4.0)),
        Velocity(spawn.velocity),
        Pickable::default(),
        DespawnOnExit(AppState::InGame),
    ));
    // 实体级观察者：鼠标悬停高亮、点击锁定
    ec.observe(highlight_on_over).observe(unhighlight_on_out).observe(lock_on_click);

    match kind {
        EnemyKind::Meteor => {
            ec.insert((Wrapping, Spin(spawn.velocity.x.signum() * 1.5)));
        }
        EnemyKind::Hunter => {
            if let Some(p) = player {
                // 关系：猎手 → 玩家
                ec.insert(Targeting(p));
            }
        }
        EnemyKind::Carrier => {
            ec.insert(Spin(0.4));
            let armor = config.carrier_armor;
            let plate_img = assets.plate.clone();
            ec.with_children(|parent| {
                for i in 0..4 {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_2;
                    let offset = Vec2::from_angle(angle) * 40.0;
                    parent
                        .spawn((
                            Name::new(format!("ArmorPlate{i}")),
                            ArmorPlate { absorb: armor, durability: 3 },
                            Sprite { image: plate_img.clone(), custom_size: Some(Vec2::splat(22.0)), ..default() },
                            Transform::from_translation(offset.extend(0.5)),
                        ))
                        // 装甲吸收伤害；不够吸收的部分继续冒泡到父实体（母舰）
                        .observe(armor_absorb);
                }
            });
        }
    }
    ec.id()
}

/// 装甲观察者：削减伤害，全部吸收时截断冒泡；耐久耗尽则销毁。
pub fn armor_absorb(mut damage: On<Damage>, mut plates: Query<&mut ArmorPlate>, mut commands: Commands) {
    let Ok(mut plate) = plates.get_mut(damage.entity) else { return };
    let through = (damage.amount - plate.absorb).max(0);
    plate.durability -= 1;
    if plate.durability <= 0 {
        commands.entity(damage.entity).despawn();
    }
    if through == 0 {
        damage.propagate(false);
    } else {
        damage.amount = through;
    }
}

fn highlight_on_over(over: On<Pointer<Over>>, mut sprites: Query<&mut Sprite>) {
    if let Ok(mut s) = sprites.get_mut(over.event_target()) {
        s.color = Color::srgb(1.6, 1.6, 1.6);
    }
}

fn unhighlight_on_out(out: On<Pointer<Out>>, mut sprites: Query<&mut Sprite>) {
    if let Ok(mut s) = sprites.get_mut(out.event_target()) {
        s.color = Color::WHITE;
    }
}

fn lock_on_click(click: On<Pointer<Click>>, mut locked: ResMut<LockedTarget>) {
    locked.0 = Some(click.event_target());
}

/// 猎手 AI：沿 `Targeting` 关系追踪。`Populated` → 场上没有猎手时整个系统跳过。
pub fn hunter_ai(
    time: Res<Time>,
    config: Res<GameConfig>,
    mut hunters: Populated<(&mut Velocity, &mut Transform, &Targeting, &Enemy)>,
    targets: Query<&GlobalTransform>,
) {
    let dt = time.delta_secs();
    for (mut vel, mut tf, targeting, enemy) in hunters.iter_mut() {
        if enemy.kind != EnemyKind::Hunter {
            continue;
        }
        let Ok(target) = targets.get(targeting.0) else { continue };
        vel.0 = steer_towards(
            vel.0,
            tf.translation.truncate(),
            target.translation().truncate(),
            config.hunter_speed,
            config.hunter_turn_rate,
            dt,
        );
        if let Ok(dir) = Dir2::new(vel.0) {
            tf.rotation = crate::math::rotation_for_direction(dir);
        }
    }
}

/// 固定步长积分：所有带 Velocity 的实体（玩家单独处理）。
pub fn integrate_motion(time: Res<Time>, mut q: Query<(&mut Transform, &Velocity, Option<&Spin>), Without<Player>>) {
    let dt = time.delta_secs();
    for (mut tf, vel, spin) in &mut q {
        tf.translation += (vel.0 * dt).extend(0.0);
        if let Some(spin) = spin {
            tf.rotate_z(spin.0 * dt);
        }
    }
}

/// 卷绕：陨石从一边出去从另一边回来。
pub fn wrap_entities(mut q: Query<(&mut Transform, &Collider), With<Wrapping>>) {
    for (mut tf, col) in &mut q {
        let p = wrap_position(tf.translation.truncate(), ARENA_HALF, col.radius);
        tf.translation.x = p.x;
        tf.translation.y = p.y;
    }
}

/// 陨石互相弹开：`iter_combinations_mut` 遍历所有无序对。
pub fn meteor_bounce(mut q: Query<(&mut Transform, &mut Velocity, &Collider, &Enemy), With<Wrapping>>) {
    let mut combos = q.iter_combinations_mut();
    while let Some([(mut ta, mut va, ca, _), (mut tb, mut vb, cb, _)]) = combos.fetch_next() {
        let pa = ta.translation.truncate();
        let pb = tb.translation.truncate();
        let delta = pb - pa;
        let min_d = ca.radius + cb.radius;
        let d2 = delta.length_squared();
        if d2 > 0.0 && d2 < min_d * min_d {
            let n = delta / d2.sqrt();
            // 只在相向运动时交换法向速度（弹性碰撞，等质量）
            let rel = (vb.0 - va.0).dot(n);
            if rel < 0.0 {
                va.0 += n * rel;
                vb.0 -= n * rel;
            }
            // 位置分离，防止粘连
            let push = n * (min_d - d2.sqrt()) * 0.5;
            ta.translation -= push.extend(0.0);
            tb.translation += push.extend(0.0);
        }
    }
}

/// 离场太远的非卷绕敌人回收（母舰/猎手偶尔会飞出去）。
pub fn cull_far_enemies(mut commands: Commands, q: Query<(Entity, &Transform), (With<Enemy>, Without<Wrapping>)>) {
    for (e, tf) in &q {
        if crate::math::is_outside_arena(tf.translation.truncate(), ARENA_HALF, 300.0) {
            commands.entity(e).despawn();
        }
    }
}

/// `Added<Enemy>`：新敌人出生时从 0 缩放弹出。
pub fn spawn_pop_in(mut q: Query<&mut Transform, Added<Enemy>>) {
    for mut tf in &mut q {
        tf.scale = Vec3::splat(0.2);
    }
}

pub fn grow_to_full_scale(time: Res<Time>, mut q: Query<&mut Transform, With<Enemy>>) {
    let k = (time.delta_secs() * 8.0).min(1.0);
    for mut tf in &mut q {
        if tf.scale.x < 1.0 {
            let s = tf.scale.x + (1.0 - tf.scale.x) * k;
            tf.scale = Vec3::new(s, s, 1.0);
        }
    }
}

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EnemyCensus>()
            .init_resource::<WaveDirector>()
            .add_systems(OnEnter(AppState::InGame), reset_director)
            .add_systems(
                FixedUpdate,
                (hunter_ai, integrate_motion, wrap_entities, meteor_bounce).chain().in_set(PhysicsSet),
            )
            .add_systems(
                Update,
                (advance_wave, spawn_enemies, spawn_pop_in, grow_to_full_scale, cull_far_enemies)
                    .chain()
                    .in_set(GameSet::Simulate),
            );
    }
}
