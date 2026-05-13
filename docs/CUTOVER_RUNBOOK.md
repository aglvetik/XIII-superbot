# XIII Superbot Cutover Runbook

Date: 2026-05-13

Deployment is still deferred. This runbook exists to keep the real cutover disciplined when we do it later.

## Operator Assumptions

- Legacy DBs and legacy JSON/cache files remain the source of truth.
- The Superbot must not start in real write mode until the matching old service is stopped.
- Every write-capable command stays behind explicit confirm flags.
- Local validation may use `../XIII_BOTS_FULL_COPY`, but production paths must be refreshed against the VPS before enablement.

## Current Runtime Status

| Module | Runtime status | Cutover requirement |
|---|---|---|
| `clanlist` | `READY_FULL` | Fresh Clanlist state must exist and old Clanlist service must be stopped. |
| `temp_voice` | `READY_FULL` | `TEMP_VOICE_ENABLED=true` plus stopped `temp-voice-bot.service`. |
| `vacation` | `READY_FULL` | `VACATION_ENABLED=true`, fresh panel state, stopped `xiii-vacation-bot.service`. |
| `discipline` | `READY_FULL` | `DISCIPLINE_ENABLED=true`, fresh board state, stopped `xiii-discipline-bot.service`. |
| `recruit` | `READY_FULL` | `RECRUIT_ENABLED=true`, stopped `xiii-recruit-bot.service`. |
| `voice_activity` | `READY_FULL` | `VOICE_ACTIVITY_ENABLED=true`, fresh panel state, stopped old service, and clean/finalized active-session cutover. |
| `tickets` | `READY_FULL` | `TICKETS_ENABLED=true`, fresh panel state, stopped old service, Message Content intent, and valid redacted Google read-only config. |

## Current Parity Gate

Runtime readiness and legacy visible parity are separate gates:

| Module | Parity | Note |
|---|---|---|
| `clanlist` | `ACCEPTED_DIFFERENCE` | Live chunking is validated by dedicated preview/check commands instead of metadata-only audit. |
| `temp_voice` | `ACCEPTED_DIFFERENCE` | No persistent panel existed in the legacy bot. |
| `vacation` | `EXACT` | Audited legacy-visible behavior matches. |
| `discipline` | `EXACT` | Audited legacy-visible behavior matches. |
| `recruit` | `EXACT` | Audited legacy-visible behavior matches. |
| `voice_activity` | `EXACT` | Audited legacy-visible behavior matches. |
| `tickets` | `ACCEPTED_DIFFERENCE` | Safe Rust HTML transcript is intentionally used instead of Python `chat_exporter`. |

## Read-Only Commands Safe To Run Any Time

```powershell
cargo run -- module-status --env-file .env.local
cargo run -- verify-cutover --env-file .env.local
cargo run -- legacy-parity-audit --env-file .env.local
cargo run -- render-preview --env-file .env.local --modules all --format text
cargo run -- ticket-cutover-check --env-file .env.local
cargo run -- db-source-check --env-file .env.local
cargo run -- final-readiness-check --env-file .env.local
```

## Write-Capable Commands

Treat the following as production-changing commands:

- `clanlist-bootstrap-new-panels`
- `clanlist-update-panels`
- `run-clanlist`
- `bootstrap-fresh-panels` without `--dry-run`
- `sync-commands` without `--dry-run`
- `voice-finalize-cutover` without `--dry-run`
- `run-superbot` without `--dry-run`

## Old Services To Stop

| Module | Service |
|---|---|
| `clanlist` | `xiii-clanlist.service` |
| `temp_voice` | `temp-voice-bot.service` |
| `vacation` | `xiii-vacation-bot.service` |
| `discipline` | `xiii-discipline-bot.service` |
| `recruit` | `xiii-recruit-bot.service` |
| `voice_activity` | `xiii-voice-activity-bot.service` |
| `tickets` | `xiii-ticketbot.service` |

Capture status files on the VPS before enabling writers:

```bash
mkdir -p /opt/XIII/xiii-superbot/data/service-status
systemctl status xiii-clanlist.service --no-pager > /opt/XIII/xiii-superbot/data/service-status/xiii-clanlist.service.txt
systemctl status temp-voice-bot.service --no-pager > /opt/XIII/xiii-superbot/data/service-status/temp-voice-bot.service.txt
systemctl status xiii-vacation-bot.service --no-pager > /opt/XIII/xiii-superbot/data/service-status/xiii-vacation-bot.service.txt
systemctl status xiii-discipline-bot.service --no-pager > /opt/XIII/xiii-superbot/data/service-status/xiii-discipline-bot.service.txt
systemctl status xiii-recruit-bot.service --no-pager > /opt/XIII/xiii-superbot/data/service-status/xiii-recruit-bot.service.txt
systemctl status xiii-voice-activity-bot.service --no-pager > /opt/XIII/xiii-superbot/data/service-status/xiii-voice-activity-bot.service.txt
systemctl status xiii-ticketbot.service --no-pager > /opt/XIII/xiii-superbot/data/service-status/xiii-ticketbot.service.txt
```

## Voice Activity Cutover Policy

Voice Activity is the one module with a special cutover step:

1. Run the read-only check:

```bash
./xiii-superbot voice-cutover-check --env-file .env --allow-discord-read
```

2. If active legacy sessions are still open, decide during the cutover window whether to:
   - wait for zero active sessions, or
   - intentionally split in-progress sessions at cutover.

3. Preview the intentional close plan:

```bash
./xiii-superbot voice-finalize-cutover --env-file .env --dry-run
```

4. Only during the real cutover window, after backups and old-service shutdown:

```bash
./xiii-superbot voice-finalize-cutover \
  --env-file .env \
  --allow-legacy-db-write \
  --confirm-close-active-voice-sessions
```

This command is idempotent, preserves historical completed rows, clamps negative durations to zero, and writes `data/voice_activity_cutover_state.json`.

## Tickets Special Requirements

Before enabling Tickets:

1. Confirm Message Content intent is enabled for:
   - `!panel`
   - `!accept` / `!принять`
   - `!reject` / `!отклонить`
2. Run:

```bash
./xiii-superbot ticket-cutover-check --env-file .env
```

3. Ensure fresh ticket panel state exists.
4. Confirm the read-only Google config is present and redacted correctly.

The transcript implementation difference is accepted: the Superbot uses a safe Rust HTML transcript instead of Python `chat_exporter`.

## Local Pre-Deploy Validation

```powershell
cd "D:\clients\XIII 2\xiii-superbot"

cargo fmt --check
cargo check
cargo test --workspace

cargo run -- module-status --env-file .env.local
cargo run -- verify-cutover --env-file .env.local
cargo run -- db-source-check --env-file .env.local
cargo run -- final-readiness-check --env-file .env.local
cargo run -- legacy-parity-audit --env-file .env.local
cargo run -- render-preview --env-file .env.local --modules all --format text
cargo run -- ticket-cutover-check --env-file .env.local
cargo run -- voice-cutover-check --env-file .env.local --allow-discord-read
cargo run -- voice-finalize-cutover --env-file .env.local --dry-run
cargo run -- run-superbot --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-superbot --modules clanlist,temp_voice,vacation,discipline,recruit,voice_activity,tickets --dry-run
```

## Deployment Reminder

Real deployment, systemd setup, and VPS cleanup happen later. This runbook is the checklist we will use when we are ready, not a signal to start deployment now.
