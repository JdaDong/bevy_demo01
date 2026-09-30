//! 调试工具：作弊（一次性系统）、时间缩放、Gizmos、诊断、FPS 覆盖层、截图、反射快照。
//!
//! 覆盖 API：`World::register_system` + `SystemId` + `commands.run_system`（一次性系统）、
//! `commands.run_system_cached`、`input_just_pressed` 运行条件、`Time<Virtual>::set_relative_speed`、
//! `Gizmos` + 自定义 `GizmoConfigGroup` + `GizmoConfigStore`、`Diagnostics` / `RegisterDiagnostic`、
//! `FpsOverlayPlugin` / `FpsOverlayConfig`、`Screenshot` + `save_to_disk`、`log_transitions`。

use bevy::diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic};
use bevy::ecs::system::SystemId;
use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;

use crate::assets::GameAssets;
use crate::components::*;
use crate::config::GameConfig;
use crate::enemy::{EnemySpawn, WaveDirector, spawn_enemy};
use crate::events::Damage;
use crate::math::ARENA_HALF;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// 逻辑侧（无头可测）
// ---------------------------------------------------------------------------

/// 一次性系统句柄。启动时注册，按键时 `run_system(id)`。
#[derive(Resource, Debug, Clone, Copy)]
pub struct Cheats {
    pub nuke: SystemId,
    pub heal: SystemId,
}

/// 作弊：对全场敌人造成致命伤害（走正规的 Damage 流程 → 有爆炸、计分）。
pub fn cheat_nuke(mut commands: Commands, enemies: Query<(Entity, &Health), With<Enemy>>) {
    for (e, hp) in &enemies {
        commands.trigger(Damage { entity: e, amount: hp.current.max(1) });
    }
}

pub fn cheat_heal(mut q: Query<&mut Health, With<Player>>) {
    for mut h in &mut q {
        h.current = h.max;
    }
}

/// 作弊：在右上角刷一艘母舰（作为 cached 一次性系统运行）。
pub fn cheat_spawn_carrier(
    mut commands: Commands,
    assets: Res<GameAssets>,
    config: Res<GameConfig>,
    director: Res<WaveDirector>,
    player: Option<Single<Entity, With<Player>>>,
) {
    spawn_enemy(
        &mut commands,
        &assets,
        &config,
        EnemySpawn {
            kind: EnemyKind::Carrier,
            position: ARENA_HALF * 0.6,
            velocity: Vec2::new(-30.0, -10.0),
            wave: director.wave,
        },
        player.map(|p| *p),
    );
}

fn register_cheats(world: &mut World) {
    let nuke = world.register_system(cheat_nuke);
    let heal = world.register_system(cheat_heal);
    world.insert_resource(Cheats { nuke, heal });
}

fn run_nuke(mut commands: Commands, cheats: Res<Cheats>) {
    commands.run_system(cheats.nuke);
}

fn run_heal(mut commands: Commands, cheats: Res<Cheats>) {
    commands.run_system(cheats.heal);
}

fn run_spawn_carrier(mut commands: Commands) {
    // 不需要预先注册：首次运行时自动注册并缓存
    commands.run_system_cached(cheat_spawn_carrier);
}

/// 时间缩放：`[` 减速、`]` 加速、`\` 复位。影响 `Time<Virtual>`，因此 FixedUpdate 也跟着变。
pub fn time_scale_keys(keys: Res<ButtonInput<KeyCode>>, mut time: ResMut<Time<Virtual>>) {
    let speed = time.relative_speed();
    let new = if keys.just_pressed(KeyCode::BracketLeft) {
        (speed * 0.5).max(0.125)
    } else if keys.just_pressed(KeyCode::BracketRight) {
        (speed * 2.0).min(4.0)
    } else if keys.just_pressed(KeyCode::Backslash) {
        1.0
    } else {
        return;
    };
    time.set_relative_speed(new);
    info!("time scale = {new}");
}

pub struct DebugLogicPlugin;

impl Plugin for DebugLogicPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_cheats).add_systems(
            Update,
            (
                run_nuke.run_if(input_just_pressed(KeyCode::KeyK)),
                run_heal.run_if(input_just_pressed(KeyCode::KeyH)),
                run_spawn_carrier.run_if(input_just_pressed(KeyCode::KeyC)),
                time_scale_keys,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// ---------------------------------------------------------------------------
// 表现侧
// ---------------------------------------------------------------------------

/// 自定义 Gizmo 分组：可独立开关、独立线宽。
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct DebugGizmos;

pub const ENEMIES_ALIVE: DiagnosticPath = DiagnosticPath::const_new("game/enemies_alive");
pub const BULLETS_ACTIVE: DiagnosticPath = DiagnosticPath::const_new("game/bullets_active");

fn configure_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<DebugGizmos>();
    config.enabled = false; // F1 打开
    config.line.width = 1.5;
    config.line.style = GizmoLineStyle::Dashed { gap_scale: 2.0, line_scale: 4.0 };
}

fn toggle_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<DebugGizmos>();
    config.enabled ^= true;
}

/// 画碰撞体、速度箭头、追踪关系、竞技场边界。
pub fn draw_debug_gizmos(
    mut gizmos: Gizmos<DebugGizmos>,
    colliders: Query<(&GlobalTransform, &Collider, Option<&Team>)>,
    moving: Query<(&GlobalTransform, &Velocity)>,
    targeting: Query<(&GlobalTransform, &Targeting)>,
    targets: Query<&GlobalTransform>,
) {
    gizmos.rect_2d(Isometry2d::IDENTITY, ARENA_HALF * 2.0, Color::srgb(0.3, 0.3, 0.5));
    gizmos.grid_2d(Isometry2d::IDENTITY, UVec2::new(16, 9), Vec2::splat(80.0), LinearRgba::gray(0.08)).outer_edges();
    for (tf, col, team) in &colliders {
        let color = match team {
            Some(Team::Player) => Color::srgb(0.2, 1.0, 0.4),
            Some(Team::Enemy) => Color::srgb(1.0, 0.3, 0.3),
            _ => Color::srgb(0.9, 0.9, 0.2),
        };
        gizmos.circle_2d(tf.translation().truncate(), col.radius, color);
    }
    for (tf, vel) in &moving {
        let p = tf.translation().truncate();
        if vel.0.length_squared() > 1.0 {
            gizmos.arrow_2d(p, p + vel.0 * 0.15, Color::srgb(0.4, 0.8, 1.0));
        }
    }
    for (tf, t) in &targeting {
        if let Ok(target) = targets.get(t.0) {
            gizmos.line_2d(tf.translation().truncate(), target.translation().truncate(), Color::srgb(1.0, 0.4, 1.0));
        }
    }
}

fn measure_diagnostics(mut diagnostics: Diagnostics, census: Res<EnemyCensus>, bullets: Query<&Projectile>) {
    diagnostics.add_measurement(&ENEMIES_ALIVE, || census.total_alive() as f64);
    diagnostics.add_measurement(&BULLETS_ACTIVE, || bullets.iter().count() as f64);
}

fn toggle_fps_overlay(mut overlay: ResMut<bevy::dev_tools::fps_overlay::FpsOverlayConfig>) {
    overlay.enabled ^= true;
    overlay.frame_time_graph_config.enabled = overlay.enabled;
}

fn screenshot(commands: &mut Commands, name: &str) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    let path = format!("./screenshots/{name}.png");
    let _ = std::fs::create_dir_all("./screenshots");
    info!("截图 → {path}");
    // Screenshot 是一个实体；截图完成后触发 ScreenshotCaptured 事件，save_to_disk 是现成的观察者
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
}

fn take_screenshot(mut commands: Commands, mut counter: Local<u32>) {
    screenshot(&mut commands, &format!("screenshot-{}", *counter));
    *counter += 1;
}

/// Autoplay 模式下到点自动截图一次。
fn autoplay_screenshot(
    mut commands: Commands,
    time: Res<Time<Real>>,
    auto: Option<Res<crate::autoplay::Autoplay>>,
    mut done: Local<bool>,
) {
    let Some(auto) = auto else { return };
    let Some(at) = auto.screenshot_at else { return };
    if !*done && time.elapsed_secs() >= at {
        *done = true;
        let name = if auto.stay_in_menu { "autoplay-menu" } else { "autoplay-game" };
        screenshot(&mut commands, name);
    }
}

pub struct DebugPresentationPlugin;

impl Plugin for DebugPresentationPlugin {
    fn build(&self, app: &mut App) {
        use bevy::dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin, FrameTimeGraphConfig};
        use bevy::dev_tools::states::log_transitions;

        app.init_gizmo_group::<DebugGizmos>()
            .register_diagnostic(Diagnostic::new(ENEMIES_ALIVE).with_suffix(" enemies"))
            .register_diagnostic(Diagnostic::new(BULLETS_ACTIVE).with_suffix(" bullets"))
            .add_plugins(FpsOverlayPlugin {
                config: FpsOverlayConfig {
                    text_config: TextFont::from_font_size(16.0),
                    text_color: Color::srgb(0.6, 1.0, 0.6),
                    enabled: false, // F2 打开
                    frame_time_graph_config: FrameTimeGraphConfig { enabled: false, min_fps: 30.0, target_fps: 60.0 },
                    ..default()
                },
            })
            .add_systems(Startup, configure_gizmos)
            .add_systems(
                Update,
                (
                    toggle_gizmos.run_if(input_just_pressed(KeyCode::F1)),
                    toggle_fps_overlay.run_if(input_just_pressed(KeyCode::F2)),
                    take_screenshot.run_if(input_just_pressed(KeyCode::F12)),
                    autoplay_screenshot,
                    crate::save::write_snapshot
                        .pipe(crate::save::report_snapshot)
                        .run_if(input_just_pressed(KeyCode::F5)),
                    draw_debug_gizmos,
                    measure_diagnostics,
                    log_transitions::<AppState>,
                    log_transitions::<crate::state::GamePhase>,
                ),
            );
    }
}
