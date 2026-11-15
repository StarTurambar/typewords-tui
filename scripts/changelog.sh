#!/usr/bin/env bash
# 从 CHANGELOG.md 取某个版本的发布说明正文。
#
# 发布 workflow（.github/workflows/release.yml）用它生成 GitHub Release 正文，
# 本地也可以用它预览：bash scripts/changelog.sh 0.3.3
#
# 用法：bash scripts/changelog.sh <version> [--file FILE]
#   <version>   x.y.z（可带前导 v），或 unreleased 表示 [Unreleased] 小节
#   --file FILE 变更日志路径，默认是仓库根的 CHANGELOG.md
#
# 退出码：0 成功；1 找不到该版本、该小节没有任何条目，或变更日志不存在；2 参数错误。
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
file="$script_dir/../CHANGELOG.md"
version=""

usage() {
  cat >&2 <<'USAGE'
用法：bash scripts/changelog.sh <version> [--file FILE]

从 CHANGELOG.md 取出某个版本的发布说明正文（标题行之后的正文，去掉首尾空行）。
  <version>   x.y.z（可带前导 v），或 unreleased 表示 [Unreleased] 小节
  --file FILE 变更日志路径，默认是仓库根的 CHANGELOG.md

退出码：0 成功；1 找不到该版本或该小节没有任何条目；2 参数错误。
发布 workflow 用它生成 GitHub Release 正文；本地预览：bash scripts/changelog.sh 0.3.3
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --file)
      [ $# -ge 2 ] || { usage; exit 2; }
      file="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      printf '未知参数：%s\n' "$1" >&2
      usage
      exit 2
      ;;
    *)
      if [ -n "$version" ]; then
        printf '只接受一个版本号，多余的参数：%s\n' "$1" >&2
        usage
        exit 2
      fi
      version="$1"
      shift
      ;;
  esac
done

if [ -z "$version" ]; then
  usage
  exit 2
fi

if [ ! -f "$file" ]; then
  printf '找不到变更日志：%s\n' "$file" >&2
  exit 1
fi

# 标题有两种：`## [0.3.3] - 2026-11-15` 与 `## [Unreleased]`。
key="$version"
key="${key#v}"
if [ "$key" = "unreleased" ] || [ "$key" = "UNRELEASED" ]; then
  key="Unreleased"
fi

status=0
body=$(awk -v key="$key" '
  BEGIN { capturing = 0; body = ""; prefix = "## [" key "]" }
  {
    if (!capturing) {
      # 只比较标题前缀，避免 0.3.3 误匹配 0.3.30。
      if (substr($0, 1, length(prefix)) == prefix) { capturing = 1 }
      next
    }
    # 下一节标题、文件底部的链接定义（`[0.3.3]: https://...`）都不属于正文。
    if ($0 ~ /^## \[/ || $0 ~ /^\[[^]]+\]:/) { exit }
    body = body $0 "\n"
  }
  END {
    if (!capturing) { exit 3 }
    gsub(/^\n+/, "", body)
    sub(/\n+$/, "", body)
    printf "%s", body
  }
' "$file") || status=$?

if [ "$status" -eq 3 ]; then
  printf 'CHANGELOG.md 里没有 %s 的小节（标题应写成「## [%s] - YYYY-MM-DD」）\n' "$version" "$version" >&2
  exit 1
fi
if [ "$status" -ne 0 ]; then
  printf '解析 CHANGELOG.md 失败（awk 退出码 %s）\n' "$status" >&2
  exit 1
fi

if ! printf '%s\n' "$body" | grep -q '^- '; then
  printf 'CHANGELOG.md 里 %s 的小节没有任何条目，先在「## [%s] - YYYY-MM-DD」下按分类补上\n' "$version" "$version" >&2
  exit 1
fi

printf '%s\n' "$body"
