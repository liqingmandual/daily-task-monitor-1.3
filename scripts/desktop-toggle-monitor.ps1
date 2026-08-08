$ErrorActionPreference = "Stop"
function U([string]$Text) {
    return [System.Text.RegularExpressions.Regex]::Unescape($Text)
}
$Project = U "D:\\codex\u9879\u76EE\\\u6BCF\u65E5\u4EFB\u52A1\u76D1\u6D4B\u7CFB\u7EDF"
$Port = 8765
function Find-Node {
    $candidates = @(
        "C:\Users\26925\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe",
        "C:\Users\26925\AppData\Local\OpenAI\Codex\runtimes\cua_node\1b23c930bdf84ed6\bin\node.exe"
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
    $cmd = Get-Command node -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    throw "Node.js was not found."
}
function Get-MonitorProcesses {
    return @(Get-CimInstance Win32_Process | Where-Object {
        ($_.CommandLine -like "*server.js $Port*" -and $_.CommandLine -like "*$Project*") -or
        ($_.CommandLine -like "*start-recorder.ps1*" -and $_.CommandLine -like "*$Project*")
    })
}
function Stop-Monitor {
    foreach ($target in Get-MonitorProcesses) {
        Stop-Process -Id $target.ProcessId -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 300
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show((U "\u6BCF\u65E5\u4EFB\u52A1\u76D1\u6D4B\u7CFB\u7EDF\u5DF2\u505C\u6B62\u3002"), (U "\u6BCF\u65E5\u4EFB\u52A1\u76D1\u6D4B\u7CFB\u7EDF")) | Out-Null
}
function Start-Monitor {
    if (-not (Test-Path -LiteralPath $Project)) { throw "Project folder not found: $Project" }
    $dataDir = Join-Path $Project "data"
    if (-not (Test-Path -LiteralPath $dataDir)) {
        New-Item -ItemType Directory -Path $dataDir | Out-Null
    }
    $node = Find-Node
    $serverPsi = New-Object System.Diagnostics.ProcessStartInfo
    $serverPsi.FileName = $node
    $serverPsi.Arguments = "server.js $Port ."
    $serverPsi.WorkingDirectory = $Project
    $serverPsi.UseShellExecute = $false
    $serverPsi.CreateNoWindow = $true
    [System.Diagnostics.Process]::Start($serverPsi) | Out-Null
    $recorderScript = Join-Path $Project "scripts\start-recorder.ps1"
    $activityLog = Join-Path $Project "data\activity-log.jsonl"
    $recorderPsi = New-Object System.Diagnostics.ProcessStartInfo
    $recorderPsi.FileName = "powershell.exe"
    $recorderPsi.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$recorderScript`" -SampleSeconds 5 -DataPath `"$activityLog`""
    $recorderPsi.WorkingDirectory = $Project
    $recorderPsi.UseShellExecute = $false
    $recorderPsi.CreateNoWindow = $true
    [System.Diagnostics.Process]::Start($recorderPsi) | Out-Null
    Start-Sleep -Seconds 1
    Start-Process "http://localhost:$Port"
}
if ((Get-MonitorProcesses).Count -gt 0) {
    Stop-Monitor
} else {
    Start-Monitor
}