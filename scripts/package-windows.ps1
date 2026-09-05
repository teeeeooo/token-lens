$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$Package = Get-Content (Join-Path $Root 'package.json') -Raw | ConvertFrom-Json
$Version = [string]$Package.version
$Dist = Join-Path $Root 'dist-v2'
$PortableStage = Join-Path $Dist "Token-Lens-$Version-portable"

Remove-Item $Dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item $Dist -ItemType Directory -Force | Out-Null

Write-Host "Building unsigned Token Lens $Version Windows x64 package..."
npm exec tauri build -- --config src-tauri/tauri.bundle.conf.json --bundles nsis --ci --no-sign
if ($LASTEXITCODE -ne 0) { throw "Tauri NSIS build failed with exit code $LASTEXITCODE" }

$ReleaseDir = Join-Path $Root 'src-tauri\target\release'
$MainExe = Join-Path $ReleaseDir 'token-lens.exe'
$SidecarExe = Join-Path $ReleaseDir 'tokscale.exe'
if (-not (Test-Path $MainExe -PathType Leaf)) { throw "Missing release executable: $MainExe" }
if (-not (Test-Path $SidecarExe -PathType Leaf)) { throw "Missing bundled tokScale sidecar: $SidecarExe" }

$TokscaleVersion = (& $SidecarExe --version | Out-String).Trim()
if ($TokscaleVersion -ne 'tokscale 4.15.1') {
  throw "Unexpected bundled tokScale version: $TokscaleVersion"
}
$NsisDir = Join-Path $ReleaseDir 'bundle\nsis'
$Installers = @(Get-ChildItem $NsisDir -File -Filter '*.exe')
if ($Installers.Count -ne 1) {
  throw "Expected exactly one NSIS installer in $NsisDir, found $($Installers.Count)"
}

$InstallerOut = Join-Path $Dist "Token-Lens-Setup-$Version.exe"
Copy-Item $Installers[0].FullName $InstallerOut

New-Item $PortableStage -ItemType Directory -Force | Out-Null
Copy-Item $MainExe (Join-Path $PortableStage 'Token-Lens.exe')
Copy-Item $SidecarExe (Join-Path $PortableStage 'tokscale.exe')

$PortableZip = Join-Path $Dist "Token-Lens-$Version-portable.zip"
Compress-Archive -Path (Join-Path $PortableStage '*') -DestinationPath $PortableZip -CompressionLevel Optimal
Remove-Item $PortableStage -Recurse -Force

foreach ($Executable in @($InstallerOut, $MainExe, $SidecarExe)) {
  $Signature = Get-AuthenticodeSignature $Executable
  Write-Host "Signature $([IO.Path]::GetFileName($Executable)): $($Signature.Status)"
  if ($Signature.Status -eq 'Valid') {
    throw "Unexpected signing identity on unsigned downstream build: $Executable"
  }
}

$Artifacts = @(Get-Item $InstallerOut, $PortableZip | Sort-Object Name)
$ChecksumPath = Join-Path $Dist 'SHA256SUMS.txt'
$Lines = foreach ($Artifact in $Artifacts) {
  $Hash = (Get-FileHash $Artifact.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  "$Hash  $($Artifact.Name)"
}
$Lines | Set-Content $ChecksumPath -Encoding ascii

Write-Host "Windows artifacts:"
Get-ChildItem $Dist -File | Sort-Object Name | ForEach-Object {
  Write-Host "  $($_.Name)  $([math]::Round($_.Length / 1MB, 2)) MiB"
}
Get-Content $ChecksumPath | ForEach-Object { Write-Host "  $_" }
