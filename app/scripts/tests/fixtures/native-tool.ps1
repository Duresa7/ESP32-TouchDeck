# Only launched by the isolated test worker's .cmd fixtures. No host tool calls.
param([string]$Tool)
$ErrorActionPreference = 'Stop'
$root = $env:TOUCHDECK_SETUP_FIXTURE
$config = Get-Content "$root/config.json" -Raw | ConvertFrom-Json
$call = @{ tool = $Tool; arguments = @($args) } | ConvertTo-Json -Compress
Add-Content "$root/calls.jsonl" $call
if ($config.failTool -eq $Tool -and (!$config.failArgument -or $config.failArgument -in $args)) {
    [Console]::Error.WriteLine('fixture native command failure')
    exit 17
}
switch ($Tool) {
    'winget' {
        if ($args[0] -eq 'show') {
            if ($config.noNode24) { '26.0.0' } else { 'Versions'; '26.0.0'; '24.9.0'; '24.22.1'; '24.10.0' }
        } else {
            $id = $args[([array]::IndexOf($args, '--id') + 1)]
            switch ($id) {
                'OpenJS.NodeJS.LTS' {
                    if (!$config.noInstalledNode) {
                        Copy-Item "$root/staged/node.cmd" "$root/installed/node.cmd"
                        Copy-Item "$root/staged/npm.cmd" "$root/installed/npm.cmd"
                    }
                }
                'Rustlang.Rustup' {
                    Copy-Item "$root/staged/rustup.cmd" "$root/installed/rustup.cmd"
                    Copy-Item "$root/staged/rustc.cmd" "$root/installed/rustc.cmd"
                }
                'Microsoft.VisualStudio.2022.BuildTools' { if (!$config.noInstalledMsvc) { Set-Content "$root/msvc" 'installed' } }
                'Microsoft.EdgeWebView2Runtime' { if (!$config.noInstalledWebView) { Set-Content "$root/webview" 'installed' } }
                default { throw "Unexpected installer: $id" }
            }
            Set-Content "$root/path-installed" 'registered PATH'
        }
    }
    'node' { if ($config.nodeVersion) { $config.nodeVersion } else { 'v24.22.1' } }
    'pnpm' {
        if ($config.placeholderPnpm -and (!(Test-Path "$root/pnpm-updated") -or $config.keepPlaceholder)) {
            throw 'Test refused to invoke the extensionless pnpm placeholder'
        }
        if (!(Test-Path "$root/pnpm-updated") -and $config.pnpmVersion) { $config.pnpmVersion }
        elseif ($config.noopPnpm) { }
        elseif ($config.stalePnpm) { '11.0.0' }
        else { $config.expectedPnpm }
    }
    'npm' {
        if ($args[0] -eq '--version') {
            if ($config.npmVersion) { $config.npmVersion } else { '12.2.0' }
            break
        }
        if ($args[0] -ne 'install') { throw 'Unexpected npm fixture command' }
        $npmMajor = if ($config.npmVersion) { [int]($config.npmVersion -split '\.')[0] } else { 12 }
        if ($npmMajor -ge 12 -and '--allow-scripts=pnpm' -notin $args) {
            throw 'npm 12 fixture requires --allow-scripts=pnpm to repair pnpm'
        }
        if ($npmMajor -lt 12 -and '--allow-scripts=pnpm' -in $args) {
            throw 'npm 11 fixture does not support --allow-scripts'
        }
        Set-Content "$root/pnpm-updated" 'installed'
        $destination = if (Test-Path "$root/bin/pnpm.cmd") { "$root/bin/pnpm.cmd" } else { "$root/installed/pnpm.cmd" }
        Copy-Item "$root/staged/pnpm.cmd" $destination
        if ($config.keepPlaceholder) {
            (Get-Content $destination -Raw).Replace('node_modules\pnpm\bin\pnpm.exe', 'node_modules\pnpm\pnpm') | Set-Content $destination
        }
        Set-Content "$root/path-installed" 'registered PATH'
    }
    'rustup' { }
    'rustc' { 'rustc 1.92.0 (fixture)' }
    default { throw "Unexpected tool: $Tool" }
}
exit 0
