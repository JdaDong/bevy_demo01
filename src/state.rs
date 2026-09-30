//! 状态机：`States` + `SubStates` + `ComputedStates` 三件套。
//!
//! ```text
//! AppState:  Loading ──► Menu ──► InGame ──► GameOver ──► (Menu | InGame)
//!                                   │
//!                GamePhase(SubState)├─ Playing ◄──► Paused
//!                                   │
//!          Simulating(Computed) = InGame && Playing   → 驱动所有仿真系统
//!          ShowHud(Computed)    = InGame (含暂停)      → 驱动 HUD
//! ```
//!
//! 覆盖 API：`init_state`、`add_sub_state`、`add_computed_state`、`OnEnter/OnExit`、
//! `NextState`、`State`、`in_state`、`state_changed`、`Time<Virtual>::pause/unpause`、
//! `StateScopedMessagesAppExt::clear_messages_on_exit`。

use bevy::prelude::*;

/// 顶层状态。
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum AppState {
    /// 等待贴图/音频/配置加载完成。
    #[default]
    Loading,
    Menu,
    InGame,
    GameOver,
}

/// 只在 `AppState::InGame` 时存在的子状态。
#[derive(SubStates, Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
#[source(AppState = AppState::InGame)]
pub enum GamePhase {
    #[default]
    Playing,
    Paused,
}

/// 计算状态：仅在“游戏中且未暂停”时存在。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Simulating;

impl ComputedStates for Simulating {
    type SourceStates = GamePhase;

    fn compute(phase: GamePhase) -> Option<Self> {
        (phase == GamePhase::Playing).then_some(Simulating)
    }
}

/// 计算状态（多源）：游戏中（不论暂停与否）显示 HUD。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShowHud;

impl ComputedStates for ShowHud {
    type SourceStates = (AppState, Option<GamePhase>);

    fn compute((app, phase): (AppState, Option<GamePhase>)) -> Option<Self> {
        (app == AppState::InGame && phase.is_some()).then_some(ShowHud)
    }
}

/// 注册状态、暂停逻辑。
pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .add_sub_state::<GamePhase>()
            .add_computed_state::<Simulating>()
            .add_computed_state::<ShowHud>()
            .add_systems(OnEnter(GamePhase::Paused), pause_virtual_time)
            .add_systems(OnExit(GamePhase::Paused), resume_virtual_time)
            // 离开游戏时保证时间恢复（从暂停界面直接回菜单的情况）
            .add_systems(OnExit(AppState::InGame), resume_virtual_time);
    }
}

fn pause_virtual_time(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

fn resume_virtual_time(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}

/// 切换暂停；给 UI 按钮与键盘共用。
pub fn toggle_pause(current: &GamePhase) -> GamePhase {
    match current {
        GamePhase::Playing => GamePhase::Paused,
        GamePhase::Paused => GamePhase::Playing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulating_only_when_playing() {
        assert_eq!(Simulating::compute(GamePhase::Playing), Some(Simulating));
        assert_eq!(Simulating::compute(GamePhase::Paused), None);
    }

    #[test]
    fn hud_visible_in_game_even_when_paused() {
        assert_eq!(ShowHud::compute((AppState::InGame, Some(GamePhase::Paused))), Some(ShowHud));
        assert_eq!(ShowHud::compute((AppState::InGame, Some(GamePhase::Playing))), Some(ShowHud));
        assert_eq!(ShowHud::compute((AppState::Menu, None)), None);
        assert_eq!(ShowHud::compute((AppState::GameOver, None)), None);
    }

    #[test]
    fn toggle_is_involution() {
        for p in [GamePhase::Playing, GamePhase::Paused] {
            assert_eq!(toggle_pause(&toggle_pause(&p)), p);
            assert_ne!(toggle_pause(&p), p);
        }
    }

    #[test]
    fn defaults() {
        assert_eq!(AppState::default(), AppState::Loading);
        assert_eq!(GamePhase::default(), GamePhase::Playing);
    }
}
