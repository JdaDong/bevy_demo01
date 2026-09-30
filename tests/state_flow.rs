//! 状态机 + UI 交互集成测试：菜单按钮、暂停子状态、计算状态、状态作用域清理、结算与存档。

mod common;

use std::time::{Duration, Instant};

use bevy::ecs::entity_disabling::Disabled;
use bevy::prelude::*;
use bevy_demo01::audio::AudioSettings;
use bevy_demo01::components::*;
use bevy_demo01::save::{SaveDir, SaveFinished, load_highscores_from};
use bevy_demo01::score::{HighScores, LastRun, Score};
use bevy_demo01::state::{AppState, GamePhase, ShowHud, Simulating};
use bevy_demo01::ui::hud::ScoreText;
use bevy_demo01::ui::screens::VolumeLabel;
use bevy_demo01::ui::widgets::ButtonAction;
use common::*;

/// “点击”按钮：直接把 Interaction 设为 Pressed（UI 焦点系统在无头模式下不存在）。
fn click(app: &mut App, action: ButtonAction) {
    let mut q = app.world_mut().query::<(Entity, &ButtonAction)>();
    let target = q
        .iter(app.world())
        .find(|(_, a)| **a == action)
        .map(|(e, _)| e)
        .unwrap_or_else(|| panic!("按钮 {action:?} 不存在"));
    app.world_mut().entity_mut(target).insert(Interaction::Pressed);
    app.update(); // button_actions 读到 Changed<Interaction> → NextState
    app.update(); // 状态切换生效
}

fn has_state<S: bevy::state::state::States>(app: &App) -> bool {
    app.world().contains_resource::<State<S>>()
}

#[test]
fn menu_play_button_starts_game() {
    let mut app = test_app("menu_play");
    goto(&mut app, AppState::Menu);
    assert!(count::<With<Button>>(&mut app) >= 4, "菜单应有多个按钮");
    click(&mut app, ButtonAction::Play);
    assert_eq!(state(&app), AppState::InGame);
    assert_eq!(count::<With<Player>>(&mut app), 1);
    assert_eq!(count::<With<ButtonAction>>(&mut app), 0, "菜单 UI 已随 DespawnOnExit 清理");
}

#[test]
fn enter_key_starts_game_from_menu() {
    let mut app = test_app("menu_enter");
    goto(&mut app, AppState::Menu);
    tap(&mut app, KeyCode::Enter);
    assert_eq!(state(&app), AppState::InGame);
}

#[test]
fn pause_substate_computed_states_and_virtual_time() {
    let mut app = test_app("pause");
    goto(&mut app, AppState::InGame);
    assert!(has_state::<Simulating>(&app));
    assert!(has_state::<ShowHud>(&app));
    assert!(!app.world().resource::<Time<Virtual>>().is_paused());

    tap(&mut app, KeyCode::Escape);
    assert_eq!(*app.world().resource::<State<GamePhase>>().get(), GamePhase::Paused);
    assert!(!has_state::<Simulating>(&app), "暂停时 Simulating 计算状态被移除");
    assert!(has_state::<ShowHud>(&app), "暂停时 HUD 仍显示");
    assert!(app.world().resource::<Time<Virtual>>().is_paused());
    assert!(count::<With<ButtonAction>>(&mut app) >= 2, "暂停层按钮");

    // 暂停时玩家不动
    let player = single::<Player>(&mut app);
    app.world_mut().get_mut::<Velocity>(player).unwrap().0 = Vec2::new(300.0, 0.0);
    let before = app.world().get::<Transform>(player).unwrap().translation;
    run(&mut app, 30);
    assert_eq!(app.world().get::<Transform>(player).unwrap().translation, before);

    click(&mut app, ButtonAction::Resume);
    assert_eq!(*app.world().resource::<State<GamePhase>>().get(), GamePhase::Playing);
    assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    assert_eq!(count::<With<ButtonAction>>(&mut app), 0, "暂停层随 DespawnOnExit(Paused) 清理");
}

#[test]
fn substate_does_not_exist_outside_game() {
    let mut app = test_app("substate");
    goto(&mut app, AppState::Menu);
    assert!(!has_state::<GamePhase>(&app));
    assert!(!has_state::<ShowHud>(&app));
    goto(&mut app, AppState::InGame);
    assert!(has_state::<GamePhase>(&app));
}

#[test]
fn back_to_menu_from_pause_cleans_everything_including_disabled() {
    let mut app = test_app("cleanup");
    goto(&mut app, AppState::InGame);
    // 造一颗被禁用（进池）的子弹
    let pooled = app
        .world_mut()
        .spawn((
            Projectile { kind: ProjectileKind::Bullet, damage: 1, team: Team::Player },
            Disabled,
            DespawnOnExit(AppState::InGame),
        ))
        .id();
    tap(&mut app, KeyCode::Escape);
    click(&mut app, ButtonAction::BackToMenu);
    assert_eq!(state(&app), AppState::Menu);
    assert_eq!(count::<With<Player>>(&mut app), 0);
    assert_eq!(count::<With<ScoreText>>(&mut app), 0, "HUD 清理");
    assert!(app.world().get_entity(pooled).is_err(), "DespawnOnExit 也会清理 Disabled 实体");
    assert!(!app.world().resource::<Time<Virtual>>().is_paused(), "离开游戏时恢复时间");
}

#[test]
fn player_death_goes_to_gameover_and_saves_highscore() {
    let mut app = test_app("death");
    goto(&mut app, AppState::InGame);
    app.world_mut().resource_mut::<Score>().points = 4321;

    // 血量 1 + 把陨石放到玩家身上
    let player = single::<Player>(&mut app);
    app.world_mut().get_mut::<Health>(player).unwrap().current = 1;
    app.world_mut().spawn((
        Enemy { kind: EnemyKind::Meteor },
        Health::new(99),
        Collider { radius: 30.0 },
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
    run(&mut app, 3);
    assert_eq!(state(&app), AppState::GameOver);

    let last = app.world().resource::<LastRun>().clone();
    assert_eq!(last.score, 4321);
    assert_eq!(last.rank, Some(0));
    assert_eq!(app.world().resource::<HighScores>().best(), 4321);

    // 等待 IoTaskPool 异步写盘完成
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut done = false;
    while Instant::now() < deadline {
        app.update();
        if !recent::<SaveFinished>(&mut app).is_empty() {
            done = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(done, "异步保存应在 5 秒内完成");
    let path = app.world().resource::<SaveDir>().highscores();
    assert_eq!(load_highscores_from(&path).best(), 4321, "磁盘上的排行榜");
    assert!(count::<With<ButtonAction>>(&mut app) >= 2, "结算页按钮");
}

#[test]
fn retry_from_gameover_resets_score() {
    let mut app = test_app("retry");
    goto(&mut app, AppState::InGame);
    app.world_mut().resource_mut::<Score>().points = 99;
    goto(&mut app, AppState::GameOver);
    tap(&mut app, KeyCode::Enter);
    assert_eq!(state(&app), AppState::InGame);
    assert_eq!(app.world().resource::<Score>().points, 0);
    assert_eq!(count::<With<Player>>(&mut app), 1);
}

#[test]
fn quit_button_writes_app_exit() {
    let mut app = test_app("quit");
    goto(&mut app, AppState::Menu);
    let mut q = app.world_mut().query::<(Entity, &ButtonAction)>();
    let quit = q.iter(app.world()).find(|(_, a)| **a == ButtonAction::Quit).unwrap().0;
    app.world_mut().entity_mut(quit).insert(Interaction::Pressed);
    app.update();
    assert!(app.should_exit().is_some(), "应请求退出");
}

#[test]
fn volume_buttons_update_settings_and_label() {
    let mut app = test_app("volume");
    goto(&mut app, AppState::Menu);
    click(&mut app, ButtonAction::VolumeUp);
    assert_eq!(app.world().resource::<AudioSettings>().master, 0.8);
    let text = {
        let mut q = app.world_mut().query_filtered::<&Text, With<VolumeLabel>>();
        q.single(app.world()).unwrap().0.clone()
    };
    assert_eq!(text, "Volume: 80%");
    click(&mut app, ButtonAction::ToggleMute);
    assert!(app.world().resource::<AudioSettings>().muted);
}
