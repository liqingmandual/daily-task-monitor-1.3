$ErrorActionPreference = "SilentlyContinue"
$Project = "D:\codex项目\每日任务监测系统"
$Port = 8765
$targets = Get-CimInstance Win32_Process | Where-Object {
    ($_.CommandLine -like "*server.js $Port*" -and $_.CommandLine -like "*$Project*") -or
    ($_.CommandLine -like "*start-recorder.ps1*" -and $_.CommandLine -like "*$Project*")
}
foreach ($target in $targets) {
    Stop-Process -Id $target.ProcessId -Force
}
$portOwners = Get-NetTCPConnection -LocalPort $Port -ErrorAction SilentlyContinue | Select-Object -ExpandProperty OwningProcess -Unique
foreach ($ownerPid in $portOwners) {
    $proc = Get-CimInstance Win32_Process -Filter "ProcessId = $ownerPid"
    if ($proc.CommandLine -like "*server.js $Port*" -and $proc.CommandLine -like "*$Project*") {
        Stop-Process -Id $ownerPid -Force
    }
}
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.MessageBox]::Show("Daily task monitor has been stopped.", "Daily Task Monitor") | Out-Null