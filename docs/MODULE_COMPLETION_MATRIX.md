# XIII Superbot Module Completion Matrix

Date: 2026-05-13

This is the strict runtime-readiness ledger. All seven modules are now `READY_FULL`; deployment is still deferred until the operator-side cutover checklist is completed on the real VPS state.

## Current Readiness Matrix

| Module | Readiness | Runtime path | Legacy source of truth | Fresh Superbot state | Old service guard | Operational note |
|---|---|---|---|---|---|---|
| `clanlist` | `READY_FULL` | `run-clanlist`, `run-superbot --modules clanlist` | Legacy Clanlist JSON/cache directory | `data/clanlist_panel_state.json` | `xiii-clanlist.service` | Optional Google Steam source is still deferred; legacy Steam cache remains the production source. |
| `temp_voice` | `READY_FULL` | `run-superbot --modules temp_voice` | `LEGACY_TEMP_VOICE_DB_PATH` | none | `temp-voice-bot.service` | Deletes only temp channels tracked in legacy `temp_voice_channels`. |
| `vacation` | `READY_FULL` | `run-superbot --modules vacation` | `LEGACY_VACATION_DB_PATH` | `data/vacation_panel_state.json` | `xiii-vacation-bot.service` | Writes DB rows, vacation role changes, officer review messages, DMs, and fresh active-panel updates only after gates pass. |
| `discipline` | `READY_FULL` | `run-superbot --modules discipline` | `LEGACY_DISCIPLINE_DB_PATH` | `data/discipline_panel_state.json` | `xiii-discipline-bot.service` | Uses transactional issue/remove/history flows plus fresh board refresh. |
| `recruit` | `READY_FULL` | `run-superbot --modules recruit` | `LEGACY_RECRUIT_DB_PATH` | none | `xiii-recruit-bot.service` | Decision panels are idempotent through legacy DB decision message tracking. |
| `voice_activity` | `READY_FULL` | `run-superbot --modules voice_activity` | `LEGACY_VOICE_DB_PATH` | `data/voice_activity_panel_state.json`, optional `data/voice_activity_cutover_state.json` | `xiii-voice-activity-bot.service` | Requires either zero active sessions or explicit `voice-finalize-cutover` completion before real enablement. |
| `tickets` | `READY_FULL` | `run-superbot --modules tickets` | `LEGACY_TICKET_DB_PATH` | `data/ticket_panel_state.json` | `xiii-ticketbot.service` | Requires Message Content intent, fresh panel state, and read-only Google config before real enablement. |

## Current Parity Matrix

Legacy source under `../XIII_BOTS_FULL_COPY` remains the visual/text source of truth.

| Module | Parity | Notes |
|---|---|---|
| `clanlist` | `ACCEPTED_DIFFERENCE` | Titles, color `#0066FF`, marker URLs, and footer wording match. The accepted difference is that live chunking is verified through preview/target-message checks instead of metadata-only audit. |
| `temp_voice` | `ACCEPTED_DIFFERENCE` | Runtime behavior matches; no persistent panel existed in the legacy bot, so concise ephemeral wording is accepted. |
| `vacation` | `EXACT` | Request panel, modal, officer review, active vacations panel, DM embeds, and early-end prompts match the audited legacy Go sources. |
| `discipline` | `EXACT` | Board/history copy, picker/select-first flow, modal wording, and pagination behavior match the audited legacy TypeScript sources. |
| `recruit` | `EXACT` | Decision embeds, buttons, modals, decision summary fields, and accept/reject/extend DMs match the audited legacy Python sources. |
| `voice_activity` | `EXACT` | Public stats and inactive-check views now match the audited legacy row formatting, boundary buttons, period labels, and titles. |
| `tickets` | `ACCEPTED_DIFFERENCE` | Visible ticket lifecycle copy matches legacy closely. The remaining intentional difference is the safe Rust HTML transcript substitute for Python `chat_exporter`. |

## Accepted Differences

| Module | Legacy reference | New reference | Reason |
|---|---|---|---|
| `clanlist` | `../XIII_BOTS_FULL_COPY/opt/XIII/XIII-clanlist` | `crates/xiii-clanlist/src/lib.rs` plus preview/target-message commands | Read-only audit metadata cannot prove live Discord chunking; the live surface is verified with dedicated preview/check commands. |
| `temp_voice` | `../XIII_BOTS_FULL_COPY/opt/XIII/temp-voice-bot` | `src/app.rs` temp voice responses | Legacy temp voice had no persistent panel. Concise Superbot ephemeral wording preserves behavior without inventing extra UI. |
| `tickets` | `../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot/app/services/transcript_service.py` | `crates/xiii-tickets/src/render.rs::transcript_html` | Safe Rust HTML transcript preserves author/timestamp/content/attachments without reproducing Python `chat_exporter` byte-for-byte. |

## Command Readiness

| Command | Status | Writes? |
|---|---|---|
| `module-status` | Ready | No writes |
| `verify-cutover` | Ready | No writes |
| `db-source-check` | Ready | No writes |
| `production-preflight` | Ready | No writes by default; optional Discord read checks with `--allow-discord-read` |
| `final-readiness-check` | Ready | No writes by default; optional Discord read checks with `--allow-discord-read` |
| `legacy-parity-audit` | Ready | No writes |
| `render-preview` | Ready | No writes; can emit UTF-8 text/JSON reports |
| `ticket-cutover-check` | Ready | No writes |
| `voice-cutover-check` | Ready | Discord reads only when explicitly allowed |
| `voice-finalize-cutover` | Ready | Legacy voice DB write only with explicit confirm flags |
| `bootstrap-fresh-panels` | Ready | Creates only fresh Superbot-owned panels after explicit write flags |
| `sync-commands` | Ready | Guild command registration only after explicit write flags |
| `run-clanlist` | Ready | Clanlist-only production runtime |
| `run-superbot` | Ready | Real writes remain gated per module and explicit flags |

## Current Pre-Deploy Blockers

There are no remaining code-readiness blockers. The remaining blockers are operational:

1. Refresh/verify real VPS `LEGACY_*` paths before production use.
2. Stop all old per-module services before enabling the Superbot writers.
3. Bootstrap fresh panel state on the real guild where needed.
4. Run `production-preflight --env-file /opt/XIII/xiii-superbot/.env.production` after copying the real DBs/state files into place.
5. Confirm Message Content intent before ticket cutover.
6. Resolve active Voice Activity sessions by waiting for zero active sessions or running `voice-finalize-cutover` during the cutover window.
7. Keep all secrets in local env files only; do not commit them.

## Recommended Local Validation

```powershell
cargo fmt --check
cargo check
cargo test --workspace

cargo run -- module-status --env-file .env.local
cargo run -- verify-cutover --env-file .env.local
cargo run -- legacy-parity-audit --env-file .env.local
cargo run -- render-preview --env-file .env.local --modules all --format text
cargo run -- render-preview --env-file .env.local --modules all --format json --output data/render-preview.json
cargo run -- ticket-cutover-check --env-file .env.local
cargo run -- db-source-check --env-file .env.local
cargo run -- final-readiness-check --env-file .env.local
```
