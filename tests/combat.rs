//! 战斗集成测试：投射物命中、事件冒泡（装甲→母舰）、接触伤害、护盾、无敌帧、
//! 对象池（Disabled）复用、导弹关系追踪、一次性系统作弊。

mod common;

use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy_demo01::assets::GameAssets;
use bevy_demo01::components::*;
use bevy_demo01::config::GameConfig;
use bevy_demo01::debug::Cheats;
use bevy_demo01::enemy::{EnemySpawn, spawn_enemy};
use bevy_demo01::events::{Damage, EnemyDestroyed, PlayerDamaged};
use bevy_demo01::input::{IntentSource, PlayerIntent};
use bevy_demo01::player::BulletPoolStats;
use bevy_demo01::score::{RunStats, Score};
use bevy_demo01::state::AppState;
use common::*;

fn game(tag: &str) -> App {
    let mut app = test_app(tag);
    app.insert_resource(IntentSource::Scripted);
    // 关掉自然刷怪/刷星干扰：把间隔调到极大
    {
        let mut cfg = app.world_mut().resource_mut::<GameConfig>();
        cfg.enemy_base_spawn_secs = 10_000.0;
        cfg.star_spawn_secs = 10_000.0;
        cfg.powerup_drop_chance = 0.0;
    }
    goto(&mut app, AppState::InGame);
    app
}

fn spawn(app: &mut App, kind: EnemyKind, pos: Vec2) -> Entity {
    app.world_mut()
        .run_system_once(move |mut commands: Commands, assets: Res<GameAssets>, cfg: Res<GameConfig>| {
            spawn_enemy(
                &mut commands,
                &assets,
                &cfg,
                EnemySpawn { kind, position: pos, velocity: Vec2::ZERO, wave: 1 },
                None,
            )
        })
        .unwrap()
}

fn hp(app: &App, e: Entity) -> i32 {
    app.world().get::<Health>(e).map_or(-999, |h| h.current)
}

fn set_intent(app: &mut App, f: impl FnOnce(&mut PlayerIntent)) {
    f(&mut app.world_mut().resource_mut::<PlayerIntent>());
}

#[test]
fn bullets_destroy_meteor_and_award_score() {
    let mut app = game("bullets");
    let meteor = spawn(&mut app, EnemyKind::Meteor, Vec2::new(0.0, 150.0));
    app.world_mut().get_mut::<Health>(meteor).unwrap().current = 2;
    set_intent(&mut app, |i| {
        i.fire = true;
        i.aim_stick = Some(Vec2::Y);
    });
    let mut destroyed = 0;
    for _ in 0..120 {
        app.update();
        destroyed += recent::<EnemyDestroyed>(&mut app).len();
        if app.world().get_entity(meteor).is_err() {
            break;
        }
    }
    assert!(app.world().get_entity(meteor).is_err(), "陨石应被击毁");
    assert!(destroyed >= 1);
    run(&mut app, 2);
    assert!(app.world().resource::<Score>().points >= EnemyKind::Meteor.score() as u64);
    assert_eq!(app.world().resource::<RunStats>().kills[0], 1);
    assert!(app.world().resource::<RunStats>().shots >= 2);
}

#[test]
fn armor_plate_absorbs_and_propagates_damage_to_carrier() {
    let mut app = game("armor");
    let carrier = spawn(&mut app, EnemyKind::Carrier, Vec2::new(300.0, 200.0));
    app.update();
    let plates: Vec<Entity> = app.world().get::<Children>(carrier).unwrap().iter().collect();
    assert_eq!(plates.len(), 4);
    let full = hp(&app, carrier);
    let armor = app.world().resource::<GameConfig>().carrier_armor;

    // 伤害 <= 装甲：被完全吸收，冒泡被截断
    app.world_mut().trigger(Damage { entity: plates[0], amount: armor });
    assert_eq!(hp(&app, carrier), full, "完全吸收");

    // 伤害 > 装甲：穿透部分冒泡到父实体
    app.world_mut().trigger(Damage { entity: plates[1], amount: armor + 3 });
    assert_eq!(hp(&app, carrier), full - 3, "穿透 3 点");

    // 直接打本体：全额
    app.world_mut().trigger(Damage { entity: carrier, amount: 2 });
    assert_eq!(hp(&app, carrier), full - 5);

    // 装甲耐久 3：再打两次同一块 → 被销毁
    for _ in 0..2 {
        app.world_mut().trigger(Damage { entity: plates[0], amount: 1 });
    }
    app.update();
    assert!(app.world().get_entity(plates[0]).is_err(), "装甲耐久耗尽被销毁");
    assert_eq!(app.world().get::<Children>(carrier).unwrap().len(), 3);
}

#[test]
fn carrier_death_despawns_children_and_counts_kill() {
    let mut app = game("carrier_death");
    let carrier = spawn(&mut app, EnemyKind::Carrier, Vec2::new(300.0, 200.0));
    app.update();
    let plates: Vec<Entity> = app.world().get::<Children>(carrier).unwrap().iter().collect();
    app.world_mut().trigger(Damage { entity: carrier, amount: 999 });
    run(&mut app, 2);
    assert!(app.world().get_entity(carrier).is_err());
    for p in plates {
        assert!(app.world().get_entity(p).is_err(), "despawn 递归销毁子实体");
    }
    assert_eq!(app.world().resource::<RunStats>().kills[2], 1);
    assert_eq!(app.world().resource::<EnemyCensus>().count(EnemyKind::Carrier), 0);
}

#[test]
fn contact_damage_invulnerability_and_messages() {
    let mut app = game("contact");
    let player = single::<Player>(&mut app);
    let max = hp(&app, player);
    let meteor = spawn(&mut app, EnemyKind::Meteor, Vec2::ZERO);
    app.world_mut().get_mut::<Health>(meteor).unwrap().current = 999;
    run(&mut app, 2);
    assert_eq!(hp(&app, player), max - 1);
    assert!(app.world().get::<Invulnerable>(player).is_some());
    let msgs = recent::<PlayerDamaged>(&mut app);
    assert_eq!(msgs.last().map(|m| m.remaining), Some(max - 1));

    // 无敌期间持续接触不再掉血
    run(&mut app, 30);
    assert_eq!(hp(&app, player), max - 1);

    // 无敌结束（1.2s）后再次掉血
    run(&mut app, 90);
    assert!(hp(&app, player) < max - 1);
}

#[test]
fn shield_absorbs_one_hit() {
    let mut app = game("shield");
    let player = single::<Player>(&mut app);
    let max = hp(&app, player);
    let shield = app.world().get::<Children>(player).unwrap()[0];
    app.world_mut().get_mut::<Shield>(shield).unwrap().charges = 1;
    app.update();
    assert_eq!(*app.world().get::<Visibility>(shield).unwrap(), Visibility::Inherited, "有护盾时显示");

    let m = spawn(&mut app, EnemyKind::Meteor, Vec2::ZERO);
    app.world_mut().get_mut::<Health>(m).unwrap().current = 999;
    run(&mut app, 3);
    assert_eq!(hp(&app, player), max, "护盾吸收");
    assert_eq!(app.world().get::<Shield>(shield).unwrap().charges, 0);
    assert_eq!(*app.world().get::<Visibility>(shield).unwrap(), Visibility::Hidden);
}

#[test]
fn bullet_pool_reuses_disabled_entities() {
    let mut app = game("pool");
    set_intent(&mut app, |i| {
        i.fire = true;
        i.aim_stick = Some(Vec2::X);
    });
    // 连续开火 3 秒：子弹寿命 1.2s，之后应开始复用
    run(&mut app, 180);
    let stats = *app.world().resource::<BulletPoolStats>();
    assert!(stats.spawned > 0);
    assert!(stats.reused > 0, "应有复用: {stats:?}");
    let total = {
        let mut q = app.world_mut().query_filtered::<&Projectile, Allow<Disabled>>();
        q.iter(app.world()).count()
    };
    assert_eq!(total as u32, stats.spawned, "实体总数 = 新建数（复用不新建）");
    assert!(total < 20, "池大小有界: {total}");

    // 停火后全部进池
    set_intent(&mut app, |i| i.fire = false);
    run(&mut app, 120);
    assert_eq!(count::<(With<Projectile>, Without<Disabled>)>(&mut app), 0);
    let pooled = {
        let mut q = app.world_mut().query_filtered::<(), (With<Projectile>, With<Disabled>)>();
        q.iter(app.world()).count()
    };
    assert_eq!(pooled as u32, stats.spawned);
}

#[test]
fn missile_targets_enemy_via_relationship_and_forgets_when_target_dies() {
    let mut app = game("missile");
    let enemy = spawn(&mut app, EnemyKind::Meteor, Vec2::new(400.0, 250.0));
    app.world_mut().get_mut::<Health>(enemy).unwrap().current = 999;
    app.update();
    set_intent(&mut app, |i| i.missile = true);
    app.update();
    set_intent(&mut app, |i| i.missile = false);
    app.update();
    let missile = {
        let mut q = app.world_mut().query::<(Entity, &Projectile, &Targeting)>();
        let (e, p, t) = q.single(app.world()).expect("一枚导弹");
        assert_eq!(p.kind, ProjectileKind::Missile);
        assert_eq!(t.0, enemy);
        e
    };
    assert!(app.world().get::<TargetedBy>(enemy).unwrap().entities().contains(&missile));

    app.world_mut().despawn(enemy);
    app.update();
    if app.world().get_entity(missile).is_ok() {
        assert!(app.world().get::<Targeting>(missile).is_none(), "目标消失 → 关系组件被自动移除");
    }
}

#[test]
fn hunters_target_player() {
    let mut app = game("hunter");
    let player = single::<Player>(&mut app);
    let hunter = app
        .world_mut()
        .run_system_once(move |mut commands: Commands, assets: Res<GameAssets>, cfg: Res<GameConfig>| {
            spawn_enemy(
                &mut commands,
                &assets,
                &cfg,
                EnemySpawn { kind: EnemyKind::Hunter, position: Vec2::new(-500.0, 0.0), velocity: Vec2::ZERO, wave: 1 },
                Some(player),
            )
        })
        .unwrap();
    app.update();
    assert_eq!(app.world().get::<Targeting>(hunter).unwrap().0, player);
    let start = app.world().get::<Transform>(hunter).unwrap().translation.x;
    run(&mut app, 60);
    let end = app.world().get::<Transform>(hunter).unwrap().translation.x;
    assert!(end > start + 50.0, "猎手应朝玩家（右侧）移动: {start} → {end}");
}

#[test]
fn nuke_one_shot_system_kills_everything() {
    let mut app = game("nuke");
    for i in 0..5 {
        spawn(&mut app, EnemyKind::Meteor, Vec2::new(-300.0 + i as f32 * 120.0, 250.0));
    }
    app.update();
    assert_eq!(app.world().resource::<EnemyCensus>().total_alive(), 5);
    let nuke = app.world().resource::<Cheats>().nuke;
    app.world_mut().run_system(nuke).unwrap();
    run(&mut app, 2);
    assert_eq!(app.world().resource::<EnemyCensus>().total_alive(), 0);
    assert_eq!(app.world().resource::<RunStats>().kills[0], 5);
}

#[test]
fn nuke_via_keyboard_run_condition() {
    let mut app = game("nuke_key");
    spawn(&mut app, EnemyKind::Hunter, Vec2::new(300.0, 300.0));
    app.update();
    tap(&mut app, KeyCode::KeyK);
    run(&mut app, 2);
    assert_eq!(app.world().resource::<EnemyCensus>().total_alive(), 0);
}

#[test]
fn cached_one_shot_spawns_carrier() {
    let mut app = game("carrier_key");
    tap(&mut app, KeyCode::KeyC);
    app.update();
    assert_eq!(app.world().resource::<EnemyCensus>().count(EnemyKind::Carrier), 1);
}
