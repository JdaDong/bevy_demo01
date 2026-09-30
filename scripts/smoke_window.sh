#!/usr/bin/env bash
# 窗口版冒烟：真实渲染 + 音频 + UI，AI 自动驾驶，自动截图后退出，并校验截图不是全黑。
# 产物：screenshots/autoplay-menu.png、screenshots/autoplay-game.png
# 需要图形环境（macOS / 桌面 Linux）。无显示器的 CI 请用 ./scripts/sim.sh。
#
# 注意：macOS 在“显示器休眠 / 锁屏 / 窗口被完全遮挡”时不会真正合成窗口内容，截图会是全黑；
# 脚本会自动重试 3 次，仍全黑时给出提示而不是误报渲染 bug。
source "$(dirname "$0")/common.sh"
require_cargo
PY="$(find_python)"

cargo build --bin bevy_demo01 -q
mkdir -p screenshots

run_case() {
  local mode="$1" shot="$2" exit_after="$3" name="$4"
  local log="/tmp/bevy_demo01_${name}.log" png="screenshots/autoplay-${name}.png"
  for attempt in 1 2 3; do
    rm -f "$png"
    info "[$name] attempt $attempt: screenshot@${shot}s exit@${exit_after}s"
    BEVY_DEMO_AUTOPLAY="$mode" BEVY_DEMO_SCREENSHOT_AT="$shot" BEVY_DEMO_EXIT_AFTER="$exit_after" \
      ./target/debug/bevy_demo01 >"$log" 2>&1 || die "进程非 0 退出，见 $log"
    if grep -qiE "panicked| ERROR " "$log"; then
      grep -iE "panicked| ERROR " "$log" | head -20
      die "日志中有错误: $log"
    fi
    if [[ -s "$png" ]]; then
      luma="$("$PY" scripts/gen_assets.py --luma "$png")"
      if awk -v l="$luma" 'BEGIN { exit !(l > 0.5) }'; then
        info "[$name] OK $png  (mean luma $luma)"
        return 0
      fi
      warn "[$name] 截图全黑 (luma $luma) —— 窗口可能被遮挡或显示器休眠，重试"
    else
      warn "[$name] 未生成截图，重试"
    fi
  done
  die "[$name] 3 次都拿不到有效截图：请确认屏幕已唤醒且未锁屏后重试"
}

run_case menu 4 6 menu
run_case 1 10 13 game
info "窗口冒烟通过"
