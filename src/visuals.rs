//! 纯表现：视差星空背景、菜单标题的关键帧动画（AnimationClip + EasingCurve）、窗口标题。
//!
//! 覆盖 API：`AnimationClip::add_curve_to_target` + `AnimatableCurve` + `animated_field!`、
//! `EasingCurve::reparametrize_linear/ping_pong`、`AnimationGraph::from_clip`、
//! `AnimationPlayer::play().repeat()`、`AnimationTargetId` + `AnimatedBy`、
//! `Query::par_iter_mut`、`Window::title` 动态修改、`resource_changed`。

use bevy::animation::{AnimatedBy, AnimationTargetId, animated_field};
use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use bevy::window::PrimaryWindow;

use crate::math::ARENA_HALF;
use crate::rng::GameRng;
use crate::score::Score;
use crate::state::AppState;

/// 背景星点：`depth` 越大越近、越快。
#[derive(Component, Debug, Clone, Copy)]
pub struct BackgroundStar {
    pub depth: f32,
}

fn spawn_starfield(mut commands: Commands) {
    // 背景用独立 seed，不影响游戏逻辑 RNG 的可复现性
    let mut rng = GameRng::seeded(0xBAC4_6120);
    for _ in 0..220 {
        let depth = rng.range_f32(0.15, 1.0);
        let pos = rng.point_in_rect(ARENA_HALF + Vec2::splat(40.0));
        let b = 0.25 + depth * 0.9;
        commands.spawn((
            Name::new("BgStar"),
            BackgroundStar { depth },
            Sprite::from_color(Color::srgb(b, b, b * 1.15), Vec2::splat(1.0 + depth * 2.2)),
            Transform::from_translation(pos.extend(-10.0 + depth)),
        ));
    }
}

/// 视差滚动：大量实体 → `par_iter_mut` 并行。使用 `Time<Real>`，暂停时背景仍缓慢流动。
fn scroll_starfield(time: Res<Time<Real>>, mut q: Query<(&mut Transform, &BackgroundStar)>) {
    let dt = time.delta_secs();
    let limit = ARENA_HALF.y + 40.0;
    q.par_iter_mut().for_each(|(mut tf, star)| {
        tf.translation.y -= 40.0 * star.depth * dt;
        if tf.translation.y < -limit {
            tf.translation.y = limit;
        }
    });
}

/// 菜单标题：程序化构建 AnimationClip（缩放呼吸 + 上下浮动）。
fn spawn_title(
    mut commands: Commands,
    mut clips: ResMut<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let name = Name::new("TitleText");
    let target_id = AnimationTargetId::from_name(&name);

    let mut clip = AnimationClip::default();
    let bob = EasingCurve::new(Vec3::new(0.0, 190.0, 5.0), Vec3::new(0.0, 205.0, 5.0), EaseFunction::SineInOut)
        .reparametrize_linear(interval(0.0, 1.6).unwrap())
        .expect("bounded")
        .ping_pong()
        .expect("bounded");
    let breathe = EasingCurve::new(Vec3::splat(1.0), Vec3::new(1.06, 1.06, 1.0), EaseFunction::QuadraticInOut)
        .reparametrize_linear(interval(0.0, 0.8).unwrap())
        .expect("bounded")
        .ping_pong()
        .expect("bounded");
    clip.add_curve_to_target(target_id, AnimatableCurve::new(animated_field!(Transform::translation), bob));
    clip.add_curve_to_target(target_id, AnimatableCurve::new(animated_field!(Transform::scale), breathe));

    let (graph, node) = AnimationGraph::from_clip(clips.add(clip));
    let mut player = AnimationPlayer::default();
    player.play(node).repeat();

    let title = commands
        .spawn((
            name,
            Text2d::new("STAR HARVESTER"),
            TextFont::from_font_size(64.0),
            TextColor(Color::srgb(1.8, 1.5, 0.5)), // HDR → 被 Bloom 点亮
            Text2dShadow { offset: Vec2::new(4.0, -4.0), color: Color::srgba(0.0, 0.0, 0.0, 0.8) },
            Anchor::CENTER,
            Transform::from_xyz(0.0, 190.0, 5.0),
            player,
            AnimationGraphHandle(graphs.add(graph)),
            DespawnOnExit(AppState::Menu),
        ))
        .id();
    commands.entity(title).insert((target_id, AnimatedBy(title)));
}

/// 窗口标题显示分数（只在分数变化时更新）。
fn update_window_title(score: Res<Score>, mut window: Query<&mut Window, With<PrimaryWindow>>) {
    for mut w in &mut window {
        w.title = format!("Star Harvester — {} pts", score.points);
    }
}

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_starfield)
            .add_systems(OnEnter(AppState::Menu), spawn_title)
            .add_systems(Update, (scroll_starfield, update_window_title.run_if(resource_changed::<Score>)));
    }
}
