$ErrorActionPreference = "Stop"
$Project = "D:\codex项目\每日任务监测系统"
$Port = 8765
$DataDir = Join-Path $Project "data"
if (-not (Test-Path -LiteralPath $DataDir)) {
    New-Item -ItemType Directory -Path $DataDir | Out-Null
}
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
function Is-PortListening {
    return $null -ne (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1)
}
function Is-RecorderRunning {
    $escapedProject = $Project.Replace("\", "\\")
    $matches = Get-CimInstance Win32_Process | Where-Object {
        $_.CommandLine -like "*start-recorder.ps1*" -and $_.CommandLine -like "*$Project*"
    }
    return $null -ne ($matches | Select-Object -First 1)
}
if (-not (Is-PortListening)) {
    $node = Find-Node
    $serverPsi = New-Object System.Diagnostics.ProcessStartInfo
    $serverPsi.FileName = $node
    $serverPsi.Arguments = "server.js $Port ."
    $serverPsi.WorkingDirectory = $Project
    $serverPsi.UseShellExecute = $false
    $serverPsi.CreateNoWindow = $true
    [System.Diagnostics.Process]::Start($serverPsi) | Out-Null
    Start-Sleep -Seconds 1
}
if (-not (Is-RecorderRunning)) {
    $recorderScript = Join-Path $Project "scripts\start-recorder.ps1"
    $activityLog = Join-Path $Project "data\activity-log.jsonl"
    $recorderPsi = New-Object System.Diagnostics.ProcessStartInfo
    $recorderPsi.FileName = "powershell.exe"
    $recorderPsi.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$recorderScript`" -SampleSeconds 5 -DataPath `"$activityLog`""
    $recorderPsi.WorkingDirectory = $Project
    $recorderPsi.UseShellExecute = $false
    $recorderPsi.CreateNoWindow = $true
    [System.Diagnostics.Process]::Start($recorderPsi) | Out-Null
}
Start-Sleep -Seconds 1
Start-Process "http://localhost:$Port"