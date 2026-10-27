# Starts/stops only an extracted production candidate, using fake input and scratch state.
param(
    [Parameter(Mandatory)][string]$Scratch,
    [string]$PackageDir,
    [switch]$Stop
)
$ErrorActionPreference = 'Stop'
$scratchPath = [IO.Path]::GetFullPath($Scratch)
$recordPath = Join-Path $scratchPath 'smoke.json'

function Get-DefaultSnapshot {
    # Tauri uses Known Folders on Windows, not environment overrides.
    $roaming = [Environment]::GetFolderPath('ApplicationData')
    $local = [Environment]::GetFolderPath('LocalApplicationData')
    $roots = @(
        (Join-Path $roaming 'io.github.duresa7.touchdeck'),
        (Join-Path $local 'io.github.duresa7.touchdeck'),
        (Join-Path $roaming 'touchdeck'),
        (Join-Path $local 'touchdeck')
    )
    $entries = foreach ($root in $roots) {
        if (Test-Path -LiteralPath $root) {
            [ordered]@{ path = $root; kind = 'directory' }
            Get-ChildItem -LiteralPath $root -Recurse -Force | Sort-Object FullName | ForEach-Object {
                if ($_.PSIsContainer) {
                    [ordered]@{ path = $_.FullName; kind = 'directory' }
                } else {
                    [ordered]@{ path = $_.FullName; kind = 'file'; bytes = $_.Length;
                        modified = $_.LastWriteTimeUtc.ToString('o'); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
                }
            }
        } else {
            [ordered]@{ path = $root; kind = 'absent' }
        }
    }
    ConvertTo-Json -InputObject @($entries) -Depth 5 -Compress
}

if ($Stop) {
    $record = Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json
    $child = Get-Process -Id $record.pid -ErrorAction SilentlyContinue
    if ($child) {
        if ($child.Path -ne $record.executable -or $child.StartTime.ToUniversalTime().Ticks -ne $record.started_ticks) {
            throw 'Recorded process identity changed; refusing to stop another process.'
        }
        # CloseMainWindow sends WM_CLOSE, not keyboard input.
        $null = $child.CloseMainWindow()
        if (-not $child.WaitForExit(10000)) { $child.Kill(); $child.WaitForExit() }
    }
    $after = Get-DefaultSnapshot
    $before = Get-Content -LiteralPath (Join-Path $scratchPath 'default-paths-before.json') -Raw
    $after | Set-Content -LiteralPath (Join-Path $scratchPath 'default-paths-after.json')
    if ($after.Trim() -ne $before.Trim()) { throw 'Default app/source paths changed during smoke; inspect snapshots.' }
    $record | Add-Member -NotePropertyName default_paths_unchanged -NotePropertyValue $true -Force
    $record | Add-Member -NotePropertyName stopped -NotePropertyValue $true -Force
    $record | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $recordPath
    Get-Content -LiteralPath $recordPath -Raw
    exit 0
}

if (-not $PackageDir) { throw 'Pass -PackageDir for a start, or -Stop for an existing smoke.' }
if (Test-Path -LiteralPath $scratchPath) { throw 'Scratch must be a new directory.' }
$package = (Resolve-Path -LiteralPath $PackageDir).Path
$executable = Join-Path $package 'touchdeck.exe'
if (-not (Test-Path -LiteralPath $executable)) { throw 'Extract the entire production archive first.' }
New-Item -ItemType Directory -Path $scratchPath | Out-Null
Get-DefaultSnapshot | Set-Content -LiteralPath (Join-Path $scratchPath 'default-paths-before.json')
$working = Join-Path $scratchPath 'empty-working-directory'
New-Item -ItemType Directory -Path $working | Out-Null
$start = [Diagnostics.ProcessStartInfo]::new($executable)
$start.UseShellExecute = $false
$start.WorkingDirectory = $working
$start.Environment['TOUCHDECK_MOCK'] = '1'
$start.Environment['TOUCHDECK_INPUT'] = 'fake'
$start.Environment['TOUCHDECK_APP_DATA_DIR'] = Join-Path $scratchPath 'app'
$start.Environment['TOUCHDECK_CONFIG_DIR'] = Join-Path $scratchPath 'source-config'
# Prove lib.rs data_directory rather than WebView2's higher-priority environment override.
$null = $start.Environment.Remove('WEBVIEW2_USER_DATA_FOLDER')
foreach ($name in @('TEMP', 'TMP', 'APPDATA', 'LOCALAPPDATA')) {
    $folder = Join-Path $scratchPath $name.ToLowerInvariant()
    New-Item -ItemType Directory -Path $folder | Out-Null
    $start.Environment[$name] = $folder
}
$child = [Diagnostics.Process]::Start($start)
$record = [ordered]@{ pid = $child.Id; executable = $executable;
    started_ticks = $child.StartTime.ToUniversalTime().Ticks; scratch = $scratchPath;
    mock = $true; input = 'fake'; os_incidental_writes_measured = $false }
$record | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $recordPath
$webview = Join-Path $scratchPath 'app/webview/EBWebView'
$deadline = [DateTime]::UtcNow.AddSeconds(30)
while (-not (Test-Path -LiteralPath $webview -PathType Container)) {
    if ($child.HasExited) { throw "TouchDeck exited before creating isolated WebView data (exit $($child.ExitCode))." }
    if ([DateTime]::UtcNow -ge $deadline) {
        $child.Kill()
        $child.WaitForExit()
        throw "WebView data_directory override did not create $webview."
    }
    Start-Sleep -Milliseconds 200
}
$record['webview_data_directory'] = $webview
$record['webview_environment_override'] = $false
$record | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $recordPath
$record | ConvertTo-Json -Depth 6
