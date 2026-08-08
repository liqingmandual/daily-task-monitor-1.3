param(
  [switch]$NoOpen
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$Port = 8765
$Url = "http://127.0.0.1:$Port/"
$StatusUrl = "${Url}api/monitor/status"
$LogPath = Join-Path $ProjectRoot "data\launcher.log"

function Write-LauncherLog([string]$Message) {
  try {
    New-Item -ItemType Directory -Path (Split-Path -Parent $LogPath) -Force | Out-Null
    Add-Content -LiteralPath $LogPath -Value ("[{0}] {1}" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), $Message) -Encoding UTF8
  } catch {
  }
}

function Find-NodePath {
  $command = Get-Command node -ErrorAction SilentlyContinue
  if ($command) { return $command.Source }

  $candidates = @(
    (Join-Path $env:USERPROFILE ".cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe"),
    (Join-Path $env:LOCALAPPDATA "OpenAI\Codex\runtimes\cua_node\1b23c930bdf84ed6\bin\node.exe")
  )

  foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate) { return $candidate }
  }

  throw "Node.js was not found."
}

function Get-DashboardStatus {
  try {
    return Invoke-RestMethod -Uri $StatusUrl -UseBasicParsing -TimeoutSec 2
  } catch {
    return $null
  }
}

function Test-ThisDashboardReady {
  $status = Get-DashboardStatus
  if (-not $status) { return $false }
  $statusPath = [string]$status.projectPath
  return ($statusPath -eq $ProjectRoot)
}

function Stop-OtherDashboardIfNeeded {
  $status = Get-DashboardStatus
  if (-not $status) { return }
  if ([string]$status.projectPath -eq $ProjectRoot) { return }
  $pidValue = $status.dashboard.pid
  if ($pidValue) {
    Write-LauncherLog "Stopping dashboard on port $Port from $($status.projectPath), pid=$pidValue"
    Stop-Process -Id ([int]$pidValue) -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 700
  }
}

function Start-ThisDashboard {
  $nodePath = Find-NodePath
  Write-LauncherLog "Starting dashboard with node=$nodePath project=$ProjectRoot"
  Start-Process -FilePath $nodePath -ArgumentList @("server.js", "$Port", ".") -WorkingDirectory $ProjectRoot -WindowStyle Hidden | Out-Null
}

function Wait-ForDashboard {
  for ($i = 0; $i -lt 35; $i++) {
    if (Test-ThisDashboardReady) { return $true }
    Start-Sleep -Milliseconds 300
  }
  return $false
}

function Open-Dashboard {
  $chromeCandidates = @(
    (Join-Path $env:ProgramFiles "Google\Chrome\Application\chrome.exe"),
    (Join-Path ${env:ProgramFiles(x86)} "Google\Chrome\Application\chrome.exe"),
    (Join-Path $env:LOCALAPPDATA "Google\Chrome\Application\chrome.exe")
  )
  $chrome = $chromeCandidates | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1

  if ($chrome) {
    Start-Process -FilePath $chrome -ArgumentList @("--new-window", $Url) | Out-Null
  } else {
    Start-Process -FilePath "rundll32.exe" -ArgumentList @("url.dll,FileProtocolHandler", $Url) -WindowStyle Hidden | Out-Null
  }
}

Write-LauncherLog "Launcher invoked."

if (-not (Test-Path -LiteralPath (Join-Path $ProjectRoot "server.js"))) {
  throw "Project folder is missing server.js: $ProjectRoot"
}

if (-not (Test-ThisDashboardReady)) {
  Stop-OtherDashboardIfNeeded
  if (-not (Test-ThisDashboardReady)) {
    Start-ThisDashboard
  }
}

if (-not (Wait-ForDashboard)) {
  Write-LauncherLog "Dashboard did not become ready."
  throw "Dashboard did not start on port $Port."
}

Write-LauncherLog "Dashboard ready: $Url"

if (-not $NoOpen) {
  Open-Dashboard
}

exit 0
