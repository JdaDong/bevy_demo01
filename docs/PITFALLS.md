# Bevy 0.19 迁移要点 & 本项目踩坑记录

> 网上大量教程停留在 0.12~0.15，直接照抄在 0.19 上多半编译不过。下面每条都在本项目里实际遇到并验证过。

## A. API 变更速查（≤0.15 → 0.19）

| 旧写法 | 0.19 写法 | 备注 |
|---|---|---|
| `#[derive(Event)]` + `EventWriter/EventReader` + `add_event` | `#[derive(Message)]` + `MessageWriter/MessageReader` + `add_message`，`.write()` | 缓冲式“事件”改名 Message |
| `Trigger<E>` | `On<E>`；`event.entity` / `event_target()` | Event 专指观察者事件 |
| `#[derive(Event)]` 指定实体 | `#[derive(EntityEvent)]`（字段 `entity: Entity`） | 冒泡：`#[entity_event(propagate, auto_propagate)]` |
| `StateScoped(S)` | `DespawnOnExit(S)` / `DespawnOnEnter(S)`；还有 `DisableOnExit` 等 | scoped entities 默认开启 |
| `font_size: 33.0` | `font_size: FontSize::Px(33.0)` 或 `TextFont::from_font_size(33.0)` | 新增 Vw/Vh/Rem 单位 |
| 组件钩子 `on_replace` | `on_discard` | lifecycle 事件 `Replace` → `Discard` |
| `Val::Px(10.0)` | 仍可用，推荐 `px(10)` / `percent(100)` | |
| `BorderRadius` 组件 | `Node { border_radius: BorderRadius::all(px(8)), .. }` | 并入 Node |
| `BorderColor(color)` | `BorderColor::all(color)` 或四边字段 | |
| `Camera { hdr: true }` | 改为独立的 `Hdr` 标记组件；`Bloom` 带 `#[require(Hdr)]`，插 `Bloom` 即自动开启 HDR | 颜色分量 >1 才会泛光 |
| `EntityCommands::despawn_recursive` | `despawn()` 默认递归 | |
| `Query::single()` panic | 返回 `Result`；系统参数用 `Single` / `Option<Single>` / `Populated` | |
| `timer.finished()` | `timer.is_finished()` | |
| `Children` 直接 `iter()` 得 `&Entity` | 得 `Entity`（Copy） | |
| 自定义 `AssetLoader` | 还需 `#[derive(TypePath)]` | |
| 手写计时器做“N 秒后删除” | `commands.delayed().secs(2.0).entity(e).despawn()` | 0.19 新增 `DelayedCommandsExt` |
| `cond_a.and(cond_b)` | `and_then` / `and_eager` | `and` 已 deprecated |
| 3D 默认 `Tonemapping::TonyMcMapface` 随处可用 | 需要 `tonemapping_luts` 特性（属 3D 集合） | 2D 精简构建请换 `AcesFitted` |

## B. 本项目踩过的坑

### 1. 新生成实体的 `GlobalTransform` 在原点 → “幽灵碰撞” ⭐
**现象**：刷在屏幕边缘的猎手出生第一帧就“撞死”在玩家身上；子弹 1 发打掉 2 血。
**原因**：`GlobalTransform` 只在 `PostUpdate` 传播；本帧 spawn / 本帧改过 `Transform` 的实体，在 `Update`
碰撞检测时读到的是默认值（原点）或上一帧的值。
**修复**：`schedule.rs` 在 `Collide` 前插入 `GameSet::SyncTransforms`，手动运行官方
`mark_dirty_trees → propagate_parent_transforms → sync_simple_transforms`。子实体（母舰装甲）碰撞必须读
`GlobalTransform`，所以不能简单改读 `Transform`。

### 2. `MinimalPlugins` 不含 `TransformPlugin`
无头测试里若不手动加，`GlobalTransform` 永远是单位矩阵，所有实体“叠在原点”——测试会**误通过**。

### 3. 无头运行 UI 逻辑缺 `TextIterScratch`
`TextUiWriter` 需要 `ResMut<TextIterScratch>`（由 `TextPlugin` 插入）。缺失时报
`Parameter ...::scratch failed validation: Resource does not exist` 并 panic。`GameUiPlugin` 里 `init_resource` 解决。
（错误信息里的系统名显示为 `<Enable the debug feature to see the name>`，打开 `bevy/debug` 特性可看到真名。）

### 4. 多线程执行器导致不可复现
两个系统都 `ResMut<GameRng>` → 互斥但**顺序不定**，同 seed 结果会漂。用 `.after()` 显式排序；
同理，读写同一组件的 `steer_missiles` 必须 `.before(integrate_motion)`。

### 5. 设备输入覆盖脚本输入
`gather_intent` 每帧重写 `PlayerIntent`，测试写进去的值被清空。引入 `IntentSource::{Devices, Scripted}` +
`run_if(resource_equals(IntentSource::Devices))`。

### 6. 直接运行二进制找不到 assets
Bevy 资源根目录：`BEVY_ASSET_ROOT` > `CARGO_MANIFEST_DIR` > 可执行文件目录。`cargo run` 没问题，
直接跑 `./target/debug/bevy_demo01` 会去 `target/debug/assets` 找。`main.rs::asset_root()` 增加“当前目录下有 `assets/` 就用它”的回退。

### 7. `Disabled` 实体对所有查询隐身
对象池里的子弹默认查不到；统计池大小要 `Allow<Disabled>`，取可复用子弹要 `With<Disabled>`。
好消息：`DespawnOnExit` 内部带 `Allow<Disabled>`，离开状态时池会被正确清理。

### 8. 同帧消息顺序
`resolve_deaths`（写 `EnemyDestroyed`）与 `apply_score`（读）同在 `Resolve` 集合但无序，可能晚一帧计分。
测试断言前多跑 1~2 帧；需要严格同帧时用 `.chain()`。

### 9. 观察者里别直接 despawn 目标
冒泡链上后续观察者仍可能访问该实体；本项目用 `Dying` 标记 + `resolve_deaths` 统一销毁。

### 10. macOS 截图全黑
窗口被遮挡 / 锁屏 / 显示器休眠时合成器不绘制。`smoke_window.sh` 用亮度校验 + 重试；autoplay 模式下还设置
`WinitSettings { unfocused_mode: Continuous }` 防止失焦降频。

### 11. 编译时间
首次 `cargo build` ~30 分钟（M 系列，依赖 opt-level=3）。优化手段：
- 只开需要的特性：`default-features = false, features = ["2d","ui","audio","wav","bevy_dev_tools"]`
- `[profile.dev.package."*"] opt-level = 3`，自己代码 `opt-level = 1` → 增量 20~30s
- 可选：`bevy/dynamic_linking` 特性进一步加速增量链接（发布时务必关闭）
