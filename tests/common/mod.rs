//! 集成测试公共工具：无头 App 构建、状态切换、帧推进、查询辅助。
#![allow(dead_code)]

use std::time::Duration;

use bevy::prelude::*;
use bevy_demo01::headless_app;
use bevy_demo01::save::SaveDir;
use bevy_demo01::state::{AppState, GamePhase};

pub const DT: Duration = Duration::from_micros(16_667); // ~60 FPS

/// 构建无头测试 App，存档目录隔离到临时目录（每个测试独立）。
pub fn test_app(tag: &str) -> App {
    let mut app = headless_app(DT);
    let dir = std::env::temp_dir().join(format!("bevy_demo01_it_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    app.insert_resource(SaveDir(dir));
    app.update(); // 跑 Startup
    app
}

/// 切到指定状态并推进 2 帧（第 1 帧应用 NextState，第 2 帧跑 OnEnter 后的 Update）。
pub fn goto(app: &mut App, state: AppState) {
    app.world_mut().resource_mut::<NextState<AppState>>().set(state);
    app.update();
    app.update();
}

pub fn set_phase(app: &mut App, phase: GamePhase) {
    app.world_mut().resource_mut::<NextState<GamePhase>>().set(phase);
    app.update();
}

pub fn run(app: &mut App, frames: u32) {
    for _ in 0..frames {
        app.update();
    }
}

pub fn state(app: &App) -> AppState {
    *app.world().resource::<State<AppState>>().get()
}

pub fn count<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
    app.world_mut().query_filtered::<(), F>().iter(app.world()).count()
}

pub fn single<C: Component>(app: &mut App) -> Entity {
    app.world_mut().query_filtered::<Entity, With<C>>().single(app.world()).expect("expected exactly one entity")
}

pub fn press(app: &mut App, key: KeyCode) {
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
}

pub fn release_all(app: &mut App) {
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.release_all();
    keys.clear();
}

/// 模拟一次“按下再松开”（just_pressed 只持续 1 帧）。
pub fn tap(app: &mut App, key: KeyCode) {
    press(app, key);
    app.update();
    release_all(app);
    app.update();
}

/// 读取最近两帧内的某类消息（Messages 双缓冲，测试断言用）。
pub fn recent<M: Message + Clone>(app: &mut App) -> Vec<M> {
    let messages = app.world().resource::<Messages<M>>();
    let mut cursor = messages.get_cursor();
    cursor.read(messages).cloned().collect()
}
