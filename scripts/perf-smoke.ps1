[CmdletBinding()]
param(
    [string]$BinaryPath,
    [ValidateRange(1, 86400)]
    [int]$DurationSeconds = 28800,
    [ValidateRange(1, 3600)]
    [int]$SampleSeconds = 60,
    [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($BinaryPath)) {
    $BinaryPath = Join-Path $PSScriptRoot '..\src-tauri\target\release\deeppi.exe'
}
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $PSScriptRoot '..\artifacts\perf-soak.json'
}
$binary = (Resolve-Path -LiteralPath $BinaryPath -ErrorAction Stop).Path
if (-not [System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath = Join-Path (Get-Location) $OutputPath
}
$output = [System.IO.Path]::GetFullPath($OutputPath)
$outputDirectory = Split-Path -Parent $output
New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null

function Get-DescendantProcessIds([int]$RootId) {
    $all = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $seen = [System.Collections.Generic.HashSet[int]]::new()
    $frontier = [System.Collections.Generic.List[int]]::new()
    $frontier.Add($RootId)
    $result = [System.Collections.Generic.List[int]]::new()

    while ($frontier.Count -gt 0) {
        $parentId = $frontier[0]
        $frontier.RemoveAt(0)
        foreach ($child in $all | Where-Object { [int]$_.ParentProcessId -eq $parentId }) {
            $childId = [int]$child.ProcessId
            if ($seen.Add($childId)) {
                $result.Add($childId)
                $frontier.Add($childId)
            }
        }
    }

    return @($result)
}

$process = Start-Process -FilePath $binary -PassThru
$samples = [System.Collections.Generic.List[object]]::new()
$trackedProcessIds = [System.Collections.Generic.HashSet[int]]::new()
$started = [DateTime]::UtcNow
$deadline = $started.AddSeconds($DurationSeconds)

try {
    Write-Host ("Started {0} (PID {1}); sampling for {2}s" -f $binary, $process.Id, $DurationSeconds)
    do {
        $ids = @($process.Id) + (Get-DescendantProcessIds $process.Id)
        $processes = @(Get-Process -Id $ids -ErrorAction SilentlyContinue)
        foreach ($id in $ids) {
            $trackedProcessIds.Add([int]$id) | Out-Null
        }
        $workingSet = ($processes | Measure-Object -Property WorkingSet64 -Sum).Sum
        $cpu = ($processes | Measure-Object -Property CPU -Sum).Sum
        $samples.Add([pscustomobject]@{
            timestamp = [DateTime]::UtcNow.ToString('o')
            processCount = $processes.Count
            workingSetMb = [Math]::Round(([double]$workingSet / 1MB), 2)
            cpuSeconds = [Math]::Round([double]$cpu, 2)
        })

        $remaining = [int][Math]::Ceiling(($deadline - [DateTime]::UtcNow).TotalSeconds)
        if ($remaining -gt 0) {
            Start-Sleep -Seconds ([Math]::Min($SampleSeconds, $remaining))
        }
    } while ([DateTime]::UtcNow -lt $deadline)
}
finally {
    if (Get-Process -Id $process.Id -ErrorAction SilentlyContinue) {
        & taskkill.exe /PID $process.Id /T /F | Out-Null
    }
    Start-Sleep -Seconds 2
    $remaining = @($trackedProcessIds | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
    $cleanupError = $null
    if ($remaining.Count -gt 0) {
        $cleanupError = "Process cleanup failed; remaining PIDs: $($remaining -join ', ')"
    }
    $samples | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath $output -Encoding utf8
    if ($samples.Count -gt 0) {
        $first = $samples[0]
        $last = $samples[$samples.Count - 1]
        Write-Host ("Samples: {0}; working set: {1}MB -> {2}MB; output: {3}" -f $samples.Count, $first.workingSetMb, $last.workingSetMb, $output)
    }
    if ($cleanupError) {
        throw $cleanupError
    }
}
