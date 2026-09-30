//! # Star Harvester —— Bevy 0.19 2D 全 API 演示游戏
//!
//! 玩法：驾驶飞船收集星星、击落陨石/猎手/母舰，连击翻倍，活得越久波次越高。
//!
//! ## 插件分层（关键设计）
//!
//! ```text
//! GamePlugin
//! ├── GameLogicPlugin        ← 纯数据/逻辑，可在 MinimalPlugins 下无头运行（全部集成测试基于它）
//! │   ├── StatePlugin / SchedulePlugin / EventsPlugin / ConfigLogicPlugin
//! │   ├── GameInputPlugin / PlayerPlugin / EnemyPlugin / CombatPlugin
//! │   ├── CollectiblesPlugin / ScorePlugin / EffectsPlugin / SavePlugin
//! │   └── AudioLogicPlugin / DebugLogicPlugin
//! ├── GameUiPlugin           ← Bevy UI（只写组件，同样可无头测试）
//! └── PresentationPlugin     ← 需要 DefaultPlugins：资源加载、相机、音频播放、Gizmos、动画、诊断
//! ```
//!
//! 详见 `docs/ARCHITECTURE.md` 与 `docs/API_COVERAGE.md`。

// Bevy 系统签名天然“参数多、类型长”，这两条 lint 在 Bevy 项目里是噪音（官方模板同样关闭）。
#![allow(clippy::type_complexity, clippy::too_many_arguments)]

pub mod assets;
pub mod audio;
pub mod autoplay;
pub mod camera;
pub mod collectibles;
pub mod combat;
pub mod components;
pub mod config;
pub mod debug;
pub mod effects;
pub mod enemy;
pub mod events;
pub mod input;
pub mod math;
pub mod player;
pub mod rng;
pub mod save;
pub mod schedule;
pub mod score;
pub mod state;
pub mod ui;
pub mod visuals;

use bevy::prelude::*;

/// 逻辑层：不依赖窗口/渲染/音频设备。
pub struct GameLogicPlugin;

impl Plugin for GameLogicPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            state::StatePlugin,
            schedule::SchedulePlugin,
            events::EventsPlugin,
            config::ConfigLogicPlugin,
            input::GameInputPlugin,
            player::PlayerPlugin,
            enemy::EnemyPlugin,
            combat::CombatPlugin,
            collectibles::CollectiblesPlugin,
            score::ScorePlugin,
            effects::EffectsPlugin,
            save::SavePlugin,
            audio::AudioLogicPlugin,
            debug::DebugLogicPlugin,
        ))
        .init_resource::<assets::GameAssets>()
        .init_resource::<rng::GameRng>()
        .add_systems(Startup, seed_rng)
        .add_systems(OnEnter(state::AppState::InGame), seed_rng);
        register_reflect_types(app);
    }
}

/// 表现层：需要 `DefaultPlugins`。
pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            config::ConfigAssetPlugin,
            assets::AssetLoadingPlugin,
            camera::GameCameraPlugin,
            audio::AudioPresentationPlugin,
            visuals::VisualsPlugin,
            debug::DebugPresentationPlugin,
            bevy::diagnostic::EntityCountDiagnosticsPlugin::default(),
        ));
    }
}

/// 完整游戏。
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((GameLogicPlugin, ui::GameUiPlugin, PresentationPlugin, autoplay::AutoplayPlugin));
    }
}

/// 每局开始按配置重置 RNG → 同配置同 seed 的对局完全可复现。
fn seed_rng(mut commands: Commands, config: Res<config::GameConfig>) {
    commands.insert_resource(rng::GameRng::seeded(config.rng_seed));
}

/// 显式注册反射类型（`reflect_auto_register` 特性已自动注册大部分，这里显式列出以示范 API，
/// 同时保证关闭该特性时快照功能依然可用）。
fn register_reflect_types(app: &mut App) {
    use components::*;
    app.register_type::<Player>()
        .register_type::<Health>()
        .register_type::<Velocity>()
        .register_type::<Collider>()
        .register_type::<Team>()
        .register_type::<FireCooldown>()
        .register_type::<MissileCooldown>()
        .register_type::<Enemy>()
        .register_type::<Targeting>()
        .register_type::<TargetedBy>()
        .register_type::<Projectile>()
        .register_type::<Star>()
        .register_type::<PowerUp>()
        .register_type::<score::Score>()
        .register_type::<score::RunStats>()
        .register_type::<score::HighScores>()
        .register_type::<config::GameConfig>()
        .register_type::<audio::AudioSettings>()
        .register_type::<rng::GameRng>()
        .register_type::<enemy::WaveDirector>()
        .register_type::<effects::CameraTrauma>()
        .register_type::<input::PlayerIntent>();
}

/// 构建无头 App：`MinimalPlugins` + 状态 + 手动时间步进。测试与 `headless_sim` 二进制共用。
pub fn headless_app(frame_dt: std::time::Duration) -> App {
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;

    let mut app = App::new();
    // TransformPlugin 必须加：碰撞检测读的是 GlobalTransform（由它在 PostUpdate 传播）
    app.add_plugins((MinimalPlugins, StatesPlugin, bevy::transform::TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame_dt))
        .add_plugins((GameLogicPlugin, ui::GameUiPlugin));
    app
}
