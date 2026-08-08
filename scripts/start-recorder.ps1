param(
    [int]$SampleSeconds = 5,
    [int]$IdleThresholdSeconds = 900,
    [string]$DataPath = "",
    [string]$RulesPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Convert-EscapedUnicode {
    param([string]$Text)
    return [System.Text.RegularExpressions.Regex]::Unescape($Text)
}

$FallbackCategory = Convert-EscapedUnicode "\u672A\u5206\u7CBB"
$IdleCategory = Convert-EscapedUnicode "\u7A7A\u95F2"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($DataPath)) {
    $DataPath = Join-Path $ProjectRoot "data\activity-log.jsonl"
}
if ([string]::IsNullOrWhiteSpace($RulesPath)) {
    $RulesPath = Join-Path $ProjectRoot "config\categories.json"
}

$DataDir = Split-Path -Parent $DataPath
if (-not (Test-Path -LiteralPath $DataDir)) {
    New-Item -ItemType Directory -Path $DataDir | Out-Null
}

if (-not ([System.Management.Automation.PSTypeName]"DailyTaskWin32").Type) {
    Add-Type -TypeDefinition @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class DailyTaskWin32
{
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll", SetLastError = true)]
    private static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll")]
    private static extern bool GetLastInputInfo(ref LASTINPUTINFO plii);

    [DllImport("kernel32.dll")]
    private static extern uint GetTickCount();

    [StructLayout(LayoutKind.Sequential)]
    private struct LASTINPUTINFO
    {
        public uint cbSize;
        public uint dwTime;
    }

    public static string GetWindowTitle(IntPtr handle)
    {
        StringBuilder buffer = new StringBuilder(512);
        GetWindowText(handle, buffer, buffer.Capacity);
        return buffer.ToString();
    }

    public static uint GetProcessId(IntPtr handle)
    {
        uint processId;
        GetWindowThreadProcessId(handle, out processId);
        return processId;
    }

    public static uint GetIdleMilliseconds()
    {
        LASTINPUTINFO lastInput = new LASTINPUTINFO();
        lastInput.cbSize = (uint)Marshal.SizeOf(typeof(LASTINPUTINFO));
        if (!GetLastInputInfo(ref lastInput))
        {
            return 0;
        }
        return GetTickCount() - lastInput.dwTime;
    }
}
"@
}

function Read-CategoryRules {
    if (-not (Test-Path -LiteralPath $RulesPath)) {
        return [pscustomobject]@{
            fallback = $FallbackCategory
            categories = @()
        }
    }

    $raw = Get-Content -LiteralPath $RulesPath -Raw -Encoding UTF8
    return $raw | ConvertFrom-Json
}

function Test-TextContains {
    param(
        [string]$Text,
        [string]$Pattern
    )

    if ([string]::IsNullOrWhiteSpace($Text) -or [string]::IsNullOrWhiteSpace($Pattern)) {
        return $false
    }

    return $Text.IndexOf($Pattern, [System.StringComparison]::InvariantCultureIgnoreCase) -ge 0
}

function Get-RuleProperty {
    param(
        [object]$Rule,
        [string]$Name
    )

    $prop = $Rule.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return ""
    }
    return [string]$prop.Value
}

function Get-CategoryName {
    param(
        [string]$ProcessName,
        [string]$Title,
        [object]$Rules
    )

    foreach ($category in @($Rules.categories)) {
        foreach ($rule in @($category.rules)) {
            $processPattern = Get-RuleProperty -Rule $rule -Name "process"
            $titlePattern = Get-RuleProperty -Rule $rule -Name "title"
            $hasCondition = $false
            $matched = $true

            if (-not [string]::IsNullOrWhiteSpace($processPattern)) {
                $hasCondition = $true
                $matched = $matched -and (Test-TextContains -Text $ProcessName -Pattern $processPattern)
            }

            if (-not [string]::IsNullOrWhiteSpace($titlePattern)) {
                $hasCondition = $true
                $matched = $matched -and (Test-TextContains -Text $Title -Pattern $titlePattern)
            }

            if ($hasCondition -and $matched) {
                return [string]$category.name
            }
        }
    }

    if ($Rules.PSObject.Properties["fallback"]) {
        return [string]$Rules.fallback
    }
    return $FallbackCategory
}

function Get-ActiveSnapshot {
    param([object]$Rules)

    $handle = [DailyTaskWin32]::GetForegroundWindow()
    $processId = [DailyTaskWin32]::GetProcessId($handle)
    $title = [DailyTaskWin32]::GetWindowTitle($handle)
    $processName = "unknown"

    try {
        $process = Get-Process -Id $processId -ErrorAction Stop
        $processName = $process.ProcessName
    }
    catch {
        $processName = "unknown"
    }

    if ([string]::IsNullOrWhiteSpace($title)) {
        $title = "(No window title)"
    }

    $idleSeconds = [math]::Round(([DailyTaskWin32]::GetIdleMilliseconds() / 1000), 0)
    $isIdle = $idleSeconds -ge $IdleThresholdSeconds
    $category = if ($isIdle) { $IdleCategory } else { Get-CategoryName -ProcessName $processName -Title $title -Rules $Rules }

    return [pscustomobject]@{
        app = $processName
        title = $title
        category = $category
        isIdle = $isIdle
        idleSeconds = [int]$idleSeconds
        idleStartsAtLastInput = $false
        idleThresholdSeconds = [int]$IdleThresholdSeconds
    }
}

function Get-SnapshotKey {
    param([object]$Snapshot)
    return "$($Snapshot.app)|$($Snapshot.title)|$($Snapshot.category)|$($Snapshot.isIdle)"
}

function Write-ActivityEvent {
    param(
        [datetime]$Start,
        [datetime]$End,
        [object]$Snapshot
    )

    $duration = [int][math]::Round(($End - $Start).TotalSeconds, 0)
    if ($duration -lt 1) {
        return
    }

    $event = [pscustomobject]@{
        start = $Start.ToString("o")
        end = $End.ToString("o")
        durationSeconds = $duration
        app = $Snapshot.app
        title = $Snapshot.title
        category = $Snapshot.category
        isIdle = [bool]$Snapshot.isIdle
        idleSeconds = [int]$Snapshot.idleSeconds
        idleStartsAtLastInput = [bool]$Snapshot.idleStartsAtLastInput
        idleThresholdSeconds = [int]$Snapshot.idleThresholdSeconds
    }

    $json = $event | ConvertTo-Json -Compress -Depth 4
    Add-Content -LiteralPath $DataPath -Value $json -Encoding UTF8
}

function Get-LastInputTime {
    param(
        [datetime]$Now,
        [object]$Snapshot,
        [datetime]$FallbackStart
    )

    $idleSeconds = [int]$Snapshot.idleSeconds
    if ($idleSeconds -lt 0) {
        $idleSeconds = 0
    }

    $lastInput = $Now.AddSeconds(-1 * $idleSeconds)
    if ($lastInput -lt $FallbackStart) {
        return $FallbackStart
    }
    if ($lastInput -gt $Now) {
        return $Now
    }
    return $lastInput
}

$rules = Read-CategoryRules
$current = Get-ActiveSnapshot -Rules $rules
$currentKey = Get-SnapshotKey -Snapshot $current
$segmentStart = Get-Date
if ([bool]$current.isIdle) {
    $segmentStart = $segmentStart.AddSeconds(-1 * [int]$current.idleSeconds)
    $current.idleStartsAtLastInput = $true
}

Write-Host "Recording desktop activity. Data path: $DataPath"
Write-Host "Sample interval: $SampleSeconds seconds. Idle threshold: $IdleThresholdSeconds seconds. Press Ctrl+C to stop."

try {
    while ($true) {
        Start-Sleep -Seconds $SampleSeconds
        $now = Get-Date
        $rules = Read-CategoryRules
        $next = Get-ActiveSnapshot -Rules $rules
        $nextKey = Get-SnapshotKey -Snapshot $next

        if ([bool]$next.isIdle -and -not [bool]$current.isIdle) {
            $idleStart = Get-LastInputTime -Now $now -Snapshot $next -FallbackStart $segmentStart
            Write-ActivityEvent -Start $segmentStart -End $idleStart -Snapshot $current
            $segmentStart = $idleStart
            $next.idleStartsAtLastInput = $true
            $current = $next
            $currentKey = Get-SnapshotKey -Snapshot $current
            continue
        }

        if (-not [bool]$next.isIdle -and [bool]$current.isIdle) {
            $activeStart = Get-LastInputTime -Now $now -Snapshot $next -FallbackStart $segmentStart
            Write-ActivityEvent -Start $segmentStart -End $activeStart -Snapshot $current
            $segmentStart = $activeStart
            $current = $next
            $currentKey = Get-SnapshotKey -Snapshot $current
            continue
        }

        if ($nextKey -ne $currentKey) {
            Write-ActivityEvent -Start $segmentStart -End $now -Snapshot $current
            $segmentStart = $now
            $current = $next
            $currentKey = $nextKey
        }
    }
}
finally {
    Write-ActivityEvent -Start $segmentStart -End (Get-Date) -Snapshot $current
}
