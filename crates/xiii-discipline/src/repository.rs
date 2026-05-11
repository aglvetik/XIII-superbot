use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};

use crate::state::{Punishment, PunishmentStatus, PunishmentType};

pub trait DisciplineRepository {
    fn active_punishments_for_user(&self, user_id: u64) -> Result<Vec<Punishment>, String>;
    fn writes_enabled(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyDisciplineRepositoryPlan {
    pub path: String,
    pub read_only: bool,
}

impl LegacyDisciplineRepositoryPlan {
    pub fn read_only(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            read_only: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisciplinePunishmentRecord {
    pub id: i64,
    pub guild_id: u64,
    pub user_id: u64,
    pub kind: PunishmentType,
    pub status: String,
    pub reason: String,
    pub issuer_id: Option<u64>,
    pub issued_at: i64,
    pub expires_at: Option<i64>,
    pub converted_into_id: Option<i64>,
    pub removed_by_id: Option<u64>,
    pub removed_reason: Option<String>,
    pub removed_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl DisciplinePunishmentRecord {
    pub fn to_punishment(&self) -> Punishment {
        Punishment {
            id: self.id,
            user_id: self.user_id,
            kind: self.kind,
            status: match self.status.as_str() {
                "active" => PunishmentStatus::Active,
                "expired" => PunishmentStatus::Expired,
                _ => PunishmentStatus::Removed,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionLogDraft {
    pub guild_id: u64,
    pub action_type: String,
    pub user_id: u64,
    pub issuer_id: Option<u64>,
    pub punishment_id: Option<i64>,
    pub payload_json: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuePunishmentDraft {
    pub guild_id: u64,
    pub user_id: u64,
    pub kind: PunishmentType,
    pub reason: String,
    pub issuer_id: Option<u64>,
    pub issued_at: i64,
    pub expires_at: Option<i64>,
    pub convert_active_ids: Vec<i64>,
    pub action_type: String,
    pub payload_json: String,
}

#[derive(Debug, Clone)]
pub struct LegacySqliteDisciplineRepository {
    path: PathBuf,
    pool: SqlitePool,
    writes_enabled: bool,
}

impl LegacySqliteDisciplineRepository {
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
                    "failed to open discipline DB {} {}: {err}",
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

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        sqlx::query("SELECT value FROM settings WHERE key=?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to read discipline setting {key}: {err}"))?
            .map(|row| row.try_get::<String, _>("value").map_err(read_err("value")))
            .transpose()
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query(
            "INSERT INTO settings (key, value)
             VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to set discipline setting {key}: {err}"))?;
        Ok(())
    }

    pub async fn active_punishments(
        &self,
        guild_id: u64,
        user_id: u64,
    ) -> Result<Vec<DisciplinePunishmentRecord>, String> {
        let rows = sqlx::query(
            punishment_select_sql(
                "WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=? AND status='active'
                 ORDER BY issued_at ASC",
            )
            .as_str(),
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list active punishments for {user_id}: {err}"))?;

        rows.iter().map(punishment_from_row).collect()
    }

    pub async fn list_active_for_board(
        &self,
        guild_id: u64,
        limit: i64,
    ) -> Result<Vec<DisciplinePunishmentRecord>, String> {
        let rows = sqlx::query(
            punishment_select_sql(
                "WHERE CAST(guild_id AS TEXT)=? AND status='active'
                 ORDER BY issued_at DESC LIMIT ?",
            )
            .as_str(),
        )
        .bind(guild_id.to_string())
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list active punishments for board: {err}"))?;

        rows.iter().map(punishment_from_row).collect()
    }

    pub async fn list_active_for_board_page(
        &self,
        guild_id: u64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DisciplinePunishmentRecord>, String> {
        let rows = sqlx::query(
            punishment_select_sql(
                "WHERE CAST(guild_id AS TEXT)=? AND status='active'
                 ORDER BY
                   CASE type WHEN 'strict' THEN 0 WHEN 'verbal' THEN 1 ELSE 2 END,
                   issued_at DESC
                 LIMIT ? OFFSET ?",
            )
            .as_str(),
        )
        .bind(guild_id.to_string())
        .bind(limit.max(1))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list active punishments board page: {err}"))?;

        rows.iter().map(punishment_from_row).collect()
    }

    pub async fn punishment_history(
        &self,
        guild_id: u64,
        user_id: u64,
        limit: i64,
    ) -> Result<Vec<DisciplinePunishmentRecord>, String> {
        let rows = sqlx::query(
            punishment_select_sql(
                "WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=?
                 ORDER BY issued_at DESC LIMIT ?",
            )
            .as_str(),
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list punishment history for {user_id}: {err}"))?;

        rows.iter().map(punishment_from_row).collect()
    }

    pub async fn get_punishment(
        &self,
        punishment_id: i64,
    ) -> Result<Option<DisciplinePunishmentRecord>, String> {
        sqlx::query(punishment_select_sql("WHERE id=?").as_str())
            .bind(punishment_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to get punishment {punishment_id}: {err}"))?
            .as_ref()
            .map(punishment_from_row)
            .transpose()
    }

    pub async fn issue_punishment_with_log(
        &self,
        draft: IssuePunishmentDraft,
    ) -> Result<i64, String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin issue punishment transaction: {err}"))?;
        let result = sqlx::query(
            "INSERT INTO punishments (
                guild_id, user_id, type, status, reason, issuer_id, issued_at,
                expires_at, converted_into_id, removed_by_id, removed_reason,
                removed_at, created_at, updated_at
             ) VALUES (?, ?, ?, 'active', ?, ?, ?, ?, NULL, NULL, NULL, NULL, ?, ?)",
        )
        .bind(draft.guild_id.to_string())
        .bind(draft.user_id.to_string())
        .bind(draft.kind.as_db_str())
        .bind(&draft.reason)
        .bind(draft.issuer_id.map(|id| id.to_string()))
        .bind(draft.issued_at)
        .bind(draft.expires_at)
        .bind(draft.issued_at)
        .bind(draft.issued_at)
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to create discipline punishment: {err}"))?;
        let punishment_id = result.last_insert_rowid();

        for old_id in &draft.convert_active_ids {
            sqlx::query(
                "UPDATE punishments
                 SET status='converted', converted_into_id=?, updated_at=?
                 WHERE id=? AND status='active'",
            )
            .bind(punishment_id)
            .bind(draft.issued_at)
            .bind(old_id)
            .execute(&mut *tx)
            .await
            .map_err(|err| format!("failed to convert punishment {old_id}: {err}"))?;
        }

        sqlx::query(
            "INSERT INTO action_logs(
                guild_id, action_type, user_id, issuer_id, punishment_id, payload_json, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(draft.guild_id.to_string())
        .bind(draft.action_type)
        .bind(draft.user_id.to_string())
        .bind(draft.issuer_id.map(|id| id.to_string()))
        .bind(punishment_id)
        .bind(draft.payload_json)
        .bind(draft.issued_at)
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to insert issue action log: {err}"))?;

        tx.commit()
            .await
            .map_err(|err| format!("failed to commit issue punishment transaction: {err}"))?;
        Ok(punishment_id)
    }

    pub async fn create_punishment(
        &self,
        guild_id: u64,
        user_id: u64,
        kind: PunishmentType,
        reason: &str,
        issuer_id: Option<u64>,
        issued_at: i64,
        expires_at: Option<i64>,
    ) -> Result<i64, String> {
        self.ensure_writable()?;
        let now = issued_at;
        let result = sqlx::query(
            "INSERT INTO punishments (
                guild_id, user_id, type, status, reason, issuer_id, issued_at,
                expires_at, converted_into_id, removed_by_id, removed_reason,
                removed_at, created_at, updated_at
             ) VALUES (?, ?, ?, 'active', ?, ?, ?, ?, NULL, NULL, NULL, NULL, ?, ?)",
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .bind(kind.as_db_str())
        .bind(reason)
        .bind(issuer_id.map(|id| id.to_string()))
        .bind(issued_at)
        .bind(expires_at)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create discipline punishment: {err}"))?;
        Ok(result.last_insert_rowid())
    }

    pub async fn convert_punishment(
        &self,
        old_id: i64,
        converted_into_id: i64,
        now: i64,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query(
            "UPDATE punishments
             SET status='converted', converted_into_id=?, updated_at=?
             WHERE id=? AND status='active'",
        )
        .bind(converted_into_id)
        .bind(now)
        .bind(old_id)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to convert punishment {old_id}: {err}"))?;
        Ok(())
    }

    pub async fn remove_punishment(
        &self,
        punishment_id: i64,
        removed_by_id: u64,
        reason: &str,
        now: i64,
    ) -> Result<bool, String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            "UPDATE punishments
             SET status='manually_removed', removed_by_id=?, removed_reason=?,
                 removed_at=?, updated_at=?
             WHERE id=? AND status='active'",
        )
        .bind(removed_by_id.to_string())
        .bind(reason)
        .bind(now)
        .bind(now)
        .bind(punishment_id)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to remove punishment {punishment_id}: {err}"))?
        .rows_affected();
        Ok(rows > 0)
    }

    pub async fn remove_punishment_with_log(
        &self,
        punishment_id: i64,
        removed_by_id: u64,
        reason: &str,
        now: i64,
    ) -> Result<bool, String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin remove punishment transaction: {err}"))?;
        let row = sqlx::query(punishment_select_sql("WHERE id=? AND status='active'").as_str())
            .bind(punishment_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|err| format!("failed to fetch active punishment {punishment_id}: {err}"))?;
        let Some(row) = row else {
            tx.rollback()
                .await
                .map_err(|err| format!("failed to rollback remove transaction: {err}"))?;
            return Ok(false);
        };
        let record = punishment_from_row(&row)?;
        let rows = sqlx::query(
            "UPDATE punishments
             SET status='manually_removed', removed_by_id=?, removed_reason=?,
                 removed_at=?, updated_at=?
             WHERE id=? AND status='active'",
        )
        .bind(removed_by_id.to_string())
        .bind(reason)
        .bind(now)
        .bind(now)
        .bind(punishment_id)
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to remove punishment {punishment_id}: {err}"))?
        .rows_affected();
        if rows > 0 {
            sqlx::query(
                "INSERT INTO action_logs(
                    guild_id, action_type, user_id, issuer_id, punishment_id, payload_json, created_at
                 ) VALUES (?, 'remove', ?, ?, ?, ?, ?)",
            )
            .bind(record.guild_id.to_string())
            .bind(record.user_id.to_string())
            .bind(removed_by_id.to_string())
            .bind(punishment_id)
            .bind(serde_json::json!({"reason": reason}).to_string())
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|err| format!("failed to insert remove action log: {err}"))?;
        }
        tx.commit()
            .await
            .map_err(|err| format!("failed to commit remove punishment transaction: {err}"))?;
        Ok(rows > 0)
    }

    pub async fn expire_due_punishments(
        &self,
        guild_id: u64,
        now: i64,
        limit: i64,
    ) -> Result<Vec<i64>, String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            "SELECT id FROM punishments
             WHERE CAST(guild_id AS TEXT)=? AND status='active'
               AND expires_at IS NOT NULL AND expires_at <= ?
             ORDER BY expires_at ASC LIMIT ?",
        )
        .bind(guild_id.to_string())
        .bind(now)
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to find due punishments: {err}"))?;

        let ids: Vec<i64> = rows
            .iter()
            .map(|row| row.try_get::<i64, _>("id").map_err(read_err("id")))
            .collect::<Result<_, _>>()?;
        for id in &ids {
            sqlx::query("UPDATE punishments SET status='expired', updated_at=? WHERE id=?")
                .bind(now)
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|err| format!("failed to expire punishment {id}: {err}"))?;
        }
        Ok(ids)
    }

    pub async fn expire_due_punishments_with_logs(
        &self,
        guild_id: u64,
        now: i64,
        limit: i64,
    ) -> Result<Vec<i64>, String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin expiration transaction: {err}"))?;
        let rows = sqlx::query(
            "SELECT id, CAST(user_id AS TEXT) AS user_id FROM punishments
             WHERE CAST(guild_id AS TEXT)=? AND status='active'
               AND expires_at IS NOT NULL AND expires_at <= ?
             ORDER BY expires_at ASC LIMIT ?",
        )
        .bind(guild_id.to_string())
        .bind(now)
        .bind(limit.max(1))
        .fetch_all(&mut *tx)
        .await
        .map_err(|err| format!("failed to find due punishments: {err}"))?;

        let mut ids = Vec::new();
        for row in rows {
            let id = row.try_get::<i64, _>("id").map_err(read_err("id"))?;
            let user_id = row
                .try_get::<String, _>("user_id")
                .map_err(read_err("user_id"))?;
            let affected =
                sqlx::query("UPDATE punishments SET status='expired', updated_at=? WHERE id=? AND status='active'")
                    .bind(now)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|err| format!("failed to expire punishment {id}: {err}"))?
                    .rows_affected();
            if affected > 0 {
                sqlx::query(
                    "INSERT INTO action_logs(
                        guild_id, action_type, user_id, issuer_id, punishment_id, payload_json, created_at
                     ) VALUES (?, 'expire', ?, NULL, ?, ?, ?)",
                )
                .bind(guild_id.to_string())
                .bind(user_id)
                .bind(id)
                .bind(serde_json::json!({"automatic": true}).to_string())
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(|err| format!("failed to insert expiration action log: {err}"))?;
                ids.push(id);
            }
        }
        tx.commit()
            .await
            .map_err(|err| format!("failed to commit expiration transaction: {err}"))?;
        Ok(ids)
    }

    pub async fn insert_action_log(&self, draft: ActionLogDraft) -> Result<i64, String> {
        self.ensure_writable()?;
        let result = sqlx::query(
            "INSERT INTO action_logs(
                guild_id, action_type, user_id, issuer_id, punishment_id, payload_json, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(draft.guild_id.to_string())
        .bind(draft.action_type)
        .bind(draft.user_id.to_string())
        .bind(draft.issuer_id.map(|id| id.to_string()))
        .bind(draft.punishment_id)
        .bind(draft.payload_json)
        .bind(draft.created_at)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to insert discipline action log: {err}"))?;
        Ok(result.last_insert_rowid())
    }

    pub async fn acquire_action_lock(
        &self,
        key: &str,
        expires_at: i64,
        now: i64,
    ) -> Result<bool, String> {
        self.ensure_writable()?;
        let mut tx =
            self.pool.begin().await.map_err(|err| {
                format!("failed to begin discipline action lock transaction: {err}")
            })?;
        sqlx::query("DELETE FROM action_locks WHERE expires_at <= ?")
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|err| format!("failed to clean expired action locks: {err}"))?;
        let rows =
            sqlx::query("INSERT OR IGNORE INTO action_locks (key, expires_at) VALUES (?, ?)")
                .bind(key)
                .bind(expires_at)
                .execute(&mut *tx)
                .await
                .map_err(|err| format!("failed to insert action lock {key}: {err}"))?
                .rows_affected();
        tx.commit()
            .await
            .map_err(|err| format!("failed to commit action lock transaction: {err}"))?;
        Ok(rows > 0)
    }

    pub async fn release_action_lock(&self, key: &str) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query("DELETE FROM action_locks WHERE key=?")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|err| format!("failed to release action lock {key}: {err}"))?;
        Ok(())
    }

    fn ensure_writable(&self) -> Result<(), String> {
        if self.writes_enabled {
            Ok(())
        } else {
            Err("discipline repository was opened read-only; writes are disabled".to_owned())
        }
    }

    #[cfg(test)]
    pub async fn create_schema_for_tests(&self) -> Result<(), String> {
        sqlx::query("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(&self.pool)
            .await
            .map_err(|err| format!("failed to create discipline settings fixture: {err}"))?;
        sqlx::query(
            "CREATE TABLE punishments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                guild_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                type TEXT NOT NULL,
                status TEXT NOT NULL,
                reason TEXT NOT NULL,
                issuer_id TEXT,
                issued_at INTEGER NOT NULL,
                expires_at INTEGER,
                converted_into_id INTEGER,
                removed_by_id TEXT,
                removed_reason TEXT,
                removed_at INTEGER,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create discipline punishments fixture: {err}"))?;
        sqlx::query(
            "CREATE TABLE action_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                guild_id TEXT NOT NULL,
                action_type TEXT NOT NULL,
                user_id TEXT NOT NULL,
                issuer_id TEXT,
                punishment_id INTEGER,
                payload_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create discipline action_logs fixture: {err}"))?;
        sqlx::query(
            "CREATE TABLE action_locks (key TEXT PRIMARY KEY, expires_at INTEGER NOT NULL)",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create discipline action_locks fixture: {err}"))?;
        Ok(())
    }

    #[cfg(test)]
    pub async fn action_log_count(&self) -> Result<i64, String> {
        sqlx::query("SELECT COUNT(*) AS count FROM action_logs")
            .fetch_one(&self.pool)
            .await
            .map_err(|err| format!("failed to count discipline action logs: {err}"))?
            .try_get::<i64, _>("count")
            .map_err(read_err("count"))
    }

    #[cfg(test)]
    pub async fn action_lock_count(&self) -> Result<i64, String> {
        sqlx::query("SELECT COUNT(*) AS count FROM action_locks")
            .fetch_one(&self.pool)
            .await
            .map_err(|err| format!("failed to count discipline action locks: {err}"))?
            .try_get::<i64, _>("count")
            .map_err(read_err("count"))
    }
}

fn punishment_select_sql(where_clause: &str) -> String {
    format!(
        "SELECT id, CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id,
                type, status, reason, CAST(issuer_id AS TEXT) AS issuer_id, issued_at,
                expires_at, converted_into_id, CAST(removed_by_id AS TEXT) AS removed_by_id,
                removed_reason, removed_at, created_at, updated_at
         FROM punishments {where_clause}"
    )
}

fn punishment_from_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<DisciplinePunishmentRecord, String> {
    let kind = match row
        .try_get::<String, _>("type")
        .map_err(read_err("type"))?
        .as_str()
    {
        "warning" => PunishmentType::Warning,
        "verbal" => PunishmentType::Verbal,
        "strict" => PunishmentType::Strict,
        other => return Err(format!("unknown punishment type {other:?}")),
    };

    Ok(DisciplinePunishmentRecord {
        id: row.try_get("id").map_err(read_err("id"))?,
        guild_id: parse_required_u64(row, "guild_id")?,
        user_id: parse_required_u64(row, "user_id")?,
        kind,
        status: row.try_get("status").map_err(read_err("status"))?,
        reason: row.try_get("reason").map_err(read_err("reason"))?,
        issuer_id: parse_optional_u64(row, "issuer_id")?,
        issued_at: row.try_get("issued_at").map_err(read_err("issued_at"))?,
        expires_at: row.try_get("expires_at").map_err(read_err("expires_at"))?,
        converted_into_id: row
            .try_get("converted_into_id")
            .map_err(read_err("converted_into_id"))?,
        removed_by_id: parse_optional_u64(row, "removed_by_id")?,
        removed_reason: row
            .try_get("removed_reason")
            .map_err(read_err("removed_reason"))?,
        removed_at: row.try_get("removed_at").map_err(read_err("removed_at"))?,
        created_at: row.try_get("created_at").map_err(read_err("created_at"))?,
        updated_at: row.try_get("updated_at").map_err(read_err("updated_at"))?,
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

trait PunishmentTypeDb {
    fn as_db_str(self) -> &'static str;
}

impl PunishmentTypeDb for PunishmentType {
    fn as_db_str(self) -> &'static str {
        match self {
            PunishmentType::Warning => "warning",
            PunishmentType::Verbal => "verbal",
            PunishmentType::Strict => "strict",
        }
    }
}
