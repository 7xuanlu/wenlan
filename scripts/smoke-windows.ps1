$ErrorActionPreference = "Stop"

$Port    = 17878
$DataDir = New-Item -ItemType Directory -Path "$env:TEMP\origin-smoke-$([System.Guid]::NewGuid())"
$ExePath = "target\release\origin-server.exe"

if (-not (Test-Path $ExePath)) {
    throw "expected $ExePath to exist; build origin-server first"
}

$env:ORIGIN_BIND_ADDR = "127.0.0.1:$Port"
$env:ORIGIN_DATA_DIR  = $DataDir.FullName

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
    $store = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/api/memory/store" -Method POST `
        -ContentType "application/json" `
        -Body '{"content":"Windows smoke test memory"}'

    Write-Host "==> Search"
    $search = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/api/memory/search" -Method POST `
        -ContentType "application/json" `
        -Body '{"query":"Windows smoke","limit":3}'

    if (($search | ConvertTo-Json -Depth 10) -notmatch "Windows smoke test memory") {
        throw "search did not return the stored memory"
    }
    Write-Host "==> PASS"
}
finally {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $DataDir
}
