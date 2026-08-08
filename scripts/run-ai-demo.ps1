param(
    [int]$Port = 8765
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Project = Split-Path -Parent $PSScriptRoot
$Url = "http://localhost:$Port"

function Find-Node {
    $candidates = @(
        (Join-Path $env:USERPROFILE ".cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe"),
        (Join-Path $env:ProgramFiles "nodejs\node.exe")
    )

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    $command = Get-Command node -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    throw "Node.js was not found."
}

function Test-Dashboard {
    try {
        $response = Invoke-WebRequest -Uri "$Url/" -UseBasicParsing -TimeoutSec 2
        return $response.StatusCode -eq 200
    } catch {
        return $false
    }
}

if (-not (Test-Dashboard)) {
    $node = Find-Node
    $processInfo = New-Object System.Diagnostics.ProcessStartInfo
    $processInfo.FileName = $node
    $processInfo.Arguments = "server.js $Port ."
    $processInfo.WorkingDirectory = $Project
    $processInfo.UseShellExecute = $false
    $processInfo.CreateNoWindow = $true
    [System.Diagnostics.Process]::Start($processInfo) | Out-Null

    for ($i = 0; $i -lt 20; $i++) {
        Start-Sleep -Milliseconds 300
        if (Test-Dashboard) {
            break
        }
    }
}

Start-Process $Url
