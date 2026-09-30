#!/usr/bin/env bash
# 发布构建并打包成 dist/star-harvester-<os>-<arch>.tar.gz（可执行文件 + assets）。
source "$(dirname "$0")/common.sh"
require_cargo

"$(find_python)" scripts/gen_assets.py --check >/dev/null || "$(find_python)" scripts/gen_assets.py
info "cargo build --release"
cargo build --release --bin bevy_demo01

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"
name="star-harvester-${os}-${arch}"
out="dist/$name"
rm -rf "$out"
mkdir -p "$out"
cp target/release/bevy_demo01 "$out/"
cp -R assets "$out/"
cp README.md "$out/"
tar -C dist -czf "dist/$name.tar.gz" "$name"
info "产物: dist/$name.tar.gz ($(du -h "dist/$name.tar.gz" | cut -f1))"
info "运行: cd $out && ./bevy_demo01"
