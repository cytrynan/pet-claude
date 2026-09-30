# 品質ゲート（K 用・PowerShell でそのまま動く）。scripts/gate.sh と同じ 4 段。
# 使い方: pwsh -File scripts/gate.ps1   （どの段で止まっても、その段の exit code で終わる）
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$env:PYTHONIOENCODING = 'utf-8'
uv run ruff format --check src tests
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
uv run ruff check .
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
uv run mypy src
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
uv run pytest -m "db or not db" --cov-fail-under=80 -q -p no:cacheprovider
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host 'GATE OK'
