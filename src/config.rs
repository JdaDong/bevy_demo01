//! 游戏数值配置：自定义 Asset + 自定义 AssetLoader + 热重载。
//!
//! 数据流：`assets/config/game.cfg.ron` --AssetServer--> `Assets<GameConfig>`
//! --AssetEvent--> `Res<GameConfig>`（逻辑系统只读这个资源，保证无头测试不依赖 AssetServer）。
//!
//! 覆盖 API：`Asset` derive、`TypePath`、`AssetLoader`（async load + `Reader`）、
//! `AssetApp::{init_asset, init_asset_loader}`、`AssetServer::load`、`Handle`、
//! `MessageReader<AssetEvent<T>>`、`Assets::get`、`Time<Fixed>::set_timestep_hz`、
//! `Reflect` + `#[reflect(Resource)]`、serde。

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 全部可调数值。既是 `Asset`（从磁盘加载），也是 `Resource`（运行时生效值）。
#[derive(Asset, Resource, Reflect, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[reflect(Resource)]
#[serde(default)]
pub struct GameConfig {
    pub rng_seed: u64,
    pub fixed_hz: f64,
    // --- 玩家 ---
    pub player_max_hp: i32,
    pub player_accel: f32,
    pub player_drag: f32,
    pub player_max_speed: f32,
    pub player_radius: f32,
    pub invulnerable_secs: f32,
    pub fire_cooldown_secs: f32,
    pub bullet_speed: f32,
    pub bullet_lifetime_secs: f32,
    pub bullet_damage: i32,
    pub missile_speed: f32,
    pub missile_turn_rate: f32,
    pub missile_damage: i32,
    pub missile_cooldown_secs: f32,
    // --- 敌人 ---
    pub enemy_base_spawn_secs: f32,
    pub enemy_min_spawn_secs: f32,
    pub enemy_base_cap: u32,
    pub wave_duration_secs: f32,
    pub meteor_speed: (f32, f32),
    pub meteor_hp: i32,
    pub hunter_speed: f32,
    pub hunter_turn_rate: f32,
    pub hunter_hp: i32,
    pub carrier_hp: i32,
    pub carrier_armor: i32,
    // --- 收集物 ---
    pub star_spawn_secs: f32,
    pub star_max: u32,
    pub star_value: u32,
    pub magnet_radius: f32,
    pub powerup_drop_chance: f32,
    pub combo_window_secs: f32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            rng_seed: 20_260_930,
            fixed_hz: 60.0,
            player_max_hp: 5,
            player_accel: 2600.0,
            player_drag: 4.0,
            player_max_speed: 420.0,
            player_radius: 16.0,
            invulnerable_secs: 1.2,
            fire_cooldown_secs: 0.16,
            bullet_speed: 900.0,
            bullet_lifetime_secs: 1.2,
            bullet_damage: 1,
            missile_speed: 520.0,
            missile_turn_rate: 7.0,
            missile_damage: 4,
            missile_cooldown_secs: 1.5,
            enemy_base_spawn_secs: 1.6,
            enemy_min_spawn_secs: 0.35,
            enemy_base_cap: 6,
            wave_duration_secs: 25.0,
            meteor_speed: (60.0, 160.0),
            meteor_hp: 2,
            hunter_speed: 210.0,
            hunter_turn_rate: 2.5,
            hunter_hp: 3,
            carrier_hp: 12,
            carrier_armor: 2,
            star_spawn_secs: 1.1,
            star_max: 8,
            star_value: 10,
            magnet_radius: 140.0,
            powerup_drop_chance: 0.18,
            combo_window_secs: 2.5,
        }
    }
}

/// 配置校验错误。
#[derive(Debug, Error, PartialEq)]
pub enum ConfigError {
    #[error("字段 `{field}` 必须 > 0，实际为 {value}")]
    NotPositive { field: &'static str, value: f64 },
    #[error("字段 `{field}` 必须在 [0,1]，实际为 {value}")]
    NotProbability { field: &'static str, value: f32 },
    #[error("meteor_speed 区间非法: ({0}, {1})")]
    BadRange(f32, f32),
}

impl GameConfig {
    /// 从 RON 字符串解析（缺字段用默认值填充，得益于 `#[serde(default)]`）。
    pub fn from_ron_str(s: &str) -> Result<Self, ron::error::SpannedError> {
        ron::de::from_str(s)
    }

    /// 序列化为漂亮的 RON（`scripts/gen_assets.py` 之外的第二种生成方式）。
    pub fn to_ron_string(&self) -> Result<String, ron::Error> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
    }

    /// 语义校验：挡住“能解析但会让游戏崩掉”的数值。
    pub fn validate(&self) -> Result<(), ConfigError> {
        let positives: [(&'static str, f64); 12] = [
            ("fixed_hz", self.fixed_hz),
            ("player_max_hp", self.player_max_hp as f64),
            ("player_max_speed", self.player_max_speed as f64),
            ("player_radius", self.player_radius as f64),
            ("fire_cooldown_secs", self.fire_cooldown_secs as f64),
            ("bullet_speed", self.bullet_speed as f64),
            ("bullet_lifetime_secs", self.bullet_lifetime_secs as f64),
            ("enemy_base_spawn_secs", self.enemy_base_spawn_secs as f64),
            ("enemy_min_spawn_secs", self.enemy_min_spawn_secs as f64),
            ("wave_duration_secs", self.wave_duration_secs as f64),
            ("star_spawn_secs", self.star_spawn_secs as f64),
            ("combo_window_secs", self.combo_window_secs as f64),
        ];
        for (field, value) in positives {
            // NaN 与任何数比较都为 false，必须单独拦截
            if value.is_nan() || value <= 0.0 {
                return Err(ConfigError::NotPositive { field, value });
            }
        }
        if !(0.0..=1.0).contains(&self.powerup_drop_chance) {
            return Err(ConfigError::NotProbability { field: "powerup_drop_chance", value: self.powerup_drop_chance });
        }
        let (lo, hi) = self.meteor_speed;
        if !(lo > 0.0 && lo <= hi) {
            return Err(ConfigError::BadRange(lo, hi));
        }
        Ok(())
    }
}

/// 加载错误。
#[derive(Debug, Error)]
pub enum GameConfigLoaderError {
    #[error("读取配置失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("RON 解析失败: {0}")]
    Ron(#[from] ron::error::SpannedError),
    #[error("配置语义非法: {0}")]
    Invalid(#[from] ConfigError),
}

/// 自定义 AssetLoader：`*.cfg.ron` → [`GameConfig`]。
#[derive(Default, TypePath)]
pub struct GameConfigLoader;

impl AssetLoader for GameConfigLoader {
    type Asset = GameConfig;
    type Settings = ();
    type Error = GameConfigLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let cfg: GameConfig = ron::de::from_bytes(&bytes)?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn extensions(&self) -> &[&str] {
        &["cfg.ron"]
    }
}

/// 持有配置句柄，防止被回收，同时用于匹配 AssetEvent。
#[derive(Resource, Debug, Default, Clone)]
pub struct ConfigHandle(pub Handle<GameConfig>);

/// 配置被（重新）应用时发出。HUD/日志可订阅。
#[derive(Message, Debug, Clone)]
pub struct ConfigApplied {
    pub hot_reload: bool,
}

pub const CONFIG_PATH: &str = "config/game.cfg.ron";

/// 逻辑侧：只注册资源与消息（无头测试可用）。
pub struct ConfigLogicPlugin;

impl Plugin for ConfigLogicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameConfig>()
            .add_message::<ConfigApplied>()
            .add_systems(Startup, apply_fixed_timestep)
            .add_systems(Update, apply_fixed_timestep.run_if(resource_changed::<GameConfig>));
    }
}

/// 资源侧：需要 `AssetPlugin`。
pub struct ConfigAssetPlugin;

impl Plugin for ConfigAssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<GameConfig>()
            .init_asset_loader::<GameConfigLoader>()
            .add_systems(PreStartup, load_config)
            .add_systems(PreUpdate, sync_config_from_asset);
    }
}

fn load_config(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(ConfigHandle(asset_server.load(CONFIG_PATH)));
}

/// 监听 AssetEvent，把磁盘上的配置同步到 `Res<GameConfig>`。
/// `Modified` 只有开启 `file_watcher`（`--features hot`）时才会出现。
pub fn sync_config_from_asset(
    mut events: MessageReader<AssetEvent<GameConfig>>,
    handle: Option<Res<ConfigHandle>>,
    assets: Res<Assets<GameConfig>>,
    mut config: ResMut<GameConfig>,
    mut applied: MessageWriter<ConfigApplied>,
) {
    let Some(handle) = handle else { return };
    for event in events.read() {
        let hot_reload = event.is_modified(&handle.0);
        if (event.is_loaded_with_dependencies(&handle.0) || hot_reload)
            && let Some(loaded) = assets.get(&handle.0)
        {
            if *config != *loaded {
                *config = loaded.clone();
            }
            info!("GameConfig applied (hot_reload={hot_reload})");
            applied.write(ConfigApplied { hot_reload });
        }
    }
}

fn apply_fixed_timestep(config: Res<GameConfig>, mut fixed: ResMut<Time<Fixed>>) {
    fixed.set_timestep_hz(config.fixed_hz);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        assert_eq!(GameConfig::default().validate(), Ok(()));
    }

    #[test]
    fn roundtrip_ron() {
        let cfg = GameConfig::default();
        let s = cfg.to_ron_string().unwrap();
        assert_eq!(GameConfig::from_ron_str(&s).unwrap(), cfg);
    }

    #[test]
    fn partial_ron_uses_defaults() {
        let cfg = GameConfig::from_ron_str("(player_max_hp: 9, meteor_speed: (1.0, 2.0))").unwrap();
        assert_eq!(cfg.player_max_hp, 9);
        assert_eq!(cfg.meteor_speed, (1.0, 2.0));
        assert_eq!(cfg.bullet_speed, GameConfig::default().bullet_speed);
    }

    #[test]
    fn syntax_error_is_reported() {
        assert!(GameConfig::from_ron_str("(player_max_hp: )").is_err());
        assert!(GameConfig::from_ron_str("(player_max_hp: \"five\")").is_err());
    }

    #[test]
    fn validation_catches_bad_values() {
        let cfg = GameConfig { fire_cooldown_secs: 0.0, ..default() };
        assert!(matches!(cfg.validate(), Err(ConfigError::NotPositive { field: "fire_cooldown_secs", .. })));

        let cfg = GameConfig { powerup_drop_chance: 1.5, ..default() };
        assert!(matches!(cfg.validate(), Err(ConfigError::NotProbability { .. })));

        let cfg = GameConfig { meteor_speed: (200.0, 100.0), ..default() };
        assert_eq!(cfg.validate(), Err(ConfigError::BadRange(200.0, 100.0)));

        let cfg = GameConfig { fixed_hz: f64::NAN, ..default() };
        assert!(cfg.validate().is_err(), "NaN 必须被拒绝");
    }

    #[test]
    fn shipped_config_file_is_valid() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/config/game.cfg.ron");
        let text = std::fs::read_to_string(path).expect("assets/config/game.cfg.ron 应存在");
        let cfg = GameConfig::from_ron_str(&text).expect("随包配置应能解析");
        cfg.validate().expect("随包配置应通过校验");
    }

    #[test]
    fn error_messages_are_human_readable() {
        let e = ConfigError::NotPositive { field: "x", value: -1.0 };
        assert!(e.to_string().contains("`x`"));
    }
}
