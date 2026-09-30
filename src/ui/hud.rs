//! 游戏内 HUD：分数（多段 TextSpan）、血条、心形图标、波次横幅（延迟命令自动销毁）。
//!
//! 覆盖 API：`Text` + `TextSpan` 子节点 + `TextUiWriter`（按 span 下标改字）、
//! `ImageNode`、`Node.width = percent(..)` 动态血条、`Changed<Health>`、
//! `resource_changed::<Score>`、`commands.delayed().secs(..)`（DelayedCommandsExt）、
//! `in_state(ShowHud)` 计算状态作为运行条件、`ChildOf` 父子关系插入。

use bevy::prelude::*;

use super::widgets::*;
use crate::assets::GameAssets;
use crate::components::{Health, Player};
use crate::events::WaveStarted;
use crate::score::{RunStats, Score};
use crate::state::{AppState, ShowHud};

#[derive(Component)]
pub struct ScoreText;
#[derive(Component)]
pub struct WaveText;
#[derive(Component)]
pub struct HealthFill;
#[derive(Component)]
pub struct HeartRow;
#[derive(Component)]
pub struct WaveBanner;

pub fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Name::new("Hud"),
        DespawnOnExit(AppState::InGame),
        Node {
            width: percent(100),
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(0),
            padding: UiRect::axes(px(16), px(10)),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::FlexStart,
            ..default()
        },
        children![
            // 左：分数（Text 根 + 3 个 TextSpan，下标 1/3 可写）
            (
                ScoreText,
                Text::new("SCORE "),
                TextFont::from_font_size(24.0),
                TextColor(TEXT_DIM),
                children![
                    (TextSpan::new("0"), TextFont::from_font_size(24.0), TextColor(ACCENT)),
                    (TextSpan::new("   COMBO "), TextFont::from_font_size(18.0), TextColor(TEXT_DIM)),
                    (TextSpan::new("x1"), TextFont::from_font_size(18.0), TextColor(TEXT)),
                ],
            ),
            // 中：波次
            (WaveText, label("WAVE 1", 22.0, TEXT)),
            // 右：心 + 血条
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexEnd,
                    row_gap: px(6),
                    ..default()
                },
                children![
                    (HeartRow, Node { flex_direction: FlexDirection::Row, column_gap: px(4), ..default() },),
                    (
                        Node {
                            width: px(180),
                            height: px(10),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(5)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.15, 0.05, 0.08)),
                        BorderColor::all(Color::srgb(0.5, 0.2, 0.3)),
                        children![(
                            HealthFill,
                            Node {
                                width: percent(100),
                                height: percent(100),
                                border_radius: BorderRadius::all(px(5)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(1.0, 0.3, 0.4)),
                        )],
                    ),
                ],
            ),
        ],
    ));
}

/// 纯函数：连击文本。
pub fn combo_text(score: &Score) -> String {
    format!("x{}", score.multiplier())
}

pub fn update_score_text(
    score: Res<Score>,
    root: Single<Entity, (With<ScoreText>, With<Text>)>,
    mut writer: TextUiWriter,
) {
    *writer.text(*root, 1) = score.points.to_string();
    *writer.text(*root, 3) = combo_text(&score);
}

pub fn update_wave_text(stats: Res<RunStats>, mut q: Query<&mut Text, With<WaveText>>) {
    for mut t in &mut q {
        let s = format!("WAVE {}", stats.wave);
        if t.0 != s {
            t.0 = s;
        }
    }
}

/// 血量变化 → 更新血条宽度 + 重建心形图标。
pub fn update_health_ui(
    mut commands: Commands,
    assets: Res<GameAssets>,
    player: Query<&Health, (With<Player>, Changed<Health>)>,
    mut fill: Query<&mut Node, With<HealthFill>>,
    rows: Query<(Entity, Option<&Children>), With<HeartRow>>,
) {
    let Ok(health) = player.single() else { return };
    for mut node in &mut fill {
        node.width = percent(health.fraction() * 100.0);
    }
    for (row, children) in &rows {
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }
        for _ in 0..health.current.max(0) {
            commands.spawn((
                ImageNode::new(assets.heart.clone()),
                Node { width: px(20), height: px(20), ..default() },
                ChildOf(row),
            ));
        }
    }
}

/// 波次横幅：2 秒后用“延迟命令”自动销毁，不需要自己写计时器系统。
pub fn show_wave_banner(mut commands: Commands, mut waves: MessageReader<WaveStarted>) {
    for WaveStarted(wave) in waves.read() {
        let banner = commands
            .spawn((
                Name::new("WaveBanner"),
                WaveBanner,
                DespawnOnExit(AppState::InGame),
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    top: percent(40),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                children![heading(format!("WAVE {wave}"), 56.0, ACCENT)],
            ))
            .id();
        commands.delayed().secs(2.0).entity(banner).try_despawn();
    }
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_hud).add_systems(
            Update,
            (
                update_score_text.run_if(resource_changed::<Score>),
                update_wave_text.run_if(resource_changed::<RunStats>),
                update_health_ui,
                show_wave_banner,
            )
                .run_if(in_state(ShowHud)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_text_uses_multiplier() {
        let mut s = Score::default();
        assert_eq!(combo_text(&s), "x1");
        for _ in 0..10 {
            s.add(1);
        }
        assert_eq!(combo_text(&s), "x3");
    }
}
