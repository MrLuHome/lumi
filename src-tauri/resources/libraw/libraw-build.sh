#!/bin/sh
# 可复现构建 LibRaw 静态库（LGPL-2.1 重链接材料）
#
# 用法：
#   ./libraw-build.sh /path/to/libraw_rs_vendor-1.0.0 [输出目录]
#
# 产物：raw.a / raw_r.a（与 build.rs 相同的编译选项），供第三方
# 用修改后的源码重新生成静态库，再按 RELINK.md 重链接 LUMI。
set -eu

SRC="${1:-}"
OUT="${2:-./libraw-out}"

if [ -z "$SRC" ] || [ ! -d "$SRC/libraw" ]; then
  echo "用法：$0 <解压后的 libraw_rs_vendor-1.0.0 目录> [输出目录]" >&2
  exit 2
fi

CXX="${CXX:-c++}"
mkdir -p "$OUT/obj"

# 与 libraw_rs_vendor-1.0.0/build.rs 的 LIBRAW_FILES 一致
FILES=$(cd "$SRC" && find libraw/src -name '*.cpp' | sort)

compile_one() {
  file="$1"
  obj="$OUT/obj/$(echo "$file" | tr '/' '_').o"
  "$CXX" -std=c++11 -O2 -fPIC -pthread \
    -I"$SRC/libraw" \
    -Wno-deprecated-declarations -Wno-format-truncation \
    -Wno-unused-result -Wno-format-overflow \
    -c "$SRC/$file" -o "$obj"
}

for file in $FILES; do
  compile_one "$file"
done

ar rcs "$OUT/raw.a" "$OUT"/obj/*.o
cp "$OUT/raw.a" "$OUT/raw_r.a"

echo "已生成：$OUT/raw.a 与 $OUT/raw_r.a"
echo "下一步见 RELINK.md：用 [patch.crates-io] 指向修改后的源码并 cargo build --release。"
