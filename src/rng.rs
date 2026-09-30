//! 可复现的随机数资源。
//!
//! Bevy 本身不内置 RNG（官方示例用 `rand` + `chacha20`）。这里用 xorshift64* 手写一个
//! 零依赖、可 seed、可序列化的 [`GameRng`]，保证 **同一个 seed → 同一局游戏**，
//! 这是写确定性单测/回放的前提。
//!
//! 覆盖 API：`Resource`、`Reflect`、`#[reflect(Resource)]`。

use bevy::prelude::*;

/// xorshift64* 伪随机数发生器。
#[derive(Resource, Reflect, Debug, Clone, PartialEq, Eq)]
#[reflect(Resource)]
pub struct GameRng {
    state: u64,
}

impl Default for GameRng {
    fn default() -> Self {
        Self::seeded(0x5EED_BEEF_CAFE_F00D)
    }
}

impl GameRng {
    /// seed 为 0 时 xorshift 会永远输出 0，这里强制替换成非零常量。
    pub fn seeded(seed: u64) -> Self {
        let state = if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed };
        Self { state }
    }

    /// 当前内部状态（用于存档/回放）。
    pub fn state(&self) -> u64 {
        self.state
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// [0, 1) 均匀分布。取高 24 bit，保证 f32 精度内均匀。
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// [min, max) 均匀分布；min >= max 时直接返回 min。
    pub fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        if min >= max {
            return min;
        }
        min + (max - min) * self.next_f32()
    }

    /// [min, max) 整数；min >= max 时返回 min。
    pub fn range_u32(&mut self, min: u32, max: u32) -> u32 {
        if min >= max {
            return min;
        }
        min + (self.next_u64() % u64::from(max - min)) as u32
    }

    /// 以概率 p 返回 true（p 会被 clamp 到 [0,1]）。
    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p.clamp(0.0, 1.0)
    }

    /// 单位圆上的随机方向。
    pub fn unit_vec2(&mut self) -> Vec2 {
        Vec2::from_angle(self.range_f32(0.0, std::f32::consts::TAU))
    }

    /// 以原点为中心、半尺寸为 `half` 的矩形内随机点。
    pub fn point_in_rect(&mut self, half: Vec2) -> Vec2 {
        Vec2::new(self.range_f32(-half.x, half.x), self.range_f32(-half.y, half.y))
    }

    /// 在矩形边缘上随机取一点（敌人从屏幕边缘刷出）。
    pub fn point_on_rect_edge(&mut self, half: Vec2) -> Vec2 {
        match self.range_u32(0, 4) {
            0 => Vec2::new(self.range_f32(-half.x, half.x), half.y),
            1 => Vec2::new(self.range_f32(-half.x, half.x), -half.y),
            2 => Vec2::new(-half.x, self.range_f32(-half.y, half.y)),
            _ => Vec2::new(half.x, self.range_f32(-half.y, half.y)),
        }
    }

    /// 从切片里随机挑一个元素。
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() { None } else { items.get(self.range_u32(0, items.len() as u32) as usize) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = GameRng::seeded(42);
        let mut b = GameRng::seeded(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seed_different_sequence() {
        let mut a = GameRng::seeded(1);
        let mut b = GameRng::seeded(2);
        let same = (0..100).filter(|_| a.next_u64() == b.next_u64()).count();
        assert!(same < 3, "两条序列不应几乎相同, same={same}");
    }

    #[test]
    fn zero_seed_is_not_stuck() {
        let mut r = GameRng::seeded(0);
        assert_ne!(r.next_u64(), 0);
        assert_ne!(r.next_u64(), r.next_u64());
    }

    #[test]
    fn next_f32_in_unit_interval() {
        let mut r = GameRng::seeded(7);
        for _ in 0..10_000 {
            let v = r.next_f32();
            assert!((0.0..1.0).contains(&v), "{v}");
        }
    }

    #[test]
    fn range_bounds_and_degenerate() {
        let mut r = GameRng::seeded(9);
        for _ in 0..5_000 {
            let f = r.range_f32(-3.0, 5.0);
            assert!((-3.0..5.0).contains(&f));
            let u = r.range_u32(10, 20);
            assert!((10..20).contains(&u));
        }
        assert_eq!(r.range_f32(2.0, 2.0), 2.0);
        assert_eq!(r.range_f32(5.0, 1.0), 5.0);
        assert_eq!(r.range_u32(8, 8), 8);
    }

    #[test]
    fn range_u32_hits_every_bucket() {
        let mut r = GameRng::seeded(123);
        let mut seen = [false; 6];
        for _ in 0..1_000 {
            seen[r.range_u32(0, 6) as usize] = true;
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn chance_extremes() {
        let mut r = GameRng::seeded(5);
        assert!((0..100).all(|_| !r.chance(0.0)));
        assert!((0..100).all(|_| r.chance(1.0)));
        assert!((0..100).all(|_| r.chance(2.0)), "p>1 clamp 到 1");
        assert!((0..100).all(|_| !r.chance(-1.0)), "p<0 clamp 到 0");
    }

    #[test]
    fn chance_is_roughly_calibrated() {
        let mut r = GameRng::seeded(77);
        let hits = (0..20_000).filter(|_| r.chance(0.25)).count() as f32 / 20_000.0;
        assert!((hits - 0.25).abs() < 0.02, "hits={hits}");
    }

    #[test]
    fn unit_vec_is_normalized() {
        let mut r = GameRng::seeded(3);
        for _ in 0..100 {
            assert!((r.unit_vec2().length() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn rect_points_are_inside_or_on_edge() {
        let mut r = GameRng::seeded(11);
        let half = Vec2::new(100.0, 50.0);
        for _ in 0..1000 {
            let p = r.point_in_rect(half);
            assert!(p.x.abs() <= half.x && p.y.abs() <= half.y);
            let e = r.point_on_rect_edge(half);
            let on_x_edge = (e.x.abs() - half.x).abs() < 1e-3;
            let on_y_edge = (e.y.abs() - half.y).abs() < 1e-3;
            assert!(on_x_edge || on_y_edge, "{e:?} 不在边上");
        }
    }

    #[test]
    fn pick_handles_empty_and_non_empty() {
        let mut r = GameRng::seeded(1);
        let empty: [u8; 0] = [];
        assert!(r.pick(&empty).is_none());
        let items = [1, 2, 3];
        for _ in 0..50 {
            assert!(items.contains(r.pick(&items).unwrap()));
        }
    }
}
