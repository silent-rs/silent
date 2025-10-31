#!/usr/bin/env bash
# 代码质量检查脚本
# 运行 Clippy、格式检查和文档检查

set -e

echo "🔍 Silent Framework - 代码质量检查"
echo "===================================="
echo ""

# 颜色定义
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

ERRORS=0

# 1. 格式检查
echo -e "${BLUE}📝 检查代码格式...${NC}"
if cargo fmt -- --check; then
    echo -e "${GREEN}✅ 格式检查通过${NC}"
else
    echo -e "${RED}❌ 格式检查失败${NC}"
    echo -e "${YELLOW}💡 运行 'cargo fmt' 修复格式问题${NC}"
    ERRORS=$((ERRORS + 1))
fi
echo ""

# 2. Clippy 检查（所有 features）
echo -e "${BLUE}🔧 运行 Clippy 检查（所有 features）...${NC}"
if cargo clippy --all-targets --all-features --tests --benches -- -D warnings 2>&1 | tee clippy-output.txt; then
    echo -e "${GREEN}✅ Clippy 检查通过${NC}"
else
    echo -e "${RED}❌ Clippy 检查发现问题${NC}"
    echo -e "${YELLOW}💡 详细信息已保存到 clippy-output.txt${NC}"
    ERRORS=$((ERRORS + 1))
fi
echo ""

# 3. Clippy 检查（默认 features）
echo -e "${BLUE}🔧 运行 Clippy 检查（默认 features）...${NC}"
if cargo clippy --all-targets --tests --benches -- -D warnings; then
    echo -e "${GREEN}✅ Clippy 检查通过（默认 features）${NC}"
else
    echo -e "${RED}❌ Clippy 检查发现问题（默认 features）${NC}"
    ERRORS=$((ERRORS + 1))
fi
echo ""

# 4. 文档检查
echo -e "${BLUE}📚 检查文档生成...${NC}"
if cargo doc --all-features --no-deps --document-private-items 2>&1 | tee doc-output.txt; then
    echo -e "${GREEN}✅ 文档生成成功${NC}"
else
    echo -e "${RED}❌ 文档生成失败${NC}"
    echo -e "${YELLOW}💡 详细信息已保存到 doc-output.txt${NC}"
    ERRORS=$((ERRORS + 1))
fi
echo ""

# 5. 依赖检查
echo -e "${BLUE}🔗 检查未使用的依赖...${NC}"
if command -v cargo-udeps &> /dev/null; then
    if cargo +nightly udeps --all-features; then
        echo -e "${GREEN}✅ 没有未使用的依赖${NC}"
    else
        echo -e "${YELLOW}⚠️  发现未使用的依赖${NC}"
    fi
else
    echo -e "${YELLOW}⚠️  cargo-udeps 未安装，跳过此检查${NC}"
    echo -e "${YELLOW}💡 安装: cargo install cargo-udeps${NC}"
fi
echo ""

# 6. 检查编译（所有 features）
echo -e "${BLUE}🔨 检查编译（所有 features）...${NC}"
if cargo check --all --all-features; then
    echo -e "${GREEN}✅ 编译检查通过${NC}"
else
    echo -e "${RED}❌ 编译检查失败${NC}"
    ERRORS=$((ERRORS + 1))
fi
echo ""

# 总结
echo "===================================="
if [ $ERRORS -eq 0 ]; then
    echo -e "${GREEN}✅ 所有质量检查通过！${NC}"
    echo ""
    exit 0
else
    echo -e "${RED}❌ 发现 $ERRORS 个问题${NC}"
    echo ""
    echo -e "${YELLOW}📁 输出文件:${NC}"
    echo "  - clippy-output.txt - Clippy 详细输出"
    echo "  - doc-output.txt - 文档生成输出"
    echo ""
    exit 1
fi
