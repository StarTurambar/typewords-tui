<#
.SYNOPSIS
  构建并打包 Windows release：tw.exe + assets/ + docs/ + README.md + LICENSE → dist/<名称>.tar.gz 与同名 .sha256。
.DESCRIPTION
  Linux / macOS 用 scripts/package.sh，两者产出同一套归档名与 sha256 校验格式。
.EXAMPLE
  pwsh -File scripts/package.ps1
.EXAMPLE
  pwsh -File scripts/package.ps1 -Target x86_64-pc-windows-msvc -SkipBuild
#>
param(
  [string]$Target = 'x86_64-pc-windows-msvc',
  [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
  if (-not $SkipBuild) {
    cargo build --release --locked --target $Target
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
  }

  $manifest = Get-Content (Join-Path $root 'Cargo.toml') -Raw
  $version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
  if (-not $version) { throw 'cannot read version from Cargo.toml' }

  $bin = if ($Target -like '*-pc-windows-*') { 'tw.exe' } else { 'tw' }
  $binary = Join-Path $root "target/$Target/release/$bin"
  if (-not (Test-Path $binary)) { throw "找不到构建产物：$binary（先不带 -SkipBuild 跑一次）" }

  $name = "typewords-tui-v$version-$Target"
  $dist = Join-Path $root 'dist'
  $stage = Join-Path $dist $name
  $archive = Join-Path $dist "$name.tar.gz"
  $checksum = "$archive.sha256"
  foreach ($path in @($stage, $archive, $checksum)) {
    if (Test-Path $path) { Remove-Item $path -Recurse -Force }
  }
  New-Item -ItemType Directory -Path $stage -Force | Out-Null

  Copy-Item $binary (Join-Path $stage $bin)
  Copy-Item (Join-Path $root 'assets') $stage -Recurse
  Copy-Item (Join-Path $root 'docs') $stage -Recurse
  Copy-Item (Join-Path $root 'README.md'), (Join-Path $root 'LICENSE') $stage

  # 统一用 tar.gz：Windows 10 1803+ 自带 bsdtar，与 Linux / macOS 归档格式一致
  tar -czf $archive -C $dist $name
  if ($LASTEXITCODE -ne 0) { throw 'tar failed' }

  # 自检：归档可读且关键条目齐全，避免产出空壳包（清单只读一次）
  $entries = tar -tzf $archive
  if ($LASTEXITCODE -ne 0) { throw 'tar failed' }
  foreach ($entry in @("$name/$bin", "$name/assets/list/word.json", "$name/README.md", "$name/LICENSE")) {
    if ($entries -notcontains $entry) { throw "归档缺少条目：$entry" }
  }

  $hash = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
  # 固定用 LF 写校验文件：CRLF 会让 Linux / WSL 上的 `sha256sum -c` 把文件名当成带 \r 的另一份文件
  $line = "$hash  $name.tar.gz`n"
  [System.IO.File]::WriteAllText($checksum, $line, [System.Text.Encoding]::ASCII)

  # 暂存目录只用于打包，自检通过后删掉，dist/ 只留归档与校验文件
  Remove-Item $stage -Recurse -Force

  Write-Host "打包完成：$archive"
  Write-Host "校验文件：$checksum"
  Write-Host "SHA256：$hash"
}
finally {
  Pop-Location
}
