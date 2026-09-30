//! 相机：2D 正交投影 + HDR Bloom + 色调映射 + 基于 trauma 的屏幕震动。
//!
//! 覆盖 API：`Camera2d`、`Camera { clear_color }`、`Projection::Orthographic` +
//! `ScalingMode::AutoMin`、`Bloom`、`Tonemapping`、`DebandDither`、`ClearColor`、
//! `PreUpdate`/`PostUpdate` 排程、`TransformSystems::Propagate` 排序约束、
//! 光标图标 `CursorIcon::System(SystemCursorIcon::Crosshair)`。

use bevy::camera::ScalingMode;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon};

use crate::effects::CameraTrauma;
use crate::input::MainCamera;
use crate::math::{ARENA_SIZE, noise1d, shake_amount};
use crate::state::AppState;

pub const BACKGROUND: Color = Color::srgb(0.02, 0.02, 0.05);

/// 震动前的原始变换（每帧开始恢复，保证游戏逻辑看到的相机位置是干净的）。
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ShakeOrigin(pub Transform);

pub const MAX_SHAKE_OFFSET: f32 = 14.0;
pub const MAX_SHAKE_ANGLE: f32 = 0.05;

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("MainCamera"),
        Camera2d,
        MainCamera,
        Camera { clear_color: ClearColorConfig::Custom(BACKGROUND), ..default() },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin { min_width: ARENA_SIZE.x, min_height: ARENA_SIZE.y },
            ..OrthographicProjection::default_2d()
        }),
        // TonyMcMapface 需要 `tonemapping_luts`（属于 3D 特性集），2D 精简构建里用 AcesFitted
        Tonemapping::AcesFitted,
        Bloom { intensity: 0.22, ..Bloom::NATURAL },
        DebandDither::Enabled,
        ShakeOrigin::default(),
    ));
}

fn reset_camera(mut q: Query<(&mut Transform, &ShakeOrigin), With<MainCamera>>) {
    for (mut tf, origin) in &mut q {
        *tf = origin.0;
    }
}

fn apply_shake(
    time: Res<Time>,
    trauma: Res<CameraTrauma>,
    mut q: Query<(&mut Transform, &mut ShakeOrigin), With<MainCamera>>,
) {
    let t = time.elapsed_secs() * 22.0;
    let amount = shake_amount(trauma.0, 2.0);
    for (mut tf, mut origin) in &mut q {
        origin.0 = *tf;
        if amount <= 0.0 {
            continue;
        }
        tf.translation.x += noise1d(t) * amount * MAX_SHAKE_OFFSET;
        tf.translation.y += noise1d(t + 91.0) * amount * MAX_SHAKE_OFFSET;
        tf.rotate_z(noise1d(t + 187.0) * amount * MAX_SHAKE_ANGLE);
    }
}

fn crosshair_cursor(mut commands: Commands, window: Single<Entity, With<PrimaryWindow>>) {
    commands.entity(*window).insert(CursorIcon::System(SystemCursorIcon::Crosshair));
}

fn default_cursor(mut commands: Commands, window: Single<Entity, With<PrimaryWindow>>) {
    commands.entity(*window).insert(CursorIcon::System(SystemCursorIcon::Default));
}

pub struct GameCameraPlugin;

impl Plugin for GameCameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(BACKGROUND))
            .add_systems(Startup, spawn_camera)
            .add_systems(PreUpdate, reset_camera)
            .add_systems(PostUpdate, apply_shake.before(TransformSystems::Propagate))
            .add_systems(OnEnter(AppState::InGame), crosshair_cursor)
            .add_systems(OnExit(AppState::InGame), default_cursor);
    }
}
