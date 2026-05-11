use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::Row;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use thiserror::Error;
use xiii_config::{ConfigPath, SuperbotConfig};
use xiii_core::{ModuleId, Report, StateDependency};

pub mod checksums;
pub mod legacy_verify;
pub mod sqlite;

pub type SqlitePool = sqlx::SqlitePool;

pub const SQLITE_READ_ONLY_URI_HINT: &str = "file:<path>?mode=ro";

#[derive(Debug, Error)]
pub enum DbPlanError {
    #[error("verification plans are read-only in the scaffold")]
    ReadOnlyScaffold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationQuery {
    pub module_id: ModuleId,
    pub description: String,
    pub sql: String,
}

impl VerificationQuery {
    pub fn new(module_id: ModuleId, description: &str, sql: &str) -> Self {
        Self {
            module_id,
            description: description.to_owned(),
            sql: sql.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyStateCatalog {
    pub states: Vec<StateDependency>,
    pub queries: Vec<VerificationQuery>,
}

impl LegacyStateCatalog {
    pub fn from_module_manifests(manifests: &[xiii_core::ModuleManifest]) -> Self {
        let states = manifests
            .iter()
            .flat_map(|manifest| manifest.state_dependencies.clone())
            .collect();
        Self {
            states,
            queries: verification_queries(),
        }
    }
}

pub fn verification_queries() -> Vec<VerificationQuery> {
    vec![
        VerificationQuery::new(ModuleId::Tickets, "ticket counters", "SELECT name, value FROM counters ORDER BY name;"),
        VerificationQuery::new(ModuleId::Tickets, "ticket type/status counts", "SELECT ticket_type, status, COUNT(*) FROM tickets GROUP BY ticket_type, status;"),
        VerificationQuery::new(ModuleId::VoiceActivity, "voice duration aggregate", "SELECT COUNT(*), SUM(duration_seconds) FROM voice_sessions;"),
        VerificationQuery::new(ModuleId::VoiceActivity, "active voice sessions", "SELECT COUNT(*) FROM active_voice_sessions;"),
        VerificationQuery::new(ModuleId::Recruit, "recruit statuses", "SELECT status, COUNT(*) FROM recruits GROUP BY status;"),
        VerificationQuery::new(ModuleId::Recruit, "recruit voice duration aggregate", "SELECT COUNT(*), SUM(duration_seconds) FROM voice_sessions;"),
        VerificationQuery::new(ModuleId::Vacation, "vacation request statuses", "SELECT status, COUNT(*) FROM vacation_requests GROUP BY status;"),
        VerificationQuery::new(ModuleId::Vacation, "active vacations", "SELECT id, user_id, role_id, status, expected_end_at FROM vacations WHERE status='ACTIVE';"),
        VerificationQuery::new(ModuleId::Discipline, "punishment statuses", "SELECT type, status, COUNT(*) FROM punishments GROUP BY type, status;"),
        VerificationQuery::new(ModuleId::TempVoice, "temp voice hub settings", "SELECT guild_id, hub_channel_id, updated_at FROM guild_settings;"),
    ]
}

pub trait ReadOnlyVerifier {
    fn module_id(&self) -> ModuleId;
    fn queries(&self) -> Vec<VerificationQuery>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyVerificationReport {
    pub report: Report,
}

impl LegacyVerificationReport {
    pub fn has_critical_failures(&self) -> bool {
        self.report.has_failures()
    }
}

pub async fn verify_legacy(config: &SuperbotConfig) -> LegacyVerificationReport {
    let mut report = Report::new();
    report.ok("safety", "Mode: READ ONLY");
    report.ok("safety", "Discord login: DISABLED");
    report.ok("safety", "DB writes: DISABLED");
    report.ok("safety", "Migrations: DISABLED");

    verify_tickets(config, &mut report).await;
    verify_voice(config, &mut report).await;
    verify_recruit(config, &mut report).await;
    verify_vacation(config, &mut report).await;
    verify_discipline(config, &mut report).await;
    verify_temp_voice(config, &mut report).await;
    verify_clanlist(config, &mut report).await;

    LegacyVerificationReport { report }
}

async fn verify_tickets(config: &SuperbotConfig, report: &mut Report) {
    let scope = "tickets";
    let Some(pool) = open_sqlite(
        scope,
        &config.legacy_paths.ticket_db,
        config.modules.tickets,
        report,
    )
    .await
    else {
        return;
    };
    if !verify_tables(
        scope,
        &pool,
        &[
            "counters",
            "tickets",
            "processed_forms",
            "processed_form_signatures",
            "bot_state",
        ],
        report,
    )
    .await
    {
        return;
    }

    check_count(
        scope,
        &pool,
        "tickets.counters rows",
        "SELECT COUNT(*) FROM counters",
        Some(3),
        false,
        report,
    )
    .await;
    check_count(
        scope,
        &pool,
        "tickets.tickets rows",
        "SELECT COUNT(*) FROM tickets",
        Some(23),
        false,
        report,
    )
    .await;
    check_count(
        scope,
        &pool,
        "tickets.processed_forms rows",
        "SELECT COUNT(*) FROM processed_forms",
        Some(1),
        false,
        report,
    )
    .await;
    check_count(
        scope,
        &pool,
        "tickets.processed_form_signatures rows",
        "SELECT COUNT(*) FROM processed_form_signatures",
        Some(1),
        false,
        report,
    )
    .await;
    let state = read_key_values(
        scope,
        &pool,
        "SELECT key, CAST(value AS TEXT) AS value FROM bot_state ORDER BY key",
        "tickets.bot_state",
        report,
    )
    .await;
    expect_value(
        scope,
        &state,
        "ticket_panel_message_id",
        "1499423034359152710",
        false,
        report,
    );
    read_key_values(
        scope,
        &pool,
        "SELECT name AS key, CAST(value AS TEXT) AS value FROM counters ORDER BY name",
        "tickets.counters",
        report,
    )
    .await;
    read_ticket_statuses(scope, &pool, report).await;
}

async fn verify_voice(config: &SuperbotConfig, report: &mut Report) {
    let scope = "voice_activity";
    let Some(pool) = open_sqlite(
        scope,
        &config.legacy_paths.voice_db,
        config.modules.voice_activity,
        report,
    )
    .await
    else {
        return;
    };
    if !verify_tables(
        scope,
        &pool,
        &[
            "users",
            "voice_sessions",
            "active_voice_sessions",
            "bot_state",
        ],
        report,
    )
    .await
    {
        return;
    }

    check_count(
        scope,
        &pool,
        "voice.users rows",
        "SELECT COUNT(*) FROM users",
        Some(32),
        false,
        report,
    )
    .await;
    check_count_and_sum(
        scope,
        &pool,
        "voice.voice_sessions aggregate",
        "SELECT COUNT(*) AS count, COALESCE(SUM(duration_seconds), 0) AS total FROM voice_sessions",
        Some(360),
        report,
    )
    .await;
    check_count(
        scope,
        &pool,
        "voice.active_voice_sessions rows",
        "SELECT COUNT(*) FROM active_voice_sessions",
        Some(10),
        true,
        report,
    )
    .await;
    let state = read_key_values(
        scope,
        &pool,
        "SELECT key, CAST(value AS TEXT) AS value FROM bot_state ORDER BY key",
        "voice.bot_state",
        report,
    )
    .await;
    expect_value(
        scope,
        &state,
        "public_stats_panel_message_id",
        "1501229030949519541",
        false,
        report,
    );
}

async fn verify_recruit(config: &SuperbotConfig, report: &mut Report) {
    let scope = "recruit";
    let Some(pool) = open_sqlite(
        scope,
        &config.legacy_paths.recruit_db,
        config.modules.recruit,
        report,
    )
    .await
    else {
        return;
    };
    if !verify_tables(
        scope,
        &pool,
        &["recruits", "voice_sessions", "decisions"],
        report,
    )
    .await
    {
        return;
    }

    read_group_counts(scope, &pool, "SELECT COALESCE(status, '<NULL>') AS left_key, '' AS right_key, COUNT(*) AS count FROM recruits GROUP BY status", "recruit.status", report).await;
    read_recruit_rows(scope, &pool, report).await;
    check_count_and_sum(
        scope,
        &pool,
        "recruit.voice_sessions aggregate",
        "SELECT COUNT(*) AS count, COALESCE(SUM(duration_seconds), 0) AS total FROM voice_sessions",
        Some(10),
        report,
    )
    .await;
    check_count(
        scope,
        &pool,
        "recruit.decisions rows",
        "SELECT COUNT(*) FROM decisions",
        Some(0),
        false,
        report,
    )
    .await;
}

async fn verify_vacation(config: &SuperbotConfig, report: &mut Report) {
    let scope = "vacation";
    let Some(pool) = open_sqlite(
        scope,
        &config.legacy_paths.vacation_db,
        config.modules.vacation,
        report,
    )
    .await
    else {
        return;
    };
    if !verify_tables(
        scope,
        &pool,
        &["bot_state", "vacation_requests", "vacations"],
        report,
    )
    .await
    {
        return;
    }

    let state = read_key_values(
        scope,
        &pool,
        "SELECT key, CAST(value AS TEXT) AS value FROM bot_state ORDER BY key",
        "vacation.bot_state",
        report,
    )
    .await;
    expect_value(
        scope,
        &state,
        "panel_message_id",
        "1500452180396609597",
        false,
        report,
    );
    expect_value(
        scope,
        &state,
        "active_vacations_message_id",
        "1501261338700284026",
        false,
        report,
    );
    check_count(
        scope,
        &pool,
        "vacation.vacation_requests rows",
        "SELECT COUNT(*) FROM vacation_requests",
        Some(9),
        false,
        report,
    )
    .await;
    check_count(
        scope,
        &pool,
        "vacation.vacations rows",
        "SELECT COUNT(*) FROM vacations",
        Some(6),
        false,
        report,
    )
    .await;
    read_group_counts(scope, &pool, "SELECT COALESCE(status, '<NULL>') AS left_key, '' AS right_key, COUNT(*) AS count FROM vacation_requests GROUP BY status", "vacation.requests_by_status", report).await;
    read_group_counts(scope, &pool, "SELECT COALESCE(status, '<NULL>') AS left_key, '' AS right_key, COUNT(*) AS count FROM vacations GROUP BY status", "vacation.vacations_by_status", report).await;
    read_active_vacations(scope, &pool, report).await;
    if config.vacation.vacation_role_id == 1_498_022_112_131_289_214 {
        report.ok(scope, "VACATION_ROLE_ID = 1498022112131289214");
    } else {
        report.fail(
            scope,
            format!(
                "VACATION_ROLE_ID = {}, expected actual vacation role 1498022112131289214",
                config.vacation.vacation_role_id
            ),
        );
    }
}

async fn verify_discipline(config: &SuperbotConfig, report: &mut Report) {
    let scope = "discipline";
    let Some(pool) = open_sqlite(
        scope,
        &config.legacy_paths.discipline_db,
        config.modules.discipline,
        report,
    )
    .await
    else {
        return;
    };
    if !verify_tables(
        scope,
        &pool,
        &[
            "settings",
            "punishments",
            "action_logs",
            "action_locks",
            "schema_migrations",
        ],
        report,
    )
    .await
    {
        return;
    }

    let settings = read_key_values(
        scope,
        &pool,
        "SELECT key, CAST(value AS TEXT) AS value FROM settings ORDER BY key",
        "discipline.settings",
        report,
    )
    .await;
    expect_value(
        scope,
        &settings,
        "discipline_board_message_id",
        "1501664727963664536",
        false,
        report,
    );
    expect_value(
        scope,
        &settings,
        "discipline_board_page",
        "0",
        false,
        report,
    );
    check_count(
        scope,
        &pool,
        "discipline.punishments rows",
        "SELECT COUNT(*) FROM punishments",
        Some(1),
        false,
        report,
    )
    .await;
    read_group_counts(scope, &pool, "SELECT COALESCE(type, '<NULL>') AS left_key, COALESCE(status, '<NULL>') AS right_key, COUNT(*) AS count FROM punishments GROUP BY type, status", "discipline.punishments_by_type_status", report).await;
    check_count(
        scope,
        &pool,
        "discipline.action_logs rows",
        "SELECT COUNT(*) FROM action_logs",
        Some(1),
        false,
        report,
    )
    .await;
    check_count(
        scope,
        &pool,
        "discipline.action_locks rows",
        "SELECT COUNT(*) FROM action_locks",
        Some(0),
        false,
        report,
    )
    .await;
    check_count(
        scope,
        &pool,
        "discipline.schema_migrations rows",
        "SELECT COUNT(*) FROM schema_migrations",
        Some(1),
        false,
        report,
    )
    .await;
}

async fn verify_temp_voice(config: &SuperbotConfig, report: &mut Report) {
    let scope = "temp_voice";
    let Some(pool) = open_sqlite(
        scope,
        &config.legacy_paths.temp_voice_db,
        config.modules.temp_voice,
        report,
    )
    .await
    else {
        return;
    };
    if !verify_tables(
        scope,
        &pool,
        &["guild_settings", "temp_voice_channels"],
        report,
    )
    .await
    {
        return;
    }

    check_count(
        scope,
        &pool,
        "temp_voice.guild_settings rows",
        "SELECT COUNT(*) FROM guild_settings",
        Some(1),
        false,
        report,
    )
    .await;
    check_count(
        scope,
        &pool,
        "temp_voice.temp_voice_channels rows",
        "SELECT COUNT(*) FROM temp_voice_channels",
        Some(2),
        true,
        report,
    )
    .await;
    read_temp_voice_settings(scope, &pool, report).await;
    read_temp_voice_channels(scope, &pool, report).await;
}

async fn verify_clanlist(config: &SuperbotConfig, report: &mut Report) {
    let scope = "clanlist";
    let dir = &config.legacy_paths.clanlist_data_dir;
    if !dir.resolved.is_dir() {
        if config.modules.clanlist {
            report.fail(
                scope,
                format!(
                    "legacy JSON directory missing while module is enabled: {}",
                    dir.display()
                ),
            );
        } else {
            report.warn(
                scope,
                format!(
                    "legacy JSON directory missing while module is disabled: {}",
                    dir.display()
                ),
            );
        }
        return;
    }
    report.ok(
        scope,
        format!("legacy JSON directory exists: {}", dir.display()),
    );

    verify_json_file(
        scope,
        &dir.resolved.join("main_roster_message_ids.json"),
        Some("1498766315299799185"),
        None,
        config.modules.clanlist,
        report,
    );
    verify_json_file(
        scope,
        &dir.resolved.join("admin_roster_message_ids.json"),
        Some("1498766321867821218"),
        None,
        config.modules.clanlist,
        report,
    );
    verify_json_file(
        scope,
        &dir.resolved.join("steam_roster_message_ids.json"),
        Some("1500086435506683954"),
        None,
        config.modules.clanlist,
        report,
    );
    verify_json_file(
        scope,
        &dir.resolved.join("steam_roster_cache.json"),
        None,
        Some(19),
        config.modules.clanlist,
        report,
    );
}

async fn open_sqlite(
    scope: &str,
    path: &ConfigPath,
    module_enabled: bool,
    report: &mut Report,
) -> Option<SqlitePool> {
    if !path.resolved.is_file() {
        if module_enabled {
            report.fail(
                scope,
                format!(
                    "legacy DB missing while module is enabled: {}",
                    path.display()
                ),
            );
        } else {
            report.warn(
                scope,
                format!(
                    "legacy DB missing while module is disabled: {}",
                    path.display()
                ),
            );
        }
        return None;
    }
    report.ok(scope, format!("legacy DB exists: {}", path.display()));

    match sha256_file(&path.resolved) {
        Ok(hash) => report.ok(
            scope,
            format!("sha256({}) = {hash}", path.resolved.display()),
        ),
        Err(err) => {
            report.fail(
                scope,
                format!("failed to hash {}: {err}", path.resolved.display()),
            );
            return None;
        }
    }

    let options = SqliteConnectOptions::new()
        .filename(&path.resolved)
        .read_only(true)
        .create_if_missing(false);
    let pool = match SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
    {
        Ok(pool) => pool,
        Err(err) => {
            report.fail(scope, format!("failed to open SQLite read-only: {err}"));
            return None;
        }
    };

    match sqlx::query("PRAGMA query_only=ON;").execute(&pool).await {
        Ok(_) => {
            report.ok(
                scope,
                "PRAGMA query_only=ON set for verification connection",
            );
            Some(pool)
        }
        Err(err) => {
            report.fail(scope, format!("failed to set PRAGMA query_only=ON: {err}"));
            None
        }
    }
}

async fn verify_tables(
    scope: &str,
    pool: &SqlitePool,
    expected_tables: &[&str],
    report: &mut Report,
) -> bool {
    let mut all_ok = true;
    for table in expected_tables {
        let result: Result<Option<String>, sqlx::Error> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name = ?")
                .bind(table)
                .fetch_optional(pool)
                .await;
        match result {
            Ok(Some(_)) => report.ok(scope, format!("table exists: {table}")),
            Ok(None) => {
                report.fail(scope, format!("missing expected table: {table}"));
                all_ok = false;
            }
            Err(err) => {
                report.fail(scope, format!("failed checking table {table}: {err}"));
                all_ok = false;
            }
        }
    }
    all_ok
}

async fn check_count(
    scope: &str,
    pool: &SqlitePool,
    label: &str,
    sql: &str,
    expected: Option<i64>,
    live_may_change: bool,
    report: &mut Report,
) -> Option<i64> {
    match sqlx::query_scalar::<_, i64>(sql).fetch_one(pool).await {
        Ok(count) => {
            match expected {
                Some(expected) if count != expected && live_may_change => report.warn(
                    scope,
                    format!("{label} = {count}, audit had {expected}; live state may have changed"),
                ),
                Some(expected) if count != expected => report.warn(
                    scope,
                    format!("{label} = {count}, audit had {expected}; update audit baseline if this is expected"),
                ),
                _ => report.ok(scope, format!("{label} = {count}")),
            }
            Some(count)
        }
        Err(err) => {
            report.fail(scope, format!("{label} query failed: {err}"));
            None
        }
    }
}

async fn check_count_and_sum(
    scope: &str,
    pool: &SqlitePool,
    label: &str,
    sql: &str,
    expected_count: Option<i64>,
    report: &mut Report,
) -> Option<(i64, i64)> {
    match sqlx::query(sql).fetch_one(pool).await {
        Ok(row) => {
            let count = row.try_get::<i64, _>("count").unwrap_or_default();
            let total = row.try_get::<i64, _>("total").unwrap_or_default();
            match expected_count {
                Some(expected) if count != expected => report.warn(
                    scope,
                    format!("{label}: count = {count}, SUM(duration_seconds) = {total}, audit had count {expected}"),
                ),
                _ => report.ok(
                    scope,
                    format!("{label}: count = {count}, SUM(duration_seconds) = {total}"),
                ),
            }
            Some((count, total))
        }
        Err(err) => {
            report.fail(scope, format!("{label} query failed: {err}"));
            None
        }
    }
}

async fn read_key_values(
    scope: &str,
    pool: &SqlitePool,
    sql: &str,
    label: &str,
    report: &mut Report,
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    match sqlx::query(sql).fetch_all(pool).await {
        Ok(rows) => {
            for row in rows {
                let key = row
                    .try_get::<String, _>("key")
                    .unwrap_or_else(|_| "<unreadable>".to_owned());
                let value = row
                    .try_get::<String, _>("value")
                    .unwrap_or_else(|_| "<unreadable>".to_owned());
                report.ok(scope, format!("{label}.{key} = {value}"));
                values.insert(key, value);
            }
        }
        Err(err) => report.fail(scope, format!("{label} query failed: {err}")),
    }
    values
}

fn expect_value(
    scope: &str,
    values: &BTreeMap<String, String>,
    key: &str,
    expected: &str,
    live_may_change: bool,
    report: &mut Report,
) {
    match values.get(key) {
        Some(actual) if actual == expected => report.ok(scope, format!("{key} = {actual}")),
        Some(actual) if live_may_change => report.warn(
            scope,
            format!("{key} = {actual}, audit had {expected}; live state may have changed"),
        ),
        Some(actual) => report.warn(
            scope,
            format!("{key} = {actual}, audit had {expected}; preserve current value and update migration notes"),
        ),
        None => report.fail(scope, format!("{key} missing from state table")),
    }
}

async fn read_ticket_statuses(scope: &str, pool: &SqlitePool, report: &mut Report) {
    read_group_counts(
        scope,
        pool,
        "SELECT COALESCE(ticket_type, '<NULL>') AS left_key, COALESCE(status, '<NULL>') AS right_key, COUNT(*) AS count FROM tickets GROUP BY ticket_type, status",
        "tickets.by_type_status",
        report,
    )
    .await;
}

async fn read_group_counts(
    scope: &str,
    pool: &SqlitePool,
    sql: &str,
    label: &str,
    report: &mut Report,
) {
    match sqlx::query(sql).fetch_all(pool).await {
        Ok(rows) => {
            for row in rows {
                let left = row
                    .try_get::<String, _>("left_key")
                    .unwrap_or_else(|_| "<unreadable>".to_owned());
                let right = row.try_get::<String, _>("right_key").unwrap_or_default();
                let count = row.try_get::<i64, _>("count").unwrap_or_default();
                if right.is_empty() {
                    report.ok(scope, format!("{label}.{left} = {count}"));
                } else {
                    report.ok(scope, format!("{label}.{left}.{right} = {count}"));
                }
            }
        }
        Err(err) => report.fail(scope, format!("{label} query failed: {err}")),
    }
}

async fn read_recruit_rows(scope: &str, pool: &SqlitePool, report: &mut Report) {
    let sql = "SELECT CAST(id AS TEXT) AS id, CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id, COALESCE(status, '<NULL>') AS status, COALESCE(CAST(due_at AS TEXT), '') AS due_at, COALESCE(CAST(last_decision_message_id AS TEXT), '') AS last_decision_message_id, COALESCE(CAST(last_decision_channel_id AS TEXT), '') AS last_decision_channel_id FROM recruits ORDER BY id";
    match sqlx::query(sql).fetch_all(pool).await {
        Ok(rows) => {
            let mut active_match = false;
            for row in rows {
                let id = row.try_get::<String, _>("id").unwrap_or_default();
                let guild_id = row.try_get::<String, _>("guild_id").unwrap_or_default();
                let user_id = row.try_get::<String, _>("user_id").unwrap_or_default();
                let status = row.try_get::<String, _>("status").unwrap_or_default();
                let due_at = row.try_get::<String, _>("due_at").unwrap_or_default();
                let decision_message = row
                    .try_get::<String, _>("last_decision_message_id")
                    .unwrap_or_default();
                let decision_channel = row
                    .try_get::<String, _>("last_decision_channel_id")
                    .unwrap_or_default();
                report.ok(scope, format!("recruits row id={id} guild_id={guild_id} user_id={user_id} status={status} due_at={due_at} last_decision_message_id={decision_message} last_decision_channel_id={decision_channel}"));
                if user_id == "973660882242519150"
                    && status.eq_ignore_ascii_case("active")
                    && due_at == "2026-05-18T20:49:08+00:00"
                    && decision_message == "1501259037357117641"
                    && decision_channel == "1500136438791147651"
                {
                    active_match = true;
                }
            }
            if active_match {
                report.ok(scope, "known active recruit row matches audit");
            } else {
                report.warn(scope, "known active recruit row from audit was not found exactly; live recruit state may have changed");
            }
        }
        Err(err) => report.fail(scope, format!("recruits row query failed: {err}")),
    }
}

async fn read_active_vacations(scope: &str, pool: &SqlitePool, report: &mut Report) {
    let sql = "SELECT CAST(id AS TEXT) AS id, CAST(user_id AS TEXT) AS user_id, CAST(role_id AS TEXT) AS role_id, COALESCE(status, '<NULL>') AS status, COALESCE(CAST(expected_end_at AS TEXT), '') AS expected_end_at, COALESCE(CAST(dm_message_id AS TEXT), '') AS dm_message_id FROM vacations WHERE status='ACTIVE' ORDER BY id";
    match sqlx::query(sql).fetch_all(pool).await {
        Ok(rows) => {
            let count = rows.len();
            if count == 2 {
                report.ok(scope, "vacation.active rows = 2");
            } else {
                report.warn(
                    scope,
                    format!(
                        "vacation.active rows = {count}, audit had 2; live state may have changed"
                    ),
                );
            }
            for row in rows {
                let id = row.try_get::<String, _>("id").unwrap_or_default();
                let user_id = row.try_get::<String, _>("user_id").unwrap_or_default();
                let role_id = row.try_get::<String, _>("role_id").unwrap_or_default();
                let status = row.try_get::<String, _>("status").unwrap_or_default();
                let expected_end_at = row
                    .try_get::<String, _>("expected_end_at")
                    .unwrap_or_default();
                let dm_message_id = row
                    .try_get::<String, _>("dm_message_id")
                    .unwrap_or_default();
                report.ok(scope, format!("active vacation id={id} user_id={user_id} role_id={role_id} status={status} expected_end_at={expected_end_at} dm_message_id={dm_message_id}"));
            }
        }
        Err(err) => report.fail(scope, format!("active vacation query failed: {err}")),
    }
}

async fn read_temp_voice_settings(scope: &str, pool: &SqlitePool, report: &mut Report) {
    let sql = "SELECT CAST(guild_id AS TEXT) AS guild_id, CAST(hub_channel_id AS TEXT) AS hub_channel_id, COALESCE(CAST(updated_at AS TEXT), '') AS updated_at FROM guild_settings ORDER BY guild_id";
    match sqlx::query(sql).fetch_all(pool).await {
        Ok(rows) => {
            let mut found_hub = false;
            for row in rows {
                let guild_id = row.try_get::<String, _>("guild_id").unwrap_or_default();
                let hub_channel_id = row
                    .try_get::<String, _>("hub_channel_id")
                    .unwrap_or_default();
                let updated_at = row.try_get::<String, _>("updated_at").unwrap_or_default();
                if hub_channel_id == "1499122210542194899" {
                    found_hub = true;
                }
                report.ok(scope, format!("guild_settings guild_id={guild_id} hub_channel_id={hub_channel_id} updated_at={updated_at}"));
            }
            if found_hub {
                report.ok(scope, "hub_channel_id = 1499122210542194899");
            } else {
                report.warn(scope, "hub_channel_id 1499122210542194899 from audit was not found; live hub may have changed");
            }
        }
        Err(err) => report.fail(scope, format!("temp voice settings query failed: {err}")),
    }
}

async fn read_temp_voice_channels(scope: &str, pool: &SqlitePool, report: &mut Report) {
    let sql = "SELECT CAST(channel_id AS TEXT) AS channel_id, CAST(guild_id AS TEXT) AS guild_id, CAST(owner_user_id AS TEXT) AS owner_user_id, COALESCE(CAST(created_at AS TEXT), '') AS created_at, COALESCE(CAST(last_empty_at AS TEXT), '') AS last_empty_at FROM temp_voice_channels ORDER BY channel_id";
    match sqlx::query(sql).fetch_all(pool).await {
        Ok(rows) => {
            for row in rows {
                let channel_id = row.try_get::<String, _>("channel_id").unwrap_or_default();
                let guild_id = row.try_get::<String, _>("guild_id").unwrap_or_default();
                let owner_user_id = row
                    .try_get::<String, _>("owner_user_id")
                    .unwrap_or_default();
                let created_at = row.try_get::<String, _>("created_at").unwrap_or_default();
                let last_empty_at = row
                    .try_get::<String, _>("last_empty_at")
                    .unwrap_or_default();
                report.ok(scope, format!("temp_voice_channels channel_id={channel_id} guild_id={guild_id} owner_user_id={owner_user_id} created_at={created_at} last_empty_at={last_empty_at}"));
            }
        }
        Err(err) => report.fail(scope, format!("temp voice channel query failed: {err}")),
    }
}

fn verify_json_file(
    scope: &str,
    path: &Path,
    expected_message_id: Option<&str>,
    expected_cache_records: Option<usize>,
    module_enabled: bool,
    report: &mut Report,
) {
    if !path.is_file() {
        if module_enabled {
            report.fail(
                scope,
                format!(
                    "JSON file missing while module is enabled: {}",
                    path.display()
                ),
            );
        } else {
            report.warn(
                scope,
                format!(
                    "JSON file missing while module is disabled: {}",
                    path.display()
                ),
            );
        }
        return;
    }
    report.ok(scope, format!("JSON file exists: {}", path.display()));
    match sha256_file(path) {
        Ok(hash) => report.ok(scope, format!("sha256({}) = {hash}", path.display())),
        Err(err) => {
            report.fail(scope, format!("failed to hash {}: {err}", path.display()));
            return;
        }
    }

    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            report.fail(
                scope,
                format!("failed to read JSON {}: {err}", path.display()),
            );
            return;
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(err) => {
            report.fail(
                scope,
                format!("failed to parse JSON {}: {err}", path.display()),
            );
            return;
        }
    };

    let ids = collect_snowflakes(&value);
    if ids.is_empty() {
        report.warn(
            scope,
            format!(
                "{} contains no obvious Discord snowflake values",
                path.display()
            ),
        );
    } else {
        report.ok(
            scope,
            format!(
                "{} message/snowflake IDs = {}",
                path.display(),
                ids.join(",")
            ),
        );
    }
    if let Some(expected) = expected_message_id {
        if ids.iter().any(|id| id == expected) {
            report.ok(
                scope,
                format!("expected clanlist message ID {expected} found"),
            );
        } else {
            report.warn(scope, format!("expected clanlist message ID {expected} not found; preserve current JSON value if live state changed"));
        }
    }
    if let Some(expected) = expected_cache_records {
        let count = cache_record_count(&value);
        if count == expected {
            report.ok(scope, format!("steam cache records = {count}"));
        } else {
            report.warn(
                scope,
                format!("steam cache records = {count}, audit had {expected}"),
            );
        }
    }
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_snowflakes(value: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    collect_snowflakes_inner(value, &mut ids);
    ids.sort();
    ids.dedup();
    ids
}

fn collect_snowflakes_inner(value: &Value, ids: &mut Vec<String>) {
    match value {
        Value::String(text) if looks_like_snowflake(text) => ids.push(text.to_owned()),
        Value::Number(number) => {
            let text = number.to_string();
            if looks_like_snowflake(&text) {
                ids.push(text);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_snowflakes_inner(item, ids);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_snowflakes_inner(item, ids);
            }
        }
        _ => {}
    }
}

fn looks_like_snowflake(value: &str) -> bool {
    value.len() >= 17 && value.len() <= 20 && value.chars().all(|ch| ch.is_ascii_digit())
}

fn cache_record_count(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.len(),
        Value::Object(map) => {
            if let Some(records) = map.get("records") {
                cache_record_count(records)
            } else if let Some(cache) = map.get("cache") {
                cache_record_count(cache)
            } else {
                map.len()
            }
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use xiii_core::Report;

    #[test]
    fn verification_report_uses_failure_aggregation() {
        let mut report = Report::new();
        report.ok("tickets", "db exists");
        assert!(!report.has_failures());
        report.fail("discipline", "db missing");
        assert!(report.has_failures());
        assert_eq!(report.counts().fail, 1);
    }
}
