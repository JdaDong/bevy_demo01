#!/usr/bin/env bash
# 无头模拟：多个 seed 批量跑，输出 JSON 行，便于数值平衡分析。
#   ./scripts/sim.sh                 默认 5 个 seed × 3600 帧
#   ./scripts/sim.sh 20 7200         20 个 seed × 7200 帧
source "$(dirname "$0")/common.sh"
require_cargo

seeds="${1:-5}"
frames="${2:-3600}"
cargo build --bin headless_sim -q
for ((s = 1; s <= seeds; s++)); do
  ./target/debug/headless_sim --frames "$frames" --seed "$s" --json
done
