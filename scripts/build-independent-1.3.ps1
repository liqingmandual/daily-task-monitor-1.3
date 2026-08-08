param(
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$ExtraArgs
)

$ErrorActionPreference = "Stop"
$env:DAILY_TASK_MONITOR_STORAGE_NAME = "DailyTaskMonitorIndependent13"
$env:DAILY_TASK_MONITOR_CREDENTIAL_SERVICE = "DailyTaskMonitorIndependent13"
$env:DAILY_TASK_MONITOR_MUTEX_NAME = "Local\DailyTaskMonitorDesktopIndependent13"

$arguments = @(
  "tauri",
  "build",
  "--config",
  "src-tauri/tauri.independent-1.3.conf.json"
)
if ($ExtraArgs -contains "-NoBundle" -or $ExtraArgs -contains "--no-bundle") {
  $arguments += "--no-bundle"
}

& pnpm.cmd @arguments
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}
