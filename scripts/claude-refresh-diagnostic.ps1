param(
    [string]$ReportPath = "",
    [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "This diagnostic must be run on Windows."
}

if ([string]::IsNullOrWhiteSpace($ReportPath)) {
    $stamp = [DateTime]::UtcNow.ToString("yyyyMMdd-HHmmss")
    $ReportPath = Join-Path $PSScriptRoot "claude-refresh-diagnostic-report-$stamp.txt"
}

$script:Report = [System.Collections.Generic.List[string]]::new()

function Add-ReportLine {
    param([string]$Line = "")
    $script:Report.Add($Line)
    Write-Host $Line
}

function Get-TokenFingerprint {
    param([string]$Token)
    if ([string]::IsNullOrWhiteSpace($Token)) { return $null }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($Token.Trim())
        $hash = $sha.ComputeHash($bytes)
        return ([BitConverter]::ToString($hash) -replace "-", "").Substring(0, 12).ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
    }
}

function Convert-Expiry {
    param($Value)
    if ($null -eq $Value) { return $null }
    $text = [string]$Value
    if ([string]::IsNullOrWhiteSpace($text)) { return $null }
    $number = 0L
    if ([long]::TryParse($text, [ref]$number)) {
        if ($number -gt 100000000000) {
            return [DateTimeOffset]::FromUnixTimeMilliseconds($number).UtcDateTime.ToString("o")
        }
        if ($number -gt 1000000000) {
            return [DateTimeOffset]::FromUnixTimeSeconds($number).UtcDateTime.ToString("o")
        }
    }
    try { return ([DateTimeOffset]::Parse($text)).UtcDateTime.ToString("o") } catch { return $text }
}

function Get-OauthFieldsFromJsonText {
    param([string]$Text)
    try { $json = $Text | ConvertFrom-Json -ErrorAction Stop } catch { return $null }
    if ($null -eq $json) { return $null }
    $oauth = $json
    $claudeOauthProperty = $json.PSObject.Properties["claudeAiOauth"]
    $oauthProperty = $json.PSObject.Properties["oauth"]
    if ($null -ne $claudeOauthProperty -and $null -ne $claudeOauthProperty.Value) {
        $oauth = $claudeOauthProperty.Value
    }
    elseif ($null -ne $oauthProperty -and $null -ne $oauthProperty.Value) {
        $oauth = $oauthProperty.Value
    }

    $token = $null
    foreach ($name in @("accessToken", "access_token")) {
        $property = $oauth.PSObject.Properties[$name]
        if ($null -ne $property -and -not [string]::IsNullOrWhiteSpace([string]$property.Value)) {
            $token = ([string]$property.Value).Trim()
            break
        }
    }
    if ([string]::IsNullOrWhiteSpace($token)) { return $null }

    $expires = $null
    foreach ($name in @("expiresAt", "expires_at")) {
        $property = $oauth.PSObject.Properties[$name]
        if ($null -ne $property) {
            $expires = Convert-Expiry $property.Value
            break
        }
    }
    [pscustomobject]@{ AccessToken = $token; ExpiresAt = $expires }
}

function Get-CredentialFileCandidates {
    $configDir = [Environment]::GetEnvironmentVariable("CLAUDE_CONFIG_DIR")
    if (-not [string]::IsNullOrWhiteSpace($configDir)) {
        return @([pscustomobject]@{ Path = (Join-Path $configDir ".credentials.json"); Source = "file-config" })
    }
    $items = [System.Collections.Generic.List[object]]::new()
    $native = Join-Path (Join-Path $HOME ".claude") ".credentials.json"
    $items.Add([pscustomobject]@{ Path = $native; Source = "file-native" })

    try {
        Get-ChildItem '\\wsl$\' -Directory -ErrorAction Stop | ForEach-Object {
            $distro = $_.FullName
            try {
                Get-ChildItem (Join-Path $distro "home") -Directory -ErrorAction Stop | ForEach-Object {
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

if (-not ("ClaudeCredentialNative" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class ClaudeCredentialNative {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct CREDENTIAL {
        public UInt32 Flags;
        public UInt32 Type;
        public IntPtr TargetName;
        public IntPtr Comment;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten;
        public UInt32 CredentialBlobSize;
        public IntPtr CredentialBlob;
        public UInt32 Persist;
        public UInt32 AttributeCount;
        public IntPtr Attributes;
        public IntPtr TargetAlias;
        public IntPtr UserName;
    }

    [DllImport("Advapi32.dll", EntryPoint = "CredReadW", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool CredRead(string target, UInt32 type, UInt32 flags, out IntPtr credential);

    [DllImport("Advapi32.dll", SetLastError = true)]
    public static extern void CredFree(IntPtr buffer);
}
"@
}

function Decode-CredentialBlob {
    param([byte[]]$Bytes)
    if ($null -eq $Bytes -or $Bytes.Length -eq 0) { return $null }
    $utf8 = [Text.Encoding]::UTF8.GetString($Bytes).Trim([char]0).Trim()
    if ($utf8.StartsWith("{") -or $utf8.Contains("accessToken")) { return $utf8 }
    if (($Bytes.Length % 2) -eq 0) {
        $utf16 = [Text.Encoding]::Unicode.GetString($Bytes).Trim([char]0).Trim()
        if ($utf16.StartsWith("{") -or $utf16.Contains("accessToken")) { return $utf16 }
    }
    return $null
}
function Read-WindowsCredentialJson {
    $targets = [System.Collections.Generic.List[string]]::new()
    $targets.Add("Claude Code-credentials")
    foreach ($name in @("USER", "USERNAME")) {
        $user = [Environment]::GetEnvironmentVariable($name)
        if (-not [string]::IsNullOrWhiteSpace($user)) {
            $targets.Add("Claude Code-credentials:$user")
            $targets.Add("Claude Code-credentials/$user")
        }
    }

    foreach ($target in ($targets | Select-Object -Unique)) {
        $ptr = [IntPtr]::Zero
        if (-not [ClaudeCredentialNative]::CredRead($target, 1, 0, [ref]$ptr)) { continue }
        try {
            $credential = [Runtime.InteropServices.Marshal]::PtrToStructure(
                $ptr, [type][ClaudeCredentialNative+CREDENTIAL]
            )
            if ($credential.CredentialBlobSize -eq 0) { continue }
            $bytes = New-Object byte[] $credential.CredentialBlobSize
            [Runtime.InteropServices.Marshal]::Copy(
                $credential.CredentialBlob, $bytes, 0, $credential.CredentialBlobSize
            )
            $text = Decode-CredentialBlob $bytes
            if (-not [string]::IsNullOrWhiteSpace($text)) { return $text }
        }
        finally {
            if ($ptr -ne [IntPtr]::Zero) { [ClaudeCredentialNative]::CredFree($ptr) }
        }
    }
    return $null
}
function Get-ClaudeCredentialSnapshot {
    $envToken = [Environment]::GetEnvironmentVariable("CLAUDE_CODE_OAUTH_TOKEN")
    if (-not [string]::IsNullOrWhiteSpace($envToken)) {
        return [pscustomobject]@{
            Source = "env"
            Fingerprint = Get-TokenFingerprint $envToken
            ExpiresAt = $null
        }
    }

    foreach ($candidate in Get-CredentialFileCandidates) {
        try { $text = Get-Content -Raw -LiteralPath $candidate.Path -ErrorAction Stop } catch { continue }
        $oauth = Get-OauthFieldsFromJsonText $text
        if ($null -ne $oauth) {
            return [pscustomobject]@{
                Source = $candidate.Source
                Fingerprint = Get-TokenFingerprint $oauth.AccessToken
                ExpiresAt = $oauth.ExpiresAt
            }
        }
    }

    $credentialText = Read-WindowsCredentialJson
    if (-not [string]::IsNullOrWhiteSpace($credentialText)) {
        $oauth = Get-OauthFieldsFromJsonText $credentialText
        if ($null -ne $oauth) {
            return [pscustomobject]@{
                Source = "windows-credential"
                Fingerprint = Get-TokenFingerprint $oauth.AccessToken
                ExpiresAt = $oauth.ExpiresAt
            }
        }
    }
    return [pscustomobject]@{ Source = "unavailable"; Fingerprint = $null; ExpiresAt = $null }
}
function Add-SnapshotReport {
    param([string]$Label, $Snapshot)
    $fp = if ($null -eq $Snapshot.Fingerprint) { "unavailable" } else { $Snapshot.Fingerprint }
    $expiry = if ([string]::IsNullOrWhiteSpace([string]$Snapshot.ExpiresAt)) { "unknown" } else { $Snapshot.ExpiresAt }
    Add-ReportLine "$Label.source=$($Snapshot.Source)"
    Add-ReportLine "$Label.fingerprint=$fp"
    Add-ReportLine "$Label.expiresAtUtc=$expiry"
}

function Compare-Snapshots {
    param($Before, $After)
    if ($null -eq $Before.Fingerprint -or $null -eq $After.Fingerprint) { return "unknown" }
    return ([string]($Before.Fingerprint -ne $After.Fingerprint)).ToLowerInvariant()
}

function Get-LastExitCodeSafe {
    $variable = Get-Variable LASTEXITCODE -ErrorAction SilentlyContinue
    if ($null -eq $variable -or $null -eq $variable.Value) { return "unknown" }
    return $variable.Value
}

function Invoke-ClaudeAuthStatus {
    $lines = @()
    $exitCode = "unknown"
    try {
        $lines = @(& claude auth status 2>&1 | ForEach-Object { [string]$_ })
        $exitCode = Get-LastExitCodeSafe
    }
    catch {
        return [pscustomobject]@{ ExitCode = "error"; LoggedIn = "unknown" }
    }
    $text = $lines -join "`n"
    $loggedIn = "unknown"
    if ($text -match '"loggedIn"\s*:\s*(true|false)') { $loggedIn = $Matches[1].ToLowerInvariant() }
    elseif ($text -match '(?i)logged.?in\s*[:=]\s*(true|false)') { $loggedIn = $Matches[1].ToLowerInvariant() }
    [pscustomobject]@{ ExitCode = $exitCode; LoggedIn = $loggedIn }
}

function Invoke-ClaudeInteractive {
    param([string]$Instruction)
    Write-Host ""
    Write-Host $Instruction -ForegroundColor Cyan
    [void](Read-Host "Press Enter to launch Claude")
    & claude
    return Get-LastExitCodeSafe
}
if ($SelfTest) {
    $fixtureToken = "diagnostic-fixture-access-token"
    $fixture = '{"claudeAiOauth":{"accessToken":"' + $fixtureToken + '","expiresAt":1893456000000}}'
    $parsed = Get-OauthFieldsFromJsonText $fixture
    if ($null -eq $parsed -or $parsed.AccessToken -ne $fixtureToken) {
        throw "Self-test failed: access-token parsing"
    }
    $utf8 = [Text.Encoding]::UTF8.GetBytes($fixture)
    $utf16 = [Text.Encoding]::Unicode.GetBytes($fixture)
    if ((Decode-CredentialBlob $utf8) -ne $fixture) {
        throw "Self-test failed: UTF-8 credential decoding"
    }
    if ((Decode-CredentialBlob $utf16) -ne $fixture) {
        throw "Self-test failed: UTF-16 credential decoding"
    }
    if ((Get-TokenFingerprint $fixtureToken).Length -ne 12) {
        throw "Self-test failed: fingerprint"
    }
    Write-Host "Claude refresh diagnostic self-test: PASS"
    exit 0
}

$claude = Get-Command claude -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $claude) { throw "Claude CLI was not found on PATH." }

$tokenLensProcesses = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -match "(?i)token[-_ ]?lens" })
if ($tokenLensProcesses.Count -gt 0) {
    throw "Close Token Lens before running this diagnostic so polling cannot affect the experiment."
}

Add-ReportLine "Claude Refresh Diagnostic"
Add-ReportLine "timestampUtc=$([DateTime]::UtcNow.ToString('o'))"
Add-ReportLine "powershell=$($PSVersionTable.PSVersion)"
Add-ReportLine "claudeCommandType=$($claude.CommandType)"
Add-ReportLine ""

$baseline = Get-ClaudeCredentialSnapshot
Add-ReportLine "[Baseline]"
Add-SnapshotReport "baseline" $baseline
if ($baseline.Source -eq "env") {
    Add-ReportLine "warning=CLAUDE_CODE_OAUTH_TOKEN overrides provider-owned credential stores; child processes cannot update the parent environment variable."
}

Add-ReportLine ""
Add-ReportLine "[Test1 auth-status]"
Write-Host "Running: claude auth status" -ForegroundColor Cyan
$authStatus = Invoke-ClaudeAuthStatus
Start-Sleep -Milliseconds 750
$afterTest1 = Get-ClaudeCredentialSnapshot
Add-ReportLine "test1.exitCode=$($authStatus.ExitCode)"
Add-ReportLine "test1.loggedIn=$($authStatus.LoggedIn)"
Add-SnapshotReport "test1.after" $afterTest1
Add-ReportLine "test1.tokenChanged=$(Compare-Snapshots $baseline $afterTest1)"

Add-ReportLine ""
Add-ReportLine "[Test2 bare-startup]"
$test2Exit = Invoke-ClaudeInteractive "TEST 2: Do NOT launch Claude separately. Press Enter here and this script will launch Claude. Wait for the normal Claude prompt, type /exit, and the diagnostic will resume automatically. Do not run /status or /usage."
Start-Sleep -Milliseconds 750
$afterTest2 = Get-ClaudeCredentialSnapshot
Add-ReportLine "test2.exitCode=$test2Exit"
Add-SnapshotReport "test2.after" $afterTest2
Add-ReportLine "test2.tokenChanged=$(Compare-Snapshots $afterTest1 $afterTest2)"
Add-ReportLine ""
Add-ReportLine "[Test3 status-touch]"
$test3Exit = Invoke-ClaudeInteractive "TEST 3: Do NOT launch Claude separately. Press Enter here and this script will launch Claude. In that Claude session run /status, wait for the status view, leave it if needed, then type /exit. The diagnostic will resume automatically. Do not send a model prompt."
Start-Sleep -Milliseconds 750
$afterTest3 = Get-ClaudeCredentialSnapshot
Add-ReportLine "test3.exitCode=$test3Exit"
Add-SnapshotReport "test3.after" $afterTest3
Add-ReportLine "test3.tokenChanged=$(Compare-Snapshots $afterTest2 $afterTest3)"

$t1Changed = Compare-Snapshots $baseline $afterTest1
$t2Changed = Compare-Snapshots $afterTest1 $afterTest2
$t3Changed = Compare-Snapshots $afterTest2 $afterTest3
Add-ReportLine ""
Add-ReportLine "[Conclusion]"
if ($t1Changed -eq "true") {
    Add-ReportLine "conclusion=claude auth status changed the access token; PTY-free delegated refresh may be sufficient."
}
elseif ($t2Changed -eq "true") {
    Add-ReportLine "conclusion=bare Claude startup changed the access token after auth status did not."
}
elseif ($t3Changed -eq "true") {
    Add-ReportLine "conclusion=/status changed the access token after bare startup did not."
}
elseif ($t1Changed -eq "unknown" -or $t2Changed -eq "unknown" -or $t3Changed -eq "unknown") {
    Add-ReportLine "conclusion=inconclusive because the access-token fingerprint was unavailable during at least one step."
}
else {
    Add-ReportLine "conclusion=no access-token rotation was observed; this does not prove refresh failure while the current token is still valid."
}
$reportDir = Split-Path -Parent $ReportPath
if (-not [string]::IsNullOrWhiteSpace($reportDir) -and -not (Test-Path $reportDir)) {
    New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
}
$script:Report | Set-Content -LiteralPath $ReportPath -Encoding UTF8

Write-Host ""
Write-Host "Report saved: $ReportPath" -ForegroundColor Green
Write-Host ""
Write-Host "=== COPY FROM HERE ==="
$script:Report | ForEach-Object { Write-Host $_ }
Write-Host "=== COPY END ==="
