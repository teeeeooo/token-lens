param(
    [string]$ReportPath = "",
    [int]$TimeoutSeconds = 60,
    [switch]$SelfTest
)

& (Join-Path $PSScriptRoot "provider-console-host-ab.ps1") `
    -Provider Gemini `
    -ReportPath $ReportPath `
    -TimeoutSeconds $TimeoutSeconds `
    -SelfTest:$SelfTest
exit $LASTEXITCODE
