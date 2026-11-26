.PHONY: help dev run migrate status entity check test clean

help:
	@echo "make dev       构建并启动，改代码自动重启（需要 cargo install silent-boot-cli）"
	@echo "make run       构建并启动"
	@echo "make migrate   执行迁移"
	@echo "make status    查看迁移状态"
	@echo "make entity    按库结构重新生成实体"
	@echo "make check     格式、clippy、测试"
	@echo "make clean     删除构建产物"

dev:
	silent-boot dev

run: migrate
	cargo run

migrate:
	cargo run -- migrate up

status:
	cargo run -- migrate status

entity:
	cargo run --features codegen -- generate entity

check:
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
	cargo nextest run

test:
	cargo test

clean:
	cargo clean
