#!/usr/bin/env sh
# 测试覆盖率生成脚本
# 使用 cargo-llvm-cov 生成测试覆盖率报告

set -e

printf "%s\n" "🧪 Silent Framework - 测试覆盖率报告生成"
printf "%s\n" "=========================================="
printf "\n"

# 颜色定义
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 检查 cargo-llvm-cov 是否安装
if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    printf "%s\n" "❌ cargo-llvm-cov 未安装"
    printf "%s\n" "请运行: cargo install cargo-llvm-cov"
    exit 1
fi

printf "%b\n" "${GREEN}✅ cargo-llvm-cov 已安装${NC}"
printf "\n"

# 清理之前的覆盖率数据
printf "%b\n" "${BLUE}🧹 清理旧的覆盖率数据...${NC}"
cargo llvm-cov clean --workspace

# 生成 HTML 报告
printf "%b\n" "${BLUE}📊 生成 HTML 覆盖率报告...${NC}"
cargo llvm-cov --all-features --workspace --html

# 生成 JSON 报告
printf "%b\n" "${BLUE}📄 生成 JSON 覆盖率报告...${NC}"
cargo llvm-cov --all-features --workspace --json --output-path coverage.json

# 生成文本摘要
printf "%b\n" "${BLUE}📋 生成覆盖率摘要...${NC}"
cargo llvm-cov --all-features --workspace > coverage-summary.txt

# 显示摘要
printf "\n"
printf "%s\n" "=========================================="
printf "%b\n" "${GREEN}✅ 覆盖率报告生成完成！${NC}"
printf "%s\n" "=========================================="
printf "\n"
cargo llvm-cov --all-features --workspace
printf "\n"
printf "%b\n" "${YELLOW}📁 报告位置:${NC}"
printf "%s\n" "  - HTML 报告: target/llvm-cov/html/index.html"
printf "%s\n" "  - JSON 报告: coverage.json"
printf "%s\n" "  - 文本摘要: coverage-summary.txt"
printf "\n"
printf "%b\n" "${BLUE}💡 提示: 使用 'open target/llvm-cov/html/index.html' 查看 HTML 报告${NC}"
