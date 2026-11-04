param(
    [string]$AssetsDir,
    [string]$ArtifactDir = "$env:RUNNER_TEMP/upgrade-from-02"
)

if ($env:GITHUB_ACTIONS -cne 'true' -or $env:RUNNER_ENVIRONMENT -cne 'github-hosted') {
    Write-Error 'Upgrade testing requires a disposable GitHub-hosted runner.' -ErrorAction Continue
    exit 1
}
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Upgrade testing requires Windows.' }
$null = New-Item -ItemType Directory -Force -Path $ArtifactDir
$oldDir = Join-Path $env:LOCALAPPDATA 'TouchDeck'
$newDir = Join-Path $env:LOCALAPPDATA 'Programs\Open TouchDeck'
$oldExe = Join-Path $oldDir 'touchdeck.exe'
$newExe = Join-Path $newDir 'Open TouchDeck.exe'
$companion = Join-Path $newDir 'td-companion.exe'
$dataDirs = @(
    (Join-Path $env:LOCALAPPDATA 'io.github.duresa7.touchdeck'),
    (Join-Path $env:APPDATA 'io.github.duresa7.touchdeck'),
    (Join-Path $env:APPDATA 'Open TouchDeck')
)
$runtime = Join-Path $dataDirs[0] 'runtime\companion.json'
$checks = [Collections.Generic.List[object]]::new()
$snapshots = [ordered]@{}
$facts = [ordered]@{
    status = 'running'; checks = $checks; snapshots = $snapshots; failure = $null
    untested = @('Physical Device serial contention', 'Tray icon appearance', 'Actual Windows sign-in', 'Layout import and migration semantics')
}

function Assert-That([bool]$Condition, [string]$Message) {
    $checks.Add(@{ assertion = $Message; passed = $Condition })
    if (-not $Condition) { throw $Message }
    Write-Host "PASS: $Message"
}

function Wait-Until([scriptblock]$Condition, [int]$Seconds = 60) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if (& $Condition) { return $true }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    return $false
}

function Running([string]$Path) {
    @(Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($Path)) -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $Path })
}

function Run-Installer([string]$Path) {
    $process = Start-Process -FilePath $Path -ArgumentList '/S' -WindowStyle Hidden -PassThru
    if (-not $process.WaitForExit(180000)) {
        $process.Kill($true)
        throw 'Installer timed out.'
    }
    Assert-That ($process.ExitCode -eq 0) 'Silent installer or uninstaller exits successfully.'
}

function Snapshot([string]$Name) {
    $entries = @(foreach ($root in @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
    )) {
        if (Test-Path $root) {
            Get-ChildItem $root | Get-ItemProperty | Where-Object {
                $_.PSObject.Properties['DisplayName'] -and $_.DisplayName -match 'TouchDeck'
            } | Select-Object DisplayName, DisplayVersion, InstallLocation, UninstallString
        }
    })
    $startup = @(foreach ($hive in @('HKCU', 'HKLM')) {
        foreach ($kind in @('Run', 'RunOnce')) {
            $key = "${hive}:\Software\Microsoft\Windows\CurrentVersion\$kind"
            if (Test-Path $key) {
                (Get-ItemProperty $key).PSObject.Properties | Where-Object {
                    $_.Name -notmatch '^PS' -and "$($_.Name) $($_.Value)" -match 'TouchDeck'
                } | ForEach-Object { @{ key = $key; name = $_.Name; value = [string]$_.Value } }
            }
        }
    })
    $tasks = @(Get-ScheduledTask | Where-Object {
        $actions = $_.Actions | Select-Object Execute, Arguments | ConvertTo-Json -Compress
        "$($_.TaskName) $actions" -match 'TouchDeck'
    } | Select-Object TaskName, TaskPath, State)
    $shell = New-Object -ComObject WScript.Shell
    $links = @(foreach ($folder in @('Programs', 'CommonPrograms', 'Startup', 'CommonStartup', 'DesktopDirectory', 'CommonDesktopDirectory')) {
        $root = [Environment]::GetFolderPath($folder)
        if ($root -and (Test-Path $root)) {
            Get-ChildItem $root -Filter '*.lnk' -Recurse | ForEach-Object {
                $link = $shell.CreateShortcut($_.FullName)
                if ("$($_.Name) $($link.TargetPath)" -match 'TouchDeck') {
                    @{ path = $_.FullName; target = $link.TargetPath; arguments = $link.Arguments }
                }
            }
        }
    })
    $folders = @(foreach ($dir in @($oldDir, $newDir) + $dataDirs) {
        @{ path = $dir; exists = (Test-Path $dir); files = @(
            if (Test-Path $dir) { Get-ChildItem $dir -Recurse -File | ForEach-Object { $_.FullName.Substring($dir.Length + 1) } }
        ) }
    })
    $snapshots[$Name] = @{
        uninstall = $entries; startup = $startup; tasks = $tasks; shortcuts = $links; folders = $folders
        processes = @(foreach ($path in @($oldExe, $newExe, $companion)) {
            Running $path | Select-Object ProcessName, Id, Path
        })
    }
}

function Assert-NewStarts {
    $null = Start-Process -FilePath $newExe -PassThru
    Assert-That (Wait-Until { @(Running $companion).Count -eq 1 -and (Test-Path $runtime) }) 'The installed Companion starts and publishes its runtime file.'
    Assert-That (Wait-Until { @(Running $newExe | Where-Object { $_.MainWindowHandle -ne 0 }).Count -gt 0 }) 'The new app opens a window.'
}

try {
    $release = Get-Content (Join-Path $AssetsDir 'release-manifest.json') -Raw | ConvertFrom-Json
    Assert-That ($release.test_signing -eq $true) 'The candidate uses disposable test signing.'
    $facts.candidate_version = $release.version
    Assert-That (-not (Test-Path $oldDir) -and -not (Test-Path $newDir)) 'Neither app is installed initially.'
    foreach ($dir in $dataDirs) { Assert-That (-not (Test-Path $dir)) 'The runner starts without app data.' }
    Snapshot 'clean'
    $legacy = Join-Path $ArtifactDir 'legacy-setup.exe'
    Invoke-WebRequest 'https://github.com/Duresa7/OpenTouchDeck/releases/download/v0.2.0/TouchDeck-windows-x64-setup.exe' -OutFile $legacy
    Assert-That ((Get-FileHash $legacy -Algorithm SHA256).Hash -eq 'aff9db98d74c2f59108e43d26068c22052e84df80d0625ee33a72d73baac6a04') 'The published 0.2 installer matches its pinned release digest.'
    Run-Installer $legacy
    Assert-That (Test-Path $oldExe) '0.2 installs in its expected folder.'
    Snapshot 'legacy-installed'
    $null = Start-Process -FilePath $oldExe -PassThru
    Assert-That (Wait-Until { @(Running $oldExe | Where-Object { $_.MainWindowHandle -ne 0 }).Count -gt 0 }) '0.2 opens a window.'
    Snapshot 'legacy-started'
    $sentinels = @{}
    foreach ($dir in $dataDirs[0..1]) {
        $null = New-Item -ItemType Directory -Force -Path $dir
        $path = Join-Path $dir 'upgrade-02-sentinel.bin'
        [IO.File]::WriteAllBytes($path, [byte[]]@(0, 13, 10, 255))
        $sentinels[$path] = (Get-FileHash $path).Hash
    }
    Run-Installer (Join-Path $AssetsDir $release.installer)
    Snapshot 'candidate-installed'
    Assert-NewStarts
    Snapshot 'both-started'
    Assert-That (Test-Path $oldExe) 'Installing the candidate leaves the legacy app installed.'
    Assert-That (@($snapshots['both-started'].uninstall | Where-Object { $_.DisplayName -eq 'Open TouchDeck' }).Count -eq 1) 'The candidate has its own uninstall entry.'
    Assert-That (@($snapshots['both-started'].uninstall | Where-Object { $_.DisplayName -eq 'TouchDeck' }).Count -eq 1) 'The legacy uninstall entry remains available.'
    $installedHashes = @{}
    Get-ChildItem $newDir -Recurse -File | ForEach-Object { $installedHashes[$_.FullName] = (Get-FileHash $_.FullName).Hash }
    Running $oldExe | Stop-Process -Force
    Assert-That (Wait-Until { @(Running $oldExe).Count -eq 0 }) '0.2 is closed before uninstalling it.'
    Run-Installer (Join-Path $oldDir 'uninstall.exe')
    Assert-That (Wait-Until { -not (Test-Path $oldExe) -and -not (Test-Path (Join-Path $oldDir 'uninstall.exe')) } 120) 'The legacy uninstaller removes its binaries.'
    Snapshot 'legacy-uninstalled'
    foreach ($path in $installedHashes.Keys) {
        Assert-That ((Test-Path $path) -and (Get-FileHash $path).Hash -eq $installedHashes[$path]) "Legacy uninstall preserves candidate file: $($path.Substring($newDir.Length + 1))"
    }
    foreach ($path in $sentinels.Keys) {
        Assert-That ((Test-Path $path) -and (Get-FileHash $path).Hash -eq $sentinels[$path]) 'Legacy uninstall preserves the shared-folder sentinel.'
    }
    foreach ($path in @($newExe, $companion)) { Running $path | Stop-Process -Force }
    Assert-That (Wait-Until { @(Running $newExe).Count -eq 0 -and @(Running $companion).Count -eq 0 }) 'The candidate stops before the restart check.'
    if (Test-Path $runtime) { Remove-Item -LiteralPath $runtime }
    Assert-NewStarts
    Snapshot 'candidate-restarted'
    $candidateLinks = @($snapshots['both-started'].shortcuts | Where-Object { $_.target -eq $newExe })
    Assert-That ($candidateLinks.Count -gt 0) 'The candidate creates shortcuts to its executable.'
    foreach ($link in $candidateLinks) {
        Assert-That (@($snapshots['candidate-restarted'].shortcuts | Where-Object { $_.path -eq $link.path -and $_.target -eq $newExe }).Count -eq 1) 'Legacy uninstall preserves the candidate shortcut.'
    }
    Assert-That (@($snapshots['candidate-restarted'].uninstall | Where-Object { $_.DisplayName -eq 'TouchDeck' }).Count -eq 0) 'The legacy uninstall entry is removed.'
    Assert-That (@($snapshots['candidate-restarted'].startup | Where-Object { $_.name -eq 'Open TouchDeck' -and $_.value -eq "`"$companion`" --packaged" }).Count -eq 1) 'The new Companion remains registered for sign-in.'
    $facts.status = 'passed'
} catch {
    $facts.status = 'failed'
    $facts.failure = $_.Exception.Message
} finally {
    foreach ($path in @($oldExe, $newExe, $companion)) { Running $path | Stop-Process -Force -ErrorAction SilentlyContinue }
    $json = $facts | ConvertTo-Json -Depth 12
    foreach ($name in @('LOCALAPPDATA', 'APPDATA', 'RUNNER_TEMP', 'GITHUB_WORKSPACE', 'USERPROFILE', 'HOME', 'COMPUTERNAME')) {
        $value = [Environment]::GetEnvironmentVariable($name)
        if ($value) {
            $escaped = (ConvertTo-Json $value -Compress).Trim('"')
            $json = $json.Replace($escaped, "%$name%").Replace($value.Replace('\', '/'), "%$name%")
        }
    }
    [IO.File]::WriteAllText((Join-Path $ArtifactDir 'facts.json'), $json)
    Write-Host "Upgrade scenario: $($facts.status). See sanitized facts.json."
}
if ($facts.status -ne 'passed') { exit 1 }
