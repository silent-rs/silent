#!/usr/bin/env bash
# 测试覆盖率生成脚本
# 使用 cargo-llvm-cov 生成测试覆盖率报告

set -e

echo "🧪 Silent Framework - 测试覆盖率报告生成"
echo "=========================================="
echo ""

# 颜色定义
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 检查 cargo-llvm-cov 是否安装
if ! command -v cargo-llvm-cov &> /dev/null; then
    echo "❌ cargo-llvm-cov 未安装"
    echo "请运行: cargo install cargo-llvm-cov"
    exit 1
fi

echo -e "${GREEN}✅ cargo-llvm-cov 已安装${NC}"
echo ""

# 清理之前的覆盖率数据
echo -e "${BLUE}🧹 清理旧的覆盖率数据...${NC}"
cargo llvm-cov clean --workspace

# 生成 HTML 报告
echo -e "${BLUE}📊 生成 HTML 覆盖率报告...${NC}"
cargo llvm-cov --all-features --workspace --html

# 生成 JSON 报告
echo -e "${BLUE}📄 生成 JSON 覆盖率报告...${NC}"
cargo llvm-cov --all-features --workspace --json --output-path coverage.json

# 生成文本摘要
echo -e "${BLUE}📋 生成覆盖率摘要...${NC}"
cargo llvm-cov --all-features --workspace > coverage-summary.txt

# 显示摘要
echo ""
echo "=========================================="
echo -e "${GREEN}✅ 覆盖率报告生成完成！${NC}"
echo "=========================================="
echo ""
cargo llvm-cov --all-features --workspace
echo ""
echo -e "${YELLOW}📁 报告位置:${NC}"
echo "  - HTML 报告: target/llvm-cov/html/index.html"
echo "  - JSON 报告: coverage.json"
echo "  - 文本摘要: coverage-summary.txt"
echo ""
echo -e "${BLUE}💡 提示: 使用 'open target/llvm-cov/html/index.html' 查看 HTML 报告${NC}"
