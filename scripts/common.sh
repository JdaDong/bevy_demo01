#!/usr/bin/env bash
# 公共函数：被其它脚本 source。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

c_green=$'\033[32m'; c_red=$'\033[31m'; c_yellow=$'\033[33m'; c_reset=$'\033[0m'
info()  { printf '%s==>%s %s\n' "$c_green" "$c_reset" "$*"; }
warn()  { printf '%s[warn]%s %s\n' "$c_yellow" "$c_reset" "$*"; }
die()   { printf '%s[fail]%s %s\n' "$c_red" "$c_reset" "$*" >&2; exit 1; }

# 优先使用 WorkBuddy 托管 Python，其次系统 python3
find_python() {
  local managed="$HOME/.workbuddy/binaries/python/versions/3.13.12/bin/python3"
  if [[ -x "$managed" ]]; then echo "$managed"; return; fi
  command -v python3 || die "未找到 python3"
}

require_cargo() {
  command -v cargo >/dev/null || die "未找到 cargo，请先安装 Rust: https://rustup.rs"
}
