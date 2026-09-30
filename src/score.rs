//! 计分、连击、局内统计、排行榜。
//!
//! 覆盖 API：`MessageReader`（多种消息）、`Res/ResMut`、`resource_changed` 运行条件、
//! `Local<T>`（系统私有状态）、`Timer::tick/just_finished`、serde 持久化结构。

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::GameConfig;
use crate::events::{EnemyDestroyed, PlaySfx, SpawnPopup, StarCollected};
use crate::math::combo_multiplier;
use crate::schedule::GameSet;
use crate::state::AppState;

/// 本局分数。
#[derive(Resource, Reflect, Debug, Clone, PartialEq)]
#[reflect(Resource)]
pub struct Score {
    pub points: u64,
    pub combo: u32,
    pub best_combo: u32,
    /// 连击窗口，超时清零
    pub combo_timer: Timer,
}

impl Default for Score {
    fn default() -> Self {
        Self::with_window(2.5)
    }
}

impl Score {
    pub fn with_window(secs: f32) -> Self {
        Self { points: 0, combo: 0, best_combo: 0, combo_timer: Timer::from_seconds(secs, TimerMode::Once) }
    }

    /// 加分（带连击倍率），返回实际得分。
    pub fn add(&mut self, base: u32) -> u64 {
        self.combo += 1;
        self.best_combo = self.best_combo.max(self.combo);
        self.combo_timer.reset();
        let gained = u64::from(base) * u64::from(combo_multiplier(self.combo));
        self.points += gained;
        gained
    }

    pub fn multiplier(&self) -> u32 {
        combo_multiplier(self.combo)
    }

    /// 推进连击计时，超时返回 true 并清零连击。
    pub fn tick(&mut self, delta: std::time::Duration) -> bool {
        if self.combo == 0 {
            return false;
        }
        if self.combo_timer.tick(delta).just_finished() {
            self.combo = 0;
            return true;
        }
        false
    }
}

/// 本局统计。
#[derive(Resource, Reflect, Debug, Default, Clone, PartialEq)]
#[reflect(Resource)]
pub struct RunStats {
    pub stars: u32,
    pub kills: [u32; 3],
    pub shots: u32,
    pub missiles: u32,
    pub damage_taken: u32,
    pub seconds: f32,
    pub wave: u32,
}

impl RunStats {
    pub fn total_kills(&self) -> u32 {
        self.kills.iter().sum()
    }
    pub fn accuracy(&self) -> f32 {
        if self.shots == 0 { 0.0 } else { (self.total_kills() as f32 / self.shots as f32).min(1.0) }
    }
}

/// 排行榜条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct HighScoreEntry {
    pub score: u64,
    pub wave: u32,
    pub stars: u32,
}

/// 排行榜（前 N 名，降序）。
#[derive(Resource, Reflect, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[reflect(Resource)]
pub struct HighScores {
    pub entries: Vec<HighScoreEntry>,
    pub capacity: usize,
}

impl Default for HighScores {
    fn default() -> Self {
        Self { entries: Vec::new(), capacity: 5 }
    }
}

impl HighScores {
    /// 插入并返回名次（0 起）；没进榜返回 None。
    /// 同分时先到者排前（稳定）。
    pub fn submit(&mut self, entry: HighScoreEntry) -> Option<usize> {
        if self.capacity == 0 {
            return None;
        }
        let pos = self.entries.iter().position(|e| entry.score > e.score).unwrap_or(self.entries.len());
        if pos >= self.capacity {
            return None;
        }
        self.entries.insert(pos, entry);
        self.entries.truncate(self.capacity);
        Some(pos)
    }

    pub fn best(&self) -> u64 {
        self.entries.first().map_or(0, |e| e.score)
    }

    pub fn to_ron(&self) -> Result<String, ron::Error> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
    }

    pub fn from_ron(s: &str) -> Result<Self, ron::error::SpannedError> {
        let mut hs: HighScores = ron::de::from_str(s)?;
        // 防御：文件被手动改乱序/超长时修复
        hs.entries.sort_by_key(|e| std::cmp::Reverse(e.score));
        hs.entries.truncate(hs.capacity);
        Ok(hs)
    }
}

/// 最近一局结果（GameOver 界面展示）。
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct LastRun {
    pub score: u64,
    pub rank: Option<usize>,
    pub stats: RunStats,
    pub best_combo: u32,
}

pub fn reset_run(mut commands: Commands, config: Res<GameConfig>) {
    commands.insert_resource(Score::with_window(config.combo_window_secs));
    commands.insert_resource(RunStats { wave: 1, ..default() });
}

/// 星星 + 击杀 → 加分、飘字、音效。
pub fn apply_score(
    mut stars: MessageReader<StarCollected>,
    mut kills: MessageReader<EnemyDestroyed>,
    mut score: ResMut<Score>,
    mut stats: ResMut<RunStats>,
    mut popups: MessageWriter<SpawnPopup>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    for star in stars.read() {
        let gained = score.add(star.value);
        stats.stars += 1;
        sfx.write(PlaySfx::Collect);
        popups.write(SpawnPopup {
            position: star.position,
            text: format!("+{gained}"),
            color: Color::srgb(1.0, 0.9, 0.3),
        });
    }
    for kill in kills.read() {
        let gained = score.add(kill.kind.score());
        stats.kills[kill.kind.index()] += 1;
        let mult = score.multiplier();
        popups.write(SpawnPopup {
            position: kill.position,
            text: if mult > 1 { format!("+{gained} x{mult}") } else { format!("+{gained}") },
            color: Color::srgb(1.0, 0.5, 0.3),
        });
    }
}

pub fn tick_combo(time: Res<Time>, mut score: ResMut<Score>) {
    // 只有连击进行中才 tick，否则 ResMut 不被标记为 changed
    if score.combo > 0 {
        score.tick(time.delta());
    }
}

pub fn tick_run_time(time: Res<Time>, mut stats: ResMut<RunStats>, mut acc: Local<f32>) {
    // Local：系统私有累计量，每 0.25s 才写一次 RunStats，减少 HUD 无效刷新
    *acc += time.delta_secs();
    if *acc >= 0.25 {
        stats.seconds += *acc;
        *acc = 0.0;
    }
}

pub struct ScorePlugin;

impl Plugin for ScorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>()
            .init_resource::<RunStats>()
            .init_resource::<HighScores>()
            .init_resource::<LastRun>()
            .add_systems(OnEnter(AppState::InGame), reset_run)
            .add_systems(Update, (apply_score, tick_combo, tick_run_time).in_set(GameSet::Resolve));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn entry(score: u64) -> HighScoreEntry {
        HighScoreEntry { score, wave: 1, stars: 0 }
    }

    #[test]
    fn score_combo_multiplier_applies() {
        let mut s = Score::with_window(1.0);
        for _ in 0..4 {
            assert_eq!(s.add(10), 10);
        }
        assert_eq!(s.add(10), 20, "第 5 连击起 x2");
        assert_eq!(s.points, 60);
        assert_eq!(s.best_combo, 5);
    }

    #[test]
    fn combo_expires() {
        let mut s = Score::with_window(1.0);
        s.add(1);
        s.add(1);
        assert!(!s.tick(Duration::from_millis(500)));
        assert_eq!(s.combo, 2);
        assert!(s.tick(Duration::from_millis(600)));
        assert_eq!(s.combo, 0);
        assert_eq!(s.best_combo, 2, "最佳连击保留");
        assert!(!s.tick(Duration::from_secs(10)), "无连击时 tick 无副作用");
    }

    #[test]
    fn combo_timer_resets_on_hit() {
        let mut s = Score::with_window(1.0);
        s.add(1);
        s.tick(Duration::from_millis(900));
        s.add(1); // 重置窗口
        assert!(!s.tick(Duration::from_millis(900)));
        assert_eq!(s.combo, 2);
    }

    #[test]
    fn highscores_sorted_and_capped() {
        let mut hs = HighScores { entries: vec![], capacity: 3 };
        assert_eq!(hs.submit(entry(100)), Some(0));
        assert_eq!(hs.submit(entry(300)), Some(0));
        assert_eq!(hs.submit(entry(200)), Some(1));
        assert_eq!(hs.submit(entry(50)), None, "榜满且分数最低");
        assert_eq!(hs.submit(entry(250)), Some(1));
        let scores: Vec<_> = hs.entries.iter().map(|e| e.score).collect();
        assert_eq!(scores, vec![300, 250, 200]);
        assert_eq!(hs.best(), 300);
    }

    #[test]
    fn highscores_tie_is_stable() {
        let mut hs = HighScores::default();
        hs.submit(HighScoreEntry { score: 10, wave: 1, stars: 1 });
        let rank = hs.submit(HighScoreEntry { score: 10, wave: 2, stars: 2 });
        assert_eq!(rank, Some(1), "同分后来者排后");
    }

    #[test]
    fn highscores_zero_capacity() {
        let mut hs = HighScores { entries: vec![], capacity: 0 };
        assert_eq!(hs.submit(entry(1)), None);
        assert_eq!(hs.best(), 0);
    }

    #[test]
    fn highscores_ron_roundtrip_and_repair() {
        let mut hs = HighScores::default();
        hs.submit(entry(5));
        hs.submit(entry(7));
        let text = hs.to_ron().unwrap();
        assert_eq!(HighScores::from_ron(&text).unwrap(), hs);

        let messy = "(entries: [(score: 1, wave: 1, stars: 0), (score: 9, wave: 1, stars: 0), (score: 3, wave: 1, stars: 0)], capacity: 2)";
        let fixed = HighScores::from_ron(messy).unwrap();
        assert_eq!(fixed.entries.iter().map(|e| e.score).collect::<Vec<_>>(), vec![9, 3]);
        assert!(HighScores::from_ron("garbage").is_err());
    }

    #[test]
    fn stats_accuracy() {
        let mut s = RunStats::default();
        assert_eq!(s.accuracy(), 0.0);
        s.shots = 10;
        s.kills = [2, 1, 0];
        assert!((s.accuracy() - 0.3).abs() < 1e-6);
        s.shots = 1;
        assert_eq!(s.accuracy(), 1.0, "上限 1");
    }
}
