param(
    [string]$InstallerPath,
    [string]$StatuslineFixture,
    [string]$ArtifactDir = "$env:RUNNER_TEMP/installer-lifecycle"
)

# Installation changes the real user profile and registry: never run on an operator's PC.
# Keep this guard before path resolution, directory creation, or any other side effect.
if ($env:GITHUB_ACTIONS -cne 'true' -or $env:RUNNER_ENVIRONMENT -cne 'github-hosted') {
    Write-Error 'Installer lifecycle requires GITHUB_ACTIONS=true and RUNNER_ENVIRONMENT=github-hosted.' -ErrorAction Continue
    exit 1
}

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Installer lifecycle requires Windows.' }
if (-not $InstallerPath -or -not $StatuslineFixture) { throw 'Pass -InstallerPath and -StatuslineFixture.' }
$installer = (Resolve-Path -LiteralPath $InstallerPath).Path
$fixture = (Resolve-Path -LiteralPath $StatuslineFixture).Path
$null = New-Item -ItemType Directory -Path $ArtifactDir -Force
$logPath = Join-Path $ArtifactDir 'lifecycle.log'
$summaryPath = Join-Path $ArtifactDir 'summary.json'
$scratch = Join-Path $env:RUNNER_TEMP ('touchdeck-lifecycle-' + [guid]::NewGuid().ToString('N'))
$installDir = Join-Path $env:LOCALAPPDATA 'TouchDeck'
$helperPath = Join-Path $installDir 'td-statusline.exe'
$configDir = Join-Path $env:APPDATA 'io.github.duresa7.touchdeck'
$stateDir = Join-Path $env:LOCALAPPDATA 'io.github.duresa7.touchdeck'
$claudeDir = Join-Path $env:USERPROFILE '.claude'
$settingsPath = Join-Path $claudeDir 'settings.json'
$children = [Collections.Generic.List[object]]::new()
$watch = $null
$checks = [Collections.Generic.List[object]]::new()
$summary = [ordered]@{
    status = 'running'; scenario = 'preconditions'; checks = $checks
    update_arguments = @('/P', '/UPDATE'); remaining = @(); failure = $null
}

function Protect-Log([string]$Text) {
    # Upload only these sanitized logs, never settings, app/WebView logs, or raw profiles.
    foreach ($name in @('LOCALAPPDATA', 'APPDATA', 'RUNNER_TEMP', 'GITHUB_WORKSPACE', 'USERPROFILE', 'HOME', 'COMPUTERNAME')) {
        $value = [Environment]::GetEnvironmentVariable($name)
        if ($value) {
            $Text = $Text.Replace($value, "%$name%").Replace($value.Replace('\', '/'), "%$name%")
        }
    }
    return [regex]::Replace($Text, '(?i)[a-z]:[\\/]Users[\\/][^\\/\s"'']+', '%USERPROFILE%')
}

function Write-Log([string]$Text) {
    $line = Protect-Log $Text
    Add-Content -LiteralPath $logPath -Value $line -Encoding utf8
    Write-Host $line
}

function Assert-That([bool]$Condition, [string]$Message) {
    $checks.Add([ordered]@{ scenario = $summary.scenario; assertion = $Message; passed = $Condition })
    if (-not $Condition) { throw $Message }
    Write-Log "PASS: $Message"
}

function Same-Bytes([string]$Path, [byte[]]$Expected) {
    return (Test-Path -LiteralPath $Path -PathType Leaf) -and
        [Convert]::ToBase64String([IO.File]::ReadAllBytes($Path)) -ceq [Convert]::ToBase64String($Expected)
}

function Start-Child([string]$Exe, [string[]]$Arguments = @(), [hashtable]$Variables = @{}, [switch]$HoldInput) {
    $start = [Diagnostics.ProcessStartInfo]::new($Exe)
    $start.UseShellExecute = $false
    $start.WorkingDirectory = $scratch
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($arg in $Arguments) { $start.ArgumentList.Add($arg) }
    foreach ($name in $Variables.Keys) {
        if ($null -eq $Variables[$name]) { $null = $start.Environment.Remove($name) }
        else { $start.Environment[$name] = $Variables[$name] }
    }
    $process = [Diagnostics.Process]::Start($start)
    $child = [pscustomobject]@{
        Process = $process; Out = $process.StandardOutput.ReadToEndAsync(); Err = $process.StandardError.ReadToEndAsync()
        Logged = $false
    }
    $children.Add($child)
    if (-not $HoldInput) { $process.StandardInput.Close() }
    return $child
}

function Finish-Child($Child, [string]$Label, [int]$TimeoutMs = 180000) {
    if (-not $Child.Process.WaitForExit($TimeoutMs)) {
        $Child.Process.Kill($true)
        $Child.Process.WaitForExit()
        throw "$Label timed out."
    }
    Write-Log "$Label exit code: $($Child.Process.ExitCode)"
    Write-Log ($Child.Out.GetAwaiter().GetResult())
    Write-Log ($Child.Err.GetAwaiter().GetResult())
    $Child.Logged = $true
    Assert-That ($Child.Process.ExitCode -eq 0) "$Label succeeds."
}

function Get-UninstallEntries {
    foreach ($root in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
                        'HKCU:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall')) {
        if (Test-Path -LiteralPath $root) {
            Get-ChildItem -LiteralPath $root | ForEach-Object {
                $entry = Get-ItemProperty -LiteralPath $_.PSPath
                if ($entry.PSObject.Properties['DisplayName'] -and $entry.DisplayName -eq 'TouchDeck') { $entry }
            }
        }
    }
}

function Assert-Payload {
    foreach ($relative in @('touchdeck.exe', 'td-statusline.exe', 'firmware/touchdeck.tdfw', 'install-manifest.json', 'uninstall.exe')) {
        Assert-That (Test-Path -LiteralPath (Join-Path $installDir $relative) -PathType Leaf) "Installed payload contains $relative."
    }
    $manifest = Get-Content -LiteralPath (Join-Path $installDir 'install-manifest.json') -Raw | ConvertFrom-Json
    $firmwareHash = (Get-FileHash -LiteralPath (Join-Path $installDir 'firmware/touchdeck.tdfw') -Algorithm SHA256).Hash
    Assert-That ($manifest.firmware.bundle -ceq 'touchdeck.tdfw' -and $manifest.firmware.sha256 -eq $firmwareHash) 'Installed firmware matches the install manifest SHA-256.'
    Assert-That ($manifest.helper_sha256 -eq (Get-FileHash -LiteralPath $helperPath -Algorithm SHA256).Hash) 'Installed helper matches the install manifest SHA-256.'
    $entries = @(Get-UninstallEntries)
    Assert-That ($entries.Count -eq 1) 'Exactly one per-user uninstall registry entry exists.'
    Assert-That ($entries[0].UninstallString.Contains($installDir, [StringComparison]::OrdinalIgnoreCase)) 'The per-user uninstall entry targets this installation.'
}

function Get-Remaining {
    foreach ($root in @(@('install', $installDir), @('config', $configDir), @('state', $stateDir), @('claude', $claudeDir))) {
        if (Test-Path -LiteralPath $root[1]) {
            [ordered]@{ root = $root[0]; path = '.'; kind = 'directory' }
            Get-ChildItem -LiteralPath $root[1] -Force -Recurse | Sort-Object FullName | ForEach-Object {
                [ordered]@{ root = $root[0]; path = [IO.Path]::GetRelativePath($root[1], $_.FullName)
                    kind = $(if ($_.PSIsContainer) { 'directory' } else { 'file' }) }
            }
        }
    }
}

function Get-FileUsers([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return 'absent' }
    try { $ids = [TouchDeckFileUsers]::Read($Path) } catch { return "query failed: $($_.Exception.Message)" }
    $users = foreach ($id in $ids) {
        if ($id -eq $PID) { continue }  # This query's own handle.
        $process = Get-CimInstance Win32_Process -Filter "ProcessId = $id" -ErrorAction SilentlyContinue
        if ($process) { "$($process.Name)#$id<-$($process.ParentProcessId)" } else { "#$id" }
    }
    return 'used by [' + (@($users) -join ', ') + ']'
}

function Remove-Probe([string]$Path, [string]$Label) {
    Write-Log "DIAG ${Label}: touchdeck.exe $(Get-FileUsers $Path)"
    try { [IO.File]::Delete($Path); Write-Log "DIAG ${Label}: deleting touchdeck.exe succeeds." }
    catch { Write-Log "DIAG ${Label}: deleting touchdeck.exe fails: $($_.Exception.InnerException.Message ?? $_.Exception.Message)" }
}

function Wait-Uninstaller {
    # NSIS copies the uninstaller to %TEMP%\~nsu*.tmp and lets the original exit; wait for the copy to finish.
    $copies = [Collections.Generic.HashSet[string]]::new()
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    do {
        $running = @(Get-Process | Where-Object { $_.Path -like '*\~nsu*.tmp\*' })
        foreach ($copy in $running) { $null = $copies.Add($copy.ProcessName) }
        if ($running.Count -eq 0 -and @(Get-UninstallEntries).Count -eq 0 -and
            -not (Test-Path -LiteralPath (Join-Path $installDir 'uninstall.exe')) -and
            -not (Test-Path -LiteralPath (Join-Path $installDir 'touchdeck.exe'))) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    Write-Log "Uninstaller copies seen: [$(@($copies) -join ', ')]"
}

function Stop-Watch([string]$Label) {
    if (-not $script:watch) { return }
    $events = $script:watch.Stop()
    $mainGone = $script:watch.AbsentAt(0)
    $uninstallerGone = $script:watch.AbsentAt(1)
    $script:watch = $null
    Write-Log "$Label file events: $($events -join '; ')"
    # The template deletes touchdeck.exe just before uninstall.exe; the hook's first retry comes 250 ms later.
    if ($uninstallerGone -gt 0 -and $mainGone - $uninstallerGone -ge 200) {
        Write-Log "${Label}: the template's delete left touchdeck.exe; the postuninstall retry removed it $($mainGone - $uninstallerGone) ms after uninstall.exe."
    }
}

function Write-Diagnostics([string]$Label) {
    # Evidence for lock failures: which processes hold the main binary or run from the install directory.
    $running = @(Get-CimInstance Win32_Process | Where-Object {
        $_.ExecutablePath -and $_.ExecutablePath.StartsWith("$installDir\", [StringComparison]::OrdinalIgnoreCase)
    } | ForEach-Object { "$($_.Name)#$($_.ProcessId)<-$($_.ParentProcessId)" })
    $uninstallers = @(Get-Process | Where-Object { $_.Path -like '*\~nsu*.tmp\*' } | ForEach-Object { "$($_.ProcessName)#$($_.Id)" })
    $webviews = @(Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue).Count
    $shells = @(Get-Process -Name explorer -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Id)/s$($_.SessionId)" })
    Write-Log ("DIAG ${Label}: touchdeck.exe $(Get-FileUsers (Join-Path $installDir 'touchdeck.exe')); " +
        "install-dir processes [$($running -join ', ')]; uninstallers [$($uninstallers -join ', ')]; " +
        "explorer [$($shells -join ', ')]; msedgewebview2 x$webviews")
}

try {
    Assert-That (-not (Test-Path -LiteralPath $installDir)) 'Runner starts without a TouchDeck installation.'
    Assert-That (@(Get-UninstallEntries).Count -eq 0) 'Runner starts without a TouchDeck uninstall entry.'
    Assert-That (-not (Test-Path -LiteralPath $settingsPath)) 'Runner starts without Claude settings to overwrite.'
    Assert-That (-not (Test-Path -LiteralPath $configDir) -and -not (Test-Path -LiteralPath $stateDir)) 'Runner starts without TouchDeck user data.'
    $null = New-Item -ItemType Directory -Path $scratch
    $elevated = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator)
    $realtime = try { (Get-MpComputerStatus -ErrorAction Stop).RealTimeProtectionEnabled } catch { 'unknown' }
    Write-Log "DIAG runner: elevated=$elevated defender-realtime=$realtime session=$((Get-Process -Id $PID).SessionId)"
    Write-Diagnostics 'before install'

    # An identical installer produces identical hashes/timestamps: NTFS file identity proves replacement.
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Threading;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class TouchDeckFileIdentity {
    [StructLayout(LayoutKind.Sequential)]
    private struct Info {
        public uint Attributes;
        public System.Runtime.InteropServices.ComTypes.FILETIME Creation, Access, Write;
        public uint Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
    }
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetFileInformationByHandle(SafeFileHandle file, out Info info);
    public static string Read(string path) {
        using (var file = File.Open(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete)) {
            Info info;
            if (!GetFileInformationByHandle(file.SafeFileHandle, out info)) throw new Win32Exception();
            return info.Volume + ":" + info.IndexHigh + ":" + info.IndexLow;
        }
    }
}
public static class TouchDeckFileUsers {
    [StructLayout(LayoutKind.Sequential)]
    private struct IoStatus { public IntPtr Status, Information; }
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern SafeFileHandle CreateFileW(string path, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
    [DllImport("ntdll.dll")]
    private static extern int NtQueryInformationFile(SafeFileHandle file, out IoStatus status, IntPtr info, int length, int infoClass);
    // FileProcessIdsUsingFileInformation (47) lists every process with a handle or image mapping on the file.
    public static long[] Read(string path) {
        using (var file = CreateFileW(path, 0x80, 7, IntPtr.Zero, 3, 0, IntPtr.Zero)) {
            if (file.IsInvalid) throw new Win32Exception();
            for (int size = 4096; size <= 1 << 20; size *= 2) {
                IntPtr buffer = Marshal.AllocHGlobal(size);
                try {
                    IoStatus status;
                    int result = NtQueryInformationFile(file, out status, buffer, size, 47);
                    if (result == unchecked((int)0xC0000004)) continue;
                    if (result != 0) throw new InvalidOperationException("NTSTATUS 0x" + result.ToString("X8"));
                    var ids = new long[Marshal.ReadInt32(buffer)];
                    for (int i = 0; i < ids.Length; i++) ids[i] = Marshal.ReadIntPtr(buffer, IntPtr.Size * (i + 1)).ToInt64();
                    return ids;
                } finally { Marshal.FreeHGlobal(buffer); }
            }
            throw new InvalidOperationException("Too many processes use the file.");
        }
    }
}
// Polls with attribute-only handles, which Restart Manager does not report, so NSIS never closes this script.
public sealed class TouchDeckFileWatch {
    private readonly string[] paths;
    private readonly long[] absentAt;
    private readonly Stopwatch clock = Stopwatch.StartNew();
    private readonly List<string> events = new List<string>();
    private readonly Dictionary<long, string> names = new Dictionary<long, string>();
    private readonly Thread thread;
    private readonly int self = Process.GetCurrentProcess().Id;
    private volatile bool stopping;
    public TouchDeckFileWatch(string[] paths) {
        this.paths = paths;
        absentAt = new long[paths.Length];
        thread = new Thread(Run) { IsBackground = true };
        thread.Start();
    }
    public void Mark(string label) { Add(label); }
    // Milliseconds since the watch started when the file last disappeared; -1 while it exists.
    public long AbsentAt(int index) { return absentAt[index]; }
    public string[] Stop() {
        stopping = true;
        thread.Join();
        lock (events) return events.ToArray();
    }
    private void Add(string text) {
        lock (events) if (events.Count < 400) events.Add(clock.ElapsedMilliseconds + "ms " + text);
    }
    private string Describe(long id) {
        string name;
        if (!names.TryGetValue(id, out name)) {
            if (id == self) name = "self#" + id;  // This script's own diagnostics or process launch.
            else try { using (var process = Process.GetProcessById((int)id)) name = process.ProcessName + "#" + id + "/s" + process.SessionId; }
            catch (ArgumentException) { name = "#" + id; }
            names[id] = name;
        }
        return name;
    }
    private void Run() {
        var present = new bool[paths.Length];
        var users = new HashSet<string>[paths.Length];
        for (int i = 0; i < paths.Length; i++) {
            present[i] = File.Exists(paths[i]);
            absentAt[i] = present[i] ? -1 : 0;
            users[i] = new HashSet<string>();
        }
        while (!stopping) {
            for (int i = 0; i < paths.Length; i++) {
                string file = Path.GetFileName(paths[i]);
                bool exists = File.Exists(paths[i]);
                if (exists != present[i]) {
                    present[i] = exists;
                    absentAt[i] = exists ? -1 : clock.ElapsedMilliseconds;
                    Add(file + (exists ? " appears" : " disappears"));
                }
                var now = new HashSet<string>();
                if (exists) {
                    try { foreach (long id in TouchDeckFileUsers.Read(paths[i])) now.Add(Describe(id)); }
                    catch (Exception) { }  // The file can vanish between the two calls.
                }
                foreach (string user in now) if (!users[i].Contains(user)) Add(file + " used by " + user);
                foreach (string user in users[i]) if (!now.Contains(user)) Add(file + " released by " + user);
                users[i] = now;
            }
            Thread.Sleep(1);
        }
    }
}
'@

    $summary.scenario = 'fresh-install'
    Finish-Child (Start-Child $installer @('/S')) 'Fresh silent install'
    Assert-Payload
    Write-Diagnostics 'after fresh install'
    $app = Start-Child (Join-Path $installDir 'touchdeck.exe') -Variables @{
        TOUCHDECK_MOCK = '1'; TOUCHDECK_INPUT = 'fake'; TOUCHDECK_APP_DATA_DIR = (Join-Path $scratch 'app')
        TOUCHDECK_CONFIG_DIR = $null; TOUCHDECK_STATE_DIR = $null; WEBVIEW2_USER_DATA_FOLDER = $null
    }
    Assert-That (-not $app.Process.WaitForExit(10000)) 'Installed TouchDeck stays alive for 10 seconds in mock mode with scratch app data.'
    $app.Process.Kill($true)
    $app.Process.WaitForExit()
    Write-Diagnostics 'after the mock app stopped'

    # Pin both resolver variables to the actual runner fixture, including child uninstall processes.
    $env:HOME = $env:USERPROFILE
    $env:CLAUDE_CONFIG_DIR = $claudeDir
    $null = New-Item -ItemType Directory -Path $claudeDir -Force
    $original = @'
{
  "theme": "fixture",
  "statusLine": {"type": "command", "command": "echo \"original status\" && echo $value", "padding": 2}
}
'@
    $original = $original.Replace("`r`n", "`n").Replace("`n", "`r`n") + "`r`n"
    [IO.File]::WriteAllText($settingsPath, $original, [Text.UTF8Encoding]::new($false))
    $originalBytes = [IO.File]::ReadAllBytes($settingsPath)
    Finish-Child (Start-Child $fixture @($settingsPath, $helperPath)) 'Shared status-line Install fixture'
    $wrapped = [IO.File]::ReadAllBytes($settingsPath)
    $wrappedCommand = (Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json).statusLine.command
    Assert-That ($wrappedCommand.Contains('td-statusline.exe') -and $wrappedCommand.Contains(' --then ')) 'Fixture wraps the original command using the shared status-line editor.'

    $summary.scenario = 'update'
    $sentinels = @{}
    foreach ($dir in @($configDir, $stateDir)) {
        $null = New-Item -ItemType Directory -Path $dir -Force
        $path = Join-Path $dir 'lifecycle-sentinel.bin'
        $bytes = [byte[]]@(0, 1, 13, 10, 127, 128, 255)
        [IO.File]::WriteAllBytes($path, $bytes)
        $sentinels[$path] = $bytes
    }
    $stale = Join-Path $installDir 'firmware/touchdeck-old.tdfw'
    [IO.File]::WriteAllText($stale, 'stale firmware fixture')
    $oldIdentity = [TouchDeckFileIdentity]::Read($helperPath)
    $held = Start-Child $helperPath -HoldInput -Variables @{
        TOUCHDECK_APP_DATA_DIR = $null; TOUCHDECK_CONFIG_DIR = $null; TOUCHDECK_STATE_DIR = $null
    }
    $held.Process.StandardInput.Write('{"session_id":"installer-lifecycle","model":{"display_name":"fixture"}}')
    $held.Process.StandardInput.Flush()
    Assert-That (-not $held.Process.WaitForExit(1000)) 'Installed helper stays running while stdin remains open.'
    $writeLocked = $false
    try {
        $probe = [IO.File]::Open($helperPath, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
        $probe.Dispose()
    } catch [IO.IOException] { $writeLocked = $true }
    catch [UnauthorizedAccessException] { $writeLocked = $true }
    Assert-That $writeLocked 'The running helper image is locked against overwriting.'
    # tauri-plugin-updater 2.12.0: config.rs nsis_args (Passive => /P), updater.rs updater_parameters adds /UPDATE.
    # tauri.conf.json selects passive mode; app_updates.rs disables restart, so no /R or /ARGS.
    $main = Join-Path $installDir 'touchdeck.exe'
    $uninstaller = Join-Path $installDir 'uninstall.exe'
    $script:watch = [TouchDeckFileWatch]::new([string[]]@($main, $uninstaller))
    Finish-Child (Start-Child $installer @('/P', '/UPDATE')) 'Passive updater-mode install'
    $script:watch.Mark('update exited')
    Write-Diagnostics 'after passive update'
    Assert-Payload
    $renamed = @(Get-ChildItem -LiteralPath $installDir -Filter 'ns*.tmp' -File | ForEach-Object Name)
    $pending = @((Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager' -ErrorAction SilentlyContinue).PSObject.Properties |
        Where-Object Name -EQ 'PendingFileRenameOperations' | ForEach-Object Value)
    $scheduled = @($renamed | Where-Object { $name = $_; @($pending | Where-Object { $_ -like "*\$name" }).Count -gt 0 })
    Write-Log "DIAG renamed helpers [$($renamed -join ', ')]; scheduled for reboot deletion [$($scheduled -join ', ')]"
    Assert-That (-not (Test-Path -LiteralPath $stale)) 'Update removes the stale firmware bundle.'
    Assert-That ([TouchDeckFileIdentity]::Read($helperPath) -cne $oldIdentity) 'Update replaces the locked helper with a different NTFS file.'
    Assert-That (-not $held.Process.HasExited) 'The old helper survives replacement until its stdin is closed.'
    foreach ($path in $sentinels.Keys) {
        $label = $(if ($path.StartsWith($configDir)) { 'config' } else { 'state' })
        Assert-That (Same-Bytes $path $sentinels[$path]) "Update preserves the $label sentinel byte-for-byte."
    }
    Assert-That (Same-Bytes $settingsPath $wrapped) 'Update leaves the Claude status line wrapped byte-for-byte.'
    $held.Process.StandardInput.Close()
    Finish-Child $held 'Held helper after update'
    $session = Join-Path $stateDir 'claude-status/installer-lifecycle.json'
    Assert-That (Test-Path -LiteralPath $session -PathType Leaf) 'The helper writes status state under the identifier data folder.'
    Assert-That (-not (Test-Path -LiteralPath (Join-Path $installDir 'claude-status')) -and
        -not (Test-Path -LiteralPath (Join-Path $installDir 'sources.json'))) 'The install directory contains no source config or status state (R7).'

    $summary.scenario = 'uninstall'
    Write-Diagnostics 'before uninstall'
    $script:watch.Mark('uninstall starts')
    Finish-Child (Start-Child $uninstaller @('/S')) 'Silent uninstall'
    Wait-Uninstaller
    Stop-Watch 'Update and uninstall'
    Write-Diagnostics 'after uninstall'
    Assert-That (Same-Bytes $settingsPath $originalBytes) 'Uninstall restores the entire original Claude settings file byte-for-byte.'
    Assert-That (@(Get-UninstallEntries).Count -eq 0) 'Uninstall removes the per-user registry entry.'
    Assert-That (-not (Test-Path -LiteralPath (Join-Path $installDir 'claude-status')) -and
        -not (Test-Path -LiteralPath (Join-Path $installDir 'sources.json'))) 'Uninstall leaves no app state under the install directory (R7).'
    foreach ($relative in @('touchdeck.exe', 'td-statusline.exe', 'firmware/touchdeck.tdfw', 'install-manifest.json', 'uninstall.exe')) {
        Assert-That (-not (Test-Path -LiteralPath (Join-Path $installDir $relative))) "Uninstall removes $relative."
    }
    # Documented leftovers: a silent uninstall keeps identifier-folder data (the UI's Delete app data box is separate)
    # and the user's Claude settings with their backup; the update's renamed busy helper waits for reboot deletion.
    $allowed = @('install|.') + @($renamed | ForEach-Object { "install|$_" }) + @('config|.', 'config|lifecycle-sentinel.bin',
        'state|.', 'state|lifecycle-sentinel.bin', 'state|claude-status', 'state|claude-status\installer-lifecycle.json',
        'claude|.', 'claude|settings.json', 'claude|settings.json.touchdeck.bak')
    $unexpected = @(Get-Remaining | ForEach-Object { "$($_.root)|$($_.path)" } | Where-Object { $_ -notin $allowed })
    if ($unexpected) { Write-Log "Unexpected leftovers: $($unexpected -join ', ')" }
    Assert-That ($unexpected.Count -eq 0) 'Uninstall leaves only the kept user data, the Claude settings backup, and the renamed helper.'
    foreach ($path in $sentinels.Keys) {
        $label = $(if ($path.StartsWith($configDir)) { 'config' } else { 'state' })
        Assert-That (Same-Bytes $path $sentinels[$path]) "Silent uninstall keeps the $label sentinel byte-for-byte."
    }

    # Uninstalling right after an update once left touchdeck.exe behind while another process briefly held it.
    # Repeat that sequence so the race is likely to recur, and require the uninstaller to finish the job each time.
    $summary.scenario = 'update-then-uninstall'
    foreach ($round in 1..5) {
        Finish-Child (Start-Child $installer @('/S')) "Round $round silent install"
        Start-Sleep -Seconds 3  # Like the lifecycle above, update an installation that has settled.
        $script:watch = [TouchDeckFileWatch]::new([string[]]@($main, $uninstaller))
        Finish-Child (Start-Child $installer @('/P', '/UPDATE')) "Round $round passive update"
        $script:watch.Mark('uninstall starts')
        Finish-Child (Start-Child $uninstaller @('/S')) "Round $round silent uninstall"
        Wait-Uninstaller
        Stop-Watch "Round $round"
        Write-Diagnostics "after round $round"
        Assert-That (-not (Test-Path -LiteralPath $main) -and @(Get-UninstallEntries).Count -eq 0) "Round ${round}: uninstall right after an update removes touchdeck.exe and the registry entry."
    }
    Assert-That (Same-Bytes $settingsPath $originalBytes) 'The rounds leave the original Claude settings byte-for-byte.'
    $summary.status = 'passed'
} catch {
    $summary.status = 'failed'
    $summary.failure = Protect-Log $_.Exception.Message
    Write-Log "FAIL: $($summary.failure)"
} finally {
    Stop-Watch 'Interrupted'
    $summary.remaining = @(Get-Remaining)
    Write-Log ('Remaining fixture/install entries: ' + (ConvertTo-Json -InputObject $summary.remaining -Depth 5 -Compress))
    # Diagnostics only, after the assertions: does a leftover main binary unlock once this script's child handles close?
    $main = Join-Path $installDir 'touchdeck.exe'
    $probe = $summary.scenario -eq 'uninstall' -and (Test-Path -LiteralPath $main)
    if ($probe) { Remove-Probe $main 'with child handles open' }
    foreach ($child in $children) {
        if (-not $child.Process.HasExited) { $child.Process.Kill($true); $child.Process.WaitForExit() }
        if (-not $child.Logged) {
            Write-Log ($child.Out.GetAwaiter().GetResult())
            Write-Log ($child.Err.GetAwaiter().GetResult())
        }
        $child.Process.Dispose()
    }
    if ($probe -and (Test-Path -LiteralPath $main)) { Remove-Probe $main 'after child handles closed' }
    $summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $summaryPath -Encoding utf8
}
if ($summary.status -ne 'passed') { exit 1 }
