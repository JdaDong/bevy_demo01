# Bevy 0.19 API 覆盖索引

> 格式：**API** → 代码位置 → 验证它的测试（`u:` 单元测试，`it:` 集成测试文件::用例）。
> 标 `👁` 的为纯表现层，靠 `scripts/smoke_window.sh` 截图人工/亮度校验。

## App / Plugin / 调度

| API | 位置 | 测试 |
|---|---|---|
| `App::new` / `add_plugins` / `Plugin` trait / 插件元组 | `lib.rs` 全部插件 | it: 所有 |
| `PluginGroup::set`（定制 Window/Image/Asset/Log） | `main.rs` | 👁 |
| `MinimalPlugins` + `StatesPlugin` + `TransformPlugin` 无头组合 | `lib.rs::headless_app` | it: 所有 |
| `App::set_error_handler(warn)` | `main.rs` | 👁 |
| `App::run -> AppExit`、`MessageWriter<AppExit>`、`App::should_exit` | `main.rs`、`ui/screens.rs::button_actions`、`autoplay.rs` | it: `state_flow::quit_button_writes_app_exit` |
| `Startup` / `PreStartup` / `PreUpdate` / `Update` / `PostUpdate` / `FixedUpdate` | 各插件 | it: 所有 |
| `#[derive(SystemSet)]` + `configure_sets` + `.chain()` | `schedule.rs` | it: `smoke`、`combat` |
| `.before()` / `.after()` 显式排序 | `collectibles.rs`、`player.rs`、`ui/mod.rs` | it: `ecs_features::simulation_is_deterministic` |
| 手动调用官方系统（`mark_dirty_trees` 等）进自定义集合 | `schedule.rs::SyncTransforms` | it: `combat::hunters_target_player` 等（修复前会失败） |
| `TransformSystems::Propagate` 排序约束 | `camera.rs` | 👁 |

## 运行条件（Run Conditions）

| API | 位置 | 测试 |
|---|---|---|
| `in_state` | 全局 | it: `state_flow::*` |
| `resource_changed` / `resource_exists` / `resource_equals` | hud、audio、config、input、collectibles | it: `ecs_features::fixed_timestep_follows_config`、`smoke` |
| `input_just_pressed` | `debug.rs` | it: `combat::nuke_via_keyboard_run_condition` |
| `on_timer`（等价的 Timer 驱动） / `Timer` Repeating & Once | enemy、score、player | u: `score::combo_*`；it: `ecs_features::wave_*` |
| 条件组合 `.and_then()` | `ui/mod.rs`、`autoplay.rs` | it: `smoke` |

## ECS 核心

| API | 位置 | 测试 |
|---|---|---|
| `#[derive(Component)]`、`Resource`、`Bundle` | `components.rs` | u: `components::bundle_spawns_all_fields` |
| `#[require(A, B = expr)]` 必需组件 | `Player`/`Enemy`/`Projectile`… | u: `required_components_are_inserted`、`explicit_component_overrides_required_default` |
| `#[component(storage = "SparseSet")]` | `Invulnerable`、`RapidFire`、`Dying` | it: `combat::contact_damage_invulnerability_and_messages` |
| `#[component(immutable)]` | `SpawnedInWave` | u: `immutable_component_can_be_replaced_not_mutated` |
| 组件钩子 `on_add` / `on_remove` + `DeferredWorld` + `HookContext` | `components.rs::Enemy` → `EnemyCensus` | u: `enemy_hooks_maintain_census`；it: `combat::nuke_*` |
| 自定义关系 `#[relationship]` / `#[relationship_target]` | `Targeting` ↔ `TargetedBy` | u: `relationship_*`；it: `combat::missile_targets_*`、`hunters_target_player` |
| 层级 `children![]`、`with_children`、`ChildOf`、`Children`、递归 despawn | player 护盾、母舰装甲、UI | it: `combat::carrier_death_despawns_children_and_counts_kill` |
| `SpawnIter` + `Children::spawn` | `ui/screens.rs` 高分榜 | it: `state_flow::menu_*` |
| `Query` 过滤：`With`/`Without`/`Or`/`Changed`/`Added`/`Has`/`Option` | 全局 | it: 多处 |
| `Single` / `Option<Single>` / `Populated` | player、enemy、hud | it: `combat::hunters_target_player` |
| `ParamSet` | `collectibles.rs::magnet_stars` | it: `ecs_features::magnet_pulls_nearby_star` |
| `Local<T>` | score、collectibles、audio、debug | it: `smoke` |
| `#[derive(SystemParam)]` | `input.rs::CursorWorld` | it: `ecs_features::gamepad_left_stick_drives_intent` |
| `#[derive(QueryData)]` + `#[query_data(mutable)]` | `player.rs::PlayerShip` | it: `combat::bullet_pool_*` |
| `iter_combinations_mut` | `enemy.rs::meteor_bounce` | it: `ecs_features::simulation_is_deterministic` |
| `par_iter_mut` | effects 粒子、visuals 星空 | it: `smoke` |
| `RemovedComponents<T>` | `player.rs::restore_visibility` | it: `combat::contact_*` |
| 实体禁用 `Disabled` + `With<Disabled>` + `Allow<Disabled>` | 子弹对象池 | u: `combat::pooled_bullet_counter_sees_disabled`；it: `combat::bullet_pool_reuses_disabled_entities`、`state_flow::back_to_menu_*` |
| 独占系统 `fn(&mut World)` | `save.rs::snapshot_player`、`debug.rs::register_cheats` | it: `ecs_features::reflect_snapshot_*` |
| 可失败系统 `-> Result` + `.pipe()` + `In<T>` | `save.rs::write_snapshot.pipe(report_snapshot)` | 👁（F5） |
| 一次性系统 `register_system` + `SystemId` + `run_system` | `debug.rs::Cheats` | it: `combat::nuke_one_shot_system_kills_everything` |
| `run_system_cached` | `debug.rs::run_spawn_carrier` | it: `combat::cached_one_shot_spawns_carrier` |
| `RunSystemOnce`（测试里直接跑系统） | tests | it: `combat::*`、`ecs_features::*` |
| `World::trigger` / `World::run_system` | tests | it: `combat::armor_*` |
| `try_despawn` / `get_entity` | combat、hud | it: `state_flow::back_to_menu_*` |
| 延迟命令 `commands.delayed().secs(..)`（0.19 新增） | `ui/hud.rs::show_wave_banner` | it: `ecs_features::wave_advances_and_banner_despawns_via_delayed_commands` |

## Message（缓冲） / Event（观察者）

| API | 位置 | 测试 |
|---|---|---|
| `#[derive(Message)]` + `add_message` + `MessageWriter` / `MessageReader` | `events.rs`（7 种消息） | it: `combat::*`、`ecs_features::star_pickup_*` |
| `Messages<T>` + `get_cursor` 断言 | tests/common | it: 多处 |
| `#[derive(Event)]` + `add_observer` + `On<E>` | `Explosion`、`PlayerDied` | it: `state_flow::player_death_*` |
| `#[derive(EntityEvent)]` + `#[entity_event(propagate, auto_propagate)]` + `On::propagate(false)` | `Damage` + 装甲 | it: `combat::armor_plate_absorbs_and_propagates_damage_to_carrier` |
| 实体级观察者 `EntityCommands::observe` | 装甲、拾取高亮、截图 | it: `combat::armor_*` |
| 生命周期事件 `On<Add, T>` 等价 → 组件钩子 | components | u: hooks |

## 状态

| API | 位置 | 测试 |
|---|---|---|
| `States` + `init_state` + `NextState` + `State` | `state.rs` | it: `state_flow::*` |
| `SubStates` + `#[source]` + `add_sub_state` | `GamePhase` | it: `state_flow::substate_does_not_exist_outside_game` |
| `ComputedStates`（单源 / 多源元组） + `add_computed_state` | `Simulating`、`ShowHud` | u: `state::*`；it: `state_flow::pause_substate_computed_states_and_virtual_time` |
| `OnEnter` / `OnExit` | 全局 | it: 所有 |
| `DespawnOnExit` 状态作用域实体 | 全局 | it: `state_flow::back_to_menu_*` |
| `bevy::dev_tools::states::log_transitions` | `debug.rs` | 👁（日志） |

## 时间

| API | 位置 | 测试 |
|---|---|---|
| `Time` / `Time<Virtual>` pause/unpause/relative_speed | state、debug | it: `ecs_features::time_scale_keys_*`、`virtual_time_pauses_but_real_time_continues` |
| `Time<Real>` | visuals、autoplay | it: `ecs_features::virtual_time_*` |
| `Time<Fixed>::set_timestep_hz` + `FixedUpdate` | config、PhysicsSet | it: `ecs_features::fixed_timestep_follows_config` |
| `TimeUpdateStrategy::ManualDuration`（测试确定性时钟） | `lib.rs::headless_app` | it: 所有 |

## 资源（Asset）

| API | 位置 | 测试 |
|---|---|---|
| `#[derive(Asset, TypePath)]` + 自定义 `AssetLoader`（async + `Reader`） | `config.rs::GameConfigLoader` | it: `assets_config::custom_loader_loads_config_and_syncs_resource` |
| `init_asset` / `init_asset_loader` / 多段扩展名 `cfg.ron` | `config.rs` | it: 同上 |
| `AssetServer::load` / `load_state` / `is_loaded_with_dependencies` / `get_path` | assets、config | it: `assets_config::*` |
| `LoadState::Failed` 容错 | `assets.rs::track_loading` | it: `assets_config::invalid_config_fails_to_load` |
| `AssetEvent::{LoadedWithDependencies, Modified}` 热重载 | `config.rs::sync_config_from_asset` | it: 同上；`--features hot` 👁 |
| `Assets<T>::add`（Image/Mesh/ColorMaterial/TextureAtlasLayout/Pitch/AnimationClip/AnimationGraph） | assets、audio、visuals | u: `assets::*` |
| `Image::new_fill` + `set_color_at` + `get_color_at` + `RenderAssetUsages` | `assets.rs::paint` | u: `generated_images_have_expected_size_and_transparent_corners` |
| `TextureAtlasLayout::from_grid` + `TextureAtlas` | assets、player | u: `atlas_layout_has_all_frames` |

## 2D 渲染 / 相机 / 后处理 👁

| API | 位置 |
|---|---|
| `Sprite`（`from_image`/`from_color`/`from_atlas_image`/`custom_size`/`color` HDR） | player、enemy、effects、visuals |
| `Mesh2d` + `MeshMaterial2d<ColorMaterial>` + 原语 `Capsule2d`/`RegularPolygon`/`Annulus` | player 子弹/导弹/护盾 |
| `Text2d` + `TextFont` + `TextColor` + `Text2dShadow` + `Anchor` | effects 飘字、visuals 标题 |
| `Camera2d` + `Camera{clear_color}` + `ClearColor` | camera |
| `Projection::Orthographic` + `ScalingMode::AutoMin` | camera |
| `Bloom` + `Tonemapping::AcesFitted` + `DebandDither` | camera |
| `Visibility`（Inherited/Hidden 闪烁） | player 无敌 / 护盾 |
| `ImagePlugin::default_nearest` | main |

## 动画

| API | 位置 | 测试 |
|---|---|---|
| 精灵帧动画（`TextureAtlas::index` + Timer） | `effects.rs::animate_sprites` | u: `sprite_animation_wraps` |
| `AnimationClip` + `AnimatableCurve` + `animated_field!` | `visuals.rs::spawn_title` | 👁 |
| `EasingCurve` + `EaseFunction` + `reparametrize_linear` + `ping_pong` + `sample_clamped` | visuals、math | u: `math::easing_endpoints_and_clamp` |
| `AnimationGraph::from_clip` + `AnimationPlayer::play().repeat()` + `AnimationTargetId` + `AnimatedBy` | visuals | 👁 |

## UI

| API | 位置 | 测试 |
|---|---|---|
| `Node`（Flex、绝对定位、`px`/`percent`、`UiRect`、`row_gap`/`column_gap`） | ui/* | u: `widgets::button_template_spawns_expected_components` |
| `Button` + `Interaction` + `Changed<Interaction>` | ui/screens | it: `state_flow::menu_play_button_starts_game` 等 |
| `BackgroundColor` / `BorderColor::all` / `Node::border_radius` + `BorderRadius` | widgets | u: `each_interaction_has_distinct_colors` |
| `BoxShadow::new` / `Outline::new` | widgets | 👁 |
| `BackgroundGradient` + `LinearGradient::to_bottom` + `ColorStop` | widgets | 👁 |
| `Text` + `TextSpan` + `TextUiWriter`（按 span 下标写） | ui/hud | it: `ecs_features::hud_score_and_hearts_follow_state` |
| `TextShadow` / `TextLayout::justify(Justify::Center)` | widgets、screens | 👁 |
| `ImageNode` | hud 心形图标 | it: `ecs_features::hud_*` |
| `ZIndex` | 暂停层 | 👁 |

## 输入

| API | 位置 | 测试 |
|---|---|---|
| `ButtonInput<KeyCode>`：`pressed/any_pressed/just_pressed/any_just_pressed` | input | u: `input::*`；it: `ecs_features::keyboard_moves_player_and_arena_clamps` |
| `ButtonInput<MouseButton>` | input | it: `smoke` |
| `Gamepad`：`left_stick/right_stick/dpad/pressed/just_pressed` + `analog_mut/digital_mut` | input | it: `ecs_features::gamepad_left_stick_drives_intent` |
| `Window::cursor_position` + `Camera::viewport_to_world_2d` + `PrimaryWindow` | `input.rs::CursorWorld` | 👁 |

## 拾取（Picking）

| API | 位置 |
|---|---|
| `Pickable` + 精灵拾取后端（alpha 阈值） | enemy |
| `On<Pointer<Over>>` / `Pointer<Out>` / `Pointer<Click>` + `event_target()` | `enemy.rs` 高亮与点击锁定 → 导弹目标 👁 |

## 音频 👁

| API | 位置 | 测试 |
|---|---|---|
| `AudioPlayer` + `PlaybackSettings::{LOOP, DESPAWN}.with_volume/with_speed` | audio | u: `audio::every_sfx_has_audible_params` |
| `Pitch::new`（程序化正弦音效） | audio | 同上 |
| `AudioSink` + `AudioSinkPlayback::{pause, play, set_volume}` | audio 暂停时静音音乐 | 👁 |
| `GlobalVolume` + `Volume::Linear` | audio | u: `master_volume_steps_are_clean_and_clamped` |

## 窗口 / 平台 👁

| API | 位置 |
|---|---|
| `WindowPlugin{primary_window}` + `WindowResolution` + `PresentMode` | main |
| `Window::title` 动态更新 | `visuals.rs::update_window_title` |
| `CursorIcon::System(SystemCursorIcon::Crosshair)` | camera |
| `WinitSettings` + `UpdateMode::Continuous` | autoplay |
| `Screenshot::primary_window` + `save_to_disk` 观察者 | debug（F12 / autoplay） |

## Gizmos / 诊断 / Dev Tools 👁

| API | 位置 |
|---|---|
| `Gizmos<T>` + `#[derive(GizmoConfigGroup)]` + `init_gizmo_group` + `GizmoConfigStore::config_mut` | debug |
| `circle_2d` / `arrow_2d` / `line_2d` / `rect_2d` / `grid_2d().outer_edges()` / `GizmoLineStyle::Dashed` | debug |
| `Diagnostic` + `DiagnosticPath::const_new` + `register_diagnostic` + `Diagnostics::add_measurement` | debug |
| `EntityCountDiagnosticsPlugin` / `FpsOverlayPlugin` + `FpsOverlayConfig` + `FrameTimeGraphConfig` | lib / debug |

## 数学

| API | 位置 | 测试 |
|---|---|---|
| `BoundingCircle` / `Aabb2d` / `IntersectsVolume` / `BoundingVolume::merge` | math | u: `math::circle_collisions`、`merge_contains_both` |
| `Dir2` / `Rot2`-风格旋转 / `Quat::from_rotation_z` / `Vec2::from_angle` | math、player | u: `rotation_points_sprite_up_along_direction` |
| `ops::{exp, powf, sin, cos, atan2}`（跨平台确定性数学） | math、assets | u: math::* |
| `StableInterpolate::interpolate_stable` | `math.rs::noise1d` | u: `noise_is_bounded_and_continuous` |
| `Isometry2d` | debug gizmos | 👁 |

## 反射 / 序列化 / 异步任务

| API | 位置 | 测试 |
|---|---|---|
| `#[derive(Reflect)]` + `#[reflect(Component/Resource/Default)]` + `register_type` | 全局、`lib.rs` | it: `ecs_features::reflect_snapshot_*` |
| `AppTypeRegistry` + `ReflectComponent::reflect` 遍历实体组件 | `save.rs::snapshot_player` | 同上 |
| `ReflectSerializer` / `ReflectDeserializer` + `FromReflect` | save、tests | it: `ecs_features::reflect_serde_roundtrip_through_type_registry` |
| serde + ron（配置 / 排行榜） | config、score | u: `config::roundtrip_ron`、`score::highscores_ron_*` |
| `IoTaskPool::spawn` + `Task` + `futures::check_ready` | `save.rs` | it: `state_flow::player_death_goes_to_gameover_and_saves_highscore` |
