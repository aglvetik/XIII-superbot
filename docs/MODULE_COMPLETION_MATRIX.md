# XIII Superbot Module Completion Matrix

Date: 2026-05-10

This is the strict readiness ledger. `READY_FULL` means the module has real runtime routing, real state or DB behavior, real Discord IO, cutover/status integration, and tests sufficient to enable it after the old service is stopped. `PARTIAL` means code exists but production enablement is still unsafe. `BLOCKED` means a known migration risk remains unresolved.

## Current Matrix

| Module | Current readiness | Runtime status | DB writes needed? | Discord writes needed? | Fresh panel state needed? | Old service guard needed? | Remaining blocker |
|---|---|---|---|---|---|---|---|
| clanlist | `READY_FULL` | Production runtime works through `run-clanlist` and `run-superbot --modules clanlist`. | No legacy DB writes. | Edits only three Superbot-owned Clanlist messages from `data/clanlist_panel_state.json`. | `data/clanlist_panel_state.json` exists. | `xiii-clanlist.service`. | Optional Google Sheets Steam source remains deferred; legacy Steam cache is production source. |
| temp_voice | `READY_FULL` | Real Gateway `VOICE_STATE_UPDATE`, `/setup-voice-hub`, startup reconciliation, DB-owned delete guard. | Yes, `LEGACY_TEMP_VOICE_DB_PATH` after `TEMP_VOICE_ENABLED=true` and guard passes. | Creates/moves/deletes only legacy DB-owned temp voice channels. | No global panel. | `temp-voice-bot.service`. | None for legacy feature set. |
| vacation | `READY_FULL` | Production runtime works through `run-superbot --modules vacation`; legacy DB state is preserved, and the audited visible surfaces now match the legacy bot. | Yes, `LEGACY_VACATION_DB_PATH` after `VACATION_ENABLED=true` and guard passes. | Officer review messages, vacation role add/remove, active panel refresh, and DM attempts. | `data/vacation_panel_state.json` exists. | `xiii-vacation-bot.service`. | No runtime blocker; legacy parity is now `EXACT` for the audited request panel, modal, officer review, active panel, and DM/early-end surfaces. |
| discipline | `READY_FULL` | Production runtime works through `run-superbot --modules discipline`; legacy DB state is preserved, with real issue/remove/history flows and board refresh. | Yes, `LEGACY_DISCIPLINE_DB_PATH` after `DISCIPLINE_ENABLED=true` and guard passes. | Discipline board edits, admin logs, timeout/role operations, and DM attempts. | `data/discipline_panel_state.json` exists. | `xiii-discipline-bot.service`. | No runtime blocker; legacy parity remains `PARTIAL` because the live board actions still use modal-first ID/mention entry instead of the legacy picker-first member-selection flow. |
| recruit | `READY_FULL` | Production runtime works through `run-superbot --modules recruit`; legacy DB state is preserved, with real decision routing, due checker, and voice tracking. | Yes, `LEGACY_RECRUIT_DB_PATH` after `RECRUIT_ENABLED=true` and guard passes. | Decision panels, role transitions, DM attempts, and automatic/manual due messages. | No global panel state required. | `xiii-recruit-bot.service`. | No runtime blocker; legacy parity is still `PARTIAL` for detailed embed fields/footer and DM bodies. |
| voice_activity | `READY_FULL` | Real Gateway voice tracking, startup reconciliation, public stats panel refresh, inactive checks, auto-report guard, fresh panel bootstrap, command sync, and explicit active-session finalization. | Yes, `LEGACY_VOICE_DB_PATH` after `VOICE_ACTIVITY_ENABLED=true`, stopped service guard, and clean/finalized voice cutover state. | Fresh public stats panel edits and inactive auto-report sends. | `data/voice_activity_panel_state.json`; optional `data/voice_activity_cutover_state.json` after active-session finalization. | `xiii-voice-activity-bot.service` plus `voice-cutover-check`. | Operational: if active sessions exist, run `voice-finalize-cutover --dry-run` then execute with `--allow-legacy-db-write --confirm-close-active-voice-sessions` during cutover. Completed historical stats are preserved. |
| tickets | `READY_FULL` | Real `run-superbot` slash/component/text routing, transactional legacy DB writer, ticket channel Discord IO, Google officer-review poller, fresh panel bootstrap, and safe HTML transcript delivery. | Yes, `LEGACY_TICKET_DB_PATH` after `TICKETS_ENABLED=true` and guard passes. | Ticket channels/messages/DMs/transcripts/officer reviews and application accept role updates. | `data/ticket_panel_state.json`. | `xiii-ticketbot.service`. | Operational: bootstrap fresh ticket panel, enable Message Content intent, and stop old service before enabling. |

## Command Behavior

| Command | Status | Writes? |
|---|---|---|
| `run-clanlist` | Production-ready Clanlist-only runtime. | Edits only the three fresh Clanlist messages; writes only Superbot state/health. |
| `run-superbot --modules clanlist` | Delegates real mode to the proven Clanlist runtime. | Clanlist-only writes in real mode; none in dry-run. |
| `run-superbot --modules temp_voice` | Production-ready Temp Voice runtime with one Gateway connection. | Writes only `LEGACY_TEMP_VOICE_DB_PATH` and DB-owned temp voice channels after the old service guard passes. |
| `run-superbot --modules vacation` | Production-capable after `VACATION_ENABLED=true`, fresh state exists, and old service guard passes. | Writes only vacation DB rows, vacation role changes, DMs/officer messages, and fresh active-panel edits. |
| `run-superbot --modules recruit` | Production-capable after `RECRUIT_ENABLED=true` and old service guard passes. | Writes only recruit DB rows, recruit role transitions, DMs, and decision panels. |
| `run-superbot --modules discipline` | Production-capable after `DISCIPLINE_ENABLED=true`, fresh board state exists, and old service guard passes. | Writes only discipline DB rows, board edits, admin logs, DMs, timeouts, and configured clan-removal role changes. |
| `run-superbot --modules voice_activity` | Production-capable after `VOICE_ACTIVITY_ENABLED=true`, fresh panel state exists, old service guard passes, and voice cutover is clean or finalized. | Writes only voice activity DB rows, the fresh public stats panel, bot_state heartbeat/auto-report timestamps, and optional inactive auto-report messages. It refuses real startup with active legacy sessions unless `data/voice_activity_cutover_state.json` records `policy=closed_active_at_cutover`. |
| `voice-finalize-cutover` | Explicit voice cutover writer for open `active_voice_sessions`. | Dry-run is read-only. Execute mode requires `--allow-legacy-db-write --confirm-close-active-voice-sessions`, closes active rows once at a single timestamp, inserts completed session rows with non-negative durations, and writes `data/voice_activity_cutover_state.json`. |
| `db-source-check` | Read-only source-of-truth DB/path verification. | No writes. Prints exact legacy DB paths and row counts; fails missing/empty DBs. |
| `final-readiness-check` | Read-only all-module deployment gate. | No writes by default. Fails if required state/DB is missing or voice active sessions are unresolved; optional `--allow-discord-read` adds panel ownership checks. |
| `run-superbot --modules tickets` | Production-capable after `TICKETS_ENABLED=true`, fresh panel state exists, old service guard passes, and Message Content intent is enabled. | Writes ticket DB lifecycle/dedupe rows, creates/updates ticket channels, sends transcripts/DMs/officer reviews, and reads Google Sheets read-only. |
| `bootstrap-fresh-panels --modules vacation` | Can create missing fresh vacation request/active panels after explicit write flags. | Writes only `data/vacation_panel_state.json` and new Superbot-owned Discord messages. |
| `bootstrap-fresh-panels --modules discipline` | Can create missing fresh Discipline board after explicit write flags. | Writes only `data/discipline_panel_state.json` and a new Superbot-owned board message. |
| `bootstrap-fresh-panels --modules voice_activity` | Can create the missing fresh public stats panel after explicit write flags. | Writes only `data/voice_activity_panel_state.json` and a new Superbot-owned Discord message. |
| `bootstrap-fresh-panels --modules tickets` | Can create the missing fresh ticket panel after explicit write flags. | Writes only `data/ticket_panel_state.json` and a new Superbot-owned Discord message. |
| `sync-commands --modules temp_voice,vacation,discipline,recruit,voice_activity,tickets` | Can register guild-scoped commands after explicit write flags. | Guild command registration only. |
| `sync-commands --modules discipline` | Plans/registers guild-scoped `/discipline` after explicit write flags. | Guild command registration only. |
| `module-status` | Honest local readiness report. | No writes. |
| `verify-cutover` | Strict local verifier for enabled modules. | No writes. |

## Final Readiness Summary

```text
Module          Readiness
clanlist        READY_FULL
temp_voice      READY_FULL
vacation        READY_FULL
discipline      READY_FULL
recruit         READY_FULL
voice_activity  READY_FULL
tickets         READY_FULL
```

## Legacy Parity Status

Legacy source under `../XIII_BOTS_FULL_COPY` is the visual/text source of truth. `READY_FULL` means the runtime is production-capable; it does not automatically mean every embed/button string is pixel-identical. Use:

```powershell
cargo run -- legacy-parity-audit --env-file .env.local
cargo run -- render-preview --env-file .env.local --modules all --format text
```

| Module | Visual/behavior parity | Notes |
|---|---|---|
| clanlist | `ACCEPTED_DIFFERENCE` | Titles, color `#0066FF`, marker URLs, and Russian footer wording match. Metadata audit accepts that live chunking is verified by Clanlist preview/target-message commands. |
| temp_voice | `ACCEPTED_DIFFERENCE` | No persistent panel. `/setup-voice-hub channel_id` and runtime behavior match; concise ephemeral wording is accepted. |
| vacation | `EXACT` | Request panel, request modal, officer review, active vacations panel, DM embeds, and early-end prompts match the audited legacy Go sources. |
| discipline | `PARTIAL` | Board/history text surfaces now match closely, but the live board entry flow still differs: legacy `panel.ts` / `historyFlow.ts` are picker-first, while Superbot board actions remain modal-first. |
| recruit | `PARTIAL` | Decision title/buttons/modals/responses are ported; detailed field layout/footer and DM embed bodies still differ. |
| voice_activity | `PARTIAL` | Visible text surfaces, row formatting, and `/voice-top` wording now match closely. Remaining mismatch: live previous/next disabled state and auto-report title labelling still differ from legacy. |
| tickets | `PARTIAL` | Ticket panel title/description/buttons/color match legacy constants. Safe Rust HTML transcript is the documented accepted substitute for Python `chat_exporter`; opening and close/reopen/DM summary copy still differ. |

## Ticket Module Notes

The ticket crate now has a real SQLite writer for the legacy `tickets.db` schema:

- `counters` are incremented transactionally during ticket reservation.
- `tickets` reservation/finalization/close/reopen/delete state changes are implemented.
- `processed_forms` and `processed_form_signatures` dedupe writes are implemented.
- `bot_state` read/write helpers are implemented.
- A Twilight HTTP ticket IO adapter now models/executes channel creation, permission overwrites, panel/opening/officer/DM messages, channel rename/delete, and transcript attachment sends behind runtime gates.
- A read-only Google Sheets service-account OAuth client exists for Sheets values reads; tests cover range planning and row conversion without network calls.
- A safe Rust HTML transcript renderer is the production substitute for Python `chat_exporter`; it preserves author, timestamp, content, attachments, and escapes mentions/content.
- `run-superbot` dispatches ticket slash commands, legacy text commands, components, ticket creation/lifecycle, and the Google officer-review poller behind `TICKETS_ENABLED=true` and old-service gates.
- Tests use temporary SQLite fixtures only.

Tickets is `READY_FULL`, but it is still high risk operationally: enable Message Content intent before cutover, bootstrap `data/ticket_panel_state.json`, stop `xiii-ticketbot.service`, back up `LEGACY_TICKET_DB_PATH`, and run `ticket-cutover-check` plus `verify-cutover` before enabling `TICKETS_ENABLED=true`.

## Final Hardening Notes

- Legacy DBs and legacy Clanlist JSON/cache files remain the source of truth at deployment time. Superbot state files under `data/` hold only fresh panel IDs, health/cutover metadata, and Superbot-owned runtime state.
- `db-source-check` must be green before deployment. It warns on local `XIII_BOTS_FULL_COPY` paths so operators remember to verify VPS paths, but it still treats those paths as valid for local staging.
- `final-readiness-check` is the final no-write deployment gate. If `active_voice_sessions` are still present, it prints the exact `voice-finalize-cutover` command instead of silently allowing runtime startup.
- The voice cutover policy allows splitting current in-progress sessions at cutover. The explicit finalize command preserves all completed historical rows and closes only currently active legacy rows once.
