#!/usr/bin/env bash
# 运行游戏。
#   ./scripts/run.sh            开发模式
#   ./scripts/run.sh --release  发布模式（更流畅）
#   ./scripts/run.sh --hot      开启资源热重载（改 assets/config/game.cfg.ron 立即生效）
#   ./scripts/run.sh --autoplay AI 自动驾驶（演示模式）
source "$(dirname "$0")/common.sh"
require_cargo

profile=()
features=()
for arg in "$@"; do
  case "$arg" in
    --release)  profile=(--release) ;;
    --hot)      features=(--features hot) ;;
    --autoplay) export BEVY_DEMO_AUTOPLAY=1 ;;
    -h|--help)  sed -n '2,6p' "$0"; exit 0 ;;
    *) die "未知参数: $arg" ;;
  esac
done

"$(find_python)" scripts/gen_assets.py --check >/dev/null || "$(find_python)" scripts/gen_assets.py
info "cargo run ${profile[*]:-} ${features[*]:-}"
exec cargo run --bin bevy_demo01 ${profile[@]+"${profile[@]}"} ${features[@]+"${features[@]}"}
