//! 资源管线集成测试：真实 `AssetServer` + 自定义 `AssetLoader` 在无头模式下加载配置文件，
//! 并通过 `AssetEvent` 同步到 `Res<GameConfig>`。

use std::time::{Duration, Instant};

use bevy::asset::LoadState;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_demo01::config::{CONFIG_PATH, ConfigApplied, ConfigAssetPlugin, ConfigHandle, ConfigLogicPlugin, GameConfig};

fn asset_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        AssetPlugin { file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").to_string(), ..default() },
        ConfigLogicPlugin,
        ConfigAssetPlugin,
    ));
    app
}

fn wait_until(app: &mut App, mut done: impl FnMut(&mut App) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        app.update();
        if done(app) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

#[test]
fn custom_loader_loads_config_and_syncs_resource() {
    let mut app = asset_app();
    // 先把资源改成一个明显不同的值，验证确实被文件覆盖
    app.world_mut().resource_mut::<GameConfig>().player_max_hp = -1;

    let mut applied = 0usize;
    let ok = wait_until(&mut app, |app| {
        let messages = app.world().resource::<Messages<ConfigApplied>>();
        applied += messages.get_cursor().read(messages).count();
        applied > 0
    });
    assert!(ok, "10 秒内应完成加载并发出 ConfigApplied");

    let expected = GameConfig::from_ron_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/config/game.cfg.ron")).unwrap(),
    )
    .unwrap();
    assert_eq!(*app.world().resource::<GameConfig>(), expected);

    let handle = app.world().resource::<ConfigHandle>().0.clone();
    let server = app.world().resource::<AssetServer>();
    assert!(matches!(server.load_state(&handle), LoadState::Loaded));
    assert!(server.is_loaded_with_dependencies(&handle));
    assert_eq!(server.get_path(&handle).unwrap().to_string(), CONFIG_PATH);
}

#[test]
fn invalid_config_fails_to_load() {
    // 在临时 assets 目录里放一个语义非法的配置
    let dir = std::env::temp_dir().join(format!("bevy_demo01_badcfg_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::write(dir.join(CONFIG_PATH), "(fire_cooldown_secs: -1.0)").unwrap();

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        AssetPlugin { file_path: dir.to_string_lossy().to_string(), ..default() },
        ConfigLogicPlugin,
        ConfigAssetPlugin,
    ));
    let ok = wait_until(&mut app, |app| {
        let handle = app.world().resource::<ConfigHandle>().0.clone();
        matches!(app.world().resource::<AssetServer>().load_state(&handle), LoadState::Failed(_))
    });
    assert!(ok, "非法配置应导致 LoadState::Failed");
    assert_eq!(*app.world().resource::<GameConfig>(), GameConfig::default(), "加载失败时保留默认配置");
    let _ = std::fs::remove_dir_all(dir);
}
