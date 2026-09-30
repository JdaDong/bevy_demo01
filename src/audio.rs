//! 音频：背景音乐（WAV 循环）+ 程序化音效（`Pitch` 正弦波，零素材）+ 音量设置。
//!
//! 覆盖 API：`AudioPlayer`、`PlaybackSettings::{LOOP, DESPAWN}` + `with_volume/with_speed`、
//! `AudioSink` + `AudioSinkPlayback`（pause/play/set_volume）、`Pitch` 资源、
//! `GlobalVolume`、`Volume::Linear`、`resource_changed` 运行条件。

use std::time::Duration;

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::assets::GameAssets;
use crate::events::PlaySfx;
use crate::state::GamePhase;

/// 音量设置（逻辑层资源：菜单按钮可改，测试可断言）。
#[derive(Resource, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Resource)]
pub struct AudioSettings {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self { master: 0.7, music: 0.45, sfx: 0.8, muted: false }
    }
}

impl AudioSettings {
    pub const STEP: f32 = 0.1;

    pub fn step_master(&mut self, delta: f32) {
        // 四舍五入到 0.1，避免浮点累加出现 0.30000004
        self.master = ((self.master + delta).clamp(0.0, 1.0) * 10.0).round() / 10.0;
    }

    pub fn effective_master(&self) -> f32 {
        if self.muted { 0.0 } else { self.master }
    }

    /// 百分比文本（UI 用）。
    pub fn master_percent(&self) -> u32 {
        (self.effective_master() * 100.0).round() as u32
    }
}

/// 每种音效的“合成参数”：频率(Hz)、时长、播放速度。
pub fn sfx_params(sfx: PlaySfx) -> (f32, Duration, f32) {
    match sfx {
        PlaySfx::Shoot => (880.0, Duration::from_millis(40), 1.0),
        PlaySfx::Missile => (330.0, Duration::from_millis(120), 1.0),
        PlaySfx::Explosion => (110.0, Duration::from_millis(160), 0.8),
        PlaySfx::Collect => (1320.0, Duration::from_millis(60), 1.0),
        PlaySfx::Hurt => (180.0, Duration::from_millis(220), 1.0),
        PlaySfx::PowerUp => (660.0, Duration::from_millis(150), 1.2),
        PlaySfx::UiClick => (520.0, Duration::from_millis(30), 1.0),
    }
}

#[derive(Component)]
pub struct Music;

/// 同类音效在同一帧只播一次，避免连射时爆音。
fn dedup(sfx: impl Iterator<Item = PlaySfx>) -> Vec<PlaySfx> {
    let mut out: Vec<PlaySfx> = Vec::new();
    for s in sfx {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

fn start_music(mut commands: Commands, assets: Res<GameAssets>, settings: Res<AudioSettings>) {
    commands.spawn((
        Name::new("Music"),
        Music,
        AudioPlayer(assets.music.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(settings.music)),
    ));
}

fn play_sfx(
    mut commands: Commands,
    mut requests: MessageReader<PlaySfx>,
    mut pitches: ResMut<Assets<Pitch>>,
    assets: Res<GameAssets>,
    settings: Res<AudioSettings>,
    mut cache: Local<Vec<(PlaySfx, Handle<Pitch>)>>,
) {
    for sfx in dedup(requests.read().copied()) {
        let volume = Volume::Linear(settings.sfx);
        if sfx == PlaySfx::Collect {
            // 真实 WAV 文件
            commands.spawn((AudioPlayer(assets.collect_sfx.clone()), PlaybackSettings::DESPAWN.with_volume(volume)));
            continue;
        }
        // 程序化正弦波：句柄缓存，避免每次 add 新资源
        let handle = if let Some((_, h)) = cache.iter().find(|(s, _)| *s == sfx) {
            h.clone()
        } else {
            let (freq, dur, _) = sfx_params(sfx);
            let h = pitches.add(Pitch::new(freq, dur));
            cache.push((sfx, h.clone()));
            h
        };
        let (_, _, speed) = sfx_params(sfx);
        commands.spawn((AudioPlayer(handle), PlaybackSettings::DESPAWN.with_volume(volume).with_speed(speed)));
    }
}

fn apply_volume(
    settings: Res<AudioSettings>,
    mut global: ResMut<GlobalVolume>,
    mut music: Query<&mut AudioSink, With<Music>>,
) {
    global.volume = Volume::Linear(settings.effective_master());
    for mut sink in &mut music {
        sink.set_volume(Volume::Linear(settings.music));
    }
}

fn pause_music(music: Query<&AudioSink, With<Music>>) {
    for sink in &music {
        sink.pause();
    }
}

fn resume_music(music: Query<&AudioSink, With<Music>>) {
    for sink in &music {
        sink.play();
    }
}

fn toggle_mute(mut settings: ResMut<AudioSettings>) {
    settings.muted ^= true;
}

pub struct AudioLogicPlugin;

impl Plugin for AudioLogicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioSettings>();
    }
}

pub struct AudioPresentationPlugin;

impl Plugin for AudioPresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(crate::state::AppState::Loading), start_music)
            .add_systems(
                Update,
                (
                    play_sfx,
                    apply_volume.run_if(resource_changed::<AudioSettings>),
                    toggle_mute.run_if(bevy::input::common_conditions::input_just_pressed(KeyCode::KeyM)),
                ),
            )
            .add_systems(OnEnter(GamePhase::Paused), pause_music)
            .add_systems(OnExit(GamePhase::Paused), resume_music);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_volume_steps_are_clean_and_clamped() {
        let mut s = AudioSettings::default();
        s.step_master(AudioSettings::STEP);
        assert_eq!(s.master, 0.8);
        for _ in 0..20 {
            s.step_master(AudioSettings::STEP);
        }
        assert_eq!(s.master, 1.0);
        for _ in 0..30 {
            s.step_master(-AudioSettings::STEP);
        }
        assert_eq!(s.master, 0.0);
    }

    #[test]
    fn mute_zeroes_effective_volume() {
        let mut s = AudioSettings::default();
        assert_eq!(s.master_percent(), 70);
        s.muted = true;
        assert_eq!(s.effective_master(), 0.0);
        assert_eq!(s.master_percent(), 0);
    }

    #[test]
    fn every_sfx_has_audible_params() {
        for sfx in [
            PlaySfx::Shoot,
            PlaySfx::Missile,
            PlaySfx::Explosion,
            PlaySfx::Collect,
            PlaySfx::Hurt,
            PlaySfx::PowerUp,
            PlaySfx::UiClick,
        ] {
            let (f, d, s) = sfx_params(sfx);
            assert!((20.0..20_000.0).contains(&f), "{sfx:?} 频率超出人耳范围");
            assert!(d > Duration::ZERO && s > 0.0);
        }
    }

    #[test]
    fn dedup_keeps_first_occurrence_order() {
        let v = dedup([PlaySfx::Shoot, PlaySfx::Hurt, PlaySfx::Shoot, PlaySfx::Collect].into_iter());
        assert_eq!(v, vec![PlaySfx::Shoot, PlaySfx::Hurt, PlaySfx::Collect]);
    }
}
