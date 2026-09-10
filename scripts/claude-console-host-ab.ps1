param(
    [string]$ReportPath = "",
    [int]$TimeoutSeconds = 60,
    [switch]$SelfTest
)

& (Join-Path $PSScriptRoot "provider-console-host-ab.ps1") `
    -Provider Claude `
    -ReportPath $ReportPath `
    -TimeoutSeconds $TimeoutSeconds `
    -SelfTest:$SelfTest
exit $LASTEXITCODE
