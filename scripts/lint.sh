#!/usr/bin/env bash
# 格式 + clippy（警告视为错误）。  ./scripts/lint.sh --fix 自动修复格式
source "$(dirname "$0")/common.sh"
require_cargo

if [[ "${1:-}" == "--fix" ]]; then
  info "cargo fmt"
  cargo fmt --all
else
  info "cargo fmt --check"
  cargo fmt --all -- --check || die "格式不符合，运行 ./scripts/lint.sh --fix"
fi

info "cargo clippy -D warnings"
cargo clippy --all-targets -- -D warnings
info "lint 通过"
