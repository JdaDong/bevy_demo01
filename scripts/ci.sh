#!/usr/bin/env bash
# 本地 CI：资源校验 → lint → 测试 → 无头模拟。任一步失败即退出。
source "$(dirname "$0")/common.sh"
require_cargo

info "[1/4] assets"
"$(find_python)" scripts/gen_assets.py --check
info "[2/4] lint"
./scripts/lint.sh
info "[3/4] test"
./scripts/test.sh
info "[4/4] headless sim"
./scripts/sim.sh 3 1800
info "CI 全部通过 ✅"
