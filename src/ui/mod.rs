//! Bevy UI：菜单、HUD、暂停层、结算页。
//!
//! 这些系统只操作组件数据（`Node`/`Text`/`Interaction`），不依赖渲染器，
//! 因此在无头测试里也能跑：测试可以直接把按钮的 `Interaction` 设为 `Pressed` 来“点击”。

pub mod hud;
pub mod screens;
pub mod widgets;

use bevy::prelude::*;

use crate::assets::LoadingProgress;
use crate::audio::AudioSettings;
use crate::state::{AppState, GamePhase};

pub struct GameUiPlugin;

impl Plugin for GameUiPlugin {
    fn build(&self, app: &mut App) {
        use screens::*;
        // TextUiWriter 依赖该资源（通常由 TextPlugin 插入）；init 幂等，
        // 这样 UI 逻辑在没有 TextPlugin 的无头测试里也能运行。
        app.init_resource::<bevy::text::TextIterScratch>()
            .add_plugins(hud::HudPlugin)
            .add_systems(OnEnter(AppState::Loading), spawn_loading_screen)
            .add_systems(OnEnter(AppState::Menu), spawn_main_menu)
            .add_systems(OnEnter(GamePhase::Paused), spawn_pause_overlay)
            .add_systems(OnEnter(AppState::GameOver), spawn_gameover_screen.after(crate::save::submit_and_save))
            .add_systems(
                Update,
                (
                    (button_visuals, button_actions),
                    update_loading_label
                        .run_if(in_state(AppState::Loading).and_then(resource_exists::<LoadingProgress>)),
                    update_volume_label.run_if(resource_changed::<AudioSettings>),
                    menu_keyboard.run_if(in_state(AppState::Menu)),
                    gameover_keyboard.run_if(in_state(AppState::GameOver)),
                ),
            );
    }
}
