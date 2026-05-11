param(
    [string]$EnvFile = ".env.local",
    [string]$StateFile = "data/clanlist_panel_state.json",
    [int]$IntervalSeconds = 600,
    [switch]$Once,
    [string]$HealthOutput = "data/clanlist_health.json"
)

$ErrorActionPreference = "Stop"

$commandArgs = @(
    "run-clanlist",
    "--env-file", $EnvFile,
    "--state-file", $StateFile,
    "--allow-discord-read",
    "--allow-discord-write",
    "--confirm-run-clanlist",
    "--health-output", $HealthOutput
)

if ($Once) {
    $commandArgs += "--once"
} else {
    $commandArgs += @("--interval-seconds", $IntervalSeconds)
}

& cargo run -- @commandArgs
