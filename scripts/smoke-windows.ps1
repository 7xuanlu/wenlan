param(
    [string]$ExePath = "target\release\wenlan-server.exe"
)

$ErrorActionPreference = "Stop"

$Port    = 17878
$DataDir = New-Item -ItemType Directory -Path "$env:TEMP\origin-smoke-$([System.Guid]::NewGuid())"

if (-not (Test-Path $ExePath)) {
    throw "expected $ExePath to exist; build wenlan-server first"
}

$ExePath = (Resolve-Path $ExePath).Path
$ExpectedOrtDll = (Resolve-Path (Join-Path (Split-Path -Parent $ExePath) "onnxruntime.dll")).Path
$Marker = "ORT_VECTOR_$([System.Guid]::NewGuid().ToString('N'))"
$env:WENLAN_BIND_ADDR = "127.0.0.1:$Port"
$env:WENLAN_DATA_DIR  = $DataDir.FullName
Remove-Item Env:ORT_DYLIB_PATH -ErrorAction SilentlyContinue

Write-Host "==> Starting daemon"
$proc = Start-Process -FilePath $ExePath -PassThru -WindowStyle Hidden

try {
    Write-Host "==> Waiting for /api/health"
    $healthy = $false
    for ($i = 0; $i -lt 30; $i++) {
        try {
            $resp = Invoke-WebRequest -Uri "http://127.0.0.1:$Port/api/health" -UseBasicParsing -TimeoutSec 1
            if ($resp.StatusCode -eq 200) { $healthy = $true; break }
        } catch { }
        Start-Sleep -Seconds 1
    }
    if (-not $healthy) { throw "daemon did not become healthy" }

    Write-Host "==> Store a memory"
    try {
        $StoreBody = @{
            content = "$Marker A cobalt lantern calibrates tidal clocks during winter."
            memory_type = "lesson"
        } | ConvertTo-Json
        $store = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/api/memory/store" -Method POST `
            -ContentType "application/json" `
            -Body $StoreBody
        Write-Host "    store ok: $($store | ConvertTo-Json -Compress -Depth 5)"
        if ([int]$store.chunks_created -lt 1) {
            throw "store returned no embedded chunks"
        }
    } catch {
        # ErrorDetails.Message works on both pwsh 5 (WebException) and pwsh 7
        # (HttpResponseMessage); pwsh 7 dropped GetResponseStream().
        $body = $_.ErrorDetails.Message
        $status = $_.Exception.Response.StatusCode.value__
        throw "store failed: status=$status body=$body"
    }

    Write-Host "==> Semantic search with no lexical overlap"
    $SearchBody = @{
        query = "blue lamp adjusts ocean timepieces"
        limit = 3
    } | ConvertTo-Json
    $search = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/api/memory/search" -Method POST `
        -ContentType "application/json" `
        -Body $SearchBody

    if (($search | ConvertTo-Json -Depth 10) -notmatch [regex]::Escape($Marker)) {
        throw "semantic search did not return the vector-only marker $Marker"
    }

    $LoadedOrtModules = @(
        Get-Process -Id $proc.Id -Module |
            Where-Object { $_.ModuleName -ieq "onnxruntime.dll" } |
            ForEach-Object { (Resolve-Path $_.FileName).Path }
    )
    if ($LoadedOrtModules.Count -ne 1) {
        throw "expected exactly one loaded onnxruntime.dll, got $($LoadedOrtModules -join ', ')"
    }
    if (-not [string]::Equals(
        $LoadedOrtModules[0],
        $ExpectedOrtDll,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw "loaded unexpected onnxruntime.dll: expected $ExpectedOrtDll, got $($LoadedOrtModules[0])"
    }
    Write-Host "    loaded exact ORT module: $($LoadedOrtModules[0])"
    Write-Host "==> PASS"
}
finally {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    # SQLite WAL -shm file can stay locked briefly after daemon exit on Windows.
    # Retry deletion a few times before giving up; don't fail the smoke on cleanup.
    for ($i = 0; $i -lt 5; $i++) {
        Remove-Item -Recurse -Force $DataDir -ErrorAction SilentlyContinue
        if (-not (Test-Path $DataDir)) { break }
        Start-Sleep -Milliseconds 500
    }
}
