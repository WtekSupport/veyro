# Vite dev server for `tauri dev` (beforeDevCommand). Works in minimal PATH shells.

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

$nodejsDir = Join-Path ${env:ProgramFiles} "nodejs"
if (Test-Path (Join-Path $nodejsDir "node.exe")) {
    $env:PATH = "$nodejsDir;$env:PATH"
}

$nodeExe = (Get-Command node -ErrorAction Stop).Source
& $nodeExe (Join-Path $repoRoot "scripts\free-dev-port.mjs")
& $nodeExe (Join-Path $repoRoot "node_modules\vite\bin\vite.js")
