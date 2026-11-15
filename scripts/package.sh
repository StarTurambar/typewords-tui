#!/usr/bin/env bash
# 构建并打包 release：可执行文件 + assets/ + docs/ + README.md + LICENSE → dist/<名称>.tar.gz 与同名 .sha256。
# Windows 上对应 scripts/package.ps1，两者产出同一套归档名与校验格式。
# 用法：bash scripts/package.sh [--target <triple>] [--skip-build]
#   --target      构建目标（target triple），缺省为当前宿主
#   --skip-build  复用已构建的产物，只重新打包
set -euo pipefail

usage() {
  sed -n '2,6p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

target=""
skip_build=0
while [ $# -gt 0 ]; do
  case "$1" in
    -t|--target)
      target="${2:-}"
      [ -n "$target" ] || { echo "--target 需要一个 target triple" >&2; exit 2; }
      shift 2
      ;;
    --skip-build) skip_build=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "未知参数：$1（用 --help 查看用法）" >&2; exit 2 ;;
  esac
done

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

if [ -z "$target" ]; then
  target=$(rustc -vV | sed -n 's/^host: //p')
  [ -n "$target" ] || { echo "无法确定构建目标，请用 --target 指定" >&2; exit 1; }
fi

case "$target" in
  *-pc-windows-*) bin=tw.exe ;;
  *) bin=tw ;;
esac

version=$(tr -d '\r' < Cargo.toml | sed -n 's/^version = "\(.*\)"/\1/p' | head -n 1)
[ -n "$version" ] || { echo "无法从 Cargo.toml 读取 version" >&2; exit 1; }

if [ "$skip_build" -eq 0 ]; then
  cargo build --release --locked --target "$target"
fi

binary="target/$target/release/$bin"
if [ ! -f "$binary" ]; then
  echo "找不到构建产物：$binary（先不带 --skip-build 跑一次）" >&2
  exit 1
fi

name="typewords-tui-v$version-$target"
archive="dist/$name.tar.gz"
stage="dist/$name"
rm -rf "$stage" "$archive" "$archive.sha256"
mkdir -p "$stage"

cp "$binary" "$stage/$bin"
if [ "$bin" = tw ]; then
  chmod +x "$stage/$bin"
fi
cp -R assets docs "$stage"
cp README.md LICENSE "$stage"

# 归档内保留顶层目录，解压后得到 <name>/，不会把文件散到当前目录
tar -czf "$archive" -C dist "$name"

# 自检：归档可读且关键条目齐全，避免产出空壳包。
# 先把清单落到临时文件再 grep：`tar | grep -q` 会因 grep 提前退出触发 tar 的 SIGPIPE，
# 在 `set -o pipefail` 下被误判成失败。
listing=$(mktemp)
trap 'rm -f "$listing"' EXIT
tar -tzf "$archive" > "$listing"
for entry in "$name/$bin" "$name/assets/list/word.json" "$name/README.md" "$name/LICENSE"; do
  grep -qxF -- "$entry" "$listing" || { echo "归档缺少条目：$entry" >&2; exit 1; }
done

# 暂存目录只用于打包，自检通过后删掉，dist/ 只留归档与校验文件
rm -rf "$stage"

if command -v sha256sum >/dev/null 2>&1; then
  hash=$(sha256sum "$archive" | cut -d' ' -f1)
else
  hash=$(shasum -a 256 "$archive" | cut -d' ' -f1)
fi
printf '%s  %s\n' "$hash" "$name.tar.gz" > "$archive.sha256"

echo "打包完成：$archive"
echo "校验文件：$archive.sha256"
echo "SHA256：$hash"
