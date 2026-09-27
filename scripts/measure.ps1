param([int]$Seconds = 60, [int]$Repetitions = 3)
$ErrorActionPreference = 'Stop'
$nativeRoot = Split-Path $PSScriptRoot
$exe = Join-Path $nativeRoot 'target/release/isle-native.exe'
$outputDir = Join-Path $nativeRoot 'artifacts/measurement-v2'
New-Item -ItemType Directory -Force $outputDir | Out-Null
$scenarios = @(
    @{ Name = 'compact-paused'; Args = @('--paused') },
    @{ Name = 'music-simulated'; Args = @('--page','music','--long-title') },
    @{ Name = 'page-cycle'; Args = @('--scripted') }
)
$rows = @()
foreach ($scenario in $scenarios) {
    for ($run = 1; $run -le $Repetitions; $run++) {
        $log = Join-Path $outputDir "$($scenario.Name)-$run.json"
        $argsList = $scenario.Args + @('--benchmark','--exit-after',($Seconds + 7).ToString(),'--log',$log)
        $ownedProcess = Start-Process -FilePath $exe -ArgumentList $argsList -PassThru -WindowStyle Hidden
        try {
            Start-Sleep -Seconds 5
            $ownedProcess.Refresh()
            $previousCpu = $ownedProcess.TotalProcessorTime.TotalSeconds
            $previousTime = [DateTime]::UtcNow
            for ($sample = 0; $sample -lt $Seconds; $sample++) {
                Start-Sleep -Seconds 1
                $ownedProcess.Refresh()
                if ($ownedProcess.HasExited) { throw 'Prototype exited before sampling completed' }
                $now = [DateTime]::UtcNow
                $cpu = $ownedProcess.TotalProcessorTime.TotalSeconds
                $rows += [pscustomobject]@{
                    Scenario = $scenario.Name; Run = $run; Sample = $sample
                    CpuMachinePercent = 100 * ($cpu - $previousCpu) / ($now - $previousTime).TotalSeconds / [Environment]::ProcessorCount
                    PrivateMB = $ownedProcess.PrivateMemorySize64 / 1MB
                    WorkingSetMB = $ownedProcess.WorkingSet64 / 1MB
                    Handles = $ownedProcess.HandleCount; Threads = $ownedProcess.Threads.Count
                }
                $previousCpu = $cpu; $previousTime = $now
            }
            $ownedProcess.WaitForExit(10000) | Out-Null
        } finally {
            if (!$ownedProcess.HasExited) { $ownedProcess.Kill(); $ownedProcess.WaitForExit() }
        }
        $rows | Export-Csv -NoTypeInformation -Encoding utf8 (Join-Path $outputDir 'performance.csv')
        Write-Output "Completed $($scenario.Name) run $run"
    }
}
$summary = $rows | Group-Object Scenario,Run | ForEach-Object {
    [pscustomobject]@{
        ScenarioRun = $_.Name
        CpuMachinePercent = ($_.Group.CpuMachinePercent | Measure-Object -Average).Average
        PrivateMB = ($_.Group.PrivateMB | Measure-Object -Average).Average
        WorkingSetMB = ($_.Group.WorkingSetMB | Measure-Object -Average).Average
        PrivateGrowthMB = $_.Group[-1].PrivateMB - $_.Group[0].PrivateMB
        HandleGrowth = $_.Group[-1].Handles - $_.Group[0].Handles
    }
}
$summary | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $outputDir 'performance-summary.json')
$summary | Format-Table
