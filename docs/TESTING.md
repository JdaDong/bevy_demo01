# 测试指南

## 测试金字塔

| 层 | 数量 | 位置 | 跑什么 | 耗时 |
|---|---|---|---|---|
| 单元测试 | 92 | `src/**` 的 `#[cfg(test)]` | 纯函数（数学/RNG/计分/配置/UI 文本）+ 裸 `World`（钩子/关系/必需组件） | <0.1s |
| 集成测试 | 38 | `tests/*.rs` | 无头 `App` 跑真实插件，逐帧推进断言 | ~2s |
| 无头模拟 | – | `src/bin/headless_sim.rs` | AI 驾驶跑 N 帧，输出统计 JSON | 1 分钟游戏 ≈ 1.2s |
| 窗口冒烟 | – | `scripts/smoke_window.sh` | 真渲染 + 自动截图 + 亮度校验 | ~25s |

```bash
./scripts/test.sh                 # 全部
./scripts/test.sh combat          # 过滤
./scripts/test.sh --repeat 5      # 连跑 5 轮找不稳定用例
cargo test --test state_flow -- --nocapture
```

## 集成测试文件

| 文件 | 关注点 |
|---|---|
| `smoke.rs` | 启动链路；1 分钟脚本化对局不 panic（抓 B0001 查询冲突、缺资源等） |
| `state_flow.rs` | 菜单按钮、Enter、暂停子状态 + 计算状态 + 虚拟时间、DespawnOnExit（含 Disabled）、死亡结算 + 异步存档、重开、退出、音量 |
| `combat.rs` | 子弹击毁、装甲冒泡、母舰递归销毁、接触伤害 + 无敌帧、护盾、对象池复用、导弹关系、猎手追踪、一次性系统、`run_system_cached` |
| `ecs_features.rs` | 键盘/手柄、固定步长、波次 + 延迟命令、拾取、磁铁 `ParamSet`、buff 过期、HUD `TextUiWriter`、反射快照/序列化、时间缩放、**确定性回放** |
| `assets_config.rs` | 真 `AssetServer` + 自定义 `AssetLoader` 加载配置 → `AssetEvent` → 资源同步；非法配置 → `LoadState::Failed` |

## 无头测试的 8 个关键技巧

1. **`headless_app()`** = `MinimalPlugins + StatesPlugin + TransformPlugin + TimeUpdateStrategy::ManualDuration(1/60s)`。
   `MinimalPlugins` **不含** `TransformPlugin`，不加的话所有 `GlobalTransform` 永远在原点。
2. **手动时钟**：`ManualDuration` 让每次 `app.update()` 精确前进 1/60 秒，计时器/固定步长/延迟命令全部可预测。
3. **输入注入**：
   - 键盘：`ButtonInput<KeyCode>::press()`；没有 `InputPlugin` 时 `just_pressed` 不会自动清，用 `tap()`（按→update→`clear()`→update）。
   - 手柄：`world.spawn(Gamepad::default())` 后 `analog_mut().set(..)` / `digital_mut().press(..)`。
   - 高层意图：`insert_resource(IntentSource::Scripted)` 后直接写 `PlayerIntent`。
4. **点击 UI 按钮**：无头没有焦点系统，直接 `insert(Interaction::Pressed)` 即触发 `Changed<Interaction>`。
5. **状态切换要两帧**：`NextState::set` → 第 1 帧 `StateTransition` 生效并跑 `OnEnter` → 第 2 帧 `Update` 看到新世界（`common::goto`）。
6. **直接跑系统 / 触发事件**：`world.run_system_once(closure)`（生成敌人等）、`world.trigger(Damage{..})`（观察者立即执行）、`world.run_system(SystemId)`。
7. **读消息**：`Messages<T>` 双缓冲，`messages.get_cursor().read(messages)` 能看到最近两帧（`common::recent`）。
8. **缺失的渲染侧资源**：`TextUiWriter` 依赖 `TextIterScratch`（通常由 `TextPlugin` 插入）→ `GameUiPlugin` 里 `init_resource`，否则系统参数校验失败直接 panic。

## 确定性

`ecs_features::simulation_is_deterministic` 用同 seed + 同输入脚本跑两局 900 帧，断言分数、刷怪数、
所有敌人坐标完全一致。要守住它：

- 所有随机数走 `GameRng`（xorshift64*，可 seed），不用 `thread_rng`。
- **所有写 `GameRng` 的系统必须显式排序**：多线程执行器对“无序但冲突”的系统每次顺序可能不同。
- 读写同一实体同一组件的系统（`steer_missiles` vs `integrate_motion`）也要排序。
- 背景星空用**独立 seed**，不消耗游戏 RNG。

## 窗口冒烟

`scripts/smoke_window.sh`：`BEVY_DEMO_AUTOPLAY` 跳过菜单由 AI 驾驶 → `BEVY_DEMO_SCREENSHOT_AT` 截图 →
`BEVY_DEMO_EXIT_AFTER` 退出 → `gen_assets.py --luma` 纯 Python 解码 PNG 计算平均亮度（>0.5 视为有效）。
macOS 锁屏/休眠/窗口被遮挡时合成器不绘制，截图全黑，脚本会重试 3 次并提示。
