//! 冒烟测试：整套逻辑+UI 插件在无头模式下长时间运行不 panic。
//! 能抓到：系统参数冲突（B0001）、缺失资源、观察者递归、状态切换死循环等运行期错误。

mod common;

use bevy::prelude::*;
use bevy_demo01::components::{Enemy, Player, Star};
use bevy_demo01::input::{IntentSource, PlayerIntent};
use bevy_demo01::state::AppState;
use common::*;

#[test]
fn boots_into_loading_then_menu_then_game() {
    let mut app = test_app("boot");
    assert_eq!(state(&app), AppState::Loading, "默认状态");
    goto(&mut app, AppState::Menu);
    assert_eq!(state(&app), AppState::Menu);
    goto(&mut app, AppState::InGame);
    assert_eq!(state(&app), AppState::InGame);
    assert_eq!(count::<With<Player>>(&mut app), 1);
}

#[test]
fn simulate_one_minute_with_scripted_input() {
    let mut app = test_app("minute");
    app.insert_resource(IntentSource::Scripted);
    goto(&mut app, AppState::InGame);
    for frame in 0..3600u32 {
        // 画圈移动 + 一直开火 + 每秒一发导弹
        let t = frame as f32 / 60.0;
        {
            let mut intent = app.world_mut().resource_mut::<PlayerIntent>();
            intent.movement = Vec2::new(t.cos(), t.sin());
            intent.aim_stick = Some(Vec2::new((t * 2.0).cos(), (t * 2.0).sin()));
            intent.fire = true;
            intent.missile = frame % 60 == 0;
        }
        app.update();
        if state(&app) != AppState::InGame {
            break;
        }
    }
    // 一分钟内必然有星星和敌人刷出过
    let census = app.world().resource::<bevy_demo01::components::EnemyCensus>().clone();
    assert!(census.total_spawned > 5, "应刷出敌人: {census:?}");
    let _ = count::<With<Star>>(&mut app);
    let _ = count::<With<Enemy>>(&mut app);
}
