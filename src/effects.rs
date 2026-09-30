//! 视觉反馈：爆炸粒子、飘字、屏幕震动（trauma）、帧动画、生命周期回收。
//!
//! 覆盖 API：全局观察者 `add_observer` + `On<Explosion>`、观察者里用 `Commands`/`ResMut`、
//! `Text2d` + `TextFont` + `TextColor` + `Text2dShadow` + `Anchor`、`Sprite::from_image`、
//! `Query::par_iter_mut`（并行迭代）、`Timer`、`TextureAtlas` 帧动画、
//! `Visibility`、`DespawnOnExit`。

use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};

use crate::assets::GameAssets;
use crate::components::{FloatingText, Lifetime, Particle, Projectile, SpriteAnimation, Velocity};
use crate::events::{Explosion, PlaySfx, SpawnPopup};
use crate::math::{decay_trauma, fade_alpha};
use crate::rng::GameRng;
use crate::schedule::GameSet;
use crate::state::AppState;

/// 相机震动强度 [0,1]。逻辑层写，表现层（camera.rs）读。
#[derive(Resource, Reflect, Debug, Default, Clone, Copy, PartialEq)]
#[reflect(Resource)]
pub struct CameraTrauma(pub f32);

impl CameraTrauma {
    pub fn add(&mut self, amount: f32) {
        self.0 = (self.0 + amount).clamp(0.0, 1.0);
    }
}

pub const TRAUMA_DECAY: f32 = 1.2;

/// 每次爆炸生成的粒子数（按半径缩放）。
pub fn particle_count(radius: f32) -> usize {
    ((radius * 0.8) as usize).clamp(6, 40)
}

/// 全局观察者：爆炸 → 粒子 + 震屏 + 音效。
pub fn on_explosion(
    explosion: On<Explosion>,
    mut commands: Commands,
    mut rng: ResMut<GameRng>,
    mut trauma: ResMut<CameraTrauma>,
    mut sfx: MessageWriter<PlaySfx>,
    assets: Res<GameAssets>,
) {
    let e = *explosion.event();
    for _ in 0..particle_count(e.radius) {
        let dir = rng.unit_vec2();
        let speed = rng.range_f32(60.0, 260.0) * (e.radius / 20.0).clamp(0.6, 2.0);
        let size = rng.range_f32(2.0, 6.0);
        commands.spawn((
            Name::new("Particle"),
            Particle { start_alpha: 1.0 },
            Sprite { image: assets.spark.clone(), color: e.color, custom_size: Some(Vec2::splat(size)), ..default() },
            Transform::from_translation(e.position.extend(5.0)),
            Velocity(dir * speed),
            Lifetime::secs(rng.range_f32(0.35, 0.8)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    trauma.add((e.radius / 60.0).clamp(0.1, 0.6));
    sfx.write(PlaySfx::Explosion);
}

/// 消费 `SpawnPopup` → 世界空间飘字。
pub fn spawn_popups(mut commands: Commands, mut popups: MessageReader<SpawnPopup>) {
    for p in popups.read() {
        commands.spawn((
            Name::new("Popup"),
            Text2d::new(p.text.clone()),
            TextFont::from_font_size(18.0),
            TextColor(p.color),
            Text2dShadow::default(),
            Anchor::BOTTOM_CENTER,
            Transform::from_translation(p.position.extend(20.0)),
            FloatingText { rise_speed: 50.0 },
            Lifetime::secs(0.9),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// 生命周期计时。到期后：投射物交给对象池（combat 模块处理），其余直接销毁。
pub fn tick_lifetimes(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Lifetime, Has<Projectile>)>) {
    for (entity, mut life, is_projectile) in &mut q {
        if life.0.tick(time.delta()).is_finished() && !is_projectile {
            commands.entity(entity).despawn();
        }
    }
}

/// 粒子淡出 + 减速。粒子数量多，用 `par_iter_mut` 并行。
pub fn update_particles(time: Res<Time>, mut q: Query<(&mut Sprite, &mut Velocity, &Lifetime, &Particle)>) {
    let dt = time.delta_secs();
    q.par_iter_mut().for_each(|(mut sprite, mut vel, life, particle)| {
        let alpha = particle.start_alpha * fade_alpha(life.0.fraction());
        sprite.color.set_alpha(alpha);
        vel.0 *= 1.0 - (3.0 * dt).min(1.0);
    });
}

/// 飘字上升 + 淡出。
pub fn update_floating_text(time: Res<Time>, mut q: Query<(&mut Transform, &mut TextColor, &Lifetime, &FloatingText)>) {
    for (mut tf, mut color, life, ft) in &mut q {
        tf.translation.y += ft.rise_speed * time.delta_secs();
        color.0.set_alpha(fade_alpha(life.0.fraction()));
    }
}

/// 屏幕震动衰减。
pub fn decay_camera_trauma(time: Res<Time>, mut trauma: ResMut<CameraTrauma>) {
    if trauma.0 > 0.0 {
        trauma.0 = decay_trauma(trauma.0, TRAUMA_DECAY, time.delta_secs());
    }
}

/// 精灵帧动画：切 `TextureAtlas::index`。
pub fn animate_sprites(time: Res<Time>, mut q: Query<(&mut SpriteAnimation, &mut Sprite)>) {
    for (mut anim, mut sprite) in &mut q {
        if anim.timer.tick(time.delta()).just_finished() {
            let next = sprite.texture_atlas.as_ref().map(|a| anim.next_frame(a.index));
            if let (Some(next), Some(atlas)) = (next, sprite.texture_atlas.as_mut()) {
                atlas.index = next;
            }
        }
    }
}

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraTrauma>()
            .add_observer(on_explosion)
            .add_systems(
                Update,
                (
                    spawn_popups,
                    tick_lifetimes,
                    update_particles,
                    update_floating_text,
                    decay_camera_trauma,
                    animate_sprites,
                )
                    .in_set(GameSet::Feedback)
                    .run_if(in_state(crate::state::Simulating)),
            )
            .add_systems(OnExit(AppState::InGame), reset_trauma);
    }
}

fn reset_trauma(mut trauma: ResMut<CameraTrauma>) {
    trauma.0 = 0.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trauma_is_clamped() {
        let mut t = CameraTrauma::default();
        t.add(0.7);
        t.add(0.7);
        assert_eq!(t.0, 1.0);
        t.add(-5.0);
        assert_eq!(t.0, 0.0);
    }

    #[test]
    fn particle_count_is_bounded() {
        assert_eq!(particle_count(0.0), 6);
        assert_eq!(particle_count(20.0), 16);
        assert_eq!(particle_count(10_000.0), 40);
    }
}
