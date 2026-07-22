param(
    [Parameter(Mandatory = $true)]
    [string]$ModelPath,

    [string]$ProbePath = "target\release\model_probe.exe",

    [switch]$SkipHardwareInventory
)

$ErrorActionPreference = "Stop"
# The harness checks native exit codes explicitly after capturing stdout/stderr.
# Keep PowerShell 7.3+'s optional native-error promotion from pre-empting that path.
$PSNativeCommandUseErrorActionPreference = $false

if (-not (Test-Path $ProbePath -PathType Leaf)) {
    throw "expected model probe at $ProbePath; build it with cargo build --release -p wenlan-core --bin model_probe"
}
if (-not (Test-Path $ModelPath -PathType Leaf)) {
    throw "expected GGUF model at $ModelPath"
}

$ProbePath = (Resolve-Path $ProbePath).Path
$ModelPath = (Resolve-Path $ModelPath).Path
$previousRustLog = $env:RUST_LOG
$env:RUST_LOG = "wenlan_core=info"

try {
    if (-not $SkipHardwareInventory) {
        Write-Host "==> Windows video controllers"
        Get-CimInstance Win32_VideoController |
            Select-Object Name, DriverVersion, AdapterRAM |
            Format-Table -AutoSize
    }

    Write-Host "==> Running Qwen model probe"
    $output = & $ProbePath $ModelPath 2>&1
    $exitCode = $LASTEXITCODE
    $text = ($output | Out-String)
    Write-Host $text

    if ($exitCode -ne 0) {
        throw "model probe exited with code $exitCode"
    }

    $backendMarker = "[llm_engine] inference backend=CPU (OpenMP)"
    if (-not $text.Contains($backendMarker)) {
        throw "expected backend marker '$backendMarker'"
    }
    $classificationMarker = "--- Verified classification: preference ---"
    if (-not $text.Contains($classificationMarker)) {
        throw "model probe did not produce the expected classification"
    }

    Write-Host "==> PASS: Qwen produced a valid classification on CPU (OpenMP)"
}
finally {
    if ($null -eq $previousRustLog) {
        Remove-Item Env:RUST_LOG -ErrorAction SilentlyContinue
    } else {
        $env:RUST_LOG = $previousRustLog
    }
}
