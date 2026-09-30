//! 各状态的全屏界面：Loading / Menu / Paused / GameOver。
//!
//! 覆盖 API：`OnEnter` + `DespawnOnExit`（状态作用域实体）、`SpawnIter` 批量子节点、
//! `with_children`、`Changed<Interaction>`、`AppExit`、`MessageWriter<AppExit>`、
//! `resource_changed` / `resource_exists` 运行条件、`ZIndex`。

use bevy::ecs::spawn::SpawnIter;
use bevy::prelude::*;

use super::widgets::*;
use crate::assets::LoadingProgress;
use crate::audio::AudioSettings;
use crate::events::PlaySfx;
use crate::score::{HighScores, LastRun};
use crate::state::{AppState, GamePhase};

/// 音量显示文本标记。
#[derive(Component)]
pub struct VolumeLabel;

/// 加载进度文本标记。
#[derive(Component)]
pub struct LoadingLabel;

// ---------------- Loading ----------------

pub fn spawn_loading_screen(mut commands: Commands) {
    commands.spawn((
        Name::new("LoadingScreen"),
        DespawnOnExit(AppState::Loading),
        fullscreen_column(1.0),
        children![heading("LOADING", 42.0, ACCENT), (label("0%", 22.0, TEXT_DIM), LoadingLabel),],
    ));
}

pub fn update_loading_label(progress: Res<LoadingProgress>, mut q: Query<&mut Text, With<LoadingLabel>>) {
    for mut text in &mut q {
        text.0 = format!("{:.0}%  ({}/{})", progress.fraction() * 100.0, progress.loaded, progress.total);
    }
}

// ---------------- Menu ----------------

pub fn volume_text(settings: &AudioSettings) -> String {
    if settings.muted { "Volume: MUTED".to_string() } else { format!("Volume: {}%", settings.master_percent()) }
}

pub fn highscore_lines(hs: &HighScores) -> Vec<String> {
    if hs.entries.is_empty() {
        return vec!["No scores yet - go harvest!".to_string()];
    }
    hs.entries
        .iter()
        .enumerate()
        .map(|(i, e)| format!("{}. {:>7}  wave {}  stars {}", i + 1, e.score, e.wave, e.stars))
        .collect()
}

pub fn spawn_main_menu(mut commands: Commands, settings: Res<AudioSettings>, highscores: Res<HighScores>) {
    let lines = highscore_lines(&highscores);
    commands.spawn((
        Name::new("MainMenu"),
        DespawnOnExit(AppState::Menu),
        // 透明背景：让后面的星空和 Text2d 标题透出来
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexEnd,
            align_items: AlignItems::Center,
            padding: UiRect::bottom(px(40)),
            ..default()
        },
        children![(
            panel(),
            children![
                button("Play  [Enter]", ButtonAction::Play, 260.0),
                (
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, ..default() },
                    children![
                        button("-", ButtonAction::VolumeDown, 52.0),
                        (
                            label(volume_text(&settings), 20.0, TEXT),
                            TextLayout::justify(Justify::Center),
                            VolumeLabel,
                            Node { width: px(170), justify_content: JustifyContent::Center, ..default() },
                        ),
                        button("+", ButtonAction::VolumeUp, 52.0),
                        button("Mute", ButtonAction::ToggleMute, 90.0),
                    ]
                ),
                button("Quit", ButtonAction::Quit, 260.0),
                (label("HIGH SCORES", 18.0, ACCENT), Node { margin: UiRect::top(px(14)), ..default() }),
                (
                    Node { flex_direction: FlexDirection::Column, align_items: AlignItems::FlexStart, ..default() },
                    // SpawnIter：从迭代器批量生成子节点
                    Children::spawn(SpawnIter(lines.into_iter().map(|l| label(l, 16.0, TEXT_DIM)))),
                ),
                (
                    label(
                        "WASD move - Mouse aim - LMB/Space fire - RMB/E missile - Esc pause\n\
                         F1 gizmos - F2 fps - F5 snapshot - F12 screenshot - K nuke - C carrier - [ ] time",
                        13.0,
                        TEXT_DIM
                    ),
                    Node { margin: UiRect::top(px(12)), ..default() },
                ),
            ]
        )],
    ));
}

pub fn update_volume_label(settings: Res<AudioSettings>, mut q: Query<&mut Text, With<VolumeLabel>>) {
    for mut t in &mut q {
        t.0 = volume_text(&settings);
    }
}

pub fn menu_keyboard(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::Enter) {
        next.set(AppState::InGame);
    }
}

// ---------------- Paused ----------------

pub fn spawn_pause_overlay(mut commands: Commands) {
    commands.spawn((
        Name::new("PauseOverlay"),
        DespawnOnExit(GamePhase::Paused),
        ZIndex(10),
        fullscreen_column(0.65),
        children![(
            panel(),
            children![
                heading("PAUSED", 48.0, ACCENT),
                button("Resume  [Esc]", ButtonAction::Resume, 260.0),
                button("Main Menu", ButtonAction::BackToMenu, 260.0),
            ]
        )],
    ));
}

// ---------------- GameOver ----------------

pub fn gameover_lines(run: &LastRun) -> Vec<String> {
    let s = &run.stats;
    vec![
        format!("Score        {}", run.score),
        match run.rank {
            Some(0) => "NEW HIGH SCORE!".to_string(),
            Some(r) => format!("Rank         #{}", r + 1),
            None => "Rank         -".to_string(),
        },
        format!("Wave         {}", s.wave),
        format!("Stars        {}", s.stars),
        format!("Kills        {} (M{} H{} C{})", s.total_kills(), s.kills[0], s.kills[1], s.kills[2]),
        format!("Best combo   x{}", run.best_combo),
        format!("Accuracy     {:.0}%", s.accuracy() * 100.0),
        format!("Survived     {:.0}s", s.seconds),
    ]
}

pub fn spawn_gameover_screen(mut commands: Commands, run: Res<LastRun>) {
    let lines = gameover_lines(&run);
    commands.spawn((
        Name::new("GameOver"),
        DespawnOnExit(AppState::GameOver),
        fullscreen_column(0.8),
        children![(
            panel(),
            children![
                heading("GAME OVER", 52.0, Color::srgb(1.0, 0.35, 0.35)),
                (
                    Node { flex_direction: FlexDirection::Column, margin: UiRect::vertical(px(10)), ..default() },
                    Children::spawn(SpawnIter(lines.into_iter().map(|l| label(l, 20.0, TEXT)))),
                ),
                button("Retry  [Enter]", ButtonAction::Play, 260.0),
                button("Main Menu  [Esc]", ButtonAction::BackToMenu, 260.0),
            ]
        )],
    ));
}

pub fn gameover_keyboard(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::Enter) {
        next.set(AppState::InGame);
    } else if keys.just_pressed(KeyCode::Escape) {
        next.set(AppState::Menu);
    }
}

// ---------------- 按钮统一处理 ----------------

/// 视觉反馈：只处理 Interaction 发生变化的按钮。
pub fn button_visuals(
    mut q: Query<(&Interaction, &mut BackgroundColor, &mut BorderColor), (Changed<Interaction>, With<Button>)>,
) {
    for (interaction, mut bg, mut border) in &mut q {
        let (b, c) = button_colors(*interaction);
        bg.0 = b;
        *border = BorderColor::all(c);
    }
}

/// 行为分发。
pub fn button_actions(
    q: Query<(&Interaction, &ButtonAction), Changed<Interaction>>,
    mut app_state: ResMut<NextState<AppState>>,
    mut phase: ResMut<NextState<GamePhase>>,
    mut settings: ResMut<AudioSettings>,
    mut exit: MessageWriter<AppExit>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    for (interaction, action) in &q {
        if *interaction != Interaction::Pressed {
            continue;
        }
        sfx.write(PlaySfx::UiClick);
        match action {
            ButtonAction::Play => app_state.set(AppState::InGame),
            ButtonAction::Resume => phase.set(GamePhase::Playing),
            ButtonAction::BackToMenu => app_state.set(AppState::Menu),
            ButtonAction::Quit => {
                exit.write(AppExit::Success);
            }
            ButtonAction::VolumeDown => settings.step_master(-AudioSettings::STEP),
            ButtonAction::VolumeUp => settings.step_master(AudioSettings::STEP),
            ButtonAction::ToggleMute => settings.muted ^= true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{HighScoreEntry, RunStats};

    #[test]
    fn volume_text_variants() {
        let mut s = AudioSettings::default();
        assert_eq!(volume_text(&s), "Volume: 70%");
        s.muted = true;
        assert_eq!(volume_text(&s), "Volume: MUTED");
    }

    #[test]
    fn highscore_lines_empty_and_filled() {
        assert_eq!(highscore_lines(&HighScores::default()).len(), 1);
        let mut hs = HighScores::default();
        hs.submit(HighScoreEntry { score: 1234, wave: 3, stars: 9 });
        let lines = highscore_lines(&hs);
        assert!(lines[0].starts_with("1."));
        assert!(lines[0].contains("1234"));
    }

    #[test]
    fn gameover_lines_rank_variants() {
        let mut run = LastRun { score: 10, rank: Some(0), stats: RunStats::default(), best_combo: 3 };
        assert!(gameover_lines(&run).iter().any(|l| l.contains("NEW HIGH SCORE")));
        run.rank = Some(2);
        assert!(gameover_lines(&run).iter().any(|l| l.contains("#3")));
        run.rank = None;
        assert!(gameover_lines(&run).iter().any(|l| l.ends_with('-')));
    }
}
