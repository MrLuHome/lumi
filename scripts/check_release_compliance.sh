#!/bin/sh
# 发行合规自动检查（LibRaw / LGPL-2.1）
#
# 用法：./scripts/check_release_compliance.sh [--app <path.app>] [--dmg <path.dmg>] [--nsis <path.exe>]
#
# 退出码：
#   0  材料齐备且产物包含全部合规附件（仍需人工法务审核）
#   1  材料缺失 / 产物不含附件 / 法律状态未完成 → 输出 BLOCKED
#
# 本脚本永远输出 LEGAL_REVIEW_PENDING：自动检查不能替代法律审核，
# 任何情况下都不会打印 "Release Ready" 或 "COMPLIANT"。
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
LIBRAW_DIR="$ROOT/src-tauri/resources/libraw"

APP=""
DMG=""
NSIS=""
while [ $# -gt 0 ]; do
  case "$1" in
    --app) APP="$2"; shift 2 ;;
    --dmg) DMG="$2"; shift 2 ;;
    --nsis) NSIS="$2"; shift 2 ;;
    *) echo "未知参数：$1" >&2; exit 2 ;;
  esac
done

if [ -z "$APP" ]; then
  APP=$(ls -d "$ROOT"/src-tauri/target/release/bundle/macos/*.app 2>/dev/null | head -1)
fi
if [ -z "$DMG" ]; then
  DMG=$(ls "$ROOT"/src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1)
fi

FAIL=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1"; FAIL=1; }
skip() { printf 'SKIP  %s\n' "$1"; }

echo "== LUMI 发行合规检查 =="
echo "仓库：$ROOT"
echo

echo "-- LibRaw 版本与许可 --"
LOCK=$(grep -A2 'name = "libraw_rs_vendor"' "$ROOT/src-tauri/Cargo.lock" 2>/dev/null | grep '^version' | head -1 | cut -d'"' -f2)
if [ "$LOCK" = "1.0.0" ]; then
  pass "Cargo.lock 锁定 libraw_rs_vendor 1.0.0（内置 LibRaw 0.21.1）"
else
  fail "Cargo.lock 中 libraw_rs_vendor 版本为 '${LOCK:-未找到}'，与材料登记不一致"
fi

for f in LICENSE.LGPL-2.1 LICENSE.CDDL-1.0 COPYRIGHT; do
  path="$LIBRAW_DIR/$f"
  if [ -f "$path" ] && [ "$(wc -c < "$path" | tr -d ' ')" -gt 500 ]; then
    pass "许可/版权文件存在：$f"
  else
    fail "许可/版权文件缺失或内容异常：$f"
  fi
done

echo
echo "-- 源码与重链接材料 --"
TGZ="$LIBRAW_DIR/libraw_rs_vendor-1.0.0-source.tar.gz"
if [ -f "$TGZ" ]; then
  ACTUAL=$(shasum -a 256 "$TGZ" | cut -d' ' -f1)
  DECLARED=$(grep -o '58317a2875f310a48cfbe03d92a178e148e32a3d032462eee559d8abcba924f9' \
    "$LIBRAW_DIR/SOURCE.md" 2>/dev/null | head -1)
  if [ "$ACTUAL" = "$DECLARED" ]; then
    pass "对应源码包存在且 SHA256 一致（${ACTUAL}）"
  else
    fail "源码包 SHA256 不一致：实际 ${ACTUAL}，登记 ${DECLARED:-未登记}"
  fi
else
  fail "缺少对应源码包 libraw_rs_vendor-1.0.0-source.tar.gz"
fi

for f in SOURCE.md RELINK.md MODIFICATIONS.md libraw-build.sh; do
  if [ -f "$LIBRAW_DIR/$f" ]; then
    pass "材料存在：$f"
  else
    fail "材料缺失：$f"
  fi
done

echo
echo "-- macOS 产物 --"
if [ -n "$APP" ] && [ -d "$APP" ]; then
  pass "找到 .app：$APP"
  BUNDLED="$APP/Contents/Resources/resources/libraw"
  MISSING=""
  for f in libraw_rs_vendor-1.0.0-source.tar.gz LICENSE.LGPL-2.1 LICENSE.CDDL-1.0 COPYRIGHT RELINK.md SOURCE.md MODIFICATIONS.md libraw-build.sh; do
    [ -f "$BUNDLED/$f" ] || MISSING="$MISSING $f"
  done
  if [ -z "$MISSING" ]; then
    pass ".app 内包含全部 LibRaw 合规附件"
  else
    fail ".app 内缺少合规附件:${MISSING}（需重新打包）"
  fi
else
  fail "未找到 .app 产物（先执行 npm run app:build）"
fi

if [ -n "$DMG" ] && [ -f "$DMG" ]; then
  if [ -z "$APP" ] || [ "$DMG" -ot "$APP" ]; then
    fail "DMG 早于 .app，可能未包含最新合规附件：$DMG"
  else
    pass "DMG 存在且不早于 .app：$DMG"
  fi
else
  fail "未找到 DMG 产物"
fi

echo
echo "-- Windows 产物 --"
if [ -n "$NSIS" ] && [ -f "$NSIS" ]; then
  pass "找到 NSIS 安装包：${NSIS}（内容需在 Windows 实机复核）"
else
  skip "未提供 Windows NSIS 安装包：跨平台合规未验证"
fi

echo
echo "== 结论 =="
if [ "$FAIL" -ne 0 ]; then
  echo "RESULT: BLOCKED"
  echo "原因：材料或产物检查未通过，禁止发布"
else
  echo "RESULT: BLOCKED"
  echo "原因：自动检查通过，但法律审核未完成（LEGAL_REVIEW_PENDING）"
fi
echo "法律状态：LEGAL_REVIEW_PENDING（自动检查不构成法律意见，不得标记 COMPLIANT）"
exit 1
