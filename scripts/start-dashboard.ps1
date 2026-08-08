param(
    [int]$Port = 8765,
    [string]$RootPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($RootPath)) {
    $RootPath = Split-Path -Parent $PSScriptRoot
}

$ServerScript = Join-Path $RootPath "server.js"

function Get-NodePath {
    $command = Get-Command node -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    $bundled = Join-Path $env:USERPROFILE ".cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe"
    if (Test-Path -LiteralPath $bundled) {
        return $bundled
    }

    return ""
}

$NodePath = Get-NodePath
if ([string]::IsNullOrWhiteSpace($NodePath)) {
    throw "Node.js was not found. Install Node.js LTS, then rerun this script."
}

& $NodePath $ServerScript $Port $RootPath
