//! 通信层：Message（缓冲、拉取式）与 Event（观察者、推送式）。
//!
//! | 机制 | 类型 | 触发 | 消费 | 典型用途 |
//! |---|---|---|---|---|
//! | `Message` | [`StarCollected`] 等 | `MessageWriter::write` | `MessageReader::read`（下一个读它的系统） | 计分、音效、HUD |
//! | `Event` | [`Explosion`] | `commands.trigger` | `On<Explosion>` 全局观察者（立即执行） | 特效、震屏 |
//! | `EntityEvent` | [`Damage`] | `commands.trigger` | 实体观察者 + 沿 `ChildOf` 冒泡 | 装甲 → 母舰伤害传导 |
//!
//! 覆盖 API：`#[derive(Message)]`、`#[derive(Event)]`、`#[derive(EntityEvent)]`、
//! `#[entity_event(propagate, auto_propagate)]`、`add_message`。

use bevy::prelude::*;

use crate::components::{EnemyKind, PowerUpKind};

// ---------------- Messages ----------------

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct StarCollected {
    pub value: u32,
    pub position: Vec2,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct EnemyDestroyed {
    pub kind: EnemyKind,
    pub position: Vec2,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerDamaged {
    pub remaining: i32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerUpCollected(pub PowerUpKind);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaveStarted(pub u32);

/// 音效请求。逻辑层只“说要播什么”，由表现层的音频插件决定怎么播。
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaySfx {
    Shoot,
    Missile,
    Explosion,
    Collect,
    Hurt,
    PowerUp,
    UiClick,
}

/// 请求生成飘字。
#[derive(Message, Debug, Clone, PartialEq)]
pub struct SpawnPopup {
    pub position: Vec2,
    pub text: String,
    pub color: Color,
}

// ---------------- Events (observers) ----------------

/// 全局爆炸事件：观察者负责生成粒子、加震屏、播音效。
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct Explosion {
    pub position: Vec2,
    pub radius: f32,
    pub color: Color,
}

/// 实体伤害事件：命中子实体（装甲片）后沿 `ChildOf` 自动向父实体冒泡。
/// 装甲观察者可以削减 `amount` 或 `propagate(false)` 截断。
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq)]
#[entity_event(propagate, auto_propagate)]
pub struct Damage {
    pub entity: Entity,
    pub amount: i32,
}

/// 玩家死亡（全局事件）。
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct PlayerDied {
    pub position: Vec2,
}

pub struct EventsPlugin;

impl Plugin for EventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<StarCollected>()
            .add_message::<EnemyDestroyed>()
            .add_message::<PlayerDamaged>()
            .add_message::<PowerUpCollected>()
            .add_message::<WaveStarted>()
            .add_message::<PlaySfx>()
            .add_message::<SpawnPopup>();
    }
}
