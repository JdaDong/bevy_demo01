#!/usr/bin/env bash
# 环境自检 + 生成资源 + 预编译依赖（首次约 10~30 分钟，之后增量几十秒）。
source "$(dirname "$0")/common.sh"

require_cargo
info "rustc: $(rustc --version)"
info "cargo: $(cargo --version)"

need="1.95"
have="$(rustc --version | awk '{print $2}')"
if [[ "$(printf '%s\n%s\n' "$need" "$have" | sort -V | head -1)" != "$need" ]]; then
  die "Bevy 0.19 需要 rustc >= $need，当前 $have。运行: rustup update stable"
fi

if [[ "$(uname)" == "Linux" ]]; then
  warn "Linux 需要系统库：libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev（Debian/Ubuntu）"
fi

PY="$(find_python)"
info "python: $PY"
if "$PY" scripts/gen_assets.py --check >/dev/null 2>&1; then
  info "assets 已存在，跳过生成（强制重建: $PY scripts/gen_assets.py）"
else
  info "生成 assets ..."
  "$PY" scripts/gen_assets.py
fi

info "预编译依赖（dev profile，依赖 opt-level=3）..."
cargo build --all-targets
info "完成。运行游戏: ./scripts/run.sh"
