# XIII Superbot Cutover Runbook

Date: 2026-05-09

This runbook is for safe operator cutover. It assumes legacy DBs remain the source of truth and old services are stopped before any Superbot writer is enabled.

## Current Safe State

| Module | Status | Notes |
|---|---|---|
| Clanlist | `READY_FULL` | Uses `data/clanlist_panel_state.json`; edits only the three Superbot-owned roster messages. |
| Temp voice | `READY_FULL` | Uses the legacy temp voice SQLite DB; deletes only channel IDs tracked in `temp_voice_channels`. |
| Vacation | `READY_FULL` | Needs fresh vacation panel state, `VACATION_ENABLED=true`, and stopped `xiii-vacation-bot.service`. |
| Discipline | `READY_FULL` | Needs fresh discipline board state, `DISCIPLINE_ENABLED=true`, and stopped `xiii-discipline-bot.service`. |
| Recruit | `READY_FULL` | Needs `RECRUIT_ENABLED=true` and stopped `xiii-recruit-bot.service`. |
| Voice activity | `READY_FULL` | Needs fresh public stats panel state, `VOICE_ACTIVITY_ENABLED=true`, stopped `xiii-voice-activity-bot.service`, and either zero active sessions or a finalized `data/voice_activity_cutover_state.json`. |
| Tickets | `READY_FULL` | Needs fresh ticket panel state, `TICKETS_ENABLED=true`, stopped `xiii-ticketbot.service`, Message Content intent, and redacted Google read-only config. Uses safe Rust HTML transcripts as the production substitute for Python `chat_exporter`. |

## Write-Capable Commands

These commands require explicit confirmation flags and must be treated as production-changing:

- `clanlist-bootstrap-new-panels`
- `clanlist-update-panels`
- `run-clanlist`
- `bootstrap-fresh-panels --modules vacation,discipline,voice_activity,tickets` without `--dry-run`
- `voice-finalize-cutover` without `--dry-run`
- `sync-commands --modules temp_voice,vacation,discipline,recruit,voice_activity,tickets` without `--dry-run`
- `run-superbot` real mode for `clanlist`, `temp_voice`, `vacation`, `discipline`, `recruit`, `voice_activity`, or `tickets`

The read-only source/readiness checks are safe to run at any time:

```bash
cargo run -- db-source-check --env-file .env.local
cargo run -- final-readiness-check --env-file .env.local
cargo run -- ticket-cutover-check --env-file .env.local
cargo run -- legacy-parity-audit --env-file .env.local
cargo run -- render-preview --env-file .env.local --modules all --format text
```

It opens `LEGACY_TICKET_DB_PATH` read-only, sets no production state, makes no Discord/Google calls, and redacts Google credential/sheet settings.

## Legacy Parity Gate

Deployment remains deferred until the operator accepts the visual/text parity matrix. Runtime readiness and legacy visual parity are separate gates:

| Module | Parity | Deployment note |
|---|---|---|
| clanlist | `ACCEPTED_DIFFERENCE` | Known embed model matches; live chunking is accepted as verified by Clanlist preview/target checks. |
| temp_voice | `ACCEPTED_DIFFERENCE` | No panel surface; concise ephemeral command response wording is accepted. |
| vacation | `PARTIAL` | Request panel/modal/officer review match legacy; active vacation row start/reason and DM embeds still need exact visual sign-off. |
| discipline | `PARTIAL` | Board title/color/buttons and core modal labels match; exact summary/footer and history embed layout still need the legacy pass. |
| recruit | `PARTIAL` | Decision title/buttons/modals/responses match; detailed embed fields and DMs still need exact legacy pass. |
| voice_activity | `PARTIAL` | Persistent controls and `/voice-top` text match; row formatting and refresh notice are not fully exact. |
| tickets | `PARTIAL` | Panel matches legacy; transcript uses the documented safe Rust HTML substitute, but ticket opening/close copy still differs. |

The parity commands are read-only: they do not connect to Discord, write legacy DBs, call Google, or mutate panel state.

## Old Services To Stop

| Module | Old service |
|---|---|
| clanlist | `xiii-clanlist.service` |
| temp_voice | `temp-voice-bot.service` |
| vacation | `PARTIAL` | Request panel/modal/officer review match legacy; active vacation row start/reason and DM embeds still need exact visual sign-off. |
| discipline | `xiii-discipline-bot.service` |
| recruit | `PARTIAL` | Decision title/buttons/modals/responses match; detailed embed fields and DMs still need exact legacy pass. |
| voice_activity | `PARTIAL` | Persistent controls and `/voice-top` text match; row formatting and refresh notice are not fully exact. |
| tickets | `xiii-ticketbot.service` |

Capture status files on the VPS:

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

## Backups

Before enabling any writer, copy the relevant legacy DB/state files to an offline backup:

```bash
mkdir -p /opt/XIII/backups/superbot-cutover-$(date -u +%Y%m%dT%H%M%SZ)
# Copy each LEGACY_* file or directory from .env into that backup directory.
```

Do not vacuum, checkpoint, migrate, or mutate legacy DBs during backup.

## Windows Local Validation

```powershell
cd "D:\clients\XIII 2\xiii-superbot"
cargo fmt --check
cargo check
cargo test --workspace
cargo run -- module-status --env-file .env.local
cargo run -- verify-cutover --env-file .env.local
cargo run -- db-source-check --env-file .env.local
cargo run -- final-readiness-check --env-file .env.local
cargo run -- prepare-cutover --env-file .env.local --modules clanlist,tempvoice,vacation,discipline,recruit,voice_activity,tickets
cargo run -- bootstrap-fresh-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-bootstrap --dry-run --modules vacation,discipline,voice_activity,tickets
cargo run -- sync-commands --env-file .env.local --allow-discord-write --confirm-sync-commands --modules temp_voice,vacation,discipline,recruit,voice_activity,tickets --dry-run
cargo run -- voice-cutover-check --env-file .env.local --allow-discord-read
cargo run -- voice-finalize-cutover --env-file .env.local --dry-run
cargo run -- run-superbot --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-superbot --modules clanlist,temp_voice,vacation,discipline,recruit,voice_activity,tickets --dry-run
```

## Vacation Cutover

1. Back up `LEGACY_VACATION_DB_PATH`.
2. Stop `xiii-vacation-bot.service`.
3. Capture stopped status in `data/service-status/xiii-vacation-bot.service.txt`.
4. Bootstrap fresh panels:

```bash
./xiii-superbot bootstrap-fresh-panels \
  --env-file .env \
  --allow-discord-read \
  --allow-discord-write \
  --confirm-bootstrap \
  --modules vacation
```

5. Set `VACATION_ENABLED=true`.
6. Sync commands if desired:

```bash
./xiii-superbot sync-commands \
  --env-file .env \
  --allow-discord-write \
  --confirm-sync-commands \
  --modules vacation
```

7. Run `verify-cutover`.

Vacation writes only vacation DB rows, vacation role add/remove, officer review messages with limited role pings, DMs, and the fresh active vacations panel.

## Recruit Cutover

1. Back up `LEGACY_RECRUIT_DB_PATH`.
2. Stop `xiii-recruit-bot.service`.
3. Capture stopped status in `data/service-status/xiii-recruit-bot.service.txt`.
4. Set `RECRUIT_ENABLED=true`.
5. Sync commands:

```bash
./xiii-superbot sync-commands \
  --env-file .env \
  --allow-discord-write \
  --confirm-sync-commands \
  --modules recruit
```

6. Run `verify-cutover`.

Recruit writes only recruit DB rows, decision panel messages, limited automatic decision role pings, role transitions, and DMs.

## Discipline Cutover

1. Back up `LEGACY_DISCIPLINE_DB_PATH`.
2. Stop `xiii-discipline-bot.service`.
3. Capture stopped status in `data/service-status/xiii-discipline-bot.service.txt`.
4. Bootstrap the fresh board:

```bash
./xiii-superbot bootstrap-fresh-panels \
  --env-file .env \
  --allow-discord-read \
  --allow-discord-write \
  --confirm-bootstrap \
  --modules discipline
```

5. Set `DISCIPLINE_ENABLED=true`.
6. Sync commands:

```bash
./xiii-superbot sync-commands \
  --env-file .env \
  --allow-discord-write \
  --confirm-sync-commands \
  --modules discipline
```

7. Run `verify-cutover`.

Discipline writes only discipline DB rows/action logs/locks, the fresh Superbot-owned board message, admin log messages, DMs, Discord timeouts, and configured role changes during clan removal. It never edits the old legacy board message.

## Voice Activity Cutover

1. Back up `LEGACY_VOICE_DB_PATH`.
2. Stop `xiii-voice-activity-bot.service`.
3. Capture stopped status in `data/service-status/xiii-voice-activity-bot.service.txt`.
4. Run the read-only cutover check:

```bash
./xiii-superbot voice-cutover-check \
  --env-file .env \
  --allow-discord-read
```

The command opens the legacy voice DB read-only, opens a read-only Gateway session, and fails if active DB sessions still exist. Live tracked Discord voice states are acceptable only after `data/voice_activity_cutover_state.json` records `policy=closed_active_at_cutover`.

If current sessions may be split at cutover, run the explicit finalize dry-run:

```bash
./xiii-superbot voice-finalize-cutover \
  --env-file .env \
  --dry-run
```

During the actual cutover window, after the old voice activity service is stopped and backups are complete, intentionally close open active rows exactly once:

```bash
./xiii-superbot voice-finalize-cutover \
  --env-file .env \
  --allow-legacy-db-write \
  --confirm-close-active-voice-sessions
```

This writes only `LEGACY_VOICE_DB_PATH` active/completed session rows plus `data/voice_activity_cutover_state.json`. It computes each duration as `max(0, cutover_ts - started_at)`, preserves all completed historical rows, does not delete old rows, and is idempotent after the active rows are closed. Running it a second time with no active rows is a no-op.

5. Bootstrap the fresh public stats panel:

```bash
./xiii-superbot bootstrap-fresh-panels \
  --env-file .env \
  --allow-discord-read \
  --allow-discord-write \
  --confirm-bootstrap \
  --modules voice_activity
```

6. Set `VOICE_ACTIVITY_ENABLED=true`.
7. Sync commands:

```bash
./xiii-superbot sync-commands \
  --env-file .env \
  --allow-discord-write \
  --confirm-sync-commands \
  --modules voice_activity
```

8. Run `final-readiness-check`, then `verify-cutover`.

Voice Activity writes only the legacy voice DB active/completed session rows, `bot_state` heartbeat/auto-report timestamps, the fresh Superbot-owned public stats panel, and optional inactive auto-report messages. It never edits the old legacy stats panel.

## Tickets Cutover

1. Back up `LEGACY_TICKET_DB_PATH`.
2. Confirm the Discord application has Message Content intent enabled for legacy `!panel`, `!accept`/`!принять`, and `!reject`/`!отклонить`.
3. Stop `xiii-ticketbot.service`.
4. Capture stopped status in `data/service-status/xiii-ticketbot.service.txt`.
5. Run the read-only check:

```bash
./xiii-superbot ticket-cutover-check --env-file .env
```

6. Bootstrap the fresh ticket panel:

```bash
./xiii-superbot bootstrap-fresh-panels \
  --env-file .env \
  --allow-discord-read \
  --allow-discord-write \
  --confirm-bootstrap \
  --modules tickets
```

7. Set `TICKETS_ENABLED=true`.
8. Sync commands:

```bash
./xiii-superbot sync-commands \
  --env-file .env \
  --allow-discord-write \
  --confirm-sync-commands \
  --modules tickets
```

9. Run `verify-cutover`.

Tickets writes only legacy ticket DB counter/lifecycle/dedupe rows, fresh ticket panel state, ticket channels/permissions/messages, officer review messages, DMs, and transcript attachments. Google Sheets access is read-only, credentials are never printed, and rows/signatures are marked processed only after officer review send succeeds. The Rust transcript is a safe HTML substitute for legacy `chat_exporter`, preserving message author, timestamp, content, and attachment URLs while escaping mentions.

## Combined Runtime

After each enabled module passes cutover verification:

```bash
./xiii-superbot run-superbot \
  --env-file .env \
  --allow-discord-read \
  --allow-discord-write \
  --confirm-run-superbot \
  --modules clanlist,temp_voice,vacation,discipline,recruit,voice_activity,tickets \
  --require-old-services-stopped \
  --old-services-dir data/service-status \
  --health-output data/superbot_health.json
```

## Rollback

1. Stop `xiii-superbot.service`.
2. Restore the backed-up DB/state file for the affected module if its writer had been enabled.
3. Restart only the old service for that module.
4. Re-run `verify-legacy`, `module-status`, and a module-specific smoke test.
