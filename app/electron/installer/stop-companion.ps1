# Stops the Companion installed in -InstallDir before the installer replaces or removes it.
# Exit codes: 0 stopped or not running, 3 a Device is busy, 1 it could not be stopped.
# Runs on Windows PowerShell 5.1, which every supported Windows has.
param([Parameter(Mandatory = $true)][string]$InstallDir)

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::Expect100Continue = $false
$target = [IO.Path]::GetFullPath((Join-Path $InstallDir 'td-companion.exe'))

function Get-Companions {
    # Exact path only: another install, a source build or another user's Companion is never touched.
    @(Get-Process -Name 'td-companion' -ErrorAction SilentlyContinue | Where-Object {
        $path = $null
        try { $path = $_.Path } catch { }
        $path -and [string]::Equals([IO.Path]::GetFullPath($path), $target, [StringComparison]::OrdinalIgnoreCase)
    })
}

function Invoke-Companion([string]$Origin, [string]$Method, [string]$Path, [string]$Body, [string]$Token) {
    $request = [Net.HttpWebRequest]::Create("$Origin/api/v1$Path")
    $request.Method = $Method
    $request.Proxy = $null
    $request.AllowAutoRedirect = $false
    $request.Timeout = 5000
    $request.ReadWriteTimeout = 5000
    $request.ContentType = 'application/json'
    if ($Token) { $request.Headers['Authorization'] = "Bearer $Token" }
    $bytes = [Text.Encoding]::UTF8.GetBytes($Body)
    $request.ContentLength = $bytes.Length
    if ($bytes.Length) {
        $stream = $request.GetRequestStream()
        try { $stream.Write($bytes, 0, $bytes.Length) } finally { $stream.Dispose() }
    }
    try { $response = $request.GetResponse() }
    catch [Net.WebException] {
        $response = $_.Exception.Response
        if (-not $response) { throw }
    }
    try {
        $reader = [IO.StreamReader]::new($response.GetResponseStream())
        $text = $reader.ReadToEnd()
        $reader.Dispose()
        $value = $null
        if ($text) { $value = $text | ConvertFrom-Json }
        return [pscustomobject]@{ Status = [int]$response.StatusCode; Body = $value }
    } finally { $response.Dispose() }
}

function Request-Shutdown {
    $runtimeFile = Join-Path $env:LOCALAPPDATA 'io.github.duresa7.touchdeck\runtime\companion.json'
    if (-not (Test-Path -LiteralPath $runtimeFile -PathType Leaf)) { return 0 }
    $runtime = [IO.File]::ReadAllText($runtimeFile) | ConvertFrom-Json
    $port = [int]$runtime.port
    if ($port -lt 1 -or $port -gt 65535 -or $runtime.token -cnotmatch '^tdp_[A-Za-z0-9_-]{43}$') { return 0 }
    $origin = "http://127.0.0.1:$port"
    # The pairing token goes only to a listener that proves it already knows it, as Electron checks.
    $nonce = [byte[]]::new(32)
    [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($nonce)
    $encode = { param([byte[]]$Bytes) [Convert]::ToBase64String($Bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_') }
    $proof = Invoke-Companion $origin 'POST' '/auth/proof' (@{ nonce = (& $encode $nonce) } | ConvertTo-Json -Compress) $null
    $hmac = [Security.Cryptography.HMACSHA256]::new([Text.Encoding]::UTF8.GetBytes($runtime.token))
    $message = [Text.Encoding]::UTF8.GetBytes("touchdeck-companion-proof`0") + $nonce
    $expected = & $encode $hmac.ComputeHash([byte[]]$message)
    if ($proof.Status -ne 200 -or $proof.Body.proof -cne $expected) { Write-Host 'The Companion proof did not match.'; return 0 }
    $session = Invoke-Companion $origin 'POST' '/auth/sessions' '{"kind":"window"}' $runtime.token
    if ($session.Status -ne 200 -or $session.Body.token -cnotmatch '^tds_[A-Za-z0-9_-]{43}$') { return 0 }
    $lock = Invoke-Companion $origin 'POST' '/system/lock' '' $session.Body.token
    if ($lock.Status -eq 409 -and $lock.Body -and $lock.Body.error -and $lock.Body.error.code -ceq 'system.busy') { return 3 }
    $stopped = Invoke-Companion $origin 'POST' '/system/shutdown' '' $session.Body.token
    Write-Host "The Companion answered shutdown with status $($stopped.Status)."
    return 0
}

if (-not (Get-Companions).Count) { exit 0 }
try {
    if ((Request-Shutdown) -eq 3) { Write-Host 'A Device is busy.'; exit 3 }
} catch { Write-Host "The Companion did not answer: $($_.Exception.Message)" }

$deadline = [DateTime]::UtcNow.AddSeconds(15)
while ((Get-Companions).Count -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
$left = Get-Companions
if ($left.Count) {
    Write-Host 'The Companion did not stop in 15 seconds; ending it.'
    $left | Stop-Process -Force -ErrorAction SilentlyContinue
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ((Get-Companions).Count -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
}
if ((Get-Companions).Count) { exit 1 }
Write-Host 'The Companion is stopped.'
exit 0
