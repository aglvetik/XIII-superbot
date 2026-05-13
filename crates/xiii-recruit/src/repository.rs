use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};

use crate::state::{Recruit, RecruitStatus};

pub trait RecruitRepository {
    fn active_recruits(&self, guild_id: u64) -> Result<Vec<Recruit>, String>;
    fn writes_enabled(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyRecruitRepositoryPlan {
    pub path: String,
    pub read_only: bool,
}

impl LegacyRecruitRepositoryPlan {
    pub fn read_only(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            read_only: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitRecord {
    pub id: i64,
    pub guild_id: u64,
    pub user_id: u64,
    pub status: String,
    pub started_at: String,
    pub due_at: String,
    pub completed_at: Option<String>,
    pub extensions_count: i64,
    pub last_decision_message_id: Option<u64>,
    pub last_decision_channel_id: Option<u64>,
    pub created_at: String,
    pub updated_at: String,
}

impl RecruitRecord {
    pub fn to_recruit(&self) -> Recruit {
        Recruit {
            id: self.id,
            guild_id: self.guild_id,
            user_id: self.user_id,
            status: match self.status.as_str() {
                "active" => RecruitStatus::Active,
                "accepted" => RecruitStatus::Accepted,
                "rejected" => RecruitStatus::Rejected,
                _ => RecruitStatus::Extended,
            },
            due_unix: parse_rfc3339_unix(&self.due_at).unwrap_or_default(),
            last_decision_message_id: self.last_decision_message_id,
            last_decision_channel_id: self.last_decision_channel_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitVoiceSessionRecord {
    pub id: i64,
    pub guild_id: u64,
    pub user_id: u64,
    pub channel_id: u64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<i64>,
    pub interrupted: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedRecruitVoiceSession {
    pub id: i64,
    pub guild_id: u64,
    pub user_id: u64,
    pub channel_id: u64,
    pub duration_seconds: i64,
    pub interrupted: bool,
}

#[derive(Debug, Clone)]
pub struct LegacySqliteRecruitRepository {
    path: PathBuf,
    pool: SqlitePool,
    writes_enabled: bool,
}

impl LegacySqliteRecruitRepository {
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
                    "failed to open recruit DB {} {}: {err}",
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

    pub async fn create_active_recruit(
        &self,
        guild_id: u64,
        user_id: u64,
        started_at: DateTime<Utc>,
        due_at: DateTime<Utc>,
    ) -> Result<RecruitRecord, String> {
        self.ensure_writable()?;
        let now = format_time(Utc::now());
        let result = sqlx::query(
            "INSERT INTO recruits(
                guild_id, user_id, status, started_at, due_at, completed_at,
                extensions_count, last_decision_message_id, last_decision_channel_id,
                created_at, updated_at
             ) VALUES (?, ?, 'active', ?, ?, NULL, 0, NULL, NULL, ?, ?)",
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .bind(format_time(started_at))
        .bind(format_time(due_at))
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create active recruit: {err}"))?;
        self.get_recruit_by_id(result.last_insert_rowid())
            .await?
            .ok_or_else(|| "created recruit row was not found".to_owned())
    }

    pub async fn get_recruit_by_id(
        &self,
        recruit_id: i64,
    ) -> Result<Option<RecruitRecord>, String> {
        let row = sqlx::query(recruit_select_sql("WHERE id=?").as_str())
            .bind(recruit_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to read recruit {recruit_id}: {err}"))?;
        row.map(|row| recruit_from_row(&row)).transpose()
    }

    pub async fn list_active_recruits(&self, guild_id: u64) -> Result<Vec<RecruitRecord>, String> {
        let rows = sqlx::query(
            recruit_select_sql(
                "WHERE CAST(guild_id AS TEXT)=? AND status='active'
                 ORDER BY due_at ASC, id ASC",
            )
            .as_str(),
        )
        .bind(guild_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list active recruits: {err}"))?;
        rows.iter().map(recruit_from_row).collect()
    }

    pub async fn list_due_active_without_panel(
        &self,
        guild_id: u64,
        now: DateTime<Utc>,
    ) -> Result<Vec<RecruitRecord>, String> {
        let rows = sqlx::query(
            recruit_select_sql(
                "WHERE CAST(guild_id AS TEXT)=? AND status='active' AND due_at <= ?
                   AND (last_decision_message_id IS NULL OR last_decision_channel_id IS NULL)
                 ORDER BY due_at ASC, id ASC",
            )
            .as_str(),
        )
        .bind(guild_id.to_string())
        .bind(format_time(now))
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list due active recruits: {err}"))?;
        rows.iter().map(recruit_from_row).collect()
    }

    pub async fn get_active_recruit_by_id(
        &self,
        recruit_id: i64,
    ) -> Result<Option<RecruitRecord>, String> {
        let row = sqlx::query(recruit_select_sql("WHERE id=? AND status='active'").as_str())
            .bind(recruit_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to read active recruit {recruit_id}: {err}"))?;
        row.map(|row| recruit_from_row(&row)).transpose()
    }

    pub async fn set_decision_message(
        &self,
        recruit_id: i64,
        channel_id: u64,
        message_id: u64,
        updated_at: DateTime<Utc>,
    ) -> Result<bool, String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            "UPDATE recruits
             SET last_decision_channel_id=?, last_decision_message_id=?, updated_at=?
             WHERE id=? AND status='active'",
        )
        .bind(channel_id.to_string())
        .bind(message_id.to_string())
        .bind(format_time(updated_at))
        .bind(recruit_id)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to set recruit decision message: {err}"))?
        .rows_affected();
        Ok(rows > 0)
    }

    pub async fn complete_with_decision(
        &self,
        recruit_id: i64,
        status: &str,
        decision: &str,
        admin_id: u64,
        reason: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<bool, String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin recruit completion transaction: {err}"))?;
        let Some(recruit) = self
            .get_active_recruit_by_id_in_tx(&mut tx, recruit_id)
            .await?
        else {
            tx.rollback()
                .await
                .map_err(|err| format!("failed to rollback recruit transaction: {err}"))?;
            return Ok(false);
        };
        let now_iso = format_time(now);
        let rows = sqlx::query(
            "UPDATE recruits
             SET status=?, completed_at=?, updated_at=?
             WHERE id=? AND status='active'",
        )
        .bind(status)
        .bind(&now_iso)
        .bind(&now_iso)
        .bind(recruit_id)
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to update recruit {recruit_id}: {err}"))?
        .rows_affected();
        if rows != 1 {
            tx.rollback()
                .await
                .map_err(|err| format!("failed to rollback recruit transaction: {err}"))?;
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO decisions(
                recruit_id, guild_id, user_id, decision, admin_id, reason, extension_days, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, NULL, ?)",
        )
        .bind(recruit.id)
        .bind(recruit.guild_id.to_string())
        .bind(recruit.user_id.to_string())
        .bind(decision)
        .bind(admin_id.to_string())
        .bind(reason)
        .bind(&now_iso)
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to insert recruit decision: {err}"))?;
        tx.commit()
            .await
            .map_err(|err| format!("failed to commit recruit completion transaction: {err}"))?;
        Ok(true)
    }

    pub async fn extend_with_decision(
        &self,
        recruit_id: i64,
        admin_id: u64,
        due_at: DateTime<Utc>,
        reason: &str,
        extension_days: i64,
        now: DateTime<Utc>,
    ) -> Result<bool, String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin recruit extension transaction: {err}"))?;
        let Some(recruit) = self
            .get_active_recruit_by_id_in_tx(&mut tx, recruit_id)
            .await?
        else {
            tx.rollback()
                .await
                .map_err(|err| format!("failed to rollback recruit transaction: {err}"))?;
            return Ok(false);
        };
        let now_iso = format_time(now);
        let rows = sqlx::query(
            "UPDATE recruits
             SET due_at=?, extensions_count=extensions_count + 1,
                 last_decision_message_id=NULL, last_decision_channel_id=NULL, updated_at=?
             WHERE id=? AND status='active'",
        )
        .bind(format_time(due_at))
        .bind(&now_iso)
        .bind(recruit_id)
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to extend recruit {recruit_id}: {err}"))?
        .rows_affected();
        if rows != 1 {
            tx.rollback()
                .await
                .map_err(|err| format!("failed to rollback recruit transaction: {err}"))?;
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO decisions(
                recruit_id, guild_id, user_id, decision, admin_id, reason, extension_days, created_at
             ) VALUES (?, ?, ?, 'extended', ?, ?, ?, ?)",
        )
        .bind(recruit.id)
        .bind(recruit.guild_id.to_string())
        .bind(recruit.user_id.to_string())
        .bind(admin_id.to_string())
        .bind(reason)
        .bind(extension_days)
        .bind(&now_iso)
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to insert recruit extension decision: {err}"))?;
        tx.commit()
            .await
            .map_err(|err| format!("failed to commit recruit extension transaction: {err}"))?;
        Ok(true)
    }

    pub async fn get_open_voice_session(
        &self,
        guild_id: u64,
        user_id: u64,
    ) -> Result<Option<RecruitVoiceSessionRecord>, String> {
        let row = sqlx::query(
            voice_select_sql(
                "WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=? AND ended_at IS NULL
                 ORDER BY id DESC LIMIT 1",
            )
            .as_str(),
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| format!("failed to read open recruit voice session: {err}"))?;
        row.map(|row| voice_from_row(&row)).transpose()
    }

    pub async fn voice_seconds_for_recruit(
        &self,
        recruit: &RecruitRecord,
        now: DateTime<Utc>,
    ) -> Result<i64, String> {
        let rows = sqlx::query(
            voice_select_sql(
                "WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=?
                   AND started_at >= ? AND started_at <= ?
                 ORDER BY started_at ASC, id ASC",
            )
            .as_str(),
        )
        .bind(recruit.guild_id.to_string())
        .bind(recruit.user_id.to_string())
        .bind(&recruit.started_at)
        .bind(format_time(now))
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list recruit voice sessions: {err}"))?;

        let mut total = 0_i64;
        let now_unix = now.timestamp();
        for row in rows {
            let session = voice_from_row(&row)?;
            let duration = match (session.duration_seconds, session.ended_at.as_deref()) {
                (Some(duration), _) => duration.max(0),
                (_, Some(ended_at)) => parse_rfc3339_unix(ended_at)
                    .map(|ended_unix| {
                        (ended_unix - parse_rfc3339_unix(&session.started_at).unwrap_or(ended_unix))
                            .max(0)
                    })
                    .unwrap_or(0),
                _ => {
                    (now_unix - parse_rfc3339_unix(&session.started_at).unwrap_or(now_unix)).max(0)
                }
            };
            total += duration;
        }
        Ok(total)
    }

    pub async fn open_voice_session(
        &self,
        guild_id: u64,
        user_id: u64,
        channel_id: u64,
        started_at: DateTime<Utc>,
    ) -> Result<RecruitVoiceSessionRecord, String> {
        self.ensure_writable()?;
        if let Some(existing) = self.get_open_voice_session(guild_id, user_id).await? {
            return Ok(existing);
        }
        let now = format_time(Utc::now());
        let result = sqlx::query(
            "INSERT INTO voice_sessions(
                guild_id, user_id, channel_id, started_at, ended_at,
                duration_seconds, interrupted, created_at
             ) VALUES (?, ?, ?, ?, NULL, NULL, 0, ?)",
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .bind(channel_id.to_string())
        .bind(format_time(started_at))
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to open recruit voice session: {err}"))?;
        self.get_voice_session_by_id(result.last_insert_rowid())
            .await?
            .ok_or_else(|| "created recruit voice session was not found".to_owned())
    }

    pub async fn close_open_voice_sessions(
        &self,
        guild_id: u64,
        user_id: u64,
        ended_at: DateTime<Utc>,
        interrupted: bool,
    ) -> Result<Vec<ClosedRecruitVoiceSession>, String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            voice_select_sql(
                "WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=? AND ended_at IS NULL",
            )
            .as_str(),
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list open recruit voice sessions: {err}"))?;
        let sessions: Vec<RecruitVoiceSessionRecord> =
            rows.iter().map(voice_from_row).collect::<Result<_, _>>()?;

        let mut closed = Vec::with_capacity(sessions.len());
        for session in sessions {
            let duration_seconds =
                (ended_at.timestamp() - parse_rfc3339_unix(&session.started_at)?).max(0);
            sqlx::query(
                "UPDATE voice_sessions
                 SET ended_at=?, duration_seconds=?, interrupted=?
                 WHERE id=? AND ended_at IS NULL",
            )
            .bind(format_time(ended_at))
            .bind(duration_seconds)
            .bind(if interrupted { 1 } else { 0 })
            .bind(session.id)
            .execute(&self.pool)
            .await
            .map_err(|err| {
                format!(
                    "failed to close recruit voice session {}: {err}",
                    session.id
                )
            })?;
            closed.push(ClosedRecruitVoiceSession {
                id: session.id,
                guild_id: session.guild_id,
                user_id: session.user_id,
                channel_id: session.channel_id,
                duration_seconds,
                interrupted,
            });
        }
        Ok(closed)
    }

    pub async fn get_voice_session_by_id(
        &self,
        session_id: i64,
    ) -> Result<Option<RecruitVoiceSessionRecord>, String> {
        let row = sqlx::query(voice_select_sql("WHERE id=?").as_str())
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to read recruit voice session {session_id}: {err}"))?;
        row.map(|row| voice_from_row(&row)).transpose()
    }

    async fn get_active_recruit_by_id_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        recruit_id: i64,
    ) -> Result<Option<RecruitRecord>, String> {
        let row = sqlx::query(recruit_select_sql("WHERE id=? AND status='active'").as_str())
            .bind(recruit_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|err| format!("failed to read active recruit {recruit_id}: {err}"))?;
        row.map(|row| recruit_from_row(&row)).transpose()
    }

    fn ensure_writable(&self) -> Result<(), String> {
        if self.writes_enabled {
            Ok(())
        } else {
            Err("recruit repository was opened read-only; writes are disabled".to_owned())
        }
    }

    #[cfg(test)]
    pub async fn create_schema_for_tests(&self) -> Result<(), String> {
        sqlx::query(
            "CREATE TABLE recruits (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                guild_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                due_at TEXT NOT NULL,
                completed_at TEXT NULL,
                extensions_count INTEGER NOT NULL DEFAULT 0,
                last_decision_message_id INTEGER NULL,
                last_decision_channel_id INTEGER NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create recruits fixture: {err}"))?;
        sqlx::query(
            "CREATE TABLE voice_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                guild_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                channel_id INTEGER NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT NULL,
                duration_seconds INTEGER NULL,
                interrupted INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create recruit voice_sessions fixture: {err}"))?;
        sqlx::query(
            "CREATE TABLE decisions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                recruit_id INTEGER NOT NULL,
                guild_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                decision TEXT NOT NULL,
                admin_id INTEGER NOT NULL,
                reason TEXT NULL,
                extension_days INTEGER NULL,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create decisions fixture: {err}"))?;
        Ok(())
    }
}

fn recruit_select_sql(where_clause: &str) -> String {
    format!(
        "SELECT id, CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id,
                status, started_at, due_at, completed_at, extensions_count,
                CAST(last_decision_message_id AS TEXT) AS last_decision_message_id,
                CAST(last_decision_channel_id AS TEXT) AS last_decision_channel_id,
                created_at, updated_at
         FROM recruits {where_clause}"
    )
}

fn voice_select_sql(where_clause: &str) -> String {
    format!(
        "SELECT id, CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id,
                CAST(channel_id AS TEXT) AS channel_id, started_at, ended_at,
                duration_seconds, interrupted, created_at
         FROM voice_sessions {where_clause}"
    )
}

fn recruit_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<RecruitRecord, String> {
    Ok(RecruitRecord {
        id: row.try_get("id").map_err(read_err("id"))?,
        guild_id: parse_required_u64(row, "guild_id")?,
        user_id: parse_required_u64(row, "user_id")?,
        status: row.try_get("status").map_err(read_err("status"))?,
        started_at: row.try_get("started_at").map_err(read_err("started_at"))?,
        due_at: row.try_get("due_at").map_err(read_err("due_at"))?,
        completed_at: row
            .try_get("completed_at")
            .map_err(read_err("completed_at"))?,
        extensions_count: row
            .try_get("extensions_count")
            .map_err(read_err("extensions_count"))?,
        last_decision_message_id: parse_optional_u64(row, "last_decision_message_id")?,
        last_decision_channel_id: parse_optional_u64(row, "last_decision_channel_id")?,
        created_at: row.try_get("created_at").map_err(read_err("created_at"))?,
        updated_at: row.try_get("updated_at").map_err(read_err("updated_at"))?,
    })
}

fn voice_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<RecruitVoiceSessionRecord, String> {
    Ok(RecruitVoiceSessionRecord {
        id: row.try_get("id").map_err(read_err("id"))?,
        guild_id: parse_required_u64(row, "guild_id")?,
        user_id: parse_required_u64(row, "user_id")?,
        channel_id: parse_required_u64(row, "channel_id")?,
        started_at: row.try_get("started_at").map_err(read_err("started_at"))?,
        ended_at: row.try_get("ended_at").map_err(read_err("ended_at"))?,
        duration_seconds: row
            .try_get("duration_seconds")
            .map_err(read_err("duration_seconds"))?,
        interrupted: row
            .try_get::<i64, _>("interrupted")
            .map_err(read_err("interrupted"))?
            != 0,
        created_at: row.try_get("created_at").map_err(read_err("created_at"))?,
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
