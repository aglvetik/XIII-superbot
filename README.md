# XIII Superbot

This repository contains the unified Rust Superbot that replaces the seven legacy XIII bots while keeping the legacy DBs and state files as the source of truth.

Deployment is intentionally deferred. The current goal is to keep the project clean, validated, and ready for a later cutover on the real VPS state.

## Current Status

- All seven modules are `READY_FULL`.
- Parity is currently:
  - `clanlist` — `ACCEPTED_DIFFERENCE`
  - `temp_voice` — `ACCEPTED_DIFFERENCE`
  - `vacation` — `EXACT`
  - `discipline` — `EXACT`
  - `recruit` — `EXACT`
  - `voice_activity` — `EXACT`
  - `tickets` — `ACCEPTED_DIFFERENCE`

## Not Deployed Yet

The Superbot is not deployed yet. Old services are still the production writers until a later VPS cutover, and the local validation DBs are not a substitute for fresh VPS copies.

## Safety Model

- No command writes to Discord unless explicit write flags are present.
- No module writes to a legacy DB unless the module is `READY_FULL`, its `*_ENABLED=true` flag is set, and the old-service guard passes.
- No command should touch Google unless a specific Google-backed runtime path is explicitly enabled; routine validation remains read-only.
- `.env.example` is safe to track as a production-shaped template. `.env.local` and any other secret-bearing env file are private and ignored.
- Legacy DBs and legacy JSON/cache files remain the source of truth until real deployment cutover.

## Current Readiness Matrix

| Module | Readiness | Notes |
|---|---|---|
| `clanlist` | `READY_FULL` | Fresh Superbot-owned Clanlist panels exist; old Clanlist data remains source of truth. |
| `temp_voice` | `READY_FULL` | Writes only the legacy temp voice DB and DB-owned temp channels after gates pass. |
| `vacation` | `READY_FULL` | Uses fresh vacation panel state and legacy vacation DB. |
| `discipline` | `READY_FULL` | Uses fresh discipline board state and legacy discipline DB. |
| `recruit` | `READY_FULL` | Uses legacy recruit DB and decision-channel runtime gates. |
| `voice_activity` | `READY_FULL` | Uses fresh stats panel state and legacy voice DB; active-session cutover is explicit. |
| `tickets` | `READY_FULL` | Uses fresh ticket panel state, legacy tickets DB, read-only Google polling, and gated Discord IO. |

## Current Parity Matrix

`../XIII_BOTS_FULL_COPY` is the visual/text source of truth for all legacy-facing surfaces during local validation only. Production env files must point at the real `/opt/XIII/...` legacy paths on the VPS.

| Module | Parity | Notes |
|---|---|---|
| `clanlist` | `ACCEPTED_DIFFERENCE` | Titles/color/footer match. Live chunking is verified with preview/target-message tools rather than metadata-only audit. |
| `temp_voice` | `ACCEPTED_DIFFERENCE` | No persistent visual panel existed; concise ephemeral wording is intentionally accepted. |
| `vacation` | `EXACT` | Request panel, modal, officer review, active panel, DMs, and early-end prompts match the audited legacy sources. |
| `discipline` | `EXACT` | Board/history surfaces, picker/select-first flow, modal wording, and pagination behavior match the audited legacy sources. |
| `recruit` | `EXACT` | Decision embeds, buttons, modals, decision summaries, and accept/reject/extend DMs match the audited legacy sources. |
| `voice_activity` | `EXACT` | Public stats and inactive-check embeds/views match the audited legacy row, footer, and paging behavior. |
| `tickets` | `ACCEPTED_DIFFERENCE` | Ticket lifecycle copy matches closely. The remaining intentional difference is the safe Rust HTML transcript substitute for Python `chat_exporter`. |

## Local Validation Commands

Run from the repository root:

```powershell
cargo fmt --check
cargo check
cargo test --workspace

cargo run -- check-config --env-file .env.example
cargo run -- module-status --env-file .env.local
cargo run -- verify-cutover --env-file .env.local
cargo run -- legacy-parity-audit --env-file .env.local
cargo run -- render-preview --env-file .env.local --modules all --format text
cargo run -- render-preview --env-file .env.local --modules all --format json --output data/render-preview.json
cargo run -- ticket-cutover-check --env-file .env.local
cargo run -- db-source-check --env-file .env.local
cargo run -- final-readiness-check --env-file .env.local
cargo run -- run-superbot --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-superbot --modules clanlist,temp_voice,vacation,discipline,recruit,voice_activity,tickets --dry-run

# Optional VPS-only preflight once .env.production and real legacy paths exist:
# cargo run -- production-preflight --env-file /opt/XIII/xiii-superbot/.env.production --allow-discord-read
```

## Pre-Deploy Checklist

Real deployment happens later. Before any production enablement:

1. Create a private `/opt/XIII/xiii-superbot/.env.production` from `.env.example`.
2. Replace the example `LEGACY_*` paths with the real VPS paths for the current production DBs/state files.
3. Run `production-preflight --env-file /opt/XIII/xiii-superbot/.env.production` after the real DBs/state files are in place.
4. Back up every legacy DB/state file before enabling a writer.
5. Stop the matching old services:
   - `xiii-clanlist.service`
   - `temp-voice-bot.service`
   - `xiii-vacation-bot.service`
   - `xiii-discipline-bot.service`
   - `xiii-recruit-bot.service`
   - `xiii-voice-activity-bot.service`
   - `xiii-ticketbot.service`
6. Ensure fresh Superbot-owned state files exist for:
   - `data/clanlist_panel_state.json`
   - `data/vacation_panel_state.json`
   - `data/discipline_panel_state.json`
   - `data/voice_activity_panel_state.json`
   - `data/ticket_panel_state.json`
7. Run `db-source-check` and `final-readiness-check`.
8. For Voice Activity, require either:
   - zero active legacy sessions, or
   - an intentional `voice-finalize-cutover` execution during the cutover window.
9. For Tickets, confirm Discord Message Content intent is enabled for legacy `!panel`, `!accept` / `!принять`, and `!reject` / `!отклонить`.
10. Keep command sync separate and explicit; do not auto-sync during runtime start.

Helpful operator-side scripts live under `scripts/linux/`:

- `verify-production-layout.sh` — read-only VPS layout/path validation for `.env.production`
- `collect-vps-precutover-state.sh` — safe capture of old-service status files before any service changes

Before using them on the VPS, do a shell syntax check there:

```bash
bash -n scripts/linux/collect-vps-precutover-state.sh
bash -n scripts/linux/verify-production-layout.sh
```

## Repo Hygiene Notes

- `data/` is treated as runtime state and generated output, so it stays untracked except for `data/.gitkeep`.
- Generated local reports such as render previews, parity audits, health files, and clanlist check outputs should stay out of version control.
- `Cargo.lock` is intentionally tracked.
- The legacy source snapshot under `../XIII_BOTS_FULL_COPY` is not part of this repo and must not be deleted as part of cleanup.

## Architecture Notes

The binary is operationally correct but not yet elegantly split:

- `src/app.rs` is the main coordinator and remains the largest file.
- `crates/xiii-clanlist/src/lib.rs` remains large because it still contains the proven Clanlist render/bootstrap/update path.
- A safe post-cutover refactor plan is documented in [docs/ARCHITECTURE_REFACTOR_NOTES.md](docs/ARCHITECTURE_REFACTOR_NOTES.md).

We are intentionally prioritizing stable behavior over aesthetic module boundaries until the real cutover is complete.

## Supporting Docs

- [docs/MODULE_COMPLETION_MATRIX.md](docs/MODULE_COMPLETION_MATRIX.md)
- [docs/CUTOVER_RUNBOOK.md](docs/CUTOVER_RUNBOOK.md)
- [docs/ARCHITECTURE_REFACTOR_NOTES.md](docs/ARCHITECTURE_REFACTOR_NOTES.md)
