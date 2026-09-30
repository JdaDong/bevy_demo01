//! 可复用 UI 组件：按钮、标题、面板。
//!
//! 覆盖 API：`Button` + `Interaction`、`Node`（Flex 布局）、`BorderColor::all`、
//! `Node::border_radius` + `BorderRadius`、`BoxShadow`、`Outline`、`BackgroundColor`、
//! `BackgroundGradient` + `LinearGradient` + `ColorStop`、`TextShadow`、`children![]`、
//! `impl Bundle` 返回值（UI 模板函数）。

use bevy::prelude::*;

/// 所有按钮行为集中在一个枚举里，一个系统统一处理。
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonAction {
    Play,
    Resume,
    BackToMenu,
    Quit,
    VolumeDown,
    VolumeUp,
    ToggleMute,
}

pub const TEXT: Color = Color::srgb(0.92, 0.94, 1.0);
pub const TEXT_DIM: Color = Color::srgb(0.6, 0.65, 0.8);
pub const ACCENT: Color = Color::srgb(1.0, 0.82, 0.3);
pub const BTN_NORMAL: Color = Color::srgb(0.12, 0.13, 0.22);
pub const BTN_HOVER: Color = Color::srgb(0.2, 0.22, 0.36);
pub const BTN_PRESSED: Color = Color::srgb(0.36, 0.3, 0.12);
pub const BORDER_NORMAL: Color = Color::srgb(0.3, 0.33, 0.55);
pub const BORDER_HOVER: Color = ACCENT;

/// Interaction → (背景色, 边框色)。纯函数，便于测试。
pub fn button_colors(interaction: Interaction) -> (Color, Color) {
    match interaction {
        Interaction::Pressed => (BTN_PRESSED, ACCENT),
        Interaction::Hovered => (BTN_HOVER, BORDER_HOVER),
        Interaction::None => (BTN_NORMAL, BORDER_NORMAL),
    }
}

/// 标准按钮模板。
pub fn button(label: impl Into<String>, action: ButtonAction, width: f32) -> impl Bundle {
    (
        Button,
        action,
        Name::new(format!("Button::{action:?}")),
        Node {
            width: px(width),
            height: px(52),
            margin: UiRect::all(px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(BTN_NORMAL),
        BorderColor::all(BORDER_NORMAL),
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.6), px(0), px(4), px(0), px(8)),
        children![(Text::new(label), TextFont::from_font_size(22.0), TextColor(TEXT), TextShadow::default(),)],
    )
}

/// 大标题文本。
pub fn heading(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont::from_font_size(size),
        TextColor(color),
        TextShadow { offset: Vec2::new(3.0, 3.0), color: Color::srgba(0.0, 0.0, 0.0, 0.75) },
        Node { margin: UiRect::bottom(px(12)), ..default() },
    )
}

/// 普通文本。
pub fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(text), TextFont::from_font_size(size), TextColor(color))
}

/// 全屏居中容器（带竖直渐变背景）。
pub fn fullscreen_column(alpha: f32) -> impl Bundle {
    (
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(4),
            ..default()
        },
        BackgroundGradient::from(LinearGradient::to_bottom(vec![
            ColorStop::auto(Color::srgba(0.05, 0.03, 0.15, alpha)),
            ColorStop::auto(Color::srgba(0.0, 0.0, 0.02, alpha)),
        ])),
    )
}

/// 圆角面板。
pub fn panel() -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding: UiRect::all(px(24)),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(16)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.06, 0.07, 0.14, 0.92)),
        BorderColor::all(BORDER_NORMAL),
        Outline::new(px(1), px(3), Color::srgba(1.0, 0.82, 0.3, 0.25)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_interaction_has_distinct_colors() {
        let a = button_colors(Interaction::None);
        let b = button_colors(Interaction::Hovered);
        let c = button_colors(Interaction::Pressed);
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn button_template_spawns_expected_components() {
        let mut world = World::new();
        let e = world.spawn(button("Play", ButtonAction::Play, 200.0)).id();
        let entity = world.entity(e);
        assert!(entity.contains::<Button>());
        assert_eq!(entity.get::<ButtonAction>(), Some(&ButtonAction::Play));
        assert!(entity.contains::<Interaction>(), "Button 的 required component");
        let children = entity.get::<Children>().expect("文字子节点");
        assert_eq!(children.len(), 1);
        let text = world.get::<Text>(children[0]).unwrap();
        assert_eq!(text.0, "Play");
    }
}
