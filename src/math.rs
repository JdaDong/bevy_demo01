//! 纯函数数学工具：全部无 ECS 依赖，便于单测。
//!
//! 覆盖 API：`bevy::math::bounding::{BoundingCircle, Aabb2d, IntersectsVolume, BoundingVolume}`、
//! `Vec2` / `Dir2` / `Rot2`、`ops::*`、`EasingCurve` + `EaseFunction` + `Curve::sample_clamped`、
//! `StableInterpolate`。

use bevy::math::bounding::{Aabb2d, BoundingCircle, BoundingVolume, IntersectsVolume};
use bevy::prelude::*;

/// 逻辑竞技场尺寸（世界单位）。相机用 `ScalingMode::AutoMin` 保证它总能完整显示。
pub const ARENA_SIZE: Vec2 = Vec2::new(1280.0, 720.0);
/// 竞技场半尺寸。
pub const ARENA_HALF: Vec2 = Vec2::new(640.0, 360.0);

/// 把位置夹在竞技场内（留 `margin` 边距）。
pub fn clamp_to_arena(pos: Vec2, half: Vec2, margin: f32) -> Vec2 {
    let limit = (half - Vec2::splat(margin)).max(Vec2::ZERO);
    pos.clamp(-limit, limit)
}

/// 越界卷绕（陨石从左边出去从右边回来）。`pad` 是对象半径，避免“瞬移可见”。
pub fn wrap_position(pos: Vec2, half: Vec2, pad: f32) -> Vec2 {
    let lim = half + Vec2::splat(pad);
    let mut p = pos;
    if p.x > lim.x {
        p.x = -lim.x;
    } else if p.x < -lim.x {
        p.x = lim.x;
    }
    if p.y > lim.y {
        p.y = -lim.y;
    } else if p.y < -lim.y {
        p.y = lim.y;
    }
    p
}

/// 是否已经远离竞技场（子弹/粒子回收判定）。
pub fn is_outside_arena(pos: Vec2, half: Vec2, pad: f32) -> bool {
    pos.x.abs() > half.x + pad || pos.y.abs() > half.y + pad
}

/// 圆-圆碰撞，走 Bevy 的包围体 API。
pub fn circles_overlap(a: Vec2, ra: f32, b: Vec2, rb: f32) -> bool {
    BoundingCircle::new(a, ra).intersects(&BoundingCircle::new(b, rb))
}

/// 圆-矩形碰撞（竞技场内障碍物 / UI 命中测试可复用）。
pub fn circle_hits_rect(center: Vec2, radius: f32, rect_center: Vec2, rect_half: Vec2) -> bool {
    BoundingCircle::new(center, radius).intersects(&Aabb2d::new(rect_center, rect_half))
}

/// 两个圆的“合并包围圆”，用来给一组敌人画调试边界。
pub fn merge_circles(a: Vec2, ra: f32, b: Vec2, rb: f32) -> (Vec2, f32) {
    let merged = BoundingCircle::new(a, ra).merge(&BoundingCircle::new(b, rb));
    (merged.center(), merged.radius())
}

/// 转向行为（Reynolds steering）：朝 target 转，速度上限 `max_speed`，
/// `turn_rate` 越大转得越快。返回新速度。
pub fn steer_towards(velocity: Vec2, position: Vec2, target: Vec2, max_speed: f32, turn_rate: f32, dt: f32) -> Vec2 {
    let to_target = target - position;
    if to_target.length_squared() < f32::EPSILON {
        return velocity.clamp_length_max(max_speed);
    }
    let desired = to_target.normalize() * max_speed;
    let steer = (desired - velocity) * (turn_rate * dt).clamp(0.0, 1.0);
    (velocity + steer).clamp_length_max(max_speed)
}

/// 瞄准方向：from→to 为零向量时退回 fallback。
pub fn aim_direction(from: Vec2, to: Vec2, fallback: Dir2) -> Dir2 {
    Dir2::new(to - from).unwrap_or(fallback)
}

/// 方向 → 绕 Z 轴旋转（精灵默认朝上 +Y，所以减 90°）。
pub fn rotation_for_direction(dir: Dir2) -> Quat {
    let angle = dir.to_angle() - std::f32::consts::FRAC_PI_2;
    Quat::from_rotation_z(angle)
}

/// 带加速度与阻尼的速度积分（帧率无关）。
pub fn integrate_velocity(velocity: Vec2, input: Vec2, accel: f32, drag: f32, max_speed: f32, dt: f32) -> Vec2 {
    let input = input.clamp_length_max(1.0);
    let mut v = velocity + input * accel * dt;
    // 指数阻尼：v *= e^(-drag*dt)，帧率无关
    v *= ops::exp(-drag * dt);
    v.clamp_length_max(max_speed)
}

/// 连击倍率：每 5 连击 +1，最多 5 倍。
pub fn combo_multiplier(combo: u32) -> u32 {
    (1 + combo / 5).min(5)
}

/// 第 wave 波（从 1 开始）的刷怪间隔：指数衰减，但不低于 `floor`。
pub fn spawn_interval_for_wave(base: f32, wave: u32, floor: f32) -> f32 {
    let w = wave.saturating_sub(1) as f32;
    (base * ops::powf(0.85, w)).max(floor)
}

/// 第 wave 波敌人上限。
pub fn max_enemies_for_wave(base: u32, wave: u32) -> u32 {
    base + wave.saturating_sub(1) * 2
}

/// 第 wave 波各类型敌人的权重 (meteor, hunter, carrier)。
pub fn enemy_weights_for_wave(wave: u32) -> [u32; 3] {
    match wave {
        0 | 1 => [10, 0, 0],
        2 => [7, 3, 0],
        3 => [6, 3, 1],
        _ => [5, 4, 2],
    }
}

/// 按权重抽样：`roll` ∈ [0, sum)。
pub fn weighted_index(weights: &[u32], roll: u32) -> usize {
    let mut acc = 0;
    for (i, w) in weights.iter().enumerate() {
        acc += *w;
        if roll < acc {
            return i;
        }
    }
    weights.len().saturating_sub(1)
}

/// 屏幕震动强度：trauma^exponent（GDC "Juicing your cameras" 做法）。
pub fn shake_amount(trauma: f32, exponent: f32) -> f32 {
    ops::powf(trauma.clamp(0.0, 1.0), exponent)
}

/// trauma 线性衰减。
pub fn decay_trauma(trauma: f32, per_second: f32, dt: f32) -> f32 {
    (trauma - per_second * dt).clamp(0.0, 1.0)
}

/// 1D value-noise（平滑、连续，[-1,1]），给相机震动用。
pub fn noise1d(x: f32) -> f32 {
    fn hash(i: i32) -> f32 {
        let mut n = i.wrapping_mul(374_761_393) as u32;
        n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
        n ^= n >> 16;
        (n as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
    let i = x.floor();
    let f = x - i;
    let t = f * f * (3.0 - 2.0 * f); // smoothstep
    let a = hash(i as i32);
    let b = hash(i as i32 + 1);
    a.interpolate_stable(&b, t)
}

/// 用 Bevy 的 `EasingCurve` 计算缓动值，t 会被 clamp 到 [0,1]。
pub fn ease(from: f32, to: f32, function: EaseFunction, t: f32) -> f32 {
    EasingCurve::new(from, to, function).sample_clamped(t)
}

/// 生命周期比例 → alpha（淡出）。
pub fn fade_alpha(elapsed_fraction: f32) -> f32 {
    ease(1.0, 0.0, EaseFunction::QuadraticIn, elapsed_fraction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    const EPS: f32 = 1e-4;

    #[test]
    fn clamp_keeps_inside() {
        let p = clamp_to_arena(Vec2::new(10_000.0, -10_000.0), ARENA_HALF, 20.0);
        assert_eq!(p, Vec2::new(620.0, -340.0));
        let inside = Vec2::new(1.0, 2.0);
        assert_eq!(clamp_to_arena(inside, ARENA_HALF, 20.0), inside);
    }

    #[test]
    fn clamp_with_huge_margin_collapses_to_origin() {
        assert_eq!(clamp_to_arena(Vec2::new(5.0, 5.0), Vec2::splat(10.0), 50.0), Vec2::ZERO);
    }

    #[test]
    fn wrap_crosses_both_axes() {
        let half = Vec2::new(100.0, 50.0);
        assert_eq!(wrap_position(Vec2::new(120.0, 0.0), half, 10.0), Vec2::new(-110.0, 0.0));
        assert_eq!(wrap_position(Vec2::new(-120.0, 0.0), half, 10.0), Vec2::new(110.0, 0.0));
        assert_eq!(wrap_position(Vec2::new(0.0, 61.0), half, 10.0), Vec2::new(0.0, -60.0));
        assert_eq!(wrap_position(Vec2::new(0.0, -61.0), half, 10.0), Vec2::new(0.0, 60.0));
        assert_eq!(wrap_position(Vec2::new(3.0, 4.0), half, 10.0), Vec2::new(3.0, 4.0));
    }

    #[test]
    fn outside_arena_detection() {
        assert!(!is_outside_arena(Vec2::ZERO, ARENA_HALF, 0.0));
        assert!(is_outside_arena(Vec2::new(700.0, 0.0), ARENA_HALF, 50.0));
        assert!(!is_outside_arena(Vec2::new(680.0, 0.0), ARENA_HALF, 50.0));
    }

    #[test]
    fn circle_collisions() {
        assert!(circles_overlap(Vec2::ZERO, 5.0, Vec2::new(9.0, 0.0), 5.0));
        assert!(!circles_overlap(Vec2::ZERO, 5.0, Vec2::new(11.0, 0.0), 5.0));
        assert!(circle_hits_rect(Vec2::new(12.0, 0.0), 3.0, Vec2::ZERO, Vec2::splat(10.0)));
        assert!(!circle_hits_rect(Vec2::new(14.0, 0.0), 3.0, Vec2::ZERO, Vec2::splat(10.0)));
    }

    #[test]
    fn merge_contains_both() {
        let (c, r) = merge_circles(Vec2::new(-10.0, 0.0), 2.0, Vec2::new(10.0, 0.0), 2.0);
        assert!(c.length() < EPS);
        assert!((r - 12.0).abs() < EPS);
    }

    #[test]
    fn steering_converges_and_respects_speed_cap() {
        let mut v = Vec2::new(0.0, -100.0);
        let mut p = Vec2::ZERO;
        let target = Vec2::new(0.0, 1000.0);
        for _ in 0..200 {
            v = steer_towards(v, p, target, 150.0, 4.0, 1.0 / 60.0);
            p += v / 60.0;
            assert!(v.length() <= 150.0 + EPS);
        }
        assert!(v.y > 140.0, "最终应该朝目标全速前进: {v:?}");
    }

    #[test]
    fn steering_at_target_does_not_nan() {
        let v = steer_towards(Vec2::new(500.0, 0.0), Vec2::ONE, Vec2::ONE, 100.0, 5.0, 0.016);
        assert!(v.is_finite());
        assert!(v.length() <= 100.0 + EPS);
    }

    #[test]
    fn aim_direction_fallback() {
        assert_eq!(aim_direction(Vec2::ONE, Vec2::ONE, Dir2::Y), Dir2::Y);
        let d = aim_direction(Vec2::ZERO, Vec2::new(3.0, 0.0), Dir2::Y);
        assert!((d.as_vec2() - Vec2::X).length() < EPS);
    }

    #[test]
    fn rotation_points_sprite_up_along_direction() {
        // 朝 +Y 时不旋转
        let q = rotation_for_direction(Dir2::Y);
        assert!(q.angle_between(Quat::IDENTITY) < EPS);
        // 朝 +X 时应顺时针 90°
        let q = rotation_for_direction(Dir2::X);
        let (_, angle) = q.to_axis_angle();
        assert!((angle - FRAC_PI_2).abs() < EPS);
        assert!(((q * Vec3::Y).truncate() - Vec2::X).length() < EPS);
    }

    #[test]
    fn velocity_integration_caps_and_decays() {
        let mut v = Vec2::ZERO;
        for _ in 0..600 {
            v = integrate_velocity(v, Vec2::new(5.0, 0.0), 2000.0, 3.0, 400.0, 1.0 / 60.0);
        }
        assert!((v.length() - 400.0).abs() < 1.0, "{v:?}");
        for _ in 0..600 {
            v = integrate_velocity(v, Vec2::ZERO, 2000.0, 3.0, 400.0, 1.0 / 60.0);
        }
        assert!(v.length() < 1.0, "无输入时应阻尼到静止: {v:?}");
    }

    #[test]
    fn combo_multiplier_steps_and_caps() {
        assert_eq!(combo_multiplier(0), 1);
        assert_eq!(combo_multiplier(4), 1);
        assert_eq!(combo_multiplier(5), 2);
        assert_eq!(combo_multiplier(19), 4);
        assert_eq!(combo_multiplier(1000), 5);
    }

    #[test]
    fn spawn_interval_monotonic_with_floor() {
        let mut last = f32::MAX;
        for wave in 1..30 {
            let i = spawn_interval_for_wave(2.0, wave, 0.35);
            assert!(i <= last);
            assert!(i >= 0.35);
            last = i;
        }
        assert_eq!(spawn_interval_for_wave(2.0, 1, 0.1), 2.0);
        assert_eq!(spawn_interval_for_wave(2.0, 0, 0.1), 2.0, "wave=0 视同第一波");
    }

    #[test]
    fn enemy_cap_grows() {
        assert_eq!(max_enemies_for_wave(6, 1), 6);
        assert_eq!(max_enemies_for_wave(6, 4), 12);
    }

    #[test]
    fn weights_unlock_enemy_types() {
        assert_eq!(enemy_weights_for_wave(1)[1], 0);
        assert!(enemy_weights_for_wave(2)[1] > 0);
        assert!(enemy_weights_for_wave(3)[2] > 0);
        assert_eq!(enemy_weights_for_wave(99), enemy_weights_for_wave(4));
    }

    #[test]
    fn weighted_index_boundaries() {
        let w = [2, 0, 3];
        assert_eq!(weighted_index(&w, 0), 0);
        assert_eq!(weighted_index(&w, 1), 0);
        assert_eq!(weighted_index(&w, 2), 2, "权重为 0 的桶永远不会被选中");
        assert_eq!(weighted_index(&w, 4), 2);
        assert_eq!(weighted_index(&w, 100), 2, "越界回退到最后一个");
        assert_eq!(weighted_index(&[], 0), 0);
    }

    #[test]
    fn trauma_math() {
        assert_eq!(shake_amount(0.5, 2.0), 0.25);
        assert_eq!(shake_amount(2.0, 2.0), 1.0);
        assert_eq!(decay_trauma(0.5, 1.0, 0.25), 0.25);
        assert_eq!(decay_trauma(0.1, 1.0, 1.0), 0.0);
    }

    #[test]
    fn noise_is_bounded_and_continuous() {
        let mut prev = noise1d(0.0);
        for i in 1..2000 {
            let x = i as f32 * 0.01;
            let n = noise1d(x);
            assert!((-1.0..=1.0).contains(&n));
            assert!((n - prev).abs() < 0.1, "noise 应连续: x={x}");
            prev = n;
        }
        // 整数点上等于 hash 值，非全零
        assert!((0..10).any(|i| noise1d(i as f32).abs() > 0.01));
    }

    #[test]
    fn easing_endpoints_and_clamp() {
        assert!((ease(0.0, 10.0, EaseFunction::CubicInOut, 0.0)).abs() < EPS);
        assert!((ease(0.0, 10.0, EaseFunction::CubicInOut, 1.0) - 10.0).abs() < EPS);
        assert!((ease(0.0, 10.0, EaseFunction::CubicInOut, 0.5) - 5.0).abs() < EPS);
        assert!((ease(0.0, 10.0, EaseFunction::Linear, 5.0) - 10.0).abs() < EPS, "t>1 clamp");
        assert!((fade_alpha(0.0) - 1.0).abs() < EPS);
        assert!(fade_alpha(1.0).abs() < EPS);
        assert!(fade_alpha(0.5) > 0.5, "QuadraticIn 前半段下降慢");
    }
}
