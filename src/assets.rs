//! 资源管理：文件资源（AssetServer 异步加载）+ 程序化生成资源（CPU 画像素）。
//!
//! 逻辑层只依赖 [`GameAssets`] 里的 `Handle`。无头测试里 `GameAssets::default()` 全是
//! 默认句柄，照样能 spawn 精灵实体 —— 这就是“逻辑与渲染解耦”的关键。
//!
//! 覆盖 API：`AssetServer::load`、`LoadState`、`is_loaded_with_dependencies`、
//! `Image::new_fill` + `set_color_at`、`RenderAssetUsages`、`TextureAtlasLayout::from_grid`、
//! `Assets<Mesh>/<ColorMaterial>/<Image>::add`、`Mesh2d` 原语（`Circle`/`Annulus`/`Rectangle`/
//! `RegularPolygon`/`Capsule2d`）、`ImagePlugin::default_nearest` 配合像素风。

use bevy::asset::{LoadState, RenderAssetUsages, UntypedAssetId};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::config::ConfigHandle;
use crate::state::AppState;

/// 全部游戏资源句柄。
#[derive(Resource, Debug, Default, Clone)]
pub struct GameAssets {
    // 文件资源
    pub ship: Handle<Image>,
    pub ship_layout: Handle<TextureAtlasLayout>,
    pub star: Handle<Image>,
    pub collect_sfx: Handle<AudioSource>,
    pub music: Handle<AudioSource>,
    // 程序化生成的贴图
    pub meteor: Handle<Image>,
    pub hunter: Handle<Image>,
    pub carrier: Handle<Image>,
    pub plate: Handle<Image>,
    pub powerup: Handle<Image>,
    pub heart: Handle<Image>,
    pub spark: Handle<Image>,
    // 网格与材质
    pub bullet_mesh: Handle<Mesh>,
    pub missile_mesh: Handle<Mesh>,
    pub shield_mesh: Handle<Mesh>,
    pub bullet_material: Handle<ColorMaterial>,
    pub missile_material: Handle<ColorMaterial>,
    pub shield_material: Handle<ColorMaterial>,
}

/// 飞船精灵表规格（与 `scripts/gen_assets.py` 保持一致）。
pub const SHIP_FRAME: UVec2 = UVec2::new(32, 32);
pub const SHIP_FRAMES: u32 = 4;

/// 加载进度（给 Loading 界面显示）。
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct LoadingProgress {
    pub loaded: usize,
    pub total: usize,
    pub failed: usize,
}

impl LoadingProgress {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 { 1.0 } else { self.loaded as f32 / self.total as f32 }
    }
    pub fn is_done(&self) -> bool {
        self.loaded + self.failed >= self.total
    }
}

// ---------------------------------------------------------------------------
// 程序化贴图：纯 CPU 像素运算（可单测）
// ---------------------------------------------------------------------------

fn blank(size: u32) -> Image {
    Image::new_fill(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        // 同时保留在主世界：方便 CPU 侧读取（测试 / 拾取的 alpha 判定）
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

/// 用一个 `f(uv) -> Option<Color>` 画整张图，uv ∈ [-1,1]²，y 轴向上。
pub fn paint(size: u32, f: impl Fn(Vec2) -> Option<Color>) -> Image {
    let mut img = blank(size);
    for y in 0..size {
        for x in 0..size {
            let uv = Vec2::new((x as f32 + 0.5) / size as f32 * 2.0 - 1.0, 1.0 - (y as f32 + 0.5) / size as f32 * 2.0);
            if let Some(c) = f(uv) {
                img.set_color_at(x, y, c).expect("in bounds");
            }
        }
    }
    img
}

/// 点是否在正多边形内（顶点朝上）。
pub fn inside_regular_polygon(p: Vec2, sides: u32, radius: f32) -> bool {
    let sides = sides.max(3) as f32;
    let angle = ops::atan2(p.x, p.y);
    let sector = std::f32::consts::TAU / sides;
    let a = (angle.rem_euclid(sector)) - sector / 2.0;
    let apothem = radius * ops::cos(sector / 2.0);
    p.length() * ops::cos(a) <= apothem
}

/// 经典心形隐式方程 (x²+y²−1)³ − x²y³ ≤ 0。
pub fn inside_heart(p: Vec2) -> bool {
    let p = p * 1.25 + Vec2::new(0.0, 0.1);
    let a = p.x * p.x + p.y * p.y - 1.0;
    a * a * a - p.x * p.x * p.y * p.y * p.y <= 0.0
}

pub fn meteor_image() -> Image {
    paint(48, |uv| {
        // 用角度扰动半径，得到不规则岩石边缘
        let ang = ops::atan2(uv.y, uv.x);
        let r = 0.82 + 0.1 * ops::sin(ang * 5.0) + 0.05 * ops::cos(ang * 11.0);
        let d = uv.length();
        (d <= r).then(|| {
            let crater =
                ((uv - Vec2::new(0.25, 0.2)).length() < 0.18) || ((uv - Vec2::new(-0.3, -0.25)).length() < 0.13);
            let shade = 0.55 + 0.35 * (1.0 - d / r) - if crater { 0.18 } else { 0.0 };
            Color::srgb(shade * 0.9, shade * 0.75, shade * 0.6)
        })
    })
}

pub fn hunter_image() -> Image {
    paint(32, |uv| {
        inside_regular_polygon(uv, 3, 0.95).then(|| {
            let core = uv.length() < 0.3;
            if core { Color::srgb(1.0, 0.95, 0.4) } else { Color::srgb(0.95, 0.2, 0.35) }
        })
    })
}

pub fn carrier_image() -> Image {
    paint(64, |uv| {
        inside_regular_polygon(uv, 6, 0.95).then(|| {
            let ring = (uv.length() - 0.5).abs() < 0.06;
            if ring { Color::srgb(0.9, 0.5, 1.0) } else { Color::srgb(0.35, 0.15, 0.55) }
        })
    })
}

pub fn plate_image() -> Image {
    paint(24, |uv| {
        (uv.x.abs() < 0.8 && uv.y.abs() < 0.8).then(|| {
            let edge = uv.x.abs() > 0.6 || uv.y.abs() > 0.6;
            if edge { Color::srgb(0.75, 0.75, 0.85) } else { Color::srgb(0.45, 0.45, 0.55) }
        })
    })
}

pub fn powerup_image() -> Image {
    paint(28, |uv| {
        // 菱形，白色（运行时用 Sprite.color 染色）
        (uv.x.abs() + uv.y.abs() <= 0.95).then(|| {
            let inner = uv.x.abs() + uv.y.abs() <= 0.55;
            if inner { Color::WHITE } else { Color::srgb(0.8, 0.8, 0.8) }
        })
    })
}

pub fn heart_image() -> Image {
    paint(32, |uv| inside_heart(uv).then_some(Color::srgb(1.0, 0.25, 0.35)))
}

pub fn spark_image() -> Image {
    paint(8, |uv| {
        let d = uv.length();
        (d <= 1.0).then(|| Color::srgba(1.0, 1.0, 1.0, (1.0 - d).powf(0.7)))
    })
}

// ---------------------------------------------------------------------------
// 插件
// ---------------------------------------------------------------------------

/// 表现层：真正去加载/生成资源，并在全部完成后切到 Menu。
pub struct AssetLoadingPlugin;

impl Plugin for AssetLoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingProgress>()
            .add_systems(Startup, load_game_assets)
            .add_systems(Update, track_loading.run_if(in_state(AppState::Loading)));
    }
}

pub fn load_game_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let layout = TextureAtlasLayout::from_grid(SHIP_FRAME, SHIP_FRAMES, 1, None, None);
    commands.insert_resource(GameAssets {
        ship: asset_server.load("textures/ship_sheet.png"),
        ship_layout: layouts.add(layout),
        star: asset_server.load("textures/star.png"),
        collect_sfx: asset_server.load("audio/collect.wav"),
        music: asset_server.load("audio/music.wav"),
        meteor: images.add(meteor_image()),
        hunter: images.add(hunter_image()),
        carrier: images.add(carrier_image()),
        plate: images.add(plate_image()),
        powerup: images.add(powerup_image()),
        heart: images.add(heart_image()),
        spark: images.add(spark_image()),
        bullet_mesh: meshes.add(Capsule2d::new(2.5, 10.0)),
        missile_mesh: meshes.add(RegularPolygon::new(6.0, 3)),
        shield_mesh: meshes.add(Annulus::new(24.0, 27.0)),
        // HDR 颜色（分量 > 1）配合 Bloom 会发光
        bullet_material: materials.add(Color::srgb(4.0, 3.2, 0.8)),
        missile_material: materials.add(Color::srgb(4.0, 0.8, 3.0)),
        shield_material: materials.add(Color::srgba(0.4, 1.6, 3.0, 0.55)),
    });
}

/// 统计需要等待的文件资源的加载状态。
pub fn track_loading(
    asset_server: Res<AssetServer>,
    assets: Option<Res<GameAssets>>,
    config: Option<Res<ConfigHandle>>,
    mut progress: ResMut<LoadingProgress>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Some(assets) = assets else { return };
    let mut ids: Vec<UntypedAssetId> = vec![
        assets.ship.id().untyped(),
        assets.star.id().untyped(),
        assets.collect_sfx.id().untyped(),
        assets.music.id().untyped(),
    ];
    if let Some(cfg) = config {
        ids.push(cfg.0.id().untyped());
    }
    let mut p = LoadingProgress { total: ids.len(), ..default() };
    for id in ids {
        match asset_server.load_state(id) {
            LoadState::Loaded => p.loaded += 1,
            LoadState::Failed(err) => {
                warn!("资源加载失败（使用默认占位继续）: {err}");
                p.failed += 1;
            }
            _ => {}
        }
    }
    if *progress != p {
        *progress = p;
    }
    if p.is_done() {
        info!("资源加载完成: {}/{} (失败 {})", p.loaded, p.total, p.failed);
        next.set(AppState::Menu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(img: &Image, x: u32, y: u32) -> f32 {
        img.get_color_at(x, y).unwrap().alpha()
    }

    #[test]
    fn generated_images_have_expected_size_and_transparent_corners() {
        for (img, size) in [
            (meteor_image(), 48),
            (hunter_image(), 32),
            (carrier_image(), 64),
            (plate_image(), 24),
            (powerup_image(), 28),
            (heart_image(), 32),
            (spark_image(), 8),
        ] {
            assert_eq!(img.width(), size);
            assert_eq!(img.height(), size);
            assert_eq!(alpha_at(&img, 0, 0), 0.0, "角落应透明，精灵拾取才准");
            let c = size / 2;
            assert!(alpha_at(&img, c, c) > 0.5, "中心应不透明");
        }
    }

    #[test]
    fn polygon_membership() {
        assert!(inside_regular_polygon(Vec2::ZERO, 3, 1.0));
        assert!(inside_regular_polygon(Vec2::new(0.0, 0.9), 3, 1.0), "顶点朝上");
        assert!(!inside_regular_polygon(Vec2::new(0.0, -0.9), 3, 1.0), "底边内切距只有 0.5");
        assert!(inside_regular_polygon(Vec2::new(0.8, 0.0), 6, 1.0));
        assert!(!inside_regular_polygon(Vec2::new(2.0, 0.0), 6, 1.0));
        // sides < 3 被夹到 3
        assert_eq!(
            inside_regular_polygon(Vec2::new(0.0, 0.9), 1, 1.0),
            inside_regular_polygon(Vec2::new(0.0, 0.9), 3, 1.0)
        );
    }

    #[test]
    fn heart_membership() {
        assert!(inside_heart(Vec2::ZERO));
        assert!(!inside_heart(Vec2::new(0.95, 0.95)));
        assert!(!inside_heart(Vec2::new(0.0, -0.99)));
    }

    #[test]
    fn loading_progress_math() {
        let p = LoadingProgress { loaded: 1, total: 4, failed: 0 };
        assert_eq!(p.fraction(), 0.25);
        assert!(!p.is_done());
        let p = LoadingProgress { loaded: 3, total: 4, failed: 1 };
        assert!(p.is_done(), "失败也算完成，避免卡死在 Loading");
        assert_eq!(LoadingProgress::default().fraction(), 1.0);
    }

    #[test]
    fn atlas_layout_has_all_frames() {
        let layout = TextureAtlasLayout::from_grid(SHIP_FRAME, SHIP_FRAMES, 1, None, None);
        assert_eq!(layout.len(), SHIP_FRAMES as usize);
        assert_eq!(layout.size, UVec2::new(SHIP_FRAME.x * SHIP_FRAMES, SHIP_FRAME.y));
    }
}
