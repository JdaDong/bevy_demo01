//! 自动驾驶（Attract / Demo 模式）：无人值守跑窗口版，用于冒烟验证渲染管线与截图。
//!
//! 环境变量：
//! - `BEVY_DEMO_AUTOPLAY=1`：跳过菜单直接开局，AI 接管输入
//! - `BEVY_DEMO_EXIT_AFTER=15`：运行 N 秒（真实时间）后自动退出
//! - `BEVY_DEMO_SCREENSHOT_AT=6`：第 N 秒自动截图到 `./screenshots/`
//!
//! 覆盖 API：`resource_exists` + `and_then` 组合条件、`Time<Real>`、`MessageWriter<AppExit>`、
//! `Query::iter().min_by`。

use bevy::prelude::*;

use crate::components::{Enemy, Player, Star};
use crate::input::{IntentSource, PlayerIntent};
use crate::state::AppState;

#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Autoplay {
    /// `BEVY_DEMO_AUTOPLAY=menu` 时停留在菜单（用于菜单截图）
    pub stay_in_menu: bool,
    pub exit_after: Option<f32>,
    pub screenshot_at: Option<f32>,
}

impl Autoplay {
    /// 从环境变量构造；未开启返回 None。
    pub fn from_env() -> Option<Self> {
        let mode = std::env::var("BEVY_DEMO_AUTOPLAY").ok().filter(|v| !v.is_empty() && v != "0")?;
        let parse = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
        Some(Self {
            stay_in_menu: mode == "menu",
            exit_after: parse("BEVY_DEMO_EXIT_AFTER"),
            screenshot_at: parse("BEVY_DEMO_SCREENSHOT_AT"),
        })
    }
}

fn skip_menu(auto: Res<Autoplay>, mut next: ResMut<NextState<AppState>>) {
    if !auto.stay_in_menu {
        next.set(AppState::InGame);
    }
}

fn enable_scripted(mut source: ResMut<IntentSource>) {
    *source = IntentSource::Scripted;
}

/// 纯函数 AI：绕李萨如曲线巡航、朝最近敌人（没有就朝最近星星）开火，每 2 秒一发导弹。
pub fn autopilot_intent(t: f32, player: Vec2, target: Option<Vec2>, star: Option<Vec2>) -> PlayerIntent {
    let waypoint = Vec2::new((t * 0.6).sin() * 420.0, (t * 0.9).sin() * 220.0);
    let seek = star.filter(|s| s.distance(player) < 260.0).unwrap_or(waypoint);
    PlayerIntent {
        movement: (seek - player).clamp_length_max(80.0) / 80.0,
        aim_point: target.or(star),
        aim_stick: None,
        fire: target.is_some(),
        missile: target.is_some() && (t % 2.0) < 0.02,
    }
}

fn autopilot(
    time: Res<Time>,
    mut intent: ResMut<PlayerIntent>,
    player: Option<Single<&Transform, With<Player>>>,
    enemies: Query<&Transform, With<Enemy>>,
    stars: Query<&Transform, With<Star>>,
) {
    let Some(player) = player else { return };
    let p = player.translation.truncate();
    let nearest =
        |it: &mut dyn Iterator<Item = Vec2>| it.min_by(|a, b| a.distance_squared(p).total_cmp(&b.distance_squared(p)));
    let target = nearest(&mut enemies.iter().map(|t| t.translation.truncate()));
    let star = nearest(&mut stars.iter().map(|t| t.translation.truncate()));
    *intent = autopilot_intent(time.elapsed_secs(), p, target, star);
}

fn auto_exit(time: Res<Time<Real>>, auto: Res<Autoplay>, mut exit: MessageWriter<AppExit>) {
    if auto.exit_after.is_some_and(|s| time.elapsed_secs() >= s) {
        info!("autoplay: exit after {:.1}s", time.elapsed_secs());
        exit.write(AppExit::Success);
    }
}

fn restart_after_gameover(mut next: ResMut<NextState<AppState>>) {
    next.set(AppState::InGame);
}

pub struct AutoplayPlugin;

impl Plugin for AutoplayPlugin {
    fn build(&self, app: &mut App) {
        if let Some(auto) = Autoplay::from_env() {
            app.insert_resource(auto);
            // 无人值守时窗口可能不在前台：默认 unfocused 为低功耗模式，这里强制持续刷新
            app.insert_resource(bevy::winit::WinitSettings {
                focused_mode: bevy::winit::UpdateMode::Continuous,
                unfocused_mode: bevy::winit::UpdateMode::Continuous,
            });
        }
        let enabled = resource_exists::<Autoplay>;
        app.add_systems(Startup, enable_scripted.run_if(enabled)).add_systems(
            Update,
            (
                skip_menu.run_if(enabled.and_then(in_state(AppState::Menu))),
                restart_after_gameover.run_if(enabled.and_then(in_state(AppState::GameOver))),
                autopilot.run_if(enabled.and_then(in_state(AppState::InGame))).in_set(crate::schedule::GameSet::Input),
                auto_exit.run_if(enabled),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autopilot_fires_only_with_target() {
        let i = autopilot_intent(1.0, Vec2::ZERO, None, None);
        assert!(!i.fire && !i.missile);
        let i = autopilot_intent(1.0, Vec2::ZERO, Some(Vec2::X * 100.0), None);
        assert!(i.fire);
        assert_eq!(i.aim_point, Some(Vec2::X * 100.0));
    }

    #[test]
    fn autopilot_movement_is_normalized() {
        for k in 0..100 {
            let i = autopilot_intent(k as f32 * 0.37, Vec2::new(-600.0, 300.0), None, None);
            assert!(i.movement.length() <= 1.0 + 1e-5);
        }
    }

    #[test]
    fn autopilot_prefers_nearby_star() {
        let i = autopilot_intent(0.0, Vec2::ZERO, None, Some(Vec2::new(50.0, 0.0)));
        assert!(i.movement.x > 0.0, "朝附近星星移动");
    }
}
