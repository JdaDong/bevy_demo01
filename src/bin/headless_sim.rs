//! 无头模拟器：不开窗口，用自动驾驶 AI 跑 N 帧，输出统计（CI 冒烟 / 数值平衡调参）。
//!
//! ```bash
//! cargo run --bin headless_sim -- --frames 3600 --seed 42 --json
//! ```

use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_demo01::autoplay::autopilot_intent;
use bevy_demo01::components::{Enemy, EnemyCensus, Health, Player, Star};
use bevy_demo01::config::GameConfig;
use bevy_demo01::headless_app;
use bevy_demo01::input::{IntentSource, PlayerIntent};
use bevy_demo01::player::BulletPoolStats;
use bevy_demo01::save::SaveDir;
use bevy_demo01::score::{RunStats, Score};
use bevy_demo01::state::AppState;

struct Args {
    frames: u32,
    seed: Option<u64>,
    json: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args { frames: 3600, seed: None, json: false };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--frames" => args.frames = it.next().ok_or("--frames 需要参数")?.parse().map_err(|e| format!("{e}"))?,
            "--seed" => args.seed = Some(it.next().ok_or("--seed 需要参数")?.parse().map_err(|e| format!("{e}"))?),
            "--json" => args.json = true,
            "-h" | "--help" => {
                println!("usage: headless_sim [--frames N] [--seed S] [--json]");
                std::process::exit(0);
            }
            other => return Err(format!("未知参数: {other}")),
        }
    }
    Ok(args)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    let mut app = headless_app(Duration::from_secs_f64(1.0 / 60.0));
    app.insert_resource(IntentSource::Scripted)
        .insert_resource(SaveDir(std::env::temp_dir().join("bevy_demo01_headless_sim")));
    if let Some(seed) = args.seed {
        app.world_mut().resource_mut::<GameConfig>().rng_seed = seed;
    }
    app.update();
    app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::InGame);
    app.update();

    let started = Instant::now();
    let mut frames_run = 0;
    let mut died_at = None;
    for frame in 0..args.frames {
        // 在 update 之前写入 AI 意图（Scripted 模式下不会被设备输入覆盖）
        let world = app.world_mut();
        let player =
            world.query_filtered::<&Transform, With<Player>>().iter(world).next().map(|t| t.translation.truncate());
        if let Some(p) = player {
            let near =
                |v: Vec<Vec2>| v.into_iter().min_by(|a, b| a.distance_squared(p).total_cmp(&b.distance_squared(p)));
            let enemies: Vec<Vec2> = world
                .query_filtered::<&Transform, With<Enemy>>()
                .iter(world)
                .map(|t| t.translation.truncate())
                .collect();
            let stars: Vec<Vec2> = world
                .query_filtered::<&Transform, With<Star>>()
                .iter(world)
                .map(|t| t.translation.truncate())
                .collect();
            let t = frame as f32 / 60.0;
            *world.resource_mut::<PlayerIntent>() = autopilot_intent(t, p, near(enemies), near(stars));
        }
        app.update();
        frames_run += 1;
        if *app.world().resource::<State<AppState>>().get() != AppState::InGame {
            died_at = Some(frame);
            break;
        }
    }

    let elapsed = started.elapsed();
    let w = app.world_mut();
    let score = w.resource::<Score>().clone();
    let stats = w.resource::<RunStats>().clone();
    let census = w.resource::<EnemyCensus>().clone();
    let pool = *w.resource::<BulletPoolStats>();
    let hp = w.query_filtered::<&Health, With<Player>>().iter(w).next().map_or(0, |h| h.current);
    let entities = w.query::<Entity>().iter(w).count();
    let fps = frames_run as f64 / elapsed.as_secs_f64();

    if args.json {
        println!(
            "{{\"frames\":{frames_run},\"died_at\":{},\"score\":{},\"best_combo\":{},\"wave\":{},\"stars\":{},\"kills\":{:?},\"shots\":{},\"hp\":{hp},\"enemies_spawned\":{},\"bullets_spawned\":{},\"bullets_reused\":{},\"entities\":{entities},\"sim_fps\":{fps:.0}}}",
            died_at.map_or("null".to_string(), |f| f.to_string()),
            score.points,
            score.best_combo,
            stats.wave,
            stats.stars,
            stats.kills,
            stats.shots,
            census.total_spawned,
            pool.spawned,
            pool.reused,
        );
    } else {
        println!("== headless_sim ==");
        println!(
            "frames       {frames_run} ({:.1}s game time, {:.2}s wall, {fps:.0} sim-fps)",
            frames_run as f32 / 60.0,
            elapsed.as_secs_f64()
        );
        println!("result       {}", died_at.map_or("survived".to_string(), |f| format!("died at frame {f}")));
        println!("score        {} (best combo {})", score.points, score.best_combo);
        println!("wave         {}", stats.wave);
        println!("stars        {}", stats.stars);
        println!("kills        {:?} total {}", stats.kills, stats.total_kills());
        println!("shots        {} (accuracy {:.0}%)", stats.shots, stats.accuracy() * 100.0);
        println!("player hp    {hp}");
        println!("enemies      spawned {} alive {}", census.total_spawned, census.total_alive());
        println!("bullet pool  spawned {} reused {}", pool.spawned, pool.reused);
        println!("entities     {entities}");
    }
}
