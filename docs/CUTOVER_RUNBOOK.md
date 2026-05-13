# XIII Superbot Cutover Runbook

Date: 2026-05-13

Deployment is still deferred. This runbook exists so the real cutover stays disciplined when we are ready.

## Operator Assumptions

- Legacy DBs and legacy JSON/cache files remain the source of truth.
- The Superbot must not start in real write mode until the matching old service is stopped.
- Every write-capable command stays behind explicit confirm flags.
- Local validation may use `../XIII_BOTS_FULL_COPY`, but production paths must be refreshed against the VPS before enablement.
- `.env.example` is safe to track; the real deployment env should live in a private file such as `/opt/XIII/xiii-superbot/.env.production`.

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

## VPS Pre-Cutover Preparation (No Service Changes)

This section is explicitly before stopping old bots. These steps are read-only or status-capture only.

```bash
chmod +x scripts/linux/*.sh
bash -n scripts/linux/collect-vps-precutover-state.sh
bash -n scripts/linux/verify-production-layout.sh
./scripts/linux/verify-production-layout.sh --env-file /opt/XIII/xiii-superbot/.env.production
./scripts/linux/collect-vps-precutover-state.sh --dry-run
./scripts/linux/collect-vps-precutover-state.sh
cargo run -- production-preflight --env-file /opt/XIII/xiii-superbot/.env.production
```

Notes:

- `verify-production-layout.sh` only checks the VPS layout and the production path variables parsed from `.env.production`.
- `collect-vps-precutover-state.sh` creates `/opt/XIII/xiii-superbot/data/service-status` if needed and captures `systemctl status`, `is-active`, and `is-enabled` outputs for the old services.
- Neither script stops, starts, restarts, enables, or disables any service.
- Neither script modifies any legacy DB.

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

## Production Cutover Sequence

When we are ready on the VPS, keep the sequence mechanical:

1. Copy/update the repo under `/opt/XIII/xiii-superbot`.
2. Create `/opt/XIII/xiii-superbot/.env.production` from `.env.example`.
3. Replace every example `LEGACY_*` path with the current VPS legacy DB/state path.
4. Copy or refresh the latest legacy DBs/state files from the old bot locations.
5. Run the VPS pre-cutover preparation steps above.
6. Run read-only checks:

```bash
cd /opt/XIII/xiii-superbot
./xiii-superbot check-config --env-file /opt/XIII/xiii-superbot/.env.production
./xiii-superbot db-source-check --env-file /opt/XIII/xiii-superbot/.env.production
./xiii-superbot production-preflight --env-file /opt/XIII/xiii-superbot/.env.production --allow-discord-read
```

7. Bootstrap or verify fresh Superbot-owned panel state only where it is still missing.
8. Stop the old services.
9. Re-run `production-preflight` and any module-specific read-only checks that still matter.
10. Resolve Voice Activity active sessions by waiting for zero active sessions or running `voice-finalize-cutover`.
11. Enable the required `*_ENABLED=true` flags in `.env.production`.
12. Install/start the Superbot service.
13. Monitor logs and health files.
14. If anything is wrong, stop the Superbot service, restore the backed-up legacy DB/state files, and restart the old services.

## Voice Activity Cutover Policy

Voice Activity is the one module with a special cutover step:

1. Run the read-only check:

```bash
./xiii-superbot voice-cutover-check --env-file .env.production --allow-discord-read
```

2. If active legacy sessions are still open, decide during the cutover window whether to:
   - wait for zero active sessions, or
   - intentionally split in-progress sessions at cutover.

3. Preview the intentional close plan:

```bash
./xiii-superbot voice-finalize-cutover --env-file .env.production --dry-run
```

4. Only during the real cutover window, after backups and old-service shutdown:

```bash
./xiii-superbot voice-finalize-cutover \
  --env-file .env.production \
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
./xiii-superbot ticket-cutover-check --env-file .env.production
```

3. Ensure fresh ticket panel state exists.
4. Confirm the read-only Google config is present and redacted correctly.

The transcript implementation difference is accepted: the Superbot uses a safe Rust HTML transcript instead of Python `chat_exporter`.

## Local Pre-Deploy Validation

Run from the repository root:

```powershell
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
