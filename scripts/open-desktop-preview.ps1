$ErrorActionPreference = "Stop"

$project = Split-Path -Parent $PSScriptRoot
$url = "http://127.0.0.1:1420/"
$logDirectory = Join-Path $project "data"
$logPath = Join-Path $logDirectory "desktop-preview-launcher.log"

New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null

function Test-PreviewReady {
  try {
    $response = Invoke-WebRequest -Uri $url -TimeoutSec 2 -UseBasicParsing
    return $response.StatusCode -ge 200 -and $response.StatusCode -lt 500
  } catch {
    return $false
  }
}

function Find-Executable([string]$name, [string[]]$fallbacks) {
  $command = Get-Command $name -ErrorAction SilentlyContinue
  if ($null -ne $command) {
    return $command.Source
  }
  foreach ($candidate in $fallbacks) {
    if (Test-Path -LiteralPath $candidate) {
      return $candidate
    }
  }
  return ""
}

if (-not (Test-PreviewReady)) {
  $nodePath = Find-Executable "node.exe" @(
    (Join-Path $env:USERPROFILE ".cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe")
  )
  $npmCli = Find-Executable "npm-cli.js" @(
    (Join-Path $env:LOCALAPPDATA "DailyTaskMonitorDev\npm-11.18.0\package\bin\npm-cli.js")
  )

  if ([string]::IsNullOrWhiteSpace($nodePath) -or [string]::IsNullOrWhiteSpace($npmCli)) {
    throw "Node.js or npm could not be found."
  }

  Add-Content -LiteralPath $logPath -Value ("[{0}] Starting desktop preview" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss")) -Encoding UTF8
  Start-Process -FilePath $nodePath -ArgumentList @($npmCli, "run", "dev", "--", "--host", "127.0.0.1", "--port", "1420") -WorkingDirectory $project -WindowStyle Hidden | Out-Null

  for ($attempt = 0; $attempt -lt 30; $attempt++) {
    Start-Sleep -Milliseconds 350
    if (Test-PreviewReady) {
      break
    }
  }
}

if (-not (Test-PreviewReady)) {
  throw "The desktop preview did not start. See $logPath"
}

Start-Process -FilePath "rundll32.exe" -ArgumentList @("url.dll,FileProtocolHandler", $url) -WindowStyle Hidden | Out-Null
