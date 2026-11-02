<#
  端到端冒烟：给外部自动化调用的统一入口。
  用法：powershell -File scripts/cb-e2e.ps1 [-SkipTests]
  退出码：0 = 全绿；非 0 = 失败（直接透传）。
#>
param([switch]$SkipTests)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not $SkipTests) {
    Write-Host '[1/3] cargo test --workspace'
    cargo test --workspace
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host '[2/3] cargo build -p cb-cli'
cargo build -p cb-cli
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$cb = Join-Path $root 'target/debug/cb.exe'
if (-not (Test-Path $cb)) { $cb = Join-Path $root 'target/debug/cb' }
$tmpDb = Join-Path ([System.IO.Path]::GetTempPath()) ("cb-e2e-" + [guid]::NewGuid() + ".db")

Write-Host '[3/3] selftest + tools'
& $cb --db $tmpDb --json selftest | Out-Host
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $cb --db $tmpDb --json tools | Out-Host
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Remove-Item $tmpDb, "$tmpDb-wal", "$tmpDb-shm" -ErrorAction SilentlyContinue
Write-Host 'e2e: OK'
exit 0
