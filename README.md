# XIII Superbot

Safe executable scaffold and first production-ready Clanlist module for the future XIII Superbot.

This workspace is intentionally safe and inert:

- It does not connect to Discord except for explicit `--allow-discord-read` diagnostic commands, the explicitly confirmed Clanlist write commands, and explicitly confirmed production module runtimes.
- It loads an env file only when `--env-file` is explicitly passed.
- Legacy SQLite/JSON files remain the source of truth for migrated modules. Verification opens them read-only; write mode is available only in explicitly confirmed READY_FULL module runtimes after the relevant env flag and old-service guard pass.
- It does not write migrations.
- It does not create Discord messages except for explicitly confirmed bootstrap commands. Clanlist bootstrap creates only the three Clanlist panels; global `bootstrap-fresh-panels` can create fresh Vacation panels and the fresh Discipline board once gated. It edits Discord messages only through explicitly confirmed module runtimes. Temp Voice may create/move/delete voice channels only in `run-superbot --modules temp_voice` after the old service guard passes, and deletion is limited to channel IDs tracked in the legacy temp voice DB. Vacation, Discipline, and Recruit can modify roles, send DMs, and write their legacy DBs only after their env flags and old service guards pass. It never processes Google Sheets by default or edits old legacy panels. Slash command registration is separate and explicit through `sync-commands`.

Source planning docs:

- `..\docs\XIII_SUPERBOT_AUDIT.md`
- `..\docs\XIII_SUPERBOT_MIGRATION_PLAN.md`
- `..\docs\XIII_SUPERBOT_ARCHITECTURE.md`
- `..\docs\XIII_SUPERBOT_ENV_MAPPING.md`
- `..\docs\XIII_SUPERBOT_DB_STRATEGY.md`
- `..\docs\XIII_SUPERBOT_PARITY_CHECKLIST.md`

## Target

- Rust + Twilight.
- One binary.
- One Discord Gateway connection.
- One systemd service.
- Legacy SQLite/JSON state first.
- Unified DB only after parity is proven.

## Current Production Status

Clanlist is the first migrated module. The fresh Superbot-owned panels are:

| Panel | Channel ID | Message ID |
|---|---:|---:|
| Main roster | `1498762828666896535` | `1502618001881436320` |
| Admin roster | `1498763049102868672` | `1502618004545077269` |
| Steam roster | `1500081418506862754` | `1502618005841117215` |

State is stored in `data/clanlist_panel_state.json`. Old legacy Clanlist panel IDs are reference only and are never edited by the Superbot. Temp Voice, Vacation, Discipline, Recruit, Voice Activity, and Tickets now have real runtime wiring, legacy SQLite writers, and Twilight Discord IO behind explicit safety gates.

Current readiness:

| Module | Readiness | Production enablement |
|---|---|---|
| clanlist | `READY_FULL` | Safe to run after the old Clanlist service is stopped/verified. |
| temp_voice | `READY_FULL` | Safe to run after `TEMP_VOICE_ENABLED=true` and `temp-voice-bot.service` is stopped/verified. Creates/moves/deletes only legacy DB-owned temp voice channels. |
| vacation | `READY_FULL` | Safe to enable after `VACATION_ENABLED=true`, fresh panel state exists, and `xiii-vacation-bot.service` is stopped/verified. |
| discipline | `READY_FULL` | Safe to enable after `DISCIPLINE_ENABLED=true`, fresh board state exists, and `xiii-discipline-bot.service` is stopped/verified. |
| recruit | `READY_FULL` | Safe to enable after `RECRUIT_ENABLED=true` and `xiii-recruit-bot.service` is stopped/verified. |
| voice_activity | `READY_FULL` | Safe to enable after `VOICE_ACTIVITY_ENABLED=true`, fresh panel state exists, old service is stopped/verified, and `voice-cutover-check` reports either zero active/live sessions or an explicit finalized cutover state. Active DB sessions may be intentionally closed only with `voice-finalize-cutover`. |
| tickets | `READY_FULL` | Enable only after fresh ticket panel bootstrap, Message Content intent confirmation, stopped `xiii-ticketbot.service`, and `ticket-cutover-check`. Uses read-only Google Sheets polling and safe Rust HTML transcripts. |

## Legacy Parity Status

The seven modules are runtime-ready, but legacy visual/text parity is tracked separately. `../XIII_BOTS_FULL_COPY` remains the source of truth for embed colors, titles, descriptions, footers, buttons, modals, commands, and response wording.

Run the read-only parity tools before deployment work:

```powershell
cargo run -- legacy-parity-audit --env-file .env.local
cargo run -- render-preview --env-file .env.local --modules all --format text
cargo run -- render-preview --env-file .env.local --modules all --format json --output data/render-preview.json
```

Current visual/behavior parity:

| Module | Parity | Notes |
|---|---|---|
| clanlist | `ACCEPTED_DIFFERENCE` | Titles/color/footer match; live chunking is verified with `clanlist-render-preview` rather than metadata-only parity audit. |
| temp_voice | `ACCEPTED_DIFFERENCE` | No persistent visual panel; behavior matches and short ephemeral wording is intentionally concise. |
| vacation | `EXACT` | Request panel, request modal, officer review, active vacations panel, DM embeds, and early-end prompts match the audited legacy sources. |
| discipline | `PARTIAL` | Board/history text surfaces now match closely, but the live board action flow still differs: legacy is picker-first while current Superbot board actions remain modal-first. |
| recruit | `PARTIAL` | Decision title/buttons/modals/responses match; detailed decision fields and DM embed bodies still differ. |
| voice_activity | `PARTIAL` | Visible text surfaces, row formatting, and disabled `/voice-top` text now match closely; live previous/next disabled state and auto-report title labeling still differ. |
| tickets | `PARTIAL` | Panel title/description/buttons/color match; transcript is an accepted safe Rust HTML substitute, but opening and close/reopen/DM summary copy still differ. |

See `docs\MODULE_COMPLETION_MATRIX.md` and `docs\CUTOVER_RUNBOOK.md` for the current blocker ledger and cutover runbook.

## Crates

| Crate | Purpose |
|---|---|
| `xiii-core` | Shared descriptors and module boundary traits |
| `xiii-config` | Unified config structs and safe redaction |
| `xiii-db` | Legacy state catalog and verification query planning |
| `xiii-discord` | Twilight-facing router/registry boundary |
| `xiii-scheduler` | Scheduler job registry and non-overlap descriptors |
| `xiii-permissions` | Shared permission rule descriptors |
| `xiii-tickets` | Ticket module manifest |
| `xiii-voice-activity` | Voice activity module manifest |
| `xiii-recruit` | Recruit module manifest |
| `xiii-vacation` | Vacation module manifest |
| `xiii-discipline` | Discipline module manifest |
| `xiii-clanlist` | Clanlist module manifest |
| `xiii-tempvoice` | Temp voice module manifest |

## DB Choice

The scaffold chooses `sqlx` for SQLite because the final bot will be async and long-running under `tokio`. Early DB code must use read-only legacy connections. If strict serialized writes are needed after cutover, use a single DB actor around `sqlx` rather than ad hoc concurrent writes.

## Rust Edition

The workspace uses Rust 2021. The local machine used for scaffolding did not have `rustc` or `cargo` available, so Rust 2024 stability could not be validated locally.

## Check

## Install Rust On Windows

Install Rust with `rustup` from the official installer:

1. Open [https://rustup.rs](https://rustup.rs).
2. Download and run `rustup-init.exe`.
3. Choose the default stable toolchain.
4. Restart PowerShell so `cargo` is on `PATH`.

Verify:

```powershell
rustc --version
cargo --version
```

## Cargo Check

From this directory:

```powershell
cargo check
```

Running `cargo check` creates `Cargo.lock` and `target/`.

## CLI Commands

All commands avoid legacy SQLite/JSON writes and never send HTTP requests to Google. Most commands are read-only. The only write-capable Clanlist commands are `clanlist-bootstrap-new-panels`, limited to creating exactly three fresh Discord messages, and `clanlist-update-panels`, limited to editing exactly the three fresh messages recorded in Superbot state.

Validate a unified env file:

```powershell
cargo run -- check-config --env-file .env.example
```

Verify legacy DB/JSON state in read-only mode:

```powershell
cargo run -- verify-legacy --env-file .env.example
```

Print module descriptors:

```powershell
cargo run -- print-manifest
```

Audit legacy parity without Discord, DB writes, or Google:

```powershell
cargo run -- legacy-parity-audit --env-file .env.local
```

Preview Discord-facing render metadata for all modules:

```powershell
cargo run -- render-preview --env-file .env.local --modules all --format text
cargo run -- render-preview --env-file .env.local --modules all --format json --output data/render-preview.json
```

Preview clanlist legacy JSON/cache state without Discord or Google:

```powershell
cargo run -- clanlist-preview --env-file .env.example
cargo run -- clanlist-preview --env-file .env.example --format json
cargo run -- clanlist-preview --env-file .env.example --include-steam
cargo run -- clanlist-preview --env-file .env.example --no-steam
```

Fetch a read-only Discord clanlist diagnostic snapshot:

```powershell
cargo run -- discord-readonly-clanlist-snapshot --env-file .env.local --allow-discord-read
cargo run -- discord-readonly-clanlist-snapshot --env-file .env.local --allow-discord-read --format json
cargo run -- discord-readonly-clanlist-snapshot --env-file .env.local --allow-discord-read --roles-only
cargo run -- discord-readonly-clanlist-snapshot --env-file .env.local --allow-discord-read --format json --output clanlist-snapshot.json
```

This command requires `--allow-discord-read` so accidental Discord connections fail closed. It uses Discord HTTP REST to fetch guild roles and, unless `--roles-only` is passed, guild members. It has bounded retry handling for Discord HTTP 429 responses and respects `retry_after` when Discord returns it. It does not open the Gateway, register slash commands, edit messages, send messages, modify roles, create/delete channels, send DMs, run schedulers, query Google Sheets, or write legacy JSON files.

Build a read-only Clanlist render parity preview:

```powershell
cargo run -- clanlist-render-preview --env-file .env.local --allow-discord-read
cargo run -- clanlist-render-preview --env-file .env.local --allow-discord-read --format json
cargo run -- clanlist-render-preview --env-file .env.local --allow-discord-read --no-steam
cargo run -- clanlist-render-preview --env-file .env.local --allow-discord-read --output clanlist-render-preview.txt
cargo run -- clanlist-render-preview --env-file .env.local --allow-discord-read --format json --output clanlist-render-preview.json
cargo run -- clanlist-render-preview --env-file .env.local --allow-discord-read --max-members-per-section 50
```

This command uses the legacy Clanlist JSON/cache files plus read-only Discord role/member data to preview the embeds the future Rust Clanlist module would render. Google Sheets stays disabled, so the Steam panel uses `steam_roster_cache.json` only. Text output shows up to 20 members per section by default; use `--max-members-per-section` to adjust that. JSON output still includes full lists. It does not edit Discord messages, send Discord messages, pin messages, modify roles, register commands, start schedulers, or write legacy JSON files.

`--output <path>` writes the final report as UTF-8. Parent directories must already exist. Output paths inside `LEGACY_CLANLIST_DATA_DIR` or over legacy SQLite state files are rejected. When `--format json` is used, stdout is clean JSON if no output file is given, and output files contain only the JSON report.

Build a dry-run Clanlist write plan without executing writes:

```powershell
cargo run -- clanlist-write-plan --env-file .env.local --allow-discord-read --allow-write-plan
cargo run -- clanlist-write-plan --env-file .env.local --allow-discord-read --allow-write-plan --format json --output clanlist-write-plan.json
cargo run -- clanlist-write-plan --env-file .env.local --allow-discord-read --allow-write-plan --require-old-service-stopped --old-service-status-file xiii-clanlist-service-status.txt
```

This command is a plan only. Every planned operation is `allowed=false`; it does not edit Discord messages, send messages, create/delete messages, modify roles, register commands, open Gateway, call Google Sheets, write legacy JSON/DB files, or start schedulers. The optional old service guard reads a local status file only and never calls `systemctl`.

Example status file capture on the old Linux host:

```bash
systemctl status xiii-clanlist.service --no-pager > xiii-clanlist-service-status.txt
```

On Windows or local staging, the status file is optional unless `--require-old-service-stopped` is used. If that flag is used, the file must show stopped/inactive evidence such as `Active: inactive`, `Active: failed`, `Loaded: not-found`, or `Unit xiii-clanlist.service could not be found`.

Verify the three existing Clanlist target messages with read-only Discord GET calls:

```powershell
cargo run -- clanlist-target-message-check --env-file .env.local --allow-discord-read
cargo run -- clanlist-target-message-check --env-file .env.local --allow-discord-read --format json --output clanlist-target-message-check.json
```

This command fetches only the current bot identity and the exact three target messages from the legacy Clanlist JSON files. It checks whether each message exists, who authored it, whether the current bot would be allowed to edit it in a later cutover, and the first embed title/footer/marker fields. It does not fetch message history. It performs no writes: no message edits, no sends, no deletes, no command registration, no Gateway, no Google Sheets, no schedulers, and no legacy JSON/DB writes.

Future Clanlist edit cutover requires all target messages to be authored by the current bot. Discord bots cannot edit messages created by another bot or user.

Bootstrap fresh Clanlist panel messages:

```powershell
cargo run -- clanlist-bootstrap-new-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-create-new-panels --dry-run
cargo run -- clanlist-bootstrap-new-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-create-new-panels
cargo run -- clanlist-bootstrap-new-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-create-new-panels --format json --output clanlist-bootstrap-result.json
```

This is the first write-capable command in the scaffold. It can create exactly three new Discord messages only after all three explicit flags are present: `--allow-discord-read`, `--allow-discord-write`, and `--confirm-create-new-panels`. It never edits or deletes old messages, never modifies roles, never sends DMs, never registers commands, never opens Gateway, never calls Google Sheets, and never writes legacy Clanlist JSON or legacy DB files.

Use `--dry-run` first. Dry-run builds the payloads and planned `create_message` operations but does not send messages and does not write panel state.

Panel embeds use UTC for the generated footer timestamp. Google Sheets remains disabled; the Steam panel uses only the legacy Steam cache, or an empty Steam panel if `--no-steam` is used.

Successful execution writes new Superbot-owned state to:

```text
data/clanlist_panel_state.json
```

Use `--state-output <path>` to choose a different new-state path. The command rejects state paths inside `LEGACY_CLANLIST_DATA_DIR`, legacy DB/JSON paths, and old bot directories. If a partial create happens, the command does not delete anything automatically; it writes a recovery report under `data/clanlist_bootstrap_partial_<timestamp>.json` when safe.

After a real bootstrap, manually verify the three new messages in Discord. Delete the old Clanlist panels manually only after verification. Keep the old legacy JSON files as backup/reference. Rollback is manual: stop using the new panel state and keep or restore the old Clanlist service/panels as needed; this command does not touch them.

Update the fresh Clanlist panel messages recorded in Superbot state:

```powershell
cargo run -- clanlist-update-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-update-panels --dry-run
cargo run -- clanlist-update-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-update-panels
cargo run -- clanlist-update-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-update-panels --format json --output clanlist-update-result.json
```

This command edits only the three messages in `data/clanlist_panel_state.json` by default. Use `--state-file <path>` only for a new Superbot state file; paths inside `LEGACY_CLANLIST_DATA_DIR`, legacy DB/JSON paths, and old bot directories are rejected. It verifies the state guild ID, bot user ID, non-zero unique message IDs, current bot identity, exact target message existence, and message authorship before any edit. If a target is missing or not authored by the current bot, the command fails before editing anything.

Use `--dry-run` first. Dry-run fetches/validates state, Discord roles/members, and the exact target messages, then prints three `edit_existing_message allowed=false` operations. It does not PATCH messages and does not update state.

Successful execution updates only the new state file with `last_updated_at_utc`, `last_update_source = "manual_update_command"`, `last_render_summary`, and the last successful update message IDs. State writes use a temporary file plus rename. If a partial edit occurs, the command does not delete or roll back automatically; it writes a recovery report under `data/clanlist_update_partial_<timestamp>.json` when safe.

Optional old service guard:

```powershell
cargo run -- clanlist-update-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-update-panels --require-old-service-stopped --old-service-status-file xiii-clanlist-service-status.txt
```

Rollback note: because this command edits only the three new Superbot-owned messages, rollback is manual. Rerun bootstrap to create replacement messages, or restore previous embed content manually if needed. Old legacy panels and legacy JSON are never edited by this command.

Run Clanlist as the first production module:

```powershell
cargo run -- run-clanlist --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-clanlist --once
cargo run -- run-clanlist --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-clanlist --interval-seconds 600
cargo run -- run-clanlist --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-clanlist --once --health-output data/clanlist_health.json
```

`run-clanlist --once` performs one production refresh and exits. Daemon mode performs one refresh immediately, then repeats after the configured interval. It handles Ctrl+C gracefully, never overlaps two refreshes, and keeps retrying on refresh failures while leaving config/state validation failures as startup blockers.

Hard boundaries for `run-clanlist`:

- It edits only the three fresh Superbot-owned panel messages in `data/clanlist_panel_state.json`.
- It never creates messages, deletes messages, edits old legacy panels, modifies roles, sends DMs, registers slash commands, opens Gateway, starts other module schedulers, writes legacy JSON/DB files, or calls Google Sheets by default.
- It verifies the current bot ID matches the state file and verifies each target message is authored by the current bot before any edit.
- It uses Discord HTTP REST only and the same bounded 429 retry/backoff as the diagnostic commands.
- `--google-readonly` is accepted but currently deferred: the command warns and continues using the legacy Steam cache. It does not connect to Google or write a Google-derived cache snapshot yet.

Windows helper:

```powershell
.\scripts\windows-run-clanlist.ps1 -EnvFile .env.local -Once
.\scripts\windows-run-clanlist.ps1 -EnvFile .env.local -IntervalSeconds 600
```

VPS/systemd deployment:

```bash
cargo build --release
sudo install -d /opt/XIII/xiii-superbot/data
sudo install -m 0755 target/release/xiii-superbot /opt/XIII/xiii-superbot/xiii-superbot
sudo install -m 0644 scripts/linux/xiii-superbot-clanlist.service.example /etc/systemd/system/xiii-superbot-clanlist.service
sudo systemctl daemon-reload
sudo systemctl enable --now xiii-superbot-clanlist.service
```

Before starting the VPS service, stop the old Clanlist service and verify it is stopped:

```bash
sudo systemctl stop xiii-clanlist.service
systemctl status xiii-clanlist.service --no-pager > xiii-clanlist-service-status.txt
```

You can enforce the local status-file guard:

```powershell
cargo run -- run-clanlist --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-clanlist --once --require-old-service-stopped --old-service-status-file xiii-clanlist-service-status.txt
```

Health output writes a Superbot-owned UTF-8 JSON file only, never legacy state:

```powershell
cargo run -- run-clanlist --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-clanlist --interval-seconds 600 --health-output data/clanlist_health.json
```

Global cutover tooling:

```powershell
cargo run -- prepare-cutover --env-file .env.local --modules clanlist,tempvoice,vacation,discipline,recruit,voice_activity,tickets
cargo run -- cutover-plan --env-file .env.local
cargo run -- bootstrap-fresh-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-bootstrap --dry-run --modules vacation,discipline,voice_activity,tickets
cargo run -- module-status --env-file .env.local
cargo run -- verify-cutover --env-file .env.local
cargo run -- db-source-check --env-file .env.local
cargo run -- final-readiness-check --env-file .env.local
cargo run -- temp-voice-cutover-check --env-file .env.local
cargo run -- voice-cutover-check --env-file .env.local --allow-discord-read
cargo run -- voice-finalize-cutover --env-file .env.local --dry-run
cargo run -- ticket-cutover-check --env-file .env.local
cargo run -- run-superbot --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-superbot --modules clanlist --dry-run
cargo run -- run-superbot --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-superbot --modules temp_voice --dry-run
cargo run -- run-superbot --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-superbot --modules clanlist,temp_voice,vacation,discipline,recruit,voice_activity,tickets --dry-run
```

`prepare-cutover`, `cutover-plan`, `db-source-check`, and `final-readiness-check` are read-only. They list old services to stop, legacy DB/state files to back up, fresh Superbot state files, module flags, source-of-truth DB row counts, and risk notes. `final-readiness-check` fails clearly if voice active sessions still need the explicit cutover finalize command.

`bootstrap-fresh-panels --dry-run` is the global panel/board planner. It never deletes old panels and never creates duplicates. The proven Clanlist create path remains `clanlist-bootstrap-new-panels`. Real global bootstrap is execution-enabled for Vacation, Discipline, Voice Activity, and Tickets fresh panels after explicit write flags.

`run-superbot --dry-run` performs local runtime preflight only. It does not open Gateway, does not register commands, and does not start writers. In real mode with `--modules clanlist`, it delegates to the proven `run-clanlist` path. In real mode it can run Temp Voice, Vacation, Discipline, Recruit, Voice Activity, and Tickets after each module is enabled in env and the old service guard passes.

Command registration is separate and never automatic:

```powershell
cargo run -- sync-commands --env-file .env.local --allow-discord-write --confirm-sync-commands --modules tempvoice,vacation,discipline,recruit,voice_activity,tickets --dry-run
cargo run -- sync-commands --env-file .env.local --allow-discord-write --confirm-sync-commands --modules temp_voice,vacation,discipline,recruit,voice_activity
```

Deployment helpers:

```powershell
.\scripts\windows-run-superbot.ps1 -EnvFile .env.local -Modules clanlist -DryRun
```

Linux examples:

- `scripts/linux/xiii-superbot.service.example`
- `scripts/linux/install-superbot-service.sh`

Example with a local unified env file:

```powershell
cargo run -- check-config --env-file .env.local
cargo run -- verify-legacy --env-file .env.local
cargo run -- ticket-cutover-check --env-file .env.local
cargo run -- clanlist-preview --env-file .env.local
cargo run -- clanlist-preview --env-file .env.local --format json
cargo run -- discord-readonly-clanlist-snapshot --env-file .env.local --allow-discord-read
cargo run -- clanlist-render-preview --env-file .env.local --allow-discord-read
cargo run -- clanlist-write-plan --env-file .env.local --allow-discord-read --allow-write-plan
cargo run -- clanlist-target-message-check --env-file .env.local --allow-discord-read
cargo run -- clanlist-bootstrap-new-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-create-new-panels --dry-run
cargo run -- clanlist-update-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-update-panels --dry-run
cargo run -- run-clanlist --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-clanlist --once
cargo run -- run-superbot --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-superbot --modules temp_voice --dry-run
cargo run -- bootstrap-fresh-panels --env-file .env.local --allow-discord-read --allow-discord-write --confirm-bootstrap --dry-run --modules vacation,discipline,voice_activity,tickets
cargo run -- sync-commands --env-file .env.local --allow-discord-write --confirm-sync-commands --modules temp_voice,vacation,discipline,recruit,voice_activity,tickets --dry-run
cargo run -- run-superbot --env-file .env.local --allow-discord-read --allow-discord-write --confirm-run-superbot --modules clanlist,temp_voice,vacation,discipline,recruit,voice_activity,tickets --dry-run
```

Safety notes:

- `verify-legacy` opens SQLite with read-only options.
- Verification sets `PRAGMA query_only=ON`.
- Secret-like env values are printed only as `<SET>`, `<EMPTY>`, or `<MISSING>`.
- Discord snowflake IDs and legacy state paths may be printed because they are migration inputs.
- Clanlist, Temp Voice, Vacation, Discipline, Recruit, and Voice Activity are write-enabled only behind explicit gates. Clanlist edits only the three fresh Superbot-owned panel messages plus Superbot-owned state/health files. Temp Voice writes only the legacy temp voice DB and may delete only voice channels tracked in `temp_voice_channels`. Vacation writes only vacation DB rows, vacation role changes, officer review messages, DMs, and the fresh active-vacations panel. Discipline writes only discipline DB rows/action logs/locks, the fresh Superbot-owned board, admin logs, DMs, timeouts, and configured clan-removal role changes. Recruit writes only recruit DB rows, decision panels, recruit role transitions, and DMs. Voice Activity writes only legacy voice active/completed session rows, voice `bot_state`, the fresh public stats panel, and optional inactive auto-report messages after `voice-cutover-check` is clean or `voice-finalize-cutover` has recorded `data/voice_activity_cutover_state.json`.
- `clanlist-preview` reads only `LEGACY_CLANLIST_DATA_DIR` JSON files and labels output as an offline preview. It does not claim that main/admin roster member lists are final because it does not query Discord members or Google Sheets.
- `discord-readonly-clanlist-snapshot` reads the same clanlist JSON files plus current Discord role/member data. It is diagnostics-only; optional `--output` writes a report only after legacy path safety checks.
- `clanlist-render-preview` previews legacy-style roster/Steam embed content; optional `--output` writes a report only after legacy path safety checks. It does not fetch existing Discord panel messages and does not verify or mutate live message contents.
- `clanlist-write-plan` produces a dry-run edit plan for the three existing Clanlist panel messages. It is not a writer and still keeps `write_state_allowed=false`.
- `clanlist-target-message-check` verifies the exact target messages with read-only Discord HTTP and confirms whether the current bot authored them. It never edits or sends Discord messages.
- `clanlist-bootstrap-new-panels` is write-capable only for creating exactly three fresh Clanlist panel messages after explicit confirmation. It writes only new Superbot state under `data/` by default; it never edits/deletes old panels or legacy Clanlist JSON.
- `clanlist-update-panels` is write-capable only for editing exactly the three fresh Clanlist panel messages from `data/clanlist_panel_state.json` after explicit confirmation. It never creates/deletes messages and never edits old legacy panels.
- `run-clanlist` is the Clanlist-only production refresher. It edits exactly the same three fresh messages from `data/clanlist_panel_state.json`, updates only Superbot-owned state/health files, and leaves every non-Clanlist module disabled.
- `run-superbot --modules temp_voice` is the Temp Voice production runtime. It requires the old service guard, starts one Gateway, stores `/setup-voice-hub` in `guild_settings`, creates/moves users into temporary voice channels when they join the hub, and deletes only DB-owned empty temp channels.
- `run-superbot --modules vacation` is gated production runtime for Vacation. It requires `VACATION_ENABLED=true`, fresh panel state, and stopped `xiii-vacation-bot.service`; it dispatches `vacation:*` interactions and starts non-overlapping expiry and active-panel jobs.
- `run-superbot --modules discipline` is gated production runtime for Discipline. It requires `DISCIPLINE_ENABLED=true`, fresh board state, and stopped `xiii-discipline-bot.service`; it dispatches `/discipline`, `xiii:*` discipline components/modals, and starts non-overlapping expiry and board-refresh jobs.
- `run-superbot --modules recruit` is gated production runtime for Recruit. It requires `RECRUIT_ENABLED=true` and stopped `xiii-recruit-bot.service`; it dispatches recruit slash/components/modals, tracks recruit voice time, and starts the due-panel scheduler.
- `voice-finalize-cutover` is the only command allowed to intentionally close legacy `active_voice_sessions` at cutover. Run it with `--dry-run` first; execution requires `--allow-legacy-db-write --confirm-close-active-voice-sessions`, preserves completed historical voice stats, closes open sessions once at a single timestamp, and writes `data/voice_activity_cutover_state.json`.
- `run-superbot --modules voice_activity` is gated production runtime for Voice Activity. It requires `VOICE_ACTIVITY_ENABLED=true`, fresh panel state, stopped `xiii-voice-activity-bot.service`, and either zero active legacy sessions or a valid finalized voice cutover state; it tracks voice joins/moves/leaves, refreshes the fresh public stats panel, and runs inactive report jobs.
- `run-superbot --modules tickets` is gated production runtime for Tickets. It requires `TICKETS_ENABLED=true`, fresh ticket panel state, stopped `xiii-ticketbot.service`, Message Content intent for legacy text commands, and valid redacted Google read-only config. It dispatches ticket slash/component/text routes, creates ticket channels, writes ticket lifecycle/dedupe rows, sends officer reviews/DMs/transcripts, and uses safe Rust HTML transcript attachments as the production substitute for legacy Python `chat_exporter`.
- `ticket-cutover-check` is read-only. It validates the legacy ticket DB tables/counts, counter values/status counts, reserved/open ticket state, fresh panel state presence, required channel/role IDs, message-content intent risk, and redacted Google config without opening Discord or Google.
