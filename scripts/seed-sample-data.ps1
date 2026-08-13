param(
    [string]$DataPath = "",
    [string]$NotesPath = "",
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Convert-EscapedUnicode {
    param([string]$Text)
    return [System.Text.RegularExpressions.Regex]::Unescape($Text)
}

$CatDev = Convert-EscapedUnicode "\u521B\u4F5C/\u5F00\u53D1"
$CatOutput = Convert-EscapedUnicode "\u6587\u6863/\u8F93\u51FA"
$CatResearch = Convert-EscapedUnicode "\u641C\u7D22/\u8C03\u7814"
$CatComm = Convert-EscapedUnicode "\u6C9F\u901A"
$CatIdle = Convert-EscapedUnicode "\u7A7A\u95F2"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($DataPath)) {
    $DataPath = Join-Path $ProjectRoot "data\activity-log.jsonl"
}
if ([string]::IsNullOrWhiteSpace($NotesPath)) {
    $NotesPath = Join-Path $ProjectRoot "data\daily-notes.json"
}

$DataDir = Split-Path -Parent $DataPath
if (-not (Test-Path -LiteralPath $DataDir)) {
    New-Item -ItemType Directory -Path $DataDir | Out-Null
}

if ((Test-Path -LiteralPath $DataPath) -and -not $Force) {
    $file = Get-Item -LiteralPath $DataPath
    if ($file.Length -gt 0) {
        Write-Host "Existing activity data found. Sample data was skipped."
        return
    }
}

$date = (Get-Date).Date

function New-SampleEvent {
    param(
        [datetime]$Start,
        [datetime]$End,
        [string]$App,
        [string]$Title,
        [string]$Category,
        [bool]$IsIdle = $false
    )

    return [pscustomobject]@{
        start = $Start.ToString("o")
        end = $End.ToString("o")
        durationSeconds = [int][math]::Round(($End - $Start).TotalSeconds, 0)
        app = $App
        title = $Title
        category = $Category
        isIdle = $IsIdle
        idleSeconds = if ($IsIdle) { 600 } else { 0 }
    }
}

$events = @(
    (New-SampleEvent -Start ($date.AddHours(9).AddMinutes(10)) -End ($date.AddHours(9).AddMinutes(42)) -App "Code" -Title "Daily task monitor - Visual Studio Code" -Category $CatDev)
    (New-SampleEvent -Start ($date.AddHours(9).AddMinutes(42)) -End ($date.AddHours(10).AddMinutes(6)) -App "chrome" -Title "Task time tracking methods - Google Search" -Category $CatResearch)
    (New-SampleEvent -Start ($date.AddHours(10).AddMinutes(6)) -End ($date.AddHours(10).AddMinutes(38)) -App "WINWORD" -Title "Efficiency report product plan.docx" -Category $CatOutput)
    (New-SampleEvent -Start ($date.AddHours(10).AddMinutes(38)) -End ($date.AddHours(10).AddMinutes(55)) -App "WeChat" -Title "Project discussion" -Category $CatComm)
    (New-SampleEvent -Start ($date.AddHours(10).AddMinutes(55)) -End ($date.AddHours(11).AddMinutes(12)) -App "chrome" -Title "ChatGPT - Report advice structure" -Category $CatResearch)
    (New-SampleEvent -Start ($date.AddHours(11).AddMinutes(12)) -End ($date.AddHours(11).AddMinutes(48)) -App "Code" -Title "dashboard/app.js - Visual Studio Code" -Category $CatDev)
    (New-SampleEvent -Start ($date.AddHours(11).AddMinutes(48)) -End ($date.AddHours(12).AddMinutes(20)) -App "explorer" -Title "File Explorer" -Category $CatIdle -IsIdle $true)
    (New-SampleEvent -Start ($date.AddHours(14).AddMinutes(5)) -End ($date.AddHours(14).AddMinutes(51)) -App "Code" -Title "PowerShell recorder - Visual Studio Code" -Category $CatDev)
    (New-SampleEvent -Start ($date.AddHours(14).AddMinutes(51)) -End ($date.AddHours(15).AddMinutes(15)) -App "chrome" -Title "Windows active window API - Bing" -Category $CatResearch)
    (New-SampleEvent -Start ($date.AddHours(15).AddMinutes(15)) -End ($date.AddHours(15).AddMinutes(48)) -App "typora" -Title "Daily review.md" -Category $CatOutput)
)

$lines = $events | ForEach-Object { $_ | ConvertTo-Json -Compress -Depth 5 }
Set-Content -LiteralPath $DataPath -Value $lines -Encoding UTF8

$note = [pscustomobject]@{
    date = (Get-Date).ToString("yyyy-MM-dd")
    goals = Convert-EscapedUnicode "\u5B8C\u6210\u4EFB\u52A1\u76D1\u6D4B\u7CFB\u7EDF\u7684\u7B2C\u4E00\u4E2A\u53EF\u8FD0\u884C Demo\u3002"
    outputs = Convert-EscapedUnicode "\u672C\u5730\u8BB0\u5F55\u5668\u3001\u5206\u7C7B\u89C4\u5219\u3001\u65E5\u62A5\u4EEA\u8868\u76D8\u548C\u590D\u76D8\u8868\u5355\u3002"
    score = 7
    reflection = "Research time is a little high, but output blocks are concentrated. The next useful feature is real browser URL tracking."
    updatedAt = (Get-Date).ToString("o")
}

@($note) | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $NotesPath -Encoding UTF8
Write-Host "Sample data generated: $DataPath"
