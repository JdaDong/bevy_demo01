//! ECS / 引擎特性集成测试：输入设备、固定步长、延迟命令、波次、拾取、buff、HUD、
//! 反射序列化、确定性回放、时间缩放。

mod common;

use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::reflect::FromReflect;
use bevy::reflect::serde::{ReflectDeserializer, ReflectSerializer};
use bevy_demo01::assets::GameAssets;
use bevy_demo01::collectibles::spawn_star;
use bevy_demo01::combat::spawn_powerup;
use bevy_demo01::components::*;
use bevy_demo01::config::GameConfig;
use bevy_demo01::events::{StarCollected, WaveStarted};
use bevy_demo01::input::{IntentSource, PlayerIntent};
use bevy_demo01::save::snapshot_player;
use bevy_demo01::score::{RunStats, Score};
use bevy_demo01::state::AppState;
use bevy_demo01::ui::hud::{HeartRow, ScoreText, WaveBanner};
use common::*;

fn quiet_game(tag: &str, scripted: bool) -> App {
    let mut app = test_app(tag);
    if scripted {
        app.insert_resource(IntentSource::Scripted);
    }
    {
        let mut cfg = app.world_mut().resource_mut::<GameConfig>();
        cfg.enemy_base_spawn_secs = 10_000.0;
        cfg.star_spawn_secs = 10_000.0;
    }
    goto(&mut app, AppState::InGame);
    app
}

fn player_pos(app: &mut App) -> Vec2 {
    let p = single::<Player>(app);
    app.world().get::<Transform>(p).unwrap().translation.truncate()
}

#[test]
fn keyboard_moves_player_and_arena_clamps() {
    let mut app = quiet_game("keys", false);
    press(&mut app, KeyCode::KeyD);
    run(&mut app, 30);
    let p = player_pos(&mut app);
    assert!(p.x > 50.0, "按住 D 应向右移动: {p:?}");
    assert!(p.y.abs() < 1.0);
    // 一直按住 → 被夹在竞技场内
    run(&mut app, 300);
    let p = player_pos(&mut app);
    let cfg = app.world().resource::<GameConfig>().clone();
    assert!((p.x - (640.0 - cfg.player_radius)).abs() < 0.5, "夹边: {p:?}");
    release_all(&mut app);
}

#[test]
fn gamepad_left_stick_drives_intent() {
    let mut app = quiet_game("pad", false);
    let mut pad = Gamepad::default();
    pad.analog_mut().set(GamepadAxis::LeftStickY, 1.0);
    pad.digital_mut().press(GamepadButton::South);
    app.world_mut().spawn(pad);
    app.update();
    let intent = app.world().resource::<PlayerIntent>().clone();
    assert!((intent.movement - Vec2::Y).length() < 1e-4, "{intent:?}");
    assert!(intent.fire, "South 键开火");
}

#[test]
fn fixed_timestep_follows_config() {
    let mut app = test_app("fixed");
    app.world_mut().resource_mut::<GameConfig>().fixed_hz = 30.0;
    app.update();
    let step = app.world().resource::<Time<Fixed>>().timestep();
    assert!((step.as_secs_f64() - 1.0 / 30.0).abs() < 1e-6, "{step:?}");
}

#[test]
fn wave_advances_and_banner_despawns_via_delayed_commands() {
    let mut app = test_app("wave");
    app.insert_resource(IntentSource::Scripted);
    {
        let mut cfg = app.world_mut().resource_mut::<GameConfig>();
        cfg.wave_duration_secs = 0.5;
        cfg.enemy_base_spawn_secs = 10_000.0;
    }
    goto(&mut app, AppState::InGame);
    let mut saw_wave = false;
    for _ in 0..40 {
        app.update();
        if recent::<WaveStarted>(&mut app).iter().any(|w| w.0 == 2) {
            saw_wave = true;
            break;
        }
    }
    assert!(saw_wave, "0.5s 后进入第 2 波");
    app.update();
    assert_eq!(app.world().resource::<RunStats>().wave, 2);
    assert!(count::<With<WaveBanner>>(&mut app) >= 1, "横幅出现");

    // 调长波次时长，避免新横幅干扰；2 秒后横幅被延迟命令销毁
    app.world_mut().resource_mut::<bevy_demo01::enemy::WaveDirector>().wave_timer =
        Timer::from_seconds(1000.0, TimerMode::Repeating);
    run(&mut app, 140);
    assert_eq!(count::<With<WaveBanner>>(&mut app), 0, "delayed().secs(2.0) 自动销毁");
}

#[test]
fn star_pickup_awards_score_with_popup() {
    let mut app = quiet_game("star", true);
    app.world_mut()
        .run_system_once(|mut c: Commands, a: Res<GameAssets>| {
            spawn_star(&mut c, &a, 10, Vec2::new(5.0, 0.0), 0.0);
        })
        .unwrap();
    run(&mut app, 3);
    let collected = recent::<StarCollected>(&mut app);
    assert!(!collected.is_empty() || app.world().resource::<RunStats>().stars == 1);
    run(&mut app, 2);
    assert_eq!(app.world().resource::<RunStats>().stars, 1);
    assert_eq!(app.world().resource::<Score>().points, 10);
    assert_eq!(count::<With<Star>>(&mut app), 0);
    assert!(count::<With<FloatingText>>(&mut app) >= 1, "飘字");
}

#[test]
fn magnet_pulls_nearby_star() {
    let mut app = quiet_game("magnet", true);
    let star = app
        .world_mut()
        .run_system_once(|mut c: Commands, a: Res<GameAssets>| spawn_star(&mut c, &a, 10, Vec2::new(100.0, 0.0), 0.0))
        .unwrap();
    app.update();
    let before = app.world().get::<Transform>(star).unwrap().translation.x;
    run(&mut app, 5);
    let after = app.world().get::<Transform>(star).map(|t| t.translation.x).unwrap_or(0.0);
    assert!(after < before, "磁铁吸引: {before} → {after}");
}

#[test]
fn rapid_fire_powerup_is_applied_and_expires() {
    let mut app = quiet_game("rapid", true);
    app.world_mut()
        .run_system_once(|mut c: Commands, a: Res<GameAssets>| {
            spawn_powerup(&mut c, &a, PowerUpKind::RapidFire, Vec2::ZERO);
        })
        .unwrap();
    run(&mut app, 3);
    let player = single::<Player>(&mut app);
    assert!(app.world().get::<RapidFire>(player).is_some());
    run(&mut app, 6 * 60 + 10);
    assert!(app.world().get::<RapidFire>(player).is_none(), "6 秒后移除");
}

#[test]
fn heal_powerup_restores_hp_capped_at_max() {
    let mut app = quiet_game("heal", true);
    let player = single::<Player>(&mut app);
    app.world_mut().get_mut::<Health>(player).unwrap().current = 2;
    app.world_mut()
        .run_system_once(|mut c: Commands, a: Res<GameAssets>| {
            spawn_powerup(&mut c, &a, PowerUpKind::Heal, Vec2::ZERO);
        })
        .unwrap();
    run(&mut app, 3);
    assert_eq!(app.world().get::<Health>(player).unwrap().current, 3);
}

#[test]
fn hud_score_and_hearts_follow_state() {
    let mut app = quiet_game("hud", true);
    app.world_mut().resource_mut::<Score>().points = 777;
    app.update();
    // Text 根 + 3 个 TextSpan 子节点；下标 1 是分数
    let root = single::<ScoreText>(&mut app);
    let spans: Vec<Entity> = app.world().get::<Children>(root).unwrap().iter().collect();
    assert_eq!(app.world().get::<TextSpan>(spans[0]).unwrap().0, "777");

    let row = single::<HeartRow>(&mut app);
    let hearts = app.world().get::<Children>(row).map_or(0, |c| c.len());
    let hp = app.world().resource::<GameConfig>().player_max_hp as usize;
    assert_eq!(hearts, hp);

    let player = single::<Player>(&mut app);
    app.world_mut().get_mut::<Health>(player).unwrap().current = 2;
    run(&mut app, 2);
    let hearts = app.world().get::<Children>(row).map_or(0, |c| c.len());
    assert_eq!(hearts, 2, "Changed<Health> → 重建心形图标");
}

#[test]
fn reflect_snapshot_contains_registered_components() {
    let mut app = quiet_game("snapshot", true);
    let text = snapshot_player(app.world_mut()).expect("有玩家");
    for needle in ["Health", "Velocity", "Collider", "FireCooldown"] {
        assert!(text.contains(needle), "快照应包含 {needle}:\n{text}");
    }
    goto(&mut app, AppState::Menu);
    assert!(snapshot_player(app.world_mut()).is_none(), "没有玩家时返回 None");
}

#[test]
fn reflect_serde_roundtrip_through_type_registry() {
    let app = test_app("reflect");
    let registry = app.world().resource::<AppTypeRegistry>().read();
    let original = GameConfig { player_max_hp: 42, ..default() };
    let ser = ReflectSerializer::new(&original, &registry);
    let text = ron::ser::to_string(&ser).unwrap();
    assert!(text.contains("bevy_demo01::config::GameConfig"), "带类型路径: {text}");

    let de = ReflectDeserializer::new(&registry);
    let mut ron_de = ron::de::Deserializer::from_str(&text).unwrap();
    let dynamic = serde::de::DeserializeSeed::deserialize(de, &mut ron_de).unwrap();
    let back = GameConfig::from_reflect(dynamic.as_ref()).expect("from_reflect");
    assert_eq!(back, original);
}

#[test]
fn time_scale_keys_change_virtual_speed() {
    let mut app = quiet_game("timescale", true);
    tap(&mut app, KeyCode::BracketLeft);
    assert_eq!(app.world().resource::<Time<Virtual>>().relative_speed(), 0.5);
    tap(&mut app, KeyCode::BracketRight);
    tap(&mut app, KeyCode::BracketRight);
    assert_eq!(app.world().resource::<Time<Virtual>>().relative_speed(), 2.0);
    tap(&mut app, KeyCode::Backslash);
    assert_eq!(app.world().resource::<Time<Virtual>>().relative_speed(), 1.0);
}

/// 同一 seed + 同一输入脚本 → 同一结果（确定性回放的基础）。
#[test]
fn simulation_is_deterministic() {
    fn play(tag: &str) -> (u64, u32, Vec<(i32, i32)>) {
        let mut app = test_app(tag);
        app.insert_resource(IntentSource::Scripted);
        {
            let mut cfg = app.world_mut().resource_mut::<GameConfig>();
            cfg.enemy_base_spawn_secs = 0.4;
            cfg.star_spawn_secs = 0.3;
        }
        goto(&mut app, AppState::InGame);
        for frame in 0..900u32 {
            let t = frame as f32 * 0.05;
            {
                let mut i = app.world_mut().resource_mut::<PlayerIntent>();
                i.movement = Vec2::new(t.sin(), (t * 0.7).cos());
                i.aim_stick = Some(Vec2::from_angle(t * 1.3));
                i.fire = true;
            }
            app.update();
            if state(&app) != AppState::InGame {
                break;
            }
        }
        let score = app.world().resource::<Score>().points;
        let spawned = app.world().resource::<EnemyCensus>().total_spawned;
        let mut q = app.world_mut().query_filtered::<&Transform, With<Enemy>>();
        let mut pos: Vec<(i32, i32)> =
            q.iter(app.world()).map(|t| (t.translation.x.round() as i32, t.translation.y.round() as i32)).collect();
        pos.sort();
        (score, spawned, pos)
    }
    let a = play("det_a");
    let b = play("det_b");
    assert!(a.1 > 5, "应有足够多的敌人参与: {}", a.1);
    assert_eq!(a, b);
}

#[test]
fn virtual_time_pauses_but_real_time_continues() {
    let mut app = quiet_game("realtime", true);
    tap(&mut app, KeyCode::Escape);
    let v0 = app.world().resource::<Time<Virtual>>().elapsed();
    let r0 = app.world().resource::<Time<Real>>().elapsed();
    run(&mut app, 10);
    assert_eq!(app.world().resource::<Time<Virtual>>().elapsed(), v0);
    assert!(app.world().resource::<Time<Real>>().elapsed() > r0 + Duration::from_millis(100));
}
