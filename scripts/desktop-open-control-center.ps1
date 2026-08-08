$ErrorActionPreference = "Stop"
function U([string]$Text) {
    return [System.Text.RegularExpressions.Regex]::Unescape($Text)
}
$Project = U "D:\\codex\u9879\u76EE\\\u6BCF\u65E5\u4EFB\u52A1\u76D1\u6D4B\u7CFB\u7EDF"
$Port = 8765
$Url = "http://localhost:$Port"
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
function Test-ControlCenter {
    try {
        $response = Invoke-WebRequest -Uri "$Url/api/monitor/status" -UseBasicParsing -TimeoutSec 2
        return $response.StatusCode -eq 200
    } catch {
        return $false
    }
}
if (-not (Test-ControlCenter)) {
    if (-not (Test-Path -LiteralPath $Project)) { throw "Project folder not found: $Project" }
    $node = Find-Node
    $serverPsi = New-Object System.Diagnostics.ProcessStartInfo
    $serverPsi.FileName = $node
    $serverPsi.Arguments = "server.js $Port ."
    $serverPsi.WorkingDirectory = $Project
    $serverPsi.UseShellExecute = $false
    $serverPsi.CreateNoWindow = $true
    [System.Diagnostics.Process]::Start($serverPsi) | Out-Null
    for ($i = 0; $i -lt 20; $i++) {
        Start-Sleep -Milliseconds 300
        if (Test-ControlCenter) { break }
    }
}
Start-Process $Url