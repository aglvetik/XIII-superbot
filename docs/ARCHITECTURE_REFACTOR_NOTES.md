# Architecture Refactor Notes

The binary entrypoint is already thin: `src/main.rs` delegates into `src/app.rs`, and shared helpers already live in `src/cli.rs`, `src/output.rs`, `src/report.rs`, `src/safety.rs`, `src/service_guard.rs`, and `src/discord_retry.rs`.

Large files that intentionally remain:

- `src/app.rs` (~15k lines): command dispatch, readiness/cutover checks, parity audit, render preview, and runtime orchestration all still live here. A broad split is now mechanically possible, but still risky while preserving the current no-regression baseline.
- `crates/xiii-clanlist/src/lib.rs` (~6.5k lines): still contains the proven Clanlist render/bootstrap/update path and its tests.
- `crates/xiii-config/src/lib.rs` (~1.5k lines): still centralizes env parsing, validation, and redaction.
- `crates/xiii-db/src/lib.rs` (~1.1k lines): still centralizes DB/source-of-truth verification helpers.

## Current Recommendation

Leave the current runtime code in place until real deployment cutover is complete. The code is green, parity is locked, and the remaining cost is maintainability rather than correctness.

## Safe Next Split For `src/app.rs`

After deployment cutover, the next low-risk refactor should be a mechanical extraction only:

1. `src/app/status.rs`
   - `module-status`
   - `verify-cutover`
   - common readiness matrix formatting
2. `src/app/parity.rs`
   - `legacy-parity-audit`
   - parity metadata structs/constants
3. `src/app/render_preview.rs`
   - `render-preview`
   - module visible-surface preview models
4. `src/app/cutover.rs`
   - `db-source-check`
   - `final-readiness-check`
   - `voice-cutover-check`
   - `voice-finalize-cutover`
   - `ticket-cutover-check`
5. `src/app/runtime.rs`
   - `run-superbot`
   - module runtime route glue
6. `src/app/output_helpers.rs`
   - shared human-readable / JSON output helpers that are still local to app

## Files Intentionally Left Alone

- `crates/xiii-clanlist/src/lib.rs`: stable, production-used, and already well-covered by tests.
- `crates/xiii-tickets/src/discord_io.rs`: large, but still coherent around one responsibility.
- repository-heavy module files (`xiii-discipline`, `xiii-recruit`, `xiii-vacation`, `xiii-tickets`): left intact because they encode proven legacy transaction behavior.

This is a documentation-only refactor note. No behavior should change until a later dedicated extraction pass.
