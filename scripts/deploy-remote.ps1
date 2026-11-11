# Windows wrapper: runs scripts/deploy-remote.sh with Git Bash.
#   .\scripts\deploy-remote.ps1                      # preproduction
#   .\scripts\deploy-remote.ps1 --env production
$ErrorActionPreference = 'Stop'
$bash = Join-Path (Split-Path (Split-Path (Get-Command git).Source)) 'bin\bash.exe'
if (-not (Test-Path $bash)) { throw "Git Bash not found at $bash" }
# Stop MSYS from rewriting arguments such as /preproduction into Windows paths.
$env:MSYS_NO_PATHCONV = '1'
& $bash (Join-Path $PSScriptRoot 'deploy-remote.sh') @args
exit $LASTEXITCODE
