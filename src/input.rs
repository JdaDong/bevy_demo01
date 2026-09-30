//! 输入抽象层：键盘 / 鼠标 / 手柄 → 统一的 [`PlayerIntent`]。
//!
//! 游戏逻辑只读 `PlayerIntent`，不直接碰 `ButtonInput`，因此：
//! 1. 换键位 / 加手柄只改这里；
//! 2. 测试可以直接写 `PlayerIntent`，或者往 `ButtonInput` 里 `press()`。
//!
//! 覆盖 API：`ButtonInput<KeyCode>` / `ButtonInput<MouseButton>`、`Gamepad`（摇杆/按键）、
//! `Window::cursor_position`、`Camera::viewport_to_world_2d`、`PrimaryWindow`、
//! `Option<Single<..>>`、`#[derive(SystemParam)]`。

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::schedule::GameSet;
use crate::state::{AppState, GamePhase, toggle_pause};

/// 当帧玩家意图。每帧由 [`gather_intent`] 重写。
#[derive(Resource, Reflect, Debug, Default, Clone, PartialEq)]
#[reflect(Resource)]
pub struct PlayerIntent {
    /// 移动方向，长度 ≤ 1
    pub movement: Vec2,
    /// 世界坐标的瞄准点（鼠标）；`None` 表示用摇杆/移动方向
    pub aim_point: Option<Vec2>,
    /// 摇杆瞄准方向
    pub aim_stick: Option<Vec2>,
    pub fire: bool,
    pub missile: bool,
}

/// 意图来源。`Scripted` 时不再读设备，测试 / AI 回放可直接写 `PlayerIntent`。
#[derive(Resource, Reflect, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[reflect(Resource)]
pub enum IntentSource {
    #[default]
    Devices,
    Scripted,
}

/// 主相机标记（输入层需要它做屏幕→世界坐标转换）。
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct MainCamera;

/// 键位绑定（可 Reflect，未来可做改键 UI）。
#[derive(Resource, Reflect, Debug, Clone)]
#[reflect(Resource)]
pub struct KeyBindings {
    pub up: [KeyCode; 2],
    pub down: [KeyCode; 2],
    pub left: [KeyCode; 2],
    pub right: [KeyCode; 2],
    pub fire: KeyCode,
    pub missile: KeyCode,
    pub pause: [KeyCode; 2],
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            up: [KeyCode::KeyW, KeyCode::ArrowUp],
            down: [KeyCode::KeyS, KeyCode::ArrowDown],
            left: [KeyCode::KeyA, KeyCode::ArrowLeft],
            right: [KeyCode::KeyD, KeyCode::ArrowRight],
            fire: KeyCode::Space,
            missile: KeyCode::KeyE,
            pause: [KeyCode::Escape, KeyCode::KeyP],
        }
    }
}

/// 纯函数：键盘 → 归一化移动向量。
pub fn movement_from_keys(keys: &ButtonInput<KeyCode>, bind: &KeyBindings) -> Vec2 {
    let axis = |neg: &[KeyCode; 2], pos: &[KeyCode; 2]| -> f32 {
        let n = keys.any_pressed(*neg) as i32 as f32;
        let p = keys.any_pressed(*pos) as i32 as f32;
        p - n
    };
    Vec2::new(axis(&bind.left, &bind.right), axis(&bind.down, &bind.up)).normalize_or_zero()
}

/// 摇杆死区处理。
pub fn apply_deadzone(v: Vec2, deadzone: f32) -> Vec2 {
    if v.length() < deadzone { Vec2::ZERO } else { v.clamp_length_max(1.0) }
}

/// 把“获取鼠标世界坐标”封装成自定义 SystemParam。
#[derive(SystemParam)]
pub struct CursorWorld<'w, 's> {
    window: Option<Single<'w, 's, &'static Window, With<PrimaryWindow>>>,
    camera: Option<Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<MainCamera>>>,
}

impl CursorWorld<'_, '_> {
    /// 无窗口 / 无相机 / 光标不在窗口内 → `None`
    pub fn position(&self) -> Option<Vec2> {
        let window = self.window.as_ref()?;
        let (camera, cam_tf) = **self.camera.as_ref()?;
        let cursor = window.cursor_position()?;
        camera.viewport_to_world_2d(cam_tf, cursor).ok()
    }
}

pub const STICK_DEADZONE: f32 = 0.2;

/// 汇总所有输入设备。
pub fn gather_intent(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    bindings: Res<KeyBindings>,
    gamepads: Query<&Gamepad>,
    cursor: CursorWorld,
    mut intent: ResMut<PlayerIntent>,
) {
    let mut movement = movement_from_keys(&keys, &bindings);
    let mut fire = keys.pressed(bindings.fire) || mouse.pressed(MouseButton::Left);
    let mut missile = keys.just_pressed(bindings.missile) || mouse.just_pressed(MouseButton::Right);
    let mut aim_stick = None;

    for pad in &gamepads {
        let stick = apply_deadzone(pad.left_stick() + pad.dpad(), STICK_DEADZONE);
        if stick != Vec2::ZERO {
            movement = stick;
        }
        let right = apply_deadzone(pad.right_stick(), STICK_DEADZONE);
        if right != Vec2::ZERO {
            aim_stick = Some(right);
            fire = true; // 双摇杆射击：推右摇杆即开火
        }
        fire |= pad.pressed(GamepadButton::RightTrigger2) || pad.pressed(GamepadButton::South);
        missile |= pad.just_pressed(GamepadButton::RightTrigger) || pad.just_pressed(GamepadButton::West);
    }

    *intent = PlayerIntent {
        movement,
        aim_point: if aim_stick.is_some() { None } else { cursor.position() },
        aim_stick,
        fire,
        missile,
    };
}

/// 暂停键：在 Playing / Paused 之间切换。
pub fn pause_input(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<KeyBindings>,
    gamepads: Query<&Gamepad>,
    phase: Res<State<GamePhase>>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    let pressed =
        keys.any_just_pressed(bindings.pause) || gamepads.iter().any(|g| g.just_pressed(GamepadButton::Start));
    if pressed {
        next.set(toggle_pause(phase.get()));
    }
}

pub struct GameInputPlugin;

impl Plugin for GameInputPlugin {
    fn build(&self, app: &mut App) {
        // 真实运行时 DefaultPlugins 已经插入了这些资源；init 是幂等的，
        // 这里再 init 一次是为了让无头测试不必手动插入。
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<PlayerIntent>()
            .init_resource::<KeyBindings>()
            .init_resource::<IntentSource>()
            .add_systems(
                Update,
                (
                    gather_intent.run_if(in_state(GamePhase::Playing)).run_if(resource_equals(IntentSource::Devices)),
                    pause_input.run_if(in_state(AppState::InGame)),
                )
                    .in_set(GameSet::Input),
            )
            .add_systems(OnExit(GamePhase::Playing), clear_intent);
    }
}

fn clear_intent(mut intent: ResMut<PlayerIntent>) {
    *intent = PlayerIntent::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(pressed: &[KeyCode]) -> ButtonInput<KeyCode> {
        let mut k = ButtonInput::default();
        for p in pressed {
            k.press(*p);
        }
        k
    }

    #[test]
    fn no_keys_no_movement() {
        assert_eq!(movement_from_keys(&keys(&[]), &KeyBindings::default()), Vec2::ZERO);
    }

    #[test]
    fn wasd_and_arrows_both_work() {
        let b = KeyBindings::default();
        assert_eq!(movement_from_keys(&keys(&[KeyCode::KeyW]), &b), Vec2::Y);
        assert_eq!(movement_from_keys(&keys(&[KeyCode::ArrowLeft]), &b), -Vec2::X);
    }

    #[test]
    fn diagonal_is_normalized() {
        let v = movement_from_keys(&keys(&[KeyCode::KeyW, KeyCode::KeyD]), &KeyBindings::default());
        assert!((v.length() - 1.0).abs() < 1e-5);
        assert!(v.x > 0.0 && v.y > 0.0);
    }

    #[test]
    fn opposite_keys_cancel() {
        let v = movement_from_keys(&keys(&[KeyCode::KeyA, KeyCode::KeyD]), &KeyBindings::default());
        assert_eq!(v, Vec2::ZERO);
    }

    #[test]
    fn custom_bindings_are_respected() {
        let b = KeyBindings { up: [KeyCode::KeyI, KeyCode::KeyI], ..default() };
        assert_eq!(movement_from_keys(&keys(&[KeyCode::KeyI]), &b), Vec2::Y);
        assert_eq!(movement_from_keys(&keys(&[KeyCode::KeyW]), &b), Vec2::ZERO);
    }

    #[test]
    fn deadzone() {
        assert_eq!(apply_deadzone(Vec2::new(0.1, 0.1), 0.2), Vec2::ZERO);
        assert_eq!(apply_deadzone(Vec2::new(0.5, 0.0), 0.2), Vec2::new(0.5, 0.0));
        assert!((apply_deadzone(Vec2::new(3.0, 4.0), 0.2).length() - 1.0).abs() < 1e-5);
    }
}
