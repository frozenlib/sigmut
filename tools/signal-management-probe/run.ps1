param(
    [Parameter(Mandatory)][string]$RunDirectory,
    [string]$BuildDirectory,
    [switch]$CleanWidgets
)
$ErrorActionPreference = 'Stop'
$probe = [IO.Path]::GetFullPath($RunDirectory)
$fixture = Split-Path $probe
if (-not $BuildDirectory) { $BuildDirectory = "$probe/build" }
$build = [IO.Path]::GetFullPath($BuildDirectory)
if (-not $build.StartsWith("$fixture\", [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Build directory must be inside the fixture.'
}
New-Item -ItemType Directory -Force $probe | Out-Null
if (Test-Path "$probe/done.txt") { throw 'Use a fresh run directory.' }
Copy-Item "$PSScriptRoot/monitor.ps1" "$probe/monitor.ps1"
$env:PROBE_BUILD = $build
$monitor = Start-Process pwsh -ArgumentList @('-NoProfile', '-File', "$probe/monitor.ps1") -WindowStyle Hidden -PassThru
$env:RUSTC_WRAPPER = "$fixture/wrapper.exe"
$env:PROBE_RUN = $probe
$env:RUSTC_BOOTSTRAP = '1'
$start = [DateTimeOffset]::Now
$code = -1
try {
    Push-Location "$fixture/source"
    try {
        if ($CleanWidgets) {
            cargo clean -p acoui-widgets --offline --target-dir $build *> "$probe/clean.log"
            if ($LASTEXITCODE -ne 0) { throw 'cargo clean failed' }
            if (Get-ChildItem "$build/debug/incremental/acoui_widgets-*" -ErrorAction SilentlyContinue) {
                throw 'Widgets incremental artifacts remain after cleaning.'
            }
        }
        cargo test -p acoui-widgets --lib --no-run --locked --offline --jobs 2 --target-dir $build --verbose *> "$probe/build.log"
        $code = $LASTEXITCODE
    } finally { Pop-Location }
} finally {
    Set-Content "$probe/done.txt" 'done'
    $monitor.WaitForExit()
}
[pscustomobject]@{exit_code=$code; elapsed_seconds=([DateTimeOffset]::Now-$start).TotalSeconds; build_directory=$build; clean_widgets=[bool]$CleanWidgets; command='cargo test -p acoui-widgets --lib --no-run --locked --offline --jobs 2; line-tables-only; mono stats/time-passes'} | ConvertTo-Json | Set-Content "$probe/summary.json"
exit $code
