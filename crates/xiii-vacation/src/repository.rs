use chrono::{DateTime, Duration, SecondsFormat, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};

use crate::state::{VacationRecord, VacationStatus};

pub trait VacationRepository {
    fn active_vacations(&self, guild_id: u64) -> Result<Vec<VacationRecord>, String>;
    fn set_request_status(&self, request_id: i64, status: &str) -> Result<(), String>;
    fn writes_enabled(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyVacationRepositoryPlan {
    pub path: String,
    pub read_only: bool,
}

impl LegacyVacationRepositoryPlan {
    pub fn read_only(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            read_only: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacationRequestRecord {
    pub id: i64,
    pub guild_id: u64,
    pub user_id: u64,
    pub days: i64,
    pub reason: String,
    pub status: String,
    pub officer_message_id: Option<u64>,
    pub officer_channel_id: Option<u64>,
    pub created_at: String,
    pub decided_by: Option<u64>,
    pub decided_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacationDbRecord {
    pub id: i64,
    pub request_id: i64,
    pub guild_id: u64,
    pub user_id: u64,
    pub role_id: u64,
    pub days: i64,
    pub reason: String,
    pub status: String,
    pub started_at: String,
    pub expected_end_at: String,
    pub ended_at: Option<String>,
    pub ended_by: Option<u64>,
    pub end_type: Option<String>,
    pub dm_message_id: Option<u64>,
}

impl VacationDbRecord {
    pub fn to_vacation_record(&self) -> VacationRecord {
        VacationRecord {
            id: self.id,
            user_id: self.user_id,
            role_id: self.role_id,
            status: if self.status == "ACTIVE" {
                VacationStatus::Active
            } else {
                VacationStatus::Ended
            },
            started_unix: parse_rfc3339_unix(&self.started_at).unwrap_or_default(),
            expected_end_unix: parse_rfc3339_unix(&self.expected_end_at).unwrap_or_default(),
            reason: self.reason.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LegacySqliteVacationRepository {
    path: PathBuf,
    pool: SqlitePool,
    writes_enabled: bool,
}

impl LegacySqliteVacationRepository {
    pub async fn open_existing_read_only(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::open(path.as_ref(), false, false).await
    }

    pub async fn open_existing_writable(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::open(path.as_ref(), true, false).await
    }

    pub async fn open_writable_for_tests(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::open(path.as_ref(), true, true).await
    }

    async fn open(path: &Path, writable: bool, create_if_missing: bool) -> Result<Self, String> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .read_only(!writable)
            .create_if_missing(create_if_missing);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|err| {
                format!(
                    "failed to open vacation DB {} {}: {err}",
                    path.display(),
                    if writable { "writable" } else { "read-only" }
                )
            })?;

        if !writable {
            sqlx::query("PRAGMA query_only=ON")
                .execute(&pool)
                .await
                .map_err(|err| format!("failed to set PRAGMA query_only=ON: {err}"))?;
        }

        Ok(Self {
            path: path.to_path_buf(),
            pool,
            writes_enabled: writable,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn create_request(
        &self,
        guild_id: u64,
        user_id: u64,
        days: i64,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<i64, String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin vacation request transaction: {err}"))?;

        let active_count = self
            .count_active_for_user_in_tx(&mut tx, guild_id, user_id)
            .await?;
        if active_count > 0 {
            return Err(format!(
                "user {user_id} already has an active vacation; duplicate request blocked"
            ));
        }
        let pending_count = self
            .count_pending_for_user_in_tx(&mut tx, guild_id, user_id)
            .await?;
        if pending_count > 0 {
            return Err(format!(
                "user {user_id} already has a pending vacation request; duplicate request blocked"
            ));
        }

        let result = sqlx::query(
            "INSERT INTO vacation_requests(
                guild_id, user_id, days, reason, status, officer_message_id,
                officer_channel_id, created_at, decided_by, decided_at
             ) VALUES (?, ?, ?, ?, 'PENDING', NULL, NULL, ?, NULL, NULL)",
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .bind(days)
        .bind(reason)
        .bind(format_time(now))
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to insert vacation request: {err}"))?;

        tx.commit()
            .await
            .map_err(|err| format!("failed to commit vacation request transaction: {err}"))?;
        Ok(result.last_insert_rowid())
    }

    pub async fn update_officer_message(
        &self,
        request_id: i64,
        channel_id: u64,
        message_id: u64,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query(
            "UPDATE vacation_requests
             SET officer_channel_id=?, officer_message_id=?
             WHERE id=?",
        )
        .bind(channel_id.to_string())
        .bind(message_id.to_string())
        .bind(request_id)
        .execute(&self.pool)
        .await
        .map_err(|err| {
            format!("failed to update officer message for request {request_id}: {err}")
        })?;
        Ok(())
    }

    pub async fn get_request(
        &self,
        request_id: i64,
    ) -> Result<Option<VacationRequestRecord>, String> {
        let row = sqlx::query(
            "SELECT id, CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id,
                    days, reason, status,
                    CAST(officer_message_id AS TEXT) AS officer_message_id,
                    CAST(officer_channel_id AS TEXT) AS officer_channel_id,
                    created_at, CAST(decided_by AS TEXT) AS decided_by, decided_at
             FROM vacation_requests WHERE id=?",
        )
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| format!("failed to read vacation request {request_id}: {err}"))?;

        row.map(|row| request_from_row(&row)).transpose()
    }

    pub async fn approve_request_and_create_vacation(
        &self,
        request_id: i64,
        decided_by: u64,
        vacation_role_id: u64,
        now: DateTime<Utc>,
    ) -> Result<VacationDbRecord, String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin approve vacation transaction: {err}"))?;

        let request = self.request_by_id_in_tx(&mut tx, request_id).await?;
        if request.status != "PENDING" {
            return Err(format!(
                "vacation request {request_id} is not pending; current status {}",
                request.status
            ));
        }
        let active_count = self
            .count_active_for_user_in_tx(&mut tx, request.guild_id, request.user_id)
            .await?;
        if active_count > 0 {
            return Err(format!(
                "user {} already has an active vacation; approve blocked",
                request.user_id
            ));
        }

        sqlx::query(
            "UPDATE vacation_requests
             SET status='APPROVED', decided_by=?, decided_at=?
             WHERE id=? AND status='PENDING'",
        )
        .bind(decided_by.to_string())
        .bind(format_time(now))
        .bind(request_id)
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to approve vacation request {request_id}: {err}"))?;

        let expected_end_at = now + Duration::days(request.days);
        let result = sqlx::query(
            "INSERT INTO vacations(
                request_id, guild_id, user_id, role_id, days, reason, status,
                started_at, expected_end_at, ended_at, ended_by, end_type, dm_message_id
             ) VALUES (?, ?, ?, ?, ?, ?, 'ACTIVE', ?, ?, NULL, NULL, NULL, NULL)",
        )
        .bind(request_id)
        .bind(request.guild_id.to_string())
        .bind(request.user_id.to_string())
        .bind(vacation_role_id.to_string())
        .bind(request.days)
        .bind(&request.reason)
        .bind(format_time(now))
        .bind(format_time(expected_end_at))
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to create vacation for request {request_id}: {err}"))?;
        let vacation_id = result.last_insert_rowid();

        tx.commit()
            .await
            .map_err(|err| format!("failed to commit approve vacation transaction: {err}"))?;

        self.get_vacation(vacation_id)
            .await?
            .ok_or_else(|| format!("vacation {vacation_id} disappeared after insert"))
    }

    pub async fn reject_request(
        &self,
        request_id: i64,
        decided_by: u64,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            "UPDATE vacation_requests
             SET status='REJECTED', decided_by=?, decided_at=?
             WHERE id=? AND status='PENDING'",
        )
        .bind(decided_by.to_string())
        .bind(format_time(now))
        .bind(request_id)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to reject vacation request {request_id}: {err}"))?
        .rows_affected();

        if rows == 0 {
            return Err(format!(
                "vacation request {request_id} was not pending; reject was idempotently ignored"
            ));
        }
        Ok(())
    }

    pub async fn get_vacation(&self, vacation_id: i64) -> Result<Option<VacationDbRecord>, String> {
        let row = sqlx::query(vacation_select_sql("WHERE id=?").as_str())
            .bind(vacation_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to read vacation {vacation_id}: {err}"))?;

        row.map(|row| vacation_from_row(&row)).transpose()
    }

    pub async fn list_active_vacations(
        &self,
        guild_id: u64,
    ) -> Result<Vec<VacationDbRecord>, String> {
        let rows = sqlx::query(
            vacation_select_sql(
                "WHERE CAST(guild_id AS TEXT)=? AND status='ACTIVE' ORDER BY expected_end_at ASC",
            )
            .as_str(),
        )
        .bind(guild_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list active vacations for guild {guild_id}: {err}"))?;

        rows.iter().map(vacation_from_row).collect()
    }

    pub async fn list_expired_active(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<VacationDbRecord>, String> {
        let rows = sqlx::query(
            vacation_select_sql(
                "WHERE status='ACTIVE' AND expected_end_at <= ? ORDER BY expected_end_at ASC LIMIT ?",
            )
            .as_str(),
        )
        .bind(format_time(now))
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list expired active vacations: {err}"))?;

        rows.iter().map(vacation_from_row).collect()
    }

    pub async fn end_vacation(
        &self,
        vacation_id: i64,
        ended_by: u64,
        end_type: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<VacationDbRecord>, String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            "UPDATE vacations
             SET status='ENDED', ended_at=?, ended_by=?, end_type=?
             WHERE id=? AND status='ACTIVE'",
        )
        .bind(format_time(now))
        .bind(ended_by.to_string())
        .bind(end_type)
        .bind(vacation_id)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to end vacation {vacation_id}: {err}"))?
        .rows_affected();

        if rows == 0 {
            return Ok(None);
        }
        self.get_vacation(vacation_id).await
    }

    pub async fn set_dm_message_id(&self, vacation_id: i64, message_id: u64) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query("UPDATE vacations SET dm_message_id=? WHERE id=?")
            .bind(message_id.to_string())
            .bind(vacation_id)
            .execute(&self.pool)
            .await
            .map_err(|err| format!("failed to set vacation DM message {vacation_id}: {err}"))?;
        Ok(())
    }

    async fn request_by_id_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        request_id: i64,
    ) -> Result<VacationRequestRecord, String> {
        let row = sqlx::query(
            "SELECT id, CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id,
                    days, reason, status,
                    CAST(officer_message_id AS TEXT) AS officer_message_id,
                    CAST(officer_channel_id AS TEXT) AS officer_channel_id,
                    created_at, CAST(decided_by AS TEXT) AS decided_by, decided_at
             FROM vacation_requests WHERE id=?",
        )
        .bind(request_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|err| format!("failed to read vacation request {request_id}: {err}"))?;

        row.map(|row| request_from_row(&row))
            .transpose()?
            .ok_or_else(|| format!("vacation request {request_id} does not exist"))
    }

    async fn count_active_for_user_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        guild_id: u64,
        user_id: u64,
    ) -> Result<i64, String> {
        count_in_tx(
            tx,
            "SELECT COUNT(*) AS count FROM vacations WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=? AND status='ACTIVE'",
            guild_id,
            user_id,
            "active vacation",
        )
        .await
    }

    async fn count_pending_for_user_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        guild_id: u64,
        user_id: u64,
    ) -> Result<i64, String> {
        count_in_tx(
            tx,
            "SELECT COUNT(*) AS count FROM vacation_requests WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=? AND status='PENDING'",
            guild_id,
            user_id,
            "pending vacation request",
        )
        .await
    }

    fn ensure_writable(&self) -> Result<(), String> {
        if self.writes_enabled {
            Ok(())
        } else {
            Err("vacation repository was opened read-only; writes are disabled".to_owned())
        }
    }

    #[cfg(test)]
    pub async fn create_schema_for_tests(&self) -> Result<(), String> {
        sqlx::query(
            "CREATE TABLE bot_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create vacation bot_state fixture: {err}"))?;
        sqlx::query(
            "CREATE TABLE vacation_requests (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                guild_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                days INTEGER NOT NULL,
                reason TEXT NOT NULL,
                status TEXT NOT NULL,
                officer_message_id TEXT,
                officer_channel_id TEXT,
                created_at TEXT NOT NULL,
                decided_by TEXT,
                decided_at TEXT
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create vacation_requests fixture: {err}"))?;
        sqlx::query(
            "CREATE TABLE vacations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id INTEGER NOT NULL,
                guild_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                role_id TEXT NOT NULL,
                days INTEGER NOT NULL,
                reason TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                expected_end_at TEXT NOT NULL,
                ended_at TEXT,
                ended_by TEXT,
                end_type TEXT,
                dm_message_id TEXT
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create vacations fixture: {err}"))?;
        Ok(())
    }
}

async fn count_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sql: &str,
    guild_id: u64,
    user_id: u64,
    label: &str,
) -> Result<i64, String> {
    sqlx::query(sql)
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .fetch_one(&mut **tx)
        .await
        .map_err(|err| format!("failed to count {label}: {err}"))?
        .try_get::<i64, _>("count")
        .map_err(|err| format!("failed to read {label} count: {err}"))
}

fn vacation_select_sql(where_clause: &str) -> String {
    format!(
        "SELECT id, request_id, CAST(guild_id AS TEXT) AS guild_id,
                CAST(user_id AS TEXT) AS user_id, CAST(role_id AS TEXT) AS role_id,
                days, reason, status, started_at, expected_end_at, ended_at,
                CAST(ended_by AS TEXT) AS ended_by, end_type,
                CAST(dm_message_id AS TEXT) AS dm_message_id
         FROM vacations {where_clause}"
    )
}

fn request_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<VacationRequestRecord, String> {
    Ok(VacationRequestRecord {
        id: row.try_get("id").map_err(read_err("id"))?,
        guild_id: parse_required_u64(row, "guild_id")?,
        user_id: parse_required_u64(row, "user_id")?,
        days: row.try_get("days").map_err(read_err("days"))?,
        reason: row.try_get("reason").map_err(read_err("reason"))?,
        status: row.try_get("status").map_err(read_err("status"))?,
        officer_message_id: parse_optional_u64(row, "officer_message_id")?,
        officer_channel_id: parse_optional_u64(row, "officer_channel_id")?,
        created_at: row.try_get("created_at").map_err(read_err("created_at"))?,
        decided_by: parse_optional_u64(row, "decided_by")?,
        decided_at: row.try_get("decided_at").map_err(read_err("decided_at"))?,
    })
}

fn vacation_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<VacationDbRecord, String> {
    Ok(VacationDbRecord {
        id: row.try_get("id").map_err(read_err("id"))?,
        request_id: row.try_get("request_id").map_err(read_err("request_id"))?,
        guild_id: parse_required_u64(row, "guild_id")?,
        user_id: parse_required_u64(row, "user_id")?,
        role_id: parse_required_u64(row, "role_id")?,
        days: row.try_get("days").map_err(read_err("days"))?,
        reason: row.try_get("reason").map_err(read_err("reason"))?,
        status: row.try_get("status").map_err(read_err("status"))?,
        started_at: row.try_get("started_at").map_err(read_err("started_at"))?,
        expected_end_at: row
            .try_get("expected_end_at")
            .map_err(read_err("expected_end_at"))?,
        ended_at: row.try_get("ended_at").map_err(read_err("ended_at"))?,
        ended_by: parse_optional_u64(row, "ended_by")?,
        end_type: row.try_get("end_type").map_err(read_err("end_type"))?,
        dm_message_id: parse_optional_u64(row, "dm_message_id")?,
    })
}

fn parse_required_u64(row: &sqlx::sqlite::SqliteRow, name: &str) -> Result<u64, String> {
    row.try_get::<String, _>(name)
        .map_err(read_err(name))?
        .parse::<u64>()
        .map_err(|err| format!("failed to parse {name} as snowflake: {err}"))
}

fn parse_optional_u64(row: &sqlx::sqlite::SqliteRow, name: &str) -> Result<Option<u64>, String> {
    row.try_get::<Option<String>, _>(name)
        .map_err(read_err(name))?
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|err| format!("failed to parse {name} as snowflake: {err}"))
        })
        .transpose()
}

fn read_err(name: &str) -> impl FnOnce(sqlx::Error) -> String + '_ {
    move |err| format!("failed to read {name}: {err}")
}

fn format_time(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_rfc3339_unix(value: &str) -> Result<i64, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.timestamp())
        .map_err(|err| format!("failed to parse RFC3339 time {value:?}: {err}"))
}
