$ErrorActionPreference = 'Stop'
$probe = $PSScriptRoot
$samples = [Collections.Generic.List[object]]::new()
$processSamples = [Collections.Generic.List[object]]::new()
while (-not (Test-Path -LiteralPath "$probe/done.txt")) {
    $now = [DateTimeOffset]::Now.ToString('o')
    $memory = Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory
    $processes = @(Get-CimInstance Win32_Process -Filter "name = 'rustc.exe' OR name = 'cargo.exe' OR name = 'link.exe' OR name = 'lld-link.exe'")
    $ownIds = [Collections.Generic.HashSet[uint32]]::new()
    foreach ($p in $processes) {
        if ($p.CommandLine -and $p.CommandLine.Replace('\','/') -like ('*' + $env:PROBE_BUILD.Replace('\','/') + '*')) { [void]$ownIds.Add($p.ProcessId) }
    }
    do {
        $added = $false
        foreach ($p in $processes) {
            if ($ownIds.Contains($p.ParentProcessId) -and $ownIds.Add($p.ProcessId)) { $added = $true }
        }
    } while ($added)
    [long]$ownPrivate = 0; [long]$allPrivate = 0; [long]$ownWorking = 0; $ownCount = 0
    foreach ($p in $processes) {
        $own = $ownIds.Contains($p.ProcessId)
        $allPrivate += [long]$p.PrivatePageCount
        if ($own) { $ownPrivate += [long]$p.PrivatePageCount; $ownWorking += [long]$p.WorkingSetSize; $ownCount++ }
        $crate = if ($p.CommandLine -match '--crate-name\s+(\S+)') { $Matches[1] } else { '' }
        $out = if ($p.CommandLine -match '--out-dir\s+(\S+)') { $Matches[1] } else { '' }
        $processSamples.Add([pscustomobject]@{time=$now; id=$p.ProcessId; name=$p.Name; own=$own; crate=$crate; is_test=($p.CommandLine -match '(?:^|\s)--test(?:\s|$)'); output=$out; private_bytes=[long]$p.PrivatePageCount; working_bytes=[long]$p.WorkingSetSize})
    }
    $sample = [pscustomobject]@{time=$now; committed_bytes=[long]$memory.CommittedBytes; commit_limit_bytes=[long]$memory.CommitLimit; available_bytes=[long]$memory.AvailableBytes; own_private_bytes=$ownPrivate; own_working_bytes=$ownWorking; all_build_private_bytes=$allPrivate; own_processes=$ownCount; all_build_processes=$processes.Count}
    $samples.Add($sample)
    $sample | ConvertTo-Json -Compress | Set-Content -LiteralPath "$probe/latest.json"
    $samples | Export-Csv -LiteralPath "$probe/samples.csv" -NoTypeInformation
    $processSamples | Export-Csv -LiteralPath "$probe/processes.csv" -NoTypeInformation
    Start-Sleep -Seconds 2
}
$samples | Export-Csv -LiteralPath "$probe/samples.csv" -NoTypeInformation
$processSamples | Export-Csv -LiteralPath "$probe/processes.csv" -NoTypeInformation
