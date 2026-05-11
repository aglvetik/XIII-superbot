# Architecture Refactor Notes

This pass split the binary entrypoint so `src/main.rs` is thin and the existing proven command implementation moved to `src/app.rs`.

Large files that intentionally remain:

- `src/app.rs`: still contains the already-tested Clanlist command implementation, Discord HTTP retry wrappers, output safety, and legacy service guard helpers. Moving those in one step would risk regressing the production-working Clanlist path, so placeholder boundary files now exist and extraction can continue incrementally.
- `crates/xiii-clanlist/src/lib.rs`: still contains the working Clanlist render/bootstrap/update implementation and tests. A `steam_source.rs` boundary was added for Google/legacy Steam sources, but the working renderer was not split aggressively because it is already production-used.
- `crates/xiii-config/src/lib.rs` and `crates/xiii-db/src/lib.rs`: compatibility re-export modules were added. The current implementations remain intact to preserve existing tests and command behavior.

Non-Clanlist modules now have production-shaped internal files for config, state, repository traits, render helpers, Discord IO request models, interaction IDs, runtime decisions, and tests. Their Discord and legacy DB writers remain disabled until module-specific cutover work is verified.
