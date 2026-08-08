param(
    [int]$Port = 8765,
    [int]$SampleSeconds = 5,
    [switch]$NoSeed
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$ActivityPath = Join-Path $ProjectRoot "data\activity-log.jsonl"
$SeedScript = Join-Path $PSScriptRoot "seed-sample-data.ps1"
$RecorderScript = Join-Path $PSScriptRoot "start-recorder.ps1"
$DashboardScript = Join-Path $PSScriptRoot "start-dashboard.ps1"

if (-not $NoSeed) {
    $needsSeed = $true
    if (Test-Path -LiteralPath $ActivityPath) {
        $file = Get-Item -LiteralPath $ActivityPath
        $needsSeed = $file.Length -eq 0
    }

    if ($needsSeed) {
        & $SeedScript
    }
}

Write-Host "Starting background recorder..."
$recorderJob = $null
try {
    $recorderJob = Start-Job -Name "DailyTaskRecorder" -ArgumentList $RecorderScript, $SampleSeconds -ScriptBlock {
        param($ScriptPath, $Interval)
        & $ScriptPath -SampleSeconds $Interval
    }
}
catch {
    Write-Warning "The recorder could not be started as a background job. You can run scripts\start-recorder.ps1 in a second PowerShell window."
}

try {
    & $DashboardScript -Port $Port
}
finally {
    if ($null -ne $recorderJob) {
        Write-Host "Stopping background recorder..."
        Stop-Job -Job $recorderJob -ErrorAction SilentlyContinue
        Receive-Job -Job $recorderJob -ErrorAction SilentlyContinue | Out-Null
        Remove-Job -Job $recorderJob -ErrorAction SilentlyContinue
    }
}
