//! 所有 ECS 组件。
//!
//! 覆盖 API：
//! - `#[derive(Component)]` + `#[require(...)]`（必需组件，带默认值构造）
//! - `#[component(storage = "SparseSet")]`（频繁增删的组件）
//! - `#[component(immutable)]`（不可变组件，只能整体替换）
//! - `#[component(on_add = .., on_remove = ..)]` 组件生命周期钩子 + `DeferredWorld` + `HookContext`
//! - `#[relationship]` / `#[relationship_target]` 自定义关系（`Targeting` ↔ `TargetedBy`）
//! - `Reflect` + `#[reflect(Component)]`、`Deref/DerefMut`、`Bundle` derive

use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;

// ---------------------------------------------------------------------------
// 通用物理
// ---------------------------------------------------------------------------

/// 线速度（世界单位/秒），由 `FixedUpdate` 积分。
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq, Deref, DerefMut)]
#[reflect(Component, Default)]
pub struct Velocity(pub Vec2);

/// 角速度（弧度/秒）。
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq)]
#[reflect(Component)]
pub struct Spin(pub f32);

/// 圆形碰撞体半径。
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component)]
pub struct Collider {
    pub radius: f32,
}

impl Default for Collider {
    fn default() -> Self {
        Self { radius: 8.0 }
    }
}

/// 阵营：决定谁能伤害谁。
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[reflect(Component)]
pub enum Team {
    #[default]
    Neutral,
    Player,
    Enemy,
}

/// 越界后卷绕到另一边（陨石）。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component)]
pub struct Wrapping;

/// 生命值。
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component)]
pub struct Health {
    pub current: i32,
    pub max: i32,
}

impl Health {
    pub fn new(max: i32) -> Self {
        Self { current: max, max }
    }
    pub fn is_dead(&self) -> bool {
        self.current <= 0
    }
    pub fn fraction(&self) -> f32 {
        if self.max <= 0 { 0.0 } else { (self.current.max(0) as f32 / self.max as f32).clamp(0.0, 1.0) }
    }
    /// 扣血，返回实际扣掉的量。
    pub fn apply(&mut self, amount: i32) -> i32 {
        let before = self.current;
        self.current = (self.current - amount.max(0)).max(0);
        before - self.current
    }
    /// 回血，不超过上限。
    pub fn heal(&mut self, amount: i32) {
        self.current = (self.current + amount.max(0)).min(self.max);
    }
}

impl Default for Health {
    fn default() -> Self {
        Self::new(1)
    }
}

/// 限时存在，到期后回收（粒子、飘字、子弹）。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component)]
pub struct Lifetime(pub Timer);

impl Lifetime {
    pub fn secs(secs: f32) -> Self {
        Self(Timer::from_seconds(secs, TimerMode::Once))
    }
}

// ---------------------------------------------------------------------------
// 玩家
// ---------------------------------------------------------------------------

/// 玩家飞船。`#[require]` 保证只插 `Player` 也能得到完整实体。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component)]
#[require(
    Transform,
    Visibility,
    Velocity,
    Team = Team::Player,
    Health = Health::new(5),
    Collider = Collider { radius: 16.0 },
    FireCooldown,
    MissileCooldown,
    Name = Name::new("Player")
)]
pub struct Player;

/// 射击冷却。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component)]
pub struct FireCooldown(pub Timer);

impl Default for FireCooldown {
    fn default() -> Self {
        // 初始即“已完成”，第一发不需要等待
        let mut t = Timer::from_seconds(0.16, TimerMode::Once);
        t.finish();
        Self(t)
    }
}

/// 导弹冷却。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component)]
pub struct MissileCooldown(pub Timer);

impl Default for MissileCooldown {
    fn default() -> Self {
        let mut t = Timer::from_seconds(1.5, TimerMode::Once);
        t.finish();
        Self(t)
    }
}

/// 受击后短暂无敌。频繁增删 → SparseSet 存储，避免 archetype 搬家。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component)]
#[component(storage = "SparseSet")]
pub struct Invulnerable(pub Timer);

/// 连射 buff。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component)]
#[component(storage = "SparseSet")]
pub struct RapidFire(pub Timer);

/// 护盾（玩家的子实体，可抵挡一次伤害）。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component)]
pub struct Shield {
    pub charges: u32,
}

// ---------------------------------------------------------------------------
// 敌人
// ---------------------------------------------------------------------------

#[derive(Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnemyKind {
    Meteor,
    Hunter,
    Carrier,
}

impl EnemyKind {
    pub const ALL: [EnemyKind; 3] = [EnemyKind::Meteor, EnemyKind::Hunter, EnemyKind::Carrier];

    pub fn score(self) -> u32 {
        match self {
            EnemyKind::Meteor => 25,
            EnemyKind::Hunter => 60,
            EnemyKind::Carrier => 250,
        }
    }

    pub fn radius(self) -> f32 {
        match self {
            EnemyKind::Meteor => 22.0,
            EnemyKind::Hunter => 16.0,
            EnemyKind::Carrier => 34.0,
        }
    }

    pub fn index(self) -> usize {
        match self {
            EnemyKind::Meteor => 0,
            EnemyKind::Hunter => 1,
            EnemyKind::Carrier => 2,
        }
    }
}

/// 敌人。挂了组件钩子：增删时自动维护 [`EnemyCensus`]。
#[derive(Component, Reflect, Debug, Clone, Copy)]
#[reflect(Component)]
#[require(Transform, Visibility, Velocity, Team = Team::Enemy, Health, Collider)]
#[component(on_add = on_enemy_add, on_remove = on_enemy_remove)]
pub struct Enemy {
    pub kind: EnemyKind,
}

/// 按类型统计场上敌人数量。由组件钩子维护，O(1) 查询。
#[derive(Resource, Reflect, Debug, Default, Clone, PartialEq, Eq)]
#[reflect(Resource)]
pub struct EnemyCensus {
    pub alive: [u32; 3],
    pub total_spawned: u32,
}

impl EnemyCensus {
    pub fn total_alive(&self) -> u32 {
        self.alive.iter().sum()
    }
    pub fn count(&self, kind: EnemyKind) -> u32 {
        self.alive[kind.index()]
    }
}

fn on_enemy_add(mut world: DeferredWorld, ctx: HookContext) {
    let Some(kind) = world.get::<Enemy>(ctx.entity).map(|e| e.kind) else {
        return;
    };
    if let Some(mut census) = world.get_resource_mut::<EnemyCensus>() {
        census.alive[kind.index()] += 1;
        census.total_spawned += 1;
    }
}

fn on_enemy_remove(mut world: DeferredWorld, ctx: HookContext) {
    // on_remove 在组件真正移除前触发，所以还能读到数据
    let Some(kind) = world.get::<Enemy>(ctx.entity).map(|e| e.kind) else {
        return;
    };
    if let Some(mut census) = world.get_resource_mut::<EnemyCensus>() {
        let slot = &mut census.alive[kind.index()];
        *slot = slot.saturating_sub(1);
    }
}

/// 母舰装甲片（母舰的子实体）。伤害先打到装甲，穿透部分沿层级冒泡到母舰。
#[derive(Component, Reflect, Debug, Clone, Copy)]
#[reflect(Component)]
#[require(Transform, Visibility, Collider = Collider { radius: 12.0 }, Team = Team::Enemy)]
pub struct ArmorPlate {
    pub absorb: i32,
    pub durability: i32,
}

/// 生成它的波次。不可变组件：只能整体替换，不能 `&mut` 改。
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component)]
#[component(immutable)]
pub struct SpawnedInWave(pub u32);

// ---------------------------------------------------------------------------
// 关系：谁在追踪谁（猎手 → 玩家，导弹 → 敌人）
// ---------------------------------------------------------------------------

/// 关系源：我正在追踪的实体。
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component)]
#[relationship(relationship_target = TargetedBy)]
pub struct Targeting(pub Entity);

/// 关系目标：所有正在追踪我的实体（由 Bevy 自动维护，别手动改）。
#[derive(Component, Reflect, Debug, Default)]
#[reflect(Component)]
#[relationship_target(relationship = Targeting)]
pub struct TargetedBy(Vec<Entity>);

impl TargetedBy {
    pub fn entities(&self) -> &[Entity] {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// 投射物
// ---------------------------------------------------------------------------

#[derive(Reflect, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileKind {
    Bullet,
    Missile,
}

/// 投射物。子弹走对象池（`Disabled` 复用），导弹直接 spawn/despawn。
#[derive(Component, Reflect, Debug, Clone, Copy)]
#[reflect(Component)]
#[require(Transform, Visibility, Velocity, Collider = Collider { radius: 5.0 })]
pub struct Projectile {
    pub kind: ProjectileKind,
    pub damage: i32,
    pub team: Team,
}

/// 导弹制导参数。
#[derive(Component, Reflect, Debug, Clone, Copy)]
#[reflect(Component)]
pub struct Homing {
    pub speed: f32,
    pub turn_rate: f32,
}

// ---------------------------------------------------------------------------
// 收集物
// ---------------------------------------------------------------------------

#[derive(Component, Reflect, Debug, Clone, Copy)]
#[reflect(Component)]
#[require(Transform, Visibility, Collider = Collider { radius: 12.0 }, Name = Name::new("Star"))]
pub struct Star {
    pub value: u32,
    pub phase: f32,
}

#[derive(Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerUpKind {
    Shield,
    RapidFire,
    Heal,
}

impl PowerUpKind {
    pub const ALL: [PowerUpKind; 3] = [PowerUpKind::Shield, PowerUpKind::RapidFire, PowerUpKind::Heal];

    pub fn color(self) -> Color {
        match self {
            PowerUpKind::Shield => Color::srgb(0.3, 0.8, 1.0),
            PowerUpKind::RapidFire => Color::srgb(1.0, 0.55, 0.1),
            PowerUpKind::Heal => Color::srgb(0.3, 1.0, 0.4),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PowerUpKind::Shield => "SHIELD",
            PowerUpKind::RapidFire => "RAPID",
            PowerUpKind::Heal => "+HP",
        }
    }
}

#[derive(Component, Reflect, Debug, Clone, Copy)]
#[reflect(Component)]
#[require(Transform, Visibility, Collider = Collider { radius: 14.0 }, Lifetime = Lifetime::secs(9.0))]
pub struct PowerUp(pub PowerUpKind);

// ---------------------------------------------------------------------------
// 表现相关（逻辑层也会用到的标记）
// ---------------------------------------------------------------------------

/// 爆炸碎片粒子。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component)]
#[require(Transform, Visibility, Velocity)]
pub struct Particle {
    pub start_alpha: f32,
}

/// 飘字。
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component)]
pub struct FloatingText {
    pub rise_speed: f32,
}

/// 精灵帧动画配置（逐帧切 `TextureAtlas::index`）。
#[derive(Component, Reflect, Debug, Clone)]
#[reflect(Component)]
pub struct SpriteAnimation {
    pub first: usize,
    pub last: usize,
    pub timer: Timer,
}

impl SpriteAnimation {
    pub fn new(first: usize, last: usize, fps: f32) -> Self {
        Self { first, last, timer: Timer::from_seconds(1.0 / fps.max(0.001), TimerMode::Repeating) }
    }

    /// 纯函数：给定当前帧，返回下一帧（循环）。
    pub fn next_frame(&self, current: usize) -> usize {
        if current >= self.last || current < self.first { self.first } else { current + 1 }
    }
}

/// 一组常用组件打包。演示 `#[derive(Bundle)]`（虽然 required components 让它不再必需）。
#[derive(Bundle, Default)]
pub struct PhysicsBundle {
    pub velocity: Velocity,
    pub collider: Collider,
    pub team: Team,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn health_math() {
        let mut h = Health::new(5);
        assert_eq!(h.apply(2), 2);
        assert_eq!(h.current, 3);
        assert_eq!(h.apply(10), 3, "不会扣成负数");
        assert!(h.is_dead());
        assert_eq!(h.apply(-5), 0, "负伤害不回血");
        h.heal(100);
        assert_eq!(h.current, 5);
        assert!((h.fraction() - 1.0).abs() < 1e-6);
        let zero = Health { current: 0, max: 0 };
        assert_eq!(zero.fraction(), 0.0);
    }

    #[test]
    fn required_components_are_inserted() {
        let mut world = World::new();
        let e = world.spawn(Player).id();
        let entity = world.entity(e);
        assert!(entity.contains::<Transform>());
        assert!(entity.contains::<Velocity>());
        assert!(entity.contains::<FireCooldown>());
        assert_eq!(entity.get::<Team>(), Some(&Team::Player));
        assert_eq!(entity.get::<Health>().unwrap().max, 5);
        assert_eq!(entity.get::<Collider>().unwrap().radius, 16.0);
        assert_eq!(entity.get::<Name>().unwrap().as_str(), "Player");
    }

    #[test]
    fn explicit_component_overrides_required_default() {
        let mut world = World::new();
        let e = world.spawn((Player, Health::new(9))).id();
        assert_eq!(world.get::<Health>(e).unwrap().max, 9);
    }

    #[test]
    fn enemy_hooks_maintain_census() {
        let mut world = World::new();
        world.init_resource::<EnemyCensus>();
        let a = world.spawn(Enemy { kind: EnemyKind::Meteor }).id();
        let _b = world.spawn(Enemy { kind: EnemyKind::Meteor }).id();
        let c = world.spawn(Enemy { kind: EnemyKind::Carrier }).id();
        {
            let census = world.resource::<EnemyCensus>();
            assert_eq!(census.count(EnemyKind::Meteor), 2);
            assert_eq!(census.count(EnemyKind::Carrier), 1);
            assert_eq!(census.total_alive(), 3);
            assert_eq!(census.total_spawned, 3);
        }
        world.despawn(a);
        world.entity_mut(c).remove::<Enemy>();
        let census = world.resource::<EnemyCensus>();
        assert_eq!(census.count(EnemyKind::Meteor), 1);
        assert_eq!(census.count(EnemyKind::Carrier), 0);
        assert_eq!(census.total_spawned, 3, "total_spawned 只增不减");
    }

    #[test]
    fn hooks_without_census_resource_do_not_panic() {
        let mut world = World::new();
        let e = world.spawn(Enemy { kind: EnemyKind::Hunter }).id();
        world.despawn(e);
    }

    #[test]
    fn relationship_is_maintained_automatically() {
        let mut world = World::new();
        let player = world.spawn(Name::new("p")).id();
        let h1 = world.spawn(Targeting(player)).id();
        let h2 = world.spawn(Targeting(player)).id();
        let by = world.get::<TargetedBy>(player).unwrap();
        assert_eq!(by.entities().len(), 2);
        assert!(by.entities().contains(&h1) && by.entities().contains(&h2));

        world.despawn(h1);
        assert_eq!(world.get::<TargetedBy>(player).unwrap().entities(), &[h2]);

        // 目标被销毁 → 关系源组件被自动移除
        world.despawn(player);
        assert!(world.get::<Targeting>(h2).is_none());
    }

    #[test]
    fn relationship_retarget_by_reinsert() {
        let mut world = World::new();
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let m = world.spawn(Targeting(a)).id();
        world.entity_mut(m).insert(Targeting(b));
        assert!(world.get::<TargetedBy>(a).is_none(), "空的 RelationshipTarget 会被移除");
        assert_eq!(world.get::<TargetedBy>(b).unwrap().entities(), &[m]);
    }

    #[test]
    fn immutable_component_can_be_replaced_not_mutated() {
        let mut world = World::new();
        let e = world.spawn(SpawnedInWave(1)).id();
        world.entity_mut(e).insert(SpawnedInWave(2));
        assert_eq!(world.get::<SpawnedInWave>(e), Some(&SpawnedInWave(2)));
        // `world.get_mut::<SpawnedInWave>` 无法编译 —— 这正是 immutable 的意义
    }

    #[test]
    fn sprite_animation_wraps() {
        let a = SpriteAnimation::new(1, 3, 10.0);
        assert_eq!(a.next_frame(1), 2);
        assert_eq!(a.next_frame(2), 3);
        assert_eq!(a.next_frame(3), 1);
        assert_eq!(a.next_frame(0), 1, "越界帧回到 first");
        assert_eq!(a.next_frame(99), 1);
    }

    #[test]
    fn bundle_spawns_all_fields() {
        let mut world = World::new();
        let e = world.spawn(PhysicsBundle::default()).id();
        assert!(world.entity(e).contains::<Velocity>());
        assert!(world.entity(e).contains::<Collider>());
        assert!(world.entity(e).contains::<Team>());
    }

    #[test]
    fn enemy_kind_tables_are_consistent() {
        for (i, k) in EnemyKind::ALL.iter().enumerate() {
            assert_eq!(k.index(), i);
            assert!(k.score() > 0 && k.radius() > 0.0);
        }
        for p in PowerUpKind::ALL {
            assert!(!p.label().is_empty());
        }
    }

    #[test]
    fn run_system_once_on_bare_world() {
        let mut world = World::new();
        world.spawn((Velocity(Vec2::X), Transform::default()));
        let n = world.run_system_once(|q: Query<&Velocity>| q.iter().count()).unwrap();
        assert_eq!(n, 1);
    }
}
