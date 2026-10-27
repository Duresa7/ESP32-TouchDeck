# Windows source-build prerequisites. Run from any directory with PowerShell 7:
#   pwsh -NoProfile -File app\scripts\setup-windows.ps1
# Dot-sourcing only defines functions (also used by the isolated setup tests).

function Get-InstalledPaths {
    [Environment]::GetEnvironmentVariable('Path', 'Machine')
    [Environment]::GetEnvironmentVariable('Path', 'User')
}

function Update-ProcessPath {
    # Preserve caller choices, then discover installer additions in this process.
    # Never persist PATH or discard process-only entries (for example a version manager).
    $paths = @($env:PATH) + @(Get-InstalledPaths) + @(
        "$env:USERPROFILE\.cargo\bin", "$env:APPDATA\npm", "$env:ProgramFiles\nodejs"
    )
    $env:PATH = (($paths -join ';') -split ';' | Where-Object { $_ } | Select-Object -Unique) -join ';'
}

function Find-Tool($name) {
    # Prefer native/cmd launchers over PowerShell shims requiring a policy change.
    Get-Command $name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
}

function Invoke-Checked($name, [string[]]$arguments, $helpText) {
    $command = Find-Tool $name
    if (-not $command) { throw "$name was not found on PATH. $helpText" }
    $output = @(& $command.Source @arguments)
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        $output | Out-Host
        throw "$name $($arguments -join ' ') failed with exit code $code. $helpText"
    }
    $output
}

function Install-WingetPackage($id, [string[]]$extra = @()) {
    Invoke-Checked 'winget' (@('install', '--id', $id, '--exact', '--source', 'winget', '--silent',
        '--accept-package-agreements', '--accept-source-agreements') + $extra) `
        "Check the installer output for $id; resolve any installation/restart requirement, then rerun setup. Install App Installer from Microsoft if winget is missing." | Out-Host
    Update-ProcessPath
}

function Get-MsvcInstallation {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path -LiteralPath $vswhere) {
        Invoke-Checked $vswhere @('-latest', '-products', '*', '-requires',
            'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath') `
            'Repair Visual Studio Installer and rerun setup.'
    }
}

function Get-WebViewVersion {
    # Microsoft's Evergreen detection: per-machine (32/64 bit) or per-user, pv > 0.
    $client = 'Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    foreach ($key in @("HKLM:\SOFTWARE\WOW6432Node\$client", "HKLM:\SOFTWARE\$client", "HKCU:\SOFTWARE\$client")) {
        $item = Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
        $version = $null
        if ($item -and [version]::TryParse([string]$item.pv, [ref]$version) -and $version -gt [version]'0.0.0.0') {
            return $item.pv
        }
    }
}

function Invoke-TouchDeckSetup {
    $ErrorActionPreference = 'Stop'
    # Handle native exit codes ourselves, including when the caller enables PS7's preference.
    $PSNativeCommandUseErrorActionPreference = $false
    if (-not $IsWindows) { throw 'This setup script requires Windows and PowerShell 7. See docs/setup.md for Linux.' }
    $nodeMajor = 24 # Matches .github/workflows/app.yml.
    $manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../package.json') -Raw | ConvertFrom-Json
    if ($manifest.packageManager -notmatch '^pnpm@(\d+\.\d+\.\d+)$') {
        throw 'app/package.json must declare an exact pnpm version in packageManager.'
    }
    $pnpmVersion = $Matches[1]
    Update-ProcessPath

    # Validate Node before making unrelated changes. Do not replace a version-manager selection.
    if (-not (Find-Tool 'node')) {
        $versions = Invoke-Checked 'winget' @('show', '--id', 'OpenJS.NodeJS.LTS', '--exact', '--source', 'winget', '--versions', '--accept-source-agreements') `
            'Install Node.js 24 from https://nodejs.org/en/download, open a new PowerShell, then rerun setup.'
        $nodeVersion = $versions | ForEach-Object {
            if ($_ -match "^\s*($nodeMajor\.\d+\.\d+)\s*$") { [version]$Matches[1] }
        } | Sort-Object -Descending | Select-Object -First 1
        if (-not $nodeVersion) {
            throw 'winget offers no Node.js 24 LTS version. Install Node.js 24 from https://nodejs.org/en/download, open a new PowerShell, then rerun setup.'
        }
        Install-WingetPackage 'OpenJS.NodeJS.LTS' @('--version', "$nodeVersion")
    }
    $nodeHelp = 'Install/select Node.js 24, inspect Get-Command node -All for a stale PATH entry, open a new PowerShell, then rerun setup.'
    $node = (Invoke-Checked 'node' @('--version') $nodeHelp) -join "`n"
    if ($node.Trim() -notmatch "^v$nodeMajor\.\d+\.\d+$") {
        throw "Expected Node.js 24; got '$($node.Trim())'. $nodeHelp"
    }

    $pnpmHelp = "Repair npm with Node.js 24, then run npm install --global pnpm@$pnpmVersion and rerun setup."
    $pnpm = ''
    if (Find-Tool 'pnpm') {
        # A broken existing shim is diagnosed instead of silently continuing or replacing it.
        $pnpm = (Invoke-Checked 'pnpm' @('--version') $pnpmHelp) -join "`n"
        if ([string]::IsNullOrWhiteSpace($pnpm)) {
            throw "pnpm --version returned no version (a no-op shim). Inspect Get-Command pnpm -All; remove/repair the stale shim. $pnpmHelp"
        }
    }
    if ($pnpm.Trim() -ne $pnpmVersion) {
        Invoke-Checked 'npm' @('install', '--global', "pnpm@$pnpmVersion") $pnpmHelp | Out-Host
        Update-ProcessPath
        $pnpm = (Invoke-Checked 'pnpm' @('--version') $pnpmHelp) -join "`n"
    }
    if ($pnpm.Trim() -ne $pnpmVersion) {
        throw "Expected pnpm $pnpmVersion; got '$($pnpm.Trim())' (possibly a stale/no-op shim). Inspect Get-Command pnpm -All; repair PATH/shims, open a new PowerShell, and rerun setup."
    }

    if (-not (Find-Tool 'rustup')) { Install-WingetPackage 'Rustlang.Rustup' }
    $rustHelp = 'Repair rustup (https://rustup.rs), check network access and the installer output, then rerun setup in a new PowerShell.'
    Invoke-Checked 'rustup' @('default', 'stable-msvc') $rustHelp | Out-Host
    Invoke-Checked 'rustup' @('component', 'add', 'clippy', 'rustfmt') $rustHelp | Out-Host
    $rustc = (Invoke-Checked 'rustc' @('--version') $rustHelp) -join "`n"
    if ($rustc -notmatch '^rustc \d+\.\d+\.\d+') { throw "rustc returned no valid version. $rustHelp" }

    $msvc = Get-MsvcInstallation
    if (-not $msvc) {
        Install-WingetPackage 'Microsoft.VisualStudio.2022.BuildTools' @('--override', '--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
        $msvc = Get-MsvcInstallation
        if (-not $msvc) { throw 'MSVC tools still not detected. Complete/restart the Visual Studio Installer with Desktop development with C++ and a Windows SDK, then rerun setup.' }
    }
    $webview = Get-WebViewVersion
    if (-not $webview) {
        Install-WingetPackage 'Microsoft.EdgeWebView2Runtime'
        $webview = Get-WebViewVersion
        if (-not $webview) { throw 'WebView2 Runtime still not detected. Complete the Evergreen Runtime installation from Microsoft, open a new PowerShell, then rerun setup.' }
    }
    [pscustomobject]@{ rustc = $rustc.Trim(); node = $node.Trim(); pnpm = $pnpm.Trim(); msvc = $msvc; webview = $webview } | ConvertTo-Json
}

if ($MyInvocation.InvocationName -ne '.') {
    try { Invoke-TouchDeckSetup } catch { Write-Error $_ -ErrorAction Continue; exit 1 }
}
