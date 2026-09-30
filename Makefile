# 便捷入口：make <target>
.PHONY: setup assets run release-run hot autoplay test lint fmt sim smoke release ci clean

setup:        ## 环境自检 + 生成资源 + 预编译
	./scripts/setup.sh
assets:       ## 重新生成 PNG/WAV 资源
	python3 scripts/gen_assets.py
run:          ## 开发模式运行
	./scripts/run.sh
release-run:  ## 发布模式运行
	./scripts/run.sh --release
hot:          ## 资源热重载
	./scripts/run.sh --hot
autoplay:     ## AI 自动驾驶演示
	./scripts/run.sh --autoplay
test:         ## 单元 + 集成测试
	./scripts/test.sh
lint:         ## fmt --check + clippy -D warnings
	./scripts/lint.sh
fmt:          ## 自动格式化
	./scripts/lint.sh --fix
sim:          ## 无头批量模拟
	./scripts/sim.sh
smoke:        ## 窗口版自动冒烟 + 截图
	./scripts/smoke_window.sh
release:      ## 打包 dist/
	./scripts/release.sh
ci:           ## 本地完整 CI
	./scripts/ci.sh
clean:
	cargo clean && rm -rf dist saves screenshots
help:
	@grep -E '^[a-z-]+:.*##' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-12s %s\n", $$1, $$2}'
