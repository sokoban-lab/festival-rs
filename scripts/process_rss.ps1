# Run a process and report its peak working set.
#
#   process_rss.ps1 -Out <stdout file> -Exe <exe> -ArgList "a|b|c" [-Csv samples.csv] [-IntervalMs 50]
#
# Prints "<peak bytes> <exit code>" on stdout. Arguments are pipe-separated so
# that solver flags such as "-from" are never parsed as PowerShell parameters.
# stdout/stderr are drained asynchronously into <Out> and <Out>.err; the
# process object is the solver itself, so the sampled working set is its own.
param(
    [Parameter(Mandatory = $true)][string]$Out,
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$ArgList = "",
    [string]$Csv = "",
    [int]$IntervalMs = 50
)

$ErrorActionPreference = "Stop"

$argv = @()
if ($ArgList -ne "") {
    $argv = $ArgList -split '\|'
}

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Exe
$psi.Arguments = (($argv | ForEach-Object { '"' + $_ + '"' }) -join " ")
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true

$p = New-Object System.Diagnostics.Process
$p.StartInfo = $psi
$null = $p.Start()

$stdoutTask = $p.StandardOutput.ReadToEndAsync()
$stderrTask = $p.StandardError.ReadToEndAsync()

$peak = [int64]0
$samples = New-Object System.Collections.Generic.List[string]
if ($Csv -ne "") {
    $samples.Add("ms,bytes")
}
$sw = [System.Diagnostics.Stopwatch]::StartNew()

while (-not $p.HasExited) {
    try {
        $p.Refresh()
        $ws = [int64]$p.WorkingSet64
        if ($ws -gt $peak) { $peak = $ws }
        if ($Csv -ne "") { $samples.Add("$($sw.ElapsedMilliseconds),$ws") }
    } catch {
        # The process can vanish between HasExited and Refresh.
    }
    Start-Sleep -Milliseconds $IntervalMs
}

$p.WaitForExit()
try {
    $p.Refresh()
    if ([int64]$p.PeakWorkingSet64 -gt $peak) { $peak = [int64]$p.PeakWorkingSet64 }
} catch {
}

[System.IO.File]::WriteAllText((Join-Path (Get-Location) $Out), $stdoutTask.Result)
[System.IO.File]::WriteAllText((Join-Path (Get-Location) "$Out.err"), $stderrTask.Result)

if ($Csv -ne "") {
    [System.IO.File]::WriteAllLines((Join-Path (Get-Location) $Csv), $samples)
}

Write-Output ("{0} {1}" -f $peak, $p.ExitCode)
exit 0
