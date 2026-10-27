# Dependency-free, process-isolated tests. Never execute the installer on the host.
#   pwsh -NoProfile -File app/scripts/tests/setup-windows.test.ps1
param([string]$SetupScript = (Join-Path $PSScriptRoot '../setup-windows.ps1'))
$ErrorActionPreference = 'Stop'
if (!$IsWindows) { throw 'These .cmd process fixtures require Windows and PowerShell 7.' }
$SetupScript = (Resolve-Path -LiteralPath $SetupScript).Path
$pwsh = (Get-Process -Id $PID).Path
$manifest = Get-Content (Join-Path $PSScriptRoot '../../package.json') -Raw | ConvertFrom-Json
$expectedPnpm = $manifest.packageManager -replace '^pnpm@', ''
$cases = @(
    @{ name = 'ready tools and repeat'; config = @{ repeat = $true }; ok = $true; noInstall = $true },
    @{ name = 'npm 12 placeholder shim repaired without probing'; config = @{ placeholderPnpm = $true }; ok = $true; installPnpm = $true; pnpmProbes = 1 },
    @{ name = 'npm 11 placeholder shim repaired without unsupported flag'; config = @{ placeholderPnpm = $true; npmVersion = '11.6.2' }; ok = $true; installPnpm = $true; pnpmProbes = 1 },
    @{ name = 'native pnpm exe shim left alone'; config = @{}; ok = $true; noInstall = $true; pnpmProbes = 1 },
    @{ name = 'placeholder persists after repair without probing'; config = @{ placeholderPnpm = $true; keepPlaceholder = $true }; error = 'extensionless.*placeholder.*Repair npm'; pnpmProbes = 0 },
    @{ name = 'fresh Node npm pnpm rust via refreshed PATH'; config = @{ absent = @('node', 'npm', 'pnpm', 'rustup', 'rustc') }; ok = $true; installNode = $true },
    @{ name = 'stale Node'; config = @{ nodeVersion = 'v22.18.0' }; error = 'Expected Node.js 24.*Get-Command node -All'; noInstall = $true },
    @{ name = 'future Node'; config = @{ nodeVersion = 'v26.0.0' }; error = 'Expected Node.js 24'; noInstall = $true },
    @{ name = 'stale pnpm repaired'; config = @{ pnpmVersion = '11.0.0' }; ok = $true; installPnpm = $true },
    @{ name = 'fresh pnpm global PATH'; config = @{ absent = @('pnpm') }; ok = $true; installPnpm = $true },
    @{ name = 'fresh rustup PATH'; config = @{ absent = @('rustup', 'rustc') }; ok = $true },
    @{ name = 'pnpm still stale after npm'; config = @{ pnpmVersion = '11.0.0'; stalePnpm = $true }; error = 'Expected pnpm.*Get-Command pnpm -All' },
    @{ name = 'existing silent pnpm shim'; config = @{ noopPnpm = $true }; error = 'no version.*no-op shim'; noInstall = $true },
    @{ name = 'installed silent pnpm shim'; config = @{ absent = @('pnpm'); noopPnpm = $true }; error = 'Expected pnpm.*no-op shim' },
    @{ name = 'winget install failure'; config = @{ absent = @('node'); failTool = 'winget'; failArgument = 'install' }; error = 'winget install.*exit code 17.*rerun setup' },
    @{ name = 'winget query failure'; config = @{ absent = @('node'); failTool = 'winget'; failArgument = 'show' }; error = 'winget show.*exit code 17.*Node.js 24' },
    @{ name = 'winget missing'; config = @{ absent = @('node', 'winget') }; error = 'winget was not found.*Node.js 24' },
    @{ name = 'rustup installer failure'; config = @{ absent = @('rustup'); failTool = 'winget'; failArgument = 'Rustlang.Rustup' }; error = 'winget install.*Rustlang.Rustup.*exit code 17' },
    @{ name = 'MSVC installer failure'; config = @{ missingMsvc = $true; failTool = 'winget'; failArgument = 'Microsoft.VisualStudio.2022.BuildTools' }; error = 'winget install.*BuildTools.*exit code 17' },
    @{ name = 'WebView installer failure'; config = @{ missingWebView = $true; failTool = 'winget'; failArgument = 'Microsoft.EdgeWebView2Runtime' }; error = 'winget install.*WebView2Runtime.*exit code 17' },
    @{ name = 'LTS no longer offers Node24'; config = @{ absent = @('node'); noNode24 = $true }; error = 'no Node.js 24 LTS version'; noInstall = $true },
    @{ name = 'Node absent after successful installer'; config = @{ absent = @('node'); noInstalledNode = $true }; error = 'node was not found.*new PowerShell' },
    @{ name = 'npm native failure'; config = @{ absent = @('pnpm'); failTool = 'npm'; failArgument = 'install' }; error = 'npm install.*exit code 17.*Repair npm' },
    @{ name = 'npm version native failure'; config = @{ absent = @('pnpm'); failTool = 'npm'; failArgument = '--version' }; error = 'npm --version.*exit code 17.*Repair npm' },
    @{ name = 'npm invalid version'; config = @{ absent = @('pnpm'); npmVersion = 'not-a-version' }; error = 'npm --version returned no valid version.*Repair npm'; noInstall = $true },
    @{ name = 'npm missing'; config = @{ absent = @('pnpm', 'npm') }; error = 'npm was not found.*Repair npm' },
    @{ name = 'node native failure'; config = @{ failTool = 'node' }; error = 'node --version.*exit code 17' },
    @{ name = 'pnpm native failure'; config = @{ failTool = 'pnpm' }; error = 'pnpm --version.*exit code 17' },
    @{ name = 'rustup default native failure'; config = @{ failTool = 'rustup'; failArgument = 'default' }; error = 'rustup default.*exit code 17.*Repair rustup' },
    @{ name = 'rustup component native failure'; config = @{ failTool = 'rustup'; failArgument = 'component' }; error = 'rustup component.*exit code 17' },
    @{ name = 'rustc native failure'; config = @{ failTool = 'rustc' }; error = 'rustc --version.*exit code 17' },
    @{ name = 'missing MSVC and WebView installed'; config = @{ missingMsvc = $true; missingWebView = $true }; ok = $true },
    @{ name = 'MSVC not detected after installer'; config = @{ missingMsvc = $true; noInstalledMsvc = $true }; error = 'MSVC tools still not detected.*Windows SDK' },
    @{ name = 'WebView not detected after installer'; config = @{ missingWebView = $true; noInstalledWebView = $true }; error = 'WebView2 Runtime still not detected' }
)
$failed = 0
foreach ($case in $cases) {
    $root = Join-Path ([IO.Path]::GetTempPath()) "touchdeck-setup-$([guid]::NewGuid())"
    New-Item -ItemType Directory $root | Out-Null
    try {
        $case.config.expectedPnpm = $expectedPnpm
        $case.config | ConvertTo-Json | Set-Content "$root/config.json"
        $start = [Diagnostics.ProcessStartInfo]::new($pwsh)
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        foreach ($arg in @('-NoProfile', '-File', "$PSScriptRoot/fixtures/setup-worker.ps1", '-SetupScript', $SetupScript, '-FixtureRoot', $root)) {
            $start.ArgumentList.Add($arg)
        }
        $process = [Diagnostics.Process]::Start($start)
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        if (!$process.WaitForExit(45000)) { $process.Kill($true); throw 'Worker timed out' }
        $output = $stdoutTask.GetAwaiter().GetResult() + $stderrTask.GetAwaiter().GetResult()
        $calls = if (Test-Path "$root/calls.jsonl") { @(Get-Content "$root/calls.jsonl" | ConvertFrom-Json) } else { @() }
        if ($case.ok) {
            if ($process.ExitCode -ne 0) { throw "Expected success: $output" }
            if ($output -notmatch [regex]::Escape('"pnpm": "' + $expectedPnpm + '"')) { throw "Missing verified version summary: $output" }
        } elseif ($process.ExitCode -eq 0 -or $output -notmatch $case.error) {
            throw "Expected failure matching '$($case.error)', exit=$($process.ExitCode): $output"
        }
        $installs = @($calls | Where-Object { $_.arguments[0] -eq 'install' })
        if ($case.noInstall -and $installs.Count) { throw 'Unexpected install on ready or rejected tools' }
        if ($case.installNode -and !($installs | Where-Object { '24.22.1' -in $_.arguments -and 'OpenJS.NodeJS.LTS' -in $_.arguments })) {
            throw 'Did not select the highest available Node24 version'
        }
        if ($case.installPnpm -and !($installs | Where-Object { $_.tool -eq 'npm' -and "pnpm@$expectedPnpm" -in $_.arguments })) {
            throw 'Did not install the manifest pnpm pin'
        }
        foreach ($install in @($installs | Where-Object { $_.tool -eq 'npm' })) {
            $allowScripts = !$case.config.npmVersion -or [int]($case.config.npmVersion -split '\.')[0] -ge 12
            if (('--allow-scripts=pnpm' -in $install.arguments) -ne $allowScripts) { throw 'Incorrect npm script approval flag' }
            if ('--foreground-scripts' -notin $install.arguments) { throw 'Missing foreground install script output' }
        }
        if ($case.ContainsKey('pnpmProbes') -and @($calls | Where-Object { $_.tool -eq 'pnpm' }).Count -ne $case.pnpmProbes) {
            throw 'Unexpected pnpm probe count; placeholder shims must never be invoked'
        }
        Write-Output "PASS $($case.name)"
    } catch {
        $failed++
        Write-Output "FAIL $($case.name): $_"
    } finally {
        # Only this test's exact, verified temporary directory is removed.
        $resolved = [IO.Path]::GetFullPath($root)
        $temp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if (!$resolved.StartsWith($temp, [StringComparison]::OrdinalIgnoreCase) -or
            [IO.Path]::GetFileName($resolved) -notmatch '^touchdeck-setup-[0-9a-f-]+$') { throw 'Unsafe fixture cleanup path' }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
Write-Output "$($cases.Count - $failed)/$($cases.Count) setup cases passed"
if ($failed) { exit 1 }
