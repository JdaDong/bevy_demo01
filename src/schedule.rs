//! 调度编排：用 `SystemSet` 把每帧工作切成有序阶段。
//!
//! ```text
//! PreUpdate          : 相机震动复位、延迟命令检查
//! FixedUpdate        : PhysicsSet（积分速度、卷绕、弹开） ← run_if(in_state(Simulating))
//! Update             : Input → Simulate → SyncTransforms → Collide → Resolve → Feedback
//!                      (Input 在暂停时也运行；Simulate..Resolve 只在 Simulating 时运行)
//! PostUpdate         : 相机震动（before TransformSystems::Propagate）→ 变换传播 → 渲染提取
//! ```
//!
//! ### 为什么需要 `SyncTransforms`？
//! `GlobalTransform` 默认只在 `PostUpdate` 传播一次。本帧新 spawn 的实体（或从对象池复用、
//! 直接改了 `Transform` 的实体）在 `Update` 里读到的还是旧的/原点的 `GlobalTransform`，
//! 碰撞检测就会“幽灵命中”原点处的玩家。解决方法：在碰撞前**手动再跑一遍**官方的三个传播系统
//! （Bevy 自己在 `PostStartup` 里也是这么做的）。
//!
//! 覆盖 API：`#[derive(SystemSet)]`、`configure_sets`、`.chain()`、`run_if`、`in_state`、
//! `FixedUpdate`、`bevy::transform::systems::*`。

use bevy::prelude::*;
use bevy::transform::systems::{mark_dirty_trees, propagate_parent_transforms, sync_simple_transforms};

use crate::state::{AppState, Simulating};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    /// 读输入 → `PlayerIntent`
    Input,
    /// AI、射击、生成
    Simulate,
    /// 碰撞前同步 GlobalTransform
    SyncTransforms,
    /// 碰撞检测，只产出事件
    Collide,
    /// 结算：计分、死亡、状态切换
    Resolve,
    /// 反馈：动画、粒子、HUD
    Feedback,
}

/// 固定步长物理。
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysicsSet;

pub struct SchedulePlugin;

impl Plugin for SchedulePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                GameSet::Input.run_if(in_state(AppState::InGame)),
                (GameSet::Simulate, GameSet::SyncTransforms, GameSet::Collide, GameSet::Resolve)
                    .chain()
                    .run_if(in_state(Simulating)),
                GameSet::Feedback,
            )
                .chain(),
        )
        .configure_sets(FixedUpdate, PhysicsSet.run_if(in_state(Simulating)))
        .add_systems(
            Update,
            (mark_dirty_trees, propagate_parent_transforms, sync_simple_transforms)
                .chain()
                .in_set(GameSet::SyncTransforms),
        );
    }
}
