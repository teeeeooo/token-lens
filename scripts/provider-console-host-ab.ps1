param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Claude", "Gemini")]
    [string]$Provider,
    [string]$ReportPath = "",
    [int]$TimeoutSeconds = 60,
    [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "This diagnostic must be run on Windows."
}
if ($TimeoutSeconds -lt 10 -or $TimeoutSeconds -gt 300) {
    throw "TimeoutSeconds must be between 10 and 300."
}

$script:Report = [System.Collections.Generic.List[string]]::new()
function Add-ReportLine {
    param([string]$Line = "")
    $script:Report.Add($Line)
    Write-Host $Line
}
function Get-Sha256Fingerprint {
    param([byte[]]$Bytes)
    if ($null -eq $Bytes -or $Bytes.Length -eq 0) { return $null }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha.ComputeHash($Bytes)
        return ([BitConverter]::ToString($hash) -replace "-", "").Substring(0, 12).ToLowerInvariant()
    }
    finally { $sha.Dispose() }
}

function Get-TokenFingerprint {
    param([string]$Token)
    if ([string]::IsNullOrWhiteSpace($Token)) { return $null }
    return (Get-Sha256Fingerprint ([Text.Encoding]::UTF8.GetBytes($Token.Trim())))
}

function Convert-Expiry {
    param($Value)
    if ($null -eq $Value) { return $null }
    $text = [string]$Value
    if ([string]::IsNullOrWhiteSpace($text)) { return $null }
    $number = 0L
    if ([long]::TryParse($text, [ref]$number)) {
        if ($number -gt 100000000000) { return [DateTimeOffset]::FromUnixTimeMilliseconds($number).UtcDateTime.ToString("o") }
        if ($number -gt 1000000000) { return [DateTimeOffset]::FromUnixTimeSeconds($number).UtcDateTime.ToString("o") }
    }
    try { return ([DateTimeOffset]::Parse($text)).UtcDateTime.ToString("o") } catch { return $text }
}
if (-not ("ProviderConsoleHostNative" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class ProviderConsoleHostNative {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct STARTUPINFO {
        public UInt32 cb; public string lpReserved; public string lpDesktop; public string lpTitle;
        public UInt32 dwX; public UInt32 dwY; public UInt32 dwXSize; public UInt32 dwYSize;
        public UInt32 dwXCountChars; public UInt32 dwYCountChars; public UInt32 dwFillAttribute;
        public UInt32 dwFlags; public UInt16 wShowWindow; public UInt16 cbReserved2;
        public IntPtr lpReserved2; public IntPtr hStdInput; public IntPtr hStdOutput; public IntPtr hStdError;
    }
    [StructLayout(LayoutKind.Sequential)]
    public struct PROCESS_INFORMATION {
        public IntPtr hProcess; public IntPtr hThread; public UInt32 dwProcessId; public UInt32 dwThreadId;
    }
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessW(string app, StringBuilder cmd, IntPtr pa, IntPtr ta, bool inherit,
        UInt32 flags, IntPtr env, string cwd, ref STARTUPINFO si, out PROCESS_INFORMATION pi);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool CloseHandle(IntPtr h);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct CREDENTIAL {
        public UInt32 Flags; public UInt32 Type; public IntPtr TargetName; public IntPtr Comment;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten;
        public UInt32 CredentialBlobSize; public IntPtr CredentialBlob; public UInt32 Persist;
        public UInt32 AttributeCount; public IntPtr Attributes; public IntPtr TargetAlias; public IntPtr UserName;
    }
    [DllImport("Advapi32.dll", EntryPoint = "CredReadW", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CredRead(string target, UInt32 type, UInt32 flags, out IntPtr credential);
    [DllImport("Advapi32.dll", SetLastError = true)] static extern void CredFree(IntPtr buffer);

    public static int StartNewConsole(string commandLine, string cwd, bool hidden) {
        var si = new STARTUPINFO();
        si.cb = (UInt32)Marshal.SizeOf(typeof(STARTUPINFO));
        si.dwFlags = 0x00000001;
        si.wShowWindow = (UInt16)(hidden ? 0 : 5);
        PROCESS_INFORMATION pi;
        var mutable = new StringBuilder(commandLine);
        if (!CreateProcessW(null, mutable, IntPtr.Zero, IntPtr.Zero, false, 0x00000010,
            IntPtr.Zero, cwd, ref si, out pi)) {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        try { return (int)pi.dwProcessId; }
        finally { CloseHandle(pi.hThread); CloseHandle(pi.hProcess); }
    }

    public static byte[] ReadGenericCredential(string target) {
        IntPtr ptr;
        if (!CredRead(target, 1, 0, out ptr) || ptr == IntPtr.Zero) return null;
        try {
            var item = (CREDENTIAL)Marshal.PtrToStructure(ptr, typeof(CREDENTIAL));
            if (item.CredentialBlobSize == 0 || item.CredentialBlob == IntPtr.Zero) return null;
            var bytes = new byte[item.CredentialBlobSize];
            Marshal.Copy(item.CredentialBlob, bytes, 0, bytes.Length);
            return bytes;
        }
        finally { CredFree(ptr); }
    }
}
"@
}
function Decode-JsonCredentialBlob {
    param([byte[]]$Bytes)
    if ($null -eq $Bytes -or $Bytes.Length -eq 0) { return $null }
    $utf8 = [Text.Encoding]::UTF8.GetString($Bytes).Trim([char]0).Trim()
    if ($utf8.StartsWith("{")) { return $utf8 }
    if (($Bytes.Length % 2) -eq 0) {
        $utf16 = [Text.Encoding]::Unicode.GetString($Bytes).Trim([char]0).Trim()
        if ($utf16.StartsWith("{")) { return $utf16 }
    }
    return $null
}

function Get-ClaudeFieldsFromJsonText {
    param([string]$Text)
    try { $json = $Text | ConvertFrom-Json -ErrorAction Stop } catch { return $null }
    $oauth = $json
    if ($null -ne $json.PSObject.Properties["claudeAiOauth"]) { $oauth = $json.claudeAiOauth }
    elseif ($null -ne $json.PSObject.Properties["oauth"]) { $oauth = $json.oauth }
    $token = $null
    foreach ($name in @("accessToken", "access_token")) {
        $p = $oauth.PSObject.Properties[$name]
        if ($null -ne $p -and -not [string]::IsNullOrWhiteSpace([string]$p.Value)) { $token = [string]$p.Value; break }
    }
    if ([string]::IsNullOrWhiteSpace($token)) { return $null }
    $expiry = $null
    foreach ($name in @("expiresAt", "expires_at")) {
        $p = $oauth.PSObject.Properties[$name]
        if ($null -ne $p) { $expiry = Convert-Expiry $p.Value; break }
    }
    [pscustomobject]@{ Token = $token.Trim(); ExpiresAt = $expiry }
}
function Get-GeminiFieldsFromJsonText {
    param([string]$Text)
    try { $json = $Text | ConvertFrom-Json -ErrorAction Stop } catch { return $null }
    $tokenNode = $json
    if ($null -ne $json.PSObject.Properties["token"]) { $tokenNode = $json.token }
    $token = $null
    foreach ($name in @("accessToken", "access_token")) {
        $p = $tokenNode.PSObject.Properties[$name]
        if ($null -ne $p -and -not [string]::IsNullOrWhiteSpace([string]$p.Value)) { $token = [string]$p.Value; break }
    }
    if ([string]::IsNullOrWhiteSpace($token)) { return $null }
    $expiry = $null
    foreach ($name in @("expiresAt", "expiry_date", "expires_at")) {
        $p = $tokenNode.PSObject.Properties[$name]
        if ($null -ne $p) { $expiry = Convert-Expiry $p.Value; break }
    }
    [pscustomobject]@{ Token = $token.Trim(); ExpiresAt = $expiry }
}

function New-Snapshot {
    param([string]$Source, [string]$Fingerprint, [string]$ExpiresAt, $Stores = $null)
    [pscustomobject]@{
        Source = $Source
        Fingerprint = $Fingerprint
        ExpiresAt = $ExpiresAt
        Stores = $Stores
    }
}
function Get-ClaudeCredentialFileCandidates {
    $configDir = [Environment]::GetEnvironmentVariable("CLAUDE_CONFIG_DIR")
    if (-not [string]::IsNullOrWhiteSpace($configDir)) {
        return @([pscustomobject]@{ Path = (Join-Path $configDir ".credentials.json"); Source = "file-config" })
    }
    $items = [System.Collections.Generic.List[object]]::new()
    $items.Add([pscustomobject]@{ Path = (Join-Path (Join-Path $HOME ".claude") ".credentials.json"); Source = "file-native" })
    try {
        Get-ChildItem '\\wsl$\' -Directory -ErrorAction Stop | ForEach-Object {
            try {
                Get-ChildItem (Join-Path $_.FullName "home") -Directory -ErrorAction Stop | ForEach-Object {
                    $items.Add([pscustomobject]@{
                        Path = (Join-Path (Join-Path $_.FullName ".claude") ".credentials.json")
                        Source = "file-wsl"
                    })
                }
            } catch { }
        }
    } catch { }
    return @($items | Sort-Object -Property @{ Expression = {
        try { (Get-Item $_.Path -ErrorAction Stop).LastWriteTimeUtc } catch { [DateTime]::MinValue }
    }; Descending = $true })
}

function Get-ClaudeCredentialSnapshot {
    $envToken = [Environment]::GetEnvironmentVariable("CLAUDE_CODE_OAUTH_TOKEN")
    if (-not [string]::IsNullOrWhiteSpace($envToken)) {
        return (New-Snapshot "env" (Get-TokenFingerprint $envToken) $null)
    }
    foreach ($candidate in Get-ClaudeCredentialFileCandidates) {
        try { $text = Get-Content -Raw -LiteralPath $candidate.Path -ErrorAction Stop } catch { continue }
        $fields = Get-ClaudeFieldsFromJsonText $text
        if ($null -ne $fields) {
            return (New-Snapshot $candidate.Source (Get-TokenFingerprint $fields.Token) $fields.ExpiresAt)
        }
    }
    foreach ($target in @("Claude Code-credentials", "Claude Code-credentials:$env:USERNAME", "Claude Code-credentials/$env:USERNAME")) {
        if ([string]::IsNullOrWhiteSpace($target)) { continue }
        $bytes = [ProviderConsoleHostNative]::ReadGenericCredential($target)
        $text = Decode-JsonCredentialBlob $bytes
        if (-not [string]::IsNullOrWhiteSpace($text)) {
            $fields = Get-ClaudeFieldsFromJsonText $text
            if ($null -ne $fields) {
                return (New-Snapshot "windows-credential" (Get-TokenFingerprint $fields.Token) $fields.ExpiresAt)
            }
        }
    }
    return (New-Snapshot "unavailable" $null $null)
}

function Add-GeminiStore {
    param($Stores, [string]$Source, [string]$Fingerprint, [string]$ExpiresAt)
    if (-not [string]::IsNullOrWhiteSpace($Fingerprint)) {
        $Stores.Add([pscustomobject]@{ Source = $Source; Fingerprint = $Fingerprint; ExpiresAt = $ExpiresAt })
    }
}
function Get-GeminiCredentialSnapshot {
    $stores = [System.Collections.Generic.List[object]]::new()

    $blob = [ProviderConsoleHostNative]::ReadGenericCredential("gemini-cli-oauth/main-account")
    if ($null -ne $blob -and $blob.Length -gt 0) {
        $text = Decode-JsonCredentialBlob $blob
        $fields = if ($null -ne $text) { Get-GeminiFieldsFromJsonText $text } else { $null }
        if ($null -ne $fields) {
            Add-GeminiStore $stores "windows-credential" (Get-TokenFingerprint $fields.Token) $fields.ExpiresAt
        }
        else {
            Add-GeminiStore $stores "windows-credential-raw" (Get-Sha256Fingerprint $blob) $null
        }
    }

    $geminiHome = Join-Path $HOME ".gemini"
    $fileKeychain = Join-Path $geminiHome "gemini-credentials.json"
    if (Test-Path -LiteralPath $fileKeychain) {
        try {
            $bytes = [IO.File]::ReadAllBytes($fileKeychain)
            Add-GeminiStore $stores "file-keychain" (Get-Sha256Fingerprint $bytes) $null
        } catch { }
    }

    $oauthFile = Join-Path $geminiHome "oauth_creds.json"
    if (Test-Path -LiteralPath $oauthFile) {
        try {
            $text = Get-Content -Raw -LiteralPath $oauthFile -ErrorAction Stop
            $fields = Get-GeminiFieldsFromJsonText $text
            if ($null -ne $fields) {
                Add-GeminiStore $stores "oauth-file" (Get-TokenFingerprint $fields.Token) $fields.ExpiresAt
            }
        } catch { }
    }
    if ($stores.Count -eq 0) {
        return (New-Snapshot "unavailable" $null $null @())
    }
    $descriptor = ($stores | Sort-Object Source | ForEach-Object {
        "$($_.Source):$($_.Fingerprint):$($_.ExpiresAt)"
    }) -join "|"
    $aggregate = Get-TokenFingerprint $descriptor
    $expiryStore = $stores | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_.ExpiresAt) } |
        Select-Object -First 1
    $expiry = if ($null -eq $expiryStore) { $null } else { $expiryStore.ExpiresAt }
    $sources = ($stores | ForEach-Object { $_.Source }) -join "+"
    return (New-Snapshot $sources $aggregate $expiry @($stores))
}

function Get-ProviderCredentialSnapshot {
    if ($Provider -eq "Claude") { return (Get-ClaudeCredentialSnapshot) }
    return (Get-GeminiCredentialSnapshot)
}

function Add-SnapshotReport {
    param([string]$Label, $Snapshot)
    $fp = if ($null -eq $Snapshot.Fingerprint) { "unavailable" } else { $Snapshot.Fingerprint }
    $expiry = if ([string]::IsNullOrWhiteSpace([string]$Snapshot.ExpiresAt)) { "unknown" } else { $Snapshot.ExpiresAt }
    Add-ReportLine "$Label.source=$($Snapshot.Source)"
    Add-ReportLine "$Label.fingerprint=$fp"
    Add-ReportLine "$Label.expiresAtUtc=$expiry"
    if ($null -ne $Snapshot.Stores) {
        foreach ($store in @($Snapshot.Stores)) {
            $storeExpiry = if ([string]::IsNullOrWhiteSpace([string]$store.ExpiresAt)) { "unknown" } else { $store.ExpiresAt }
            Add-ReportLine "$Label.store.$($store.Source).fingerprint=$($store.Fingerprint)"
            Add-ReportLine "$Label.store.$($store.Source).expiresAtUtc=$storeExpiry"
        }
    }
}
function Compare-Snapshots {
    param($Before, $After)
    if ($null -eq $Before.Fingerprint -or $null -eq $After.Fingerprint) { return "unknown" }
    return ([string]($Before.Fingerprint -ne $After.Fingerprint)).ToLowerInvariant()
}

function Test-ProcessAlive {
    param([int]$ProcessId)
    return $null -ne (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)
}

function Stop-ProcessTree {
    param([int]$ProcessId)
    if (-not (Test-ProcessAlive $ProcessId)) { return $true }
    try {
        & taskkill.exe /PID $ProcessId /T /F *> $null
    } catch { }
    Start-Sleep -Milliseconds 750
    if (Test-ProcessAlive $ProcessId) {
        try { Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue } catch { }
        Start-Sleep -Milliseconds 500
    }
    return -not (Test-ProcessAlive $ProcessId)
}

function New-ConsoleCommandLine {
    param([string]$ProviderCommand)
    $comspec = if ([string]::IsNullOrWhiteSpace($env:ComSpec)) { "$env:SystemRoot\System32\cmd.exe" } else { $env:ComSpec }
    return ('"{0}" /d /s /c "{1}"' -f $comspec, $ProviderCommand)
}
function Invoke-ConsoleProbe {
    param(
        [string]$Label,
        [bool]$Hidden,
        $Before,
        [string]$WorkingDirectory,
        [string]$ProviderCommand
    )

    $commandLine = New-ConsoleCommandLine $ProviderCommand
    $startedAt = [DateTime]::UtcNow
    $processId = [ProviderConsoleHostNative]::StartNewConsole($commandLine, $WorkingDirectory, $Hidden)
    $reason = "timeout"
    $changed = "false"
    $after = $Before

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        Start-Sleep -Milliseconds 500
        $after = Get-ProviderCredentialSnapshot
        $changed = Compare-Snapshots $Before $after
        if ($changed -eq "true") { $reason = "credential-changed"; break }
        if (-not (Test-ProcessAlive $processId)) {
            Start-Sleep -Milliseconds 750
            $after = Get-ProviderCredentialSnapshot
            $changed = Compare-Snapshots $Before $after
            $reason = if ($changed -eq "true") { "exited-after-credential-change" } else { "process-exited" }
            break
        }
    } while ([DateTime]::UtcNow -lt $deadline)

    $cleanupOk = Stop-ProcessTree $processId
    Start-Sleep -Milliseconds 750
    $final = Get-ProviderCredentialSnapshot
    $finalChanged = Compare-Snapshots $Before $final
    if ($finalChanged -eq "true" -and $changed -ne "true") {
        $changed = "true"
        $reason = "credential-changed-before-cleanup-completed"
        $after = $final
    }

    $elapsed = [Math]::Round(([DateTime]::UtcNow - $startedAt).TotalSeconds, 3)
    Add-ReportLine "$Label.launchMode=$(if ($Hidden) { 'hidden-real-console' } else { 'visible-real-console' })"
    Add-ReportLine "$Label.pid=$processId"
    Add-ReportLine "$Label.startedAtUtc=$($startedAt.ToString('o'))"
    Add-ReportLine "$Label.elapsedSeconds=$elapsed"
    Add-ReportLine "$Label.exitReason=$reason"
    Add-ReportLine "$Label.credentialChanged=$changed"
    Add-ReportLine "$Label.cleanupOk=$([string]$cleanupOk).ToLowerInvariant()"
    Add-SnapshotReport "$Label.after" $after

    return [pscustomobject]@{
        Changed = $changed
        After = $after
        ExitReason = $reason
        CleanupOk = $cleanupOk
    }
}
if ($SelfTest) {
    $claudeFixture = '{"claudeAiOauth":{"accessToken":"claude-fixture-token","expiresAt":1893456000000}}'
    $claudeFields = Get-ClaudeFieldsFromJsonText $claudeFixture
    if ($null -eq $claudeFields -or $claudeFields.Token -ne "claude-fixture-token") {
        throw "Self-test failed: Claude credential parser"
    }
    $geminiFixture = '{"token":{"accessToken":"gemini-fixture-token","expiresAt":1893456000000}}'
    $geminiFields = Get-GeminiFieldsFromJsonText $geminiFixture
    if ($null -eq $geminiFields -or $geminiFields.Token -ne "gemini-fixture-token") {
        throw "Self-test failed: Gemini credential parser"
    }
    if ((Get-TokenFingerprint "fixture").Length -ne 12) {
        throw "Self-test failed: SHA-256 fingerprint"
    }
    $line = New-ConsoleCommandLine "claude"
    if ($line -notmatch '/d /s /c "claude"$') {
        throw "Self-test failed: console command line"
    }
    Write-Host "$Provider console-host A/B diagnostic self-test: PASS"
    exit 0
}

$commandName = if ($Provider -eq "Claude") { "claude" } else { "gemini" }
$providerCommand = if ($Provider -eq "Claude") {
    "claude"
} else {
    "gemini --list-sessions -e none --skip-trust"
}
$resolved = Get-Command $commandName -ErrorAction SilentlyContinue | Select-Object -First 1
$cmdMatches = @(& where.exe $commandName 2>$null | ForEach-Object { [string]$_ })
if ($cmdMatches.Count -eq 0) { throw "$Provider CLI was not found by ordinary CMD PATH/PATHEXT resolution." }
$cmdResolutionNames = ($cmdMatches | ForEach-Object { Split-Path -Leaf $_ } | Select-Object -Unique) -join ","

$tokenLensProcesses = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
    $_.ProcessName -match "(?i)token[-_ ]?lens"
})
if ($tokenLensProcesses.Count -gt 0) {
    throw "Close Token Lens before running this diagnostic so background polling cannot affect the experiment."
}

if ([string]::IsNullOrWhiteSpace($ReportPath)) {
    $stamp = [DateTime]::UtcNow.ToString("yyyyMMdd-HHmmss")
    $ReportPath = Join-Path $PSScriptRoot ("{0}-console-host-ab-report-{1}.txt" -f $commandName, $stamp)
}

$probeDir = $null
$workingDirectory = $HOME
if ($Provider -eq "Gemini") {
    $probeDir = Join-Path ([IO.Path]::GetTempPath()) ("token-lens-gemini-console-host-ab-{0}-{1}" -f $PID, [DateTime]::UtcNow.ToString("yyyyMMddHHmmss"))
    New-Item -ItemType Directory -Path $probeDir -Force | Out-Null
    $workingDirectory = $probeDir
}

Add-ReportLine "$Provider Console Host A/B Diagnostic"
Add-ReportLine "timestampUtc=$([DateTime]::UtcNow.ToString('o'))"
Add-ReportLine "powershell=$($PSVersionTable.PSVersion)"
Add-ReportLine "provider=$($Provider.ToLowerInvariant())"
Add-ReportLine "commandType=$(if ($null -eq $resolved) { 'cmd-only' } else { $resolved.CommandType })"
Add-ReportLine "cmdResolutionNames=$cmdResolutionNames"
Add-ReportLine "workingDirectoryMode=$(if ($Provider -eq 'Gemini') { 'isolated-temp' } else { 'home' })"
Add-ReportLine "providerCommand=$providerCommand"
Add-ReportLine "timeoutSeconds=$TimeoutSeconds"
Add-ReportLine ""

$baseline = Get-ProviderCredentialSnapshot
Add-ReportLine "[Baseline]"
Add-SnapshotReport "baseline" $baseline
if ($Provider -eq "Claude" -and $baseline.Source -eq "env") {
    Add-ReportLine "warning=CLAUDE_CODE_OAUTH_TOKEN is inherited by children but cannot be updated in this parent process."
}

Write-Host ""
Write-Host "TEST A launches a NEW Windows console object with its window hidden." -ForegroundColor Cyan
Write-Host "Do not launch $commandName separately while the probe is running." -ForegroundColor Cyan
[void](Read-Host "Press Enter to start Test A")

Add-ReportLine ""
Add-ReportLine "[TestA hidden-real-console]"
$testA = Invoke-ConsoleProbe "testA" $true $baseline $workingDirectory $providerCommand

if ($testA.Changed -eq "true") {
    Add-ReportLine ""
    Add-ReportLine "[TestB visible-real-console]"
    Add-ReportLine "testB.skipped=true"
    Add-ReportLine "testB.skipReason=Test A already rotated the credential; preserving the experiment condition."
    $testB = $null
}
else {
    Write-Host ""
    Write-Host "TEST B launches the SAME command in a visible NEW Windows console." -ForegroundColor Cyan
    Write-Host "Do not type anything into the provider window. The script will close it automatically." -ForegroundColor Cyan
    [void](Read-Host "Press Enter to start Test B")
    Add-ReportLine ""
    Add-ReportLine "[TestB visible-real-console]"
    $testB = Invoke-ConsoleProbe "testB" $false $testA.After $workingDirectory $providerCommand
}

Add-ReportLine ""
Add-ReportLine "[Conclusion]"
if ($testA.Changed -eq "true") {
    Add-ReportLine "conclusion=hidden real Windows console rotated the provider credential."
}
elseif ($null -ne $testB -and $testB.Changed -eq "true") {
    Add-ReportLine "conclusion=hidden real console failed but the otherwise identical visible real console rotated the credential."
}
elseif ($testA.Changed -eq "unknown" -or ($null -ne $testB -and $testB.Changed -eq "unknown")) {
    Add-ReportLine "conclusion=inconclusive because a credential fingerprint was unavailable during at least one step."
}
else {
    Add-ReportLine "conclusion=no credential rotation was observed in either real-console mode."
}

$reportDir = Split-Path -Parent $ReportPath
if (-not [string]::IsNullOrWhiteSpace($reportDir) -and -not (Test-Path $reportDir)) {
    New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
}
$script:Report | Set-Content -LiteralPath $ReportPath -Encoding UTF8
if ($null -ne $probeDir) {
    try { Remove-Item -LiteralPath $probeDir -Force -Recurse -ErrorAction SilentlyContinue } catch { }
}

Write-Host ""
Write-Host "Report saved: $ReportPath" -ForegroundColor Green
Write-Host "=== COPY FROM HERE ==="
$script:Report | ForEach-Object { Write-Host $_ }
Write-Host "=== COPY END ==="
exit 0
