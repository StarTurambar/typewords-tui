<#
.SYNOPSIS
  构建并打包 Windows release：tw.exe + assets/ + README.md + LICENSE。
.EXAMPLE
  pwsh -File scripts/package.ps1
#>
param(
  [string]$Target = 'x86_64-pc-windows-msvc'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
  cargo build --release --locked
  if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

  $manifest = Get-Content (Join-Path $root 'Cargo.toml') -Raw
  $version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
  if (-not $version) { throw 'cannot read version from Cargo.toml' }

  $name = "typewords-tui-v$version-$Target"
  $dist = Join-Path $root 'dist'
  $stage = Join-Path $dist $name
  if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
  New-Item -ItemType Directory -Path $stage -Force | Out-Null

  Copy-Item (Join-Path $root 'target/release/tw.exe') $stage
  Copy-Item (Join-Path $root 'assets') $stage -Recurse
  Copy-Item (Join-Path $root 'README.md'), (Join-Path $root 'LICENSE') $stage

  $zip = Join-Path $dist "$name.zip"
  if (Test-Path $zip) { Remove-Item $zip -Force }
  Compress-Archive -Path $stage -DestinationPath $zip
  $hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
  "$hash  $name.zip" | Set-Content (Join-Path $dist "$name.zip.sha256")

  Write-Host "打包完成: $zip"
  Write-Host "SHA256: $hash"
}
finally {
  Pop-Location
}
