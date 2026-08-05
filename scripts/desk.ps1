# Bridges Windows operators into the reviewed Bash appliance launcher through WSL.
[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $DeskArguments
)

$ErrorActionPreference = 'Stop'
$RepositoryWindowsPath = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$RepositoryWslPath = (& wsl.exe wslpath -a -u $RepositoryWindowsPath).Trim()

if (-not $RepositoryWslPath) {
    throw 'WSL could not resolve the Local IT Desk directory.'
}

& wsl.exe --cd $RepositoryWslPath bash ./scripts/desk @DeskArguments
exit $LASTEXITCODE
