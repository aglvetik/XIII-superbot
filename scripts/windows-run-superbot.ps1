param(
    [string]$EnvFile = ".env.local",
    [string]$Modules = "clanlist",
    [string]$HealthOutput = "data/superbot_health.json",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$commandArgs = @(
    "run-superbot",
    "--env-file", $EnvFile,
    "--allow-discord-read",
    "--allow-discord-write",
    "--confirm-run-superbot",
    "--modules", $Modules,
    "--health-output", $HealthOutput
)

if ($DryRun) {
    $commandArgs += "--dry-run"
}

& cargo run -- @commandArgs
