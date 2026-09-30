//! 可执行入口：DefaultPlugins（窗口 / 渲染 / 音频 / 输入 / 资源）+ GamePlugin。
//!
//! 覆盖 API：`App::set_error_handler`、`DefaultPlugins.set(..)` 定制子插件
//! （`WindowPlugin` / `ImagePlugin` / `LogPlugin` / `AssetPlugin`）、`App::run -> AppExit`。

use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};

use bevy_demo01::GamePlugin;

/// 资源根目录：优先 `BEVY_ASSET_ROOT` / `CARGO_MANIFEST_DIR`（Bevy 默认行为），
/// 否则若当前目录下有 `assets/` 就用它（直接运行 `./target/release/bevy_demo01` 的场景），
/// 再否则回退到可执行文件旁边的 `assets/`。
fn asset_root() -> String {
    if std::env::var_os("BEVY_ASSET_ROOT").is_some() || std::env::var_os("CARGO_MANIFEST_DIR").is_some() {
        return "assets".into();
    }
    match std::env::current_dir() {
        Ok(cwd) if cwd.join("assets").is_dir() => cwd.join("assets").to_string_lossy().into_owned(),
        _ => "assets".into(),
    }
}

fn main() -> AppExit {
    App::new()
        // 返回 Result 的系统出错时只打 warn，不 panic（默认会 panic）
        .set_error_handler(bevy::ecs::error::warn)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Star Harvester — Bevy 0.19 2D showcase".into(),
                        resolution: WindowResolution::new(1280, 720),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                // 像素风：最近邻采样，贴图放大不糊
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin { file_path: asset_root(), ..default() })
                .set(LogPlugin {
                    filter: "wgpu=error,naga=warn,bevy_render=info,bevy_demo01=debug".into(),
                    level: bevy::log::Level::INFO,
                    ..default()
                }),
        )
        .add_plugins(GamePlugin)
        .run()
}
