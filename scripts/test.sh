#!/usr/bin/env bash
# 运行全部测试（单元 + 集成），可选过滤。
#   ./scripts/test.sh              全部
#   ./scripts/test.sh combat       只跑名字含 combat 的测试
#   ./scripts/test.sh --repeat 5   重复 5 次（抓不稳定测试 / 并发时序问题）
source "$(dirname "$0")/common.sh"
require_cargo

repeat=1
filter=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --repeat) repeat="$2"; shift 2 ;;
    *) filter+=("$1"); shift ;;
  esac
done

"$(find_python)" scripts/gen_assets.py --check >/dev/null || die "assets 缺失，先运行 ./scripts/setup.sh"

log="$(mktemp -t bevy_demo01_test.XXXXXX)"
for ((i = 1; i <= repeat; i++)); do
  info "cargo test (round $i/$repeat) ${filter[*]:-}"
  if cargo test --all-targets ${filter[@]+"${filter[@]}"} >"$log" 2>&1; then
    grep -E "Running|test result" "$log" | sed 's/^/    /'
  else
    grep -E "Running|test result|panicked|FAILED|^---- |error" "$log" | sed 's/^/    /'
    die "测试失败（第 $i 轮），完整日志: $log"
  fi
done
total="$(grep -Eo '[0-9]+ passed' "$log" | awk '{s+=$1} END {print s}')"
info "全部通过：$total 个测试 × $repeat 轮"
