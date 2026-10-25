# Installs what the TouchDeck app needs on a fresh Windows PC. Safe to run again.
#
#   pwsh -NoProfile -File app\scripts\setup-windows.ps1
#
# Rust comes from rustup with the stable MSVC toolchain. The MSVC linker and the Windows
# SDK come from the Visual Studio Build Tools C++ workload. WebView2 ships with Windows 11.
$ErrorActionPreference = 'Stop'

function Have($name) { [bool](Get-Command $name -ErrorAction SilentlyContinue) }

function WingetInstall($id, $extra = @()) {
    winget install --id $id --exact --silent --accept-package-agreements --accept-source-agreements @extra
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) { throw "winget install $id failed with $LASTEXITCODE" }
}

if (-not (Have 'rustup')) {
    WingetInstall 'Rustlang.Rustup'
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}
rustup default stable-msvc
rustup component add clippy rustfmt

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$msvc = if (Test-Path $vswhere) { & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath }
if (-not $msvc) {
    WingetInstall 'Microsoft.VisualStudio.2022.BuildTools' @('--override', '--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
}

$webview = Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue
if (-not $webview) { WingetInstall 'Microsoft.EdgeWebView2Runtime' }

if (-not (Have 'node')) { WingetInstall 'OpenJS.NodeJS.LTS' }
if (-not (Have 'pnpm')) { npm install --global pnpm@12 }

[pscustomobject]@{
    rustc   = (rustc --version)
    node    = (node --version)
    pnpm    = (pnpm --version)
    msvc    = if ($msvc) { $msvc } else { 'installed now' }
    webview = if ($webview) { $webview.pv } else { 'installed now' }
} | ConvertTo-Json
