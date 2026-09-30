//! 持久化：排行榜异步写盘 + 反射快照。
//!
//! 覆盖 API：`IoTaskPool::spawn` + `Task` + `futures::check_ready`（非阻塞轮询）、
//! 返回 `Result` 的可失败系统（fallible system）、`.pipe()` 系统管道、
//! 独占系统 `fn(&mut World)`、`AppTypeRegistry` + `ReflectComponent` +
//! `ReflectSerializer`（任意已注册组件 → RON）、`OnEnter(GameOver)`。

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::reflect::serde::ReflectSerializer;
use bevy::tasks::{IoTaskPool, Task, futures::check_ready};

use crate::components::Player;
use crate::score::{HighScoreEntry, HighScores, LastRun, RunStats, Score};
use crate::state::AppState;

/// 存档目录。默认 `./saves`，可用环境变量 `BEVY_DEMO_SAVE_DIR` 覆盖（测试用临时目录）。
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct SaveDir(pub PathBuf);

impl Default for SaveDir {
    fn default() -> Self {
        let dir = std::env::var_os("BEVY_DEMO_SAVE_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("saves"));
        Self(dir)
    }
}

impl SaveDir {
    pub fn highscores(&self) -> PathBuf {
        self.0.join("highscores.ron")
    }
    pub fn snapshot(&self) -> PathBuf {
        self.0.join("snapshot.ron")
    }
}

/// 进行中的写盘任务。
#[derive(Component)]
pub struct SaveTask(pub Task<Result<PathBuf, String>>);

/// 写盘完成。
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct SaveFinished {
    pub result: Result<PathBuf, String>,
}

pub fn write_text_file(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // 先写临时文件再 rename：崩溃时不会留下半截存档
    let tmp = path.with_extension("ron.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

pub fn load_highscores_from(path: &Path) -> HighScores {
    match std::fs::read_to_string(path) {
        Ok(text) => HighScores::from_ron(&text).unwrap_or_else(|e| {
            warn!("排行榜解析失败，使用空榜: {e}");
            HighScores::default()
        }),
        Err(_) => HighScores::default(),
    }
}

fn load_highscores(mut commands: Commands, dir: Res<SaveDir>) {
    commands.insert_resource(load_highscores_from(&dir.highscores()));
}

/// 进入 GameOver：提交分数 → 生成异步写盘任务。
pub fn submit_and_save(
    mut commands: Commands,
    score: Res<Score>,
    stats: Res<RunStats>,
    dir: Res<SaveDir>,
    mut highscores: ResMut<HighScores>,
) {
    let rank = highscores.submit(HighScoreEntry { score: score.points, wave: stats.wave, stars: stats.stars });
    commands.insert_resource(LastRun { score: score.points, rank, stats: stats.clone(), best_combo: score.best_combo });
    let text = match highscores.to_ron() {
        Ok(t) => t,
        Err(e) => {
            error!("排行榜序列化失败: {e}");
            return;
        }
    };
    let path = dir.highscores();
    let task = IoTaskPool::get().spawn(async move {
        write_text_file(&path, &text).map(|_| path.clone()).map_err(|e| format!("{}: {e}", path.display()))
    });
    commands.spawn((Name::new("SaveTask"), SaveTask(task)));
}

/// 非阻塞轮询任务。
pub fn poll_save_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut SaveTask)>,
    mut finished: MessageWriter<SaveFinished>,
) {
    for (e, mut task) in &mut tasks {
        if let Some(result) = check_ready(&mut task.0) {
            match &result {
                Ok(p) => info!("已保存 {}", p.display()),
                Err(err) => warn!("保存失败: {err}"),
            }
            finished.write(SaveFinished { result });
            commands.entity(e).despawn();
        }
    }
}

/// 反射快照：把玩家实体上所有“已注册且可反射”的组件序列化成 RON。
/// 独占系统（`&mut World`）可以随意访问任何数据，代价是不能并行。
pub fn snapshot_player(world: &mut World) -> Option<String> {
    let mut q = world.query_filtered::<Entity, With<Player>>();
    let entity = q.iter(world).next()?;
    let registry = world.resource::<AppTypeRegistry>().read();
    let entity_ref = world.entity(entity);
    let mut out = String::from("(\n");
    let mut names: Vec<(String, String)> = Vec::new();
    for registration in registry.iter() {
        let Some(reflect_component) = registration.data::<ReflectComponent>() else {
            continue;
        };
        let Some(value) = reflect_component.reflect(entity_ref) else {
            continue;
        };
        let serializer = ReflectSerializer::new(value.as_partial_reflect(), &registry);
        match ron::ser::to_string(&serializer) {
            Ok(s) => names.push((registration.type_info().type_path().to_string(), s)),
            Err(e) => names.push((registration.type_info().type_path().to_string(), format!("<err {e}>"))),
        }
    }
    // 排序保证输出稳定（便于 diff 与测试）
    names.sort();
    for (_, s) in names {
        out.push_str("  ");
        out.push_str(&s);
        out.push_str(",\n");
    }
    out.push(')');
    Some(out)
}

/// 快照系统：`snapshot_player` 的结果写盘，返回 `Result` → 交给错误处理器。
pub fn write_snapshot(world: &mut World) -> Result<PathBuf> {
    let text = snapshot_player(world).ok_or("当前没有玩家实体")?;
    let path = world.resource::<SaveDir>().snapshot();
    write_text_file(&path, &text)?;
    Ok(path)
}

/// 管道下游：把 `Result` 转成日志（演示 `.pipe()`；也可以直接交给全局 error handler）。
pub fn report_snapshot(In(result): In<Result<PathBuf>>) {
    match result {
        Ok(path) => info!("快照已写入 {}", path.display()),
        Err(e) => warn!("快照失败: {e}"),
    }
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveDir>()
            .add_message::<SaveFinished>()
            .add_systems(Startup, load_highscores)
            .add_systems(OnEnter(AppState::GameOver), submit_and_save)
            .add_systems(Update, poll_save_tasks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bevy_demo01_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn atomic_write_creates_parent_dirs() {
        let dir = temp_dir("write");
        let path = dir.join("nested/deeper/file.ron");
        write_text_file(&path, "(a: 1)").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "(a: 1)");
        assert!(!path.with_extension("ron.tmp").exists(), "临时文件应被 rename 掉");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_missing_or_corrupt_falls_back_to_default() {
        let dir = temp_dir("load");
        assert_eq!(load_highscores_from(&dir.join("nope.ron")), HighScores::default());
        let bad = dir.join("bad.ron");
        write_text_file(&bad, "not ron at all {{{").unwrap();
        assert_eq!(load_highscores_from(&bad), HighScores::default());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_dir_paths() {
        let d = SaveDir(PathBuf::from("/tmp/x"));
        assert!(d.highscores().ends_with("highscores.ron"));
        assert!(d.snapshot().ends_with("snapshot.ron"));
    }
}
