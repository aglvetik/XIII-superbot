use crate::state::{
    ActiveVoiceSession, CompletedVoiceSession, StoredVoiceUser, VoiceCutoverCloseResult,
    VoiceCutoverClosedSession,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LegacySqliteVoiceActivityRepository {
    path: PathBuf,
    pool: SqlitePool,
    writes_enabled: bool,
}

impl LegacySqliteVoiceActivityRepository {
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
                    "failed to open voice activity DB {} {}: {err}",
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

    pub fn writes_enabled(&self) -> bool {
        self.writes_enabled
    }

    pub async fn upsert_user(&self, user: &StoredVoiceUser) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query(
            "INSERT INTO users (user_id, display_name, username, last_seen_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(user_id) DO UPDATE SET
                display_name=excluded.display_name,
                username=excluded.username,
                last_seen_at=excluded.last_seen_at",
        )
        .bind(user.user_id.to_string())
        .bind(&user.display_name)
        .bind(&user.username)
        .bind(&user.last_seen_at)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to upsert voice user {}: {err}", user.user_id))?;
        Ok(())
    }

    pub async fn list_users(&self) -> Result<Vec<StoredVoiceUser>, String> {
        let rows =
            sqlx::query("SELECT CAST(user_id AS TEXT) AS user_id, display_name, username, last_seen_at FROM users")
                .fetch_all(&self.pool)
                .await
                .map_err(|err| format!("failed to list voice users: {err}"))?;
        rows.iter().map(user_from_row).collect()
    }

    pub async fn list_active_sessions(
        &self,
        guild_id: u64,
    ) -> Result<Vec<ActiveVoiceSession>, String> {
        let rows = sqlx::query(
            "SELECT CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id,
                    CAST(channel_id AS TEXT) AS channel_id, started_at, last_seen_at, recovered
             FROM active_voice_sessions
             WHERE CAST(guild_id AS TEXT)=?
             ORDER BY CAST(user_id AS TEXT)",
        )
        .bind(guild_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list active voice sessions: {err}"))?;
        rows.iter().map(active_from_row).collect()
    }

    pub async fn get_active_session(
        &self,
        guild_id: u64,
        user_id: u64,
    ) -> Result<Option<ActiveVoiceSession>, String> {
        sqlx::query(
            "SELECT CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id,
                    CAST(channel_id AS TEXT) AS channel_id, started_at, last_seen_at, recovered
             FROM active_voice_sessions
             WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=?",
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| format!("failed to get active voice session for {user_id}: {err}"))?
        .as_ref()
        .map(active_from_row)
        .transpose()
    }

    pub async fn create_or_replace_active_session(
        &self,
        session: &ActiveVoiceSession,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query(
            "INSERT OR REPLACE INTO active_voice_sessions (
                guild_id, user_id, channel_id, started_at, last_seen_at, recovered
             ) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(session.guild_id.to_string())
        .bind(session.user_id.to_string())
        .bind(session.channel_id.to_string())
        .bind(&session.started_at)
        .bind(&session.last_seen_at)
        .bind(if session.recovered { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create active voice session: {err}"))?;
        Ok(())
    }

    pub async fn update_active_session_channel(
        &self,
        guild_id: u64,
        user_id: u64,
        channel_id: u64,
        last_seen_at: &str,
    ) -> Result<bool, String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            "UPDATE active_voice_sessions
             SET channel_id=?, last_seen_at=?
             WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=?",
        )
        .bind(channel_id.to_string())
        .bind(last_seen_at)
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to update active voice session channel: {err}"))?
        .rows_affected();
        Ok(rows > 0)
    }

    pub async fn update_active_session_last_seen(
        &self,
        guild_id: u64,
        user_id: u64,
        last_seen_at: &str,
    ) -> Result<bool, String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            "UPDATE active_voice_sessions
             SET last_seen_at=?
             WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=?",
        )
        .bind(last_seen_at)
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to update active voice session last_seen: {err}"))?
        .rows_affected();
        Ok(rows > 0)
    }

    pub async fn close_active_session(
        &self,
        guild_id: u64,
        user_id: u64,
        ended_at: &str,
        close_reason: &str,
    ) -> Result<Option<CompletedVoiceSession>, String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin close voice session transaction: {err}"))?;
        let row = sqlx::query(
            "SELECT CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id,
                    CAST(channel_id AS TEXT) AS channel_id, started_at, last_seen_at, recovered
             FROM active_voice_sessions
             WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=?",
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|err| format!("failed to fetch active voice session for close: {err}"))?;
        let Some(row) = row else {
            tx.rollback()
                .await
                .map_err(|err| format!("failed to rollback close transaction: {err}"))?;
            return Ok(None);
        };
        let active = active_from_row(&row)?;
        let duration = duration_between_iso(&active.started_at, ended_at)
            .unwrap_or(0)
            .max(0);
        if duration > 0 {
            sqlx::query(
                "INSERT INTO voice_sessions (
                    guild_id, user_id, channel_id, started_at, ended_at, duration_seconds, close_reason
                 ) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(active.guild_id.to_string())
            .bind(active.user_id.to_string())
            .bind(active.channel_id.to_string())
            .bind(&active.started_at)
            .bind(ended_at)
            .bind(duration)
            .bind(close_reason)
            .execute(&mut *tx)
            .await
            .map_err(|err| format!("failed to insert completed voice session: {err}"))?;
        }
        sqlx::query(
            "DELETE FROM active_voice_sessions
             WHERE CAST(guild_id AS TEXT)=? AND CAST(user_id AS TEXT)=?",
        )
        .bind(guild_id.to_string())
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to delete active voice session: {err}"))?;
        tx.commit()
            .await
            .map_err(|err| format!("failed to commit close voice session transaction: {err}"))?;
        if duration <= 0 {
            return Ok(None);
        }
        Ok(Some(CompletedVoiceSession {
            id: None,
            guild_id: active.guild_id,
            user_id: active.user_id,
            channel_id: active.channel_id,
            started_at: active.started_at,
            ended_at: ended_at.to_owned(),
            duration_seconds: duration,
            close_reason: close_reason.to_owned(),
        }))
    }

    pub async fn close_all_active_sessions_at_cutover(
        &self,
        guild_id: u64,
        cutover_at_utc: &str,
    ) -> Result<VoiceCutoverCloseResult, String> {
        self.ensure_writable()?;
        let active = self.list_active_sessions(guild_id).await?;
        let mut closed_sessions = Vec::new();
        for session in &active {
            let duration_seconds = duration_between_iso(&session.started_at, cutover_at_utc)
                .unwrap_or(0)
                .max(0);
            let completed = self
                .close_active_session(
                    session.guild_id,
                    session.user_id,
                    cutover_at_utc,
                    "cutover_closed_active",
                )
                .await?;
            closed_sessions.push(VoiceCutoverClosedSession {
                guild_id: session.guild_id,
                user_id: session.user_id,
                channel_id: session.channel_id,
                started_at: session.started_at.clone(),
                ended_at: cutover_at_utc.to_owned(),
                duration_seconds,
                completed_row_inserted: completed.is_some(),
            });
        }

        Ok(VoiceCutoverCloseResult {
            cutover_at_utc: cutover_at_utc.to_owned(),
            active_sessions_before: active.len(),
            closed_sessions,
        })
    }

    pub async fn fetch_completed_sessions_since(
        &self,
        guild_id: u64,
        overlap_start: Option<&str>,
    ) -> Result<Vec<CompletedVoiceSession>, String> {
        let (sql, bind_start) = if overlap_start.is_some() {
            (
                "SELECT id, CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id,
                        CAST(channel_id AS TEXT) AS channel_id, started_at, ended_at,
                        duration_seconds, close_reason
                 FROM voice_sessions
                 WHERE CAST(guild_id AS TEXT)=? AND ended_at > ?
                 ORDER BY ended_at DESC",
                true,
            )
        } else {
            (
                "SELECT id, CAST(guild_id AS TEXT) AS guild_id, CAST(user_id AS TEXT) AS user_id,
                        CAST(channel_id AS TEXT) AS channel_id, started_at, ended_at,
                        duration_seconds, close_reason
                 FROM voice_sessions
                 WHERE CAST(guild_id AS TEXT)=?
                 ORDER BY ended_at DESC",
                false,
            )
        };
        let mut query = sqlx::query(sql).bind(guild_id.to_string());
        if bind_start {
            query = query.bind(overlap_start.unwrap_or_default());
        }
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| format!("failed to fetch completed voice sessions: {err}"))?;
        rows.iter().map(completed_from_row).collect()
    }

    pub async fn get_bot_state(&self, key: &str) -> Result<Option<String>, String> {
        sqlx::query("SELECT value FROM bot_state WHERE key=?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to read voice bot_state {key}: {err}"))?
            .map(|row| row.try_get::<String, _>("value").map_err(read_err("value")))
            .transpose()
    }

    pub async fn set_bot_state(&self, key: &str, value: &str) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query(
            "INSERT INTO bot_state (key, value)
             VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to write voice bot_state {key}: {err}"))?;
        Ok(())
    }

    fn ensure_writable(&self) -> Result<(), String> {
        if self.writes_enabled {
            Ok(())
        } else {
            Err("voice activity repository was opened read-only; writes are disabled".to_owned())
        }
    }

    #[cfg(test)]
    pub async fn create_schema_for_tests(&self) -> Result<(), String> {
        sqlx::query(
            "CREATE TABLE users (
                user_id INTEGER PRIMARY KEY,
                display_name TEXT NOT NULL,
                username TEXT,
                last_seen_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create users fixture: {err}"))?;
        sqlx::query(
            "CREATE TABLE active_voice_sessions (
                guild_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                channel_id INTEGER NOT NULL,
                started_at TEXT NOT NULL,
                last_seen_at TEXT NOT NULL,
                recovered INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (guild_id, user_id)
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create active sessions fixture: {err}"))?;
        sqlx::query(
            "CREATE TABLE voice_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                guild_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                channel_id INTEGER NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT NOT NULL,
                duration_seconds INTEGER NOT NULL,
                close_reason TEXT NOT NULL DEFAULT 'normal'
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create sessions fixture: {err}"))?;
        sqlx::query("CREATE TABLE bot_state (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(&self.pool)
            .await
            .map_err(|err| format!("failed to create bot_state fixture: {err}"))?;
        Ok(())
    }
}

fn user_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<StoredVoiceUser, String> {
    Ok(StoredVoiceUser {
        user_id: parse_required_u64(row, "user_id")?,
        display_name: row
            .try_get("display_name")
            .map_err(read_err("display_name"))?,
        username: row.try_get("username").map_err(read_err("username"))?,
        last_seen_at: row
            .try_get("last_seen_at")
            .map_err(read_err("last_seen_at"))?,
    })
}

fn active_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ActiveVoiceSession, String> {
    Ok(ActiveVoiceSession {
        guild_id: parse_required_u64(row, "guild_id")?,
        user_id: parse_required_u64(row, "user_id")?,
        channel_id: parse_required_u64(row, "channel_id")?,
        started_at: row.try_get("started_at").map_err(read_err("started_at"))?,
        last_seen_at: row
            .try_get("last_seen_at")
            .map_err(read_err("last_seen_at"))?,
        recovered: row
            .try_get::<i64, _>("recovered")
            .map_err(read_err("recovered"))?
            != 0,
    })
}

fn completed_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<CompletedVoiceSession, String> {
    Ok(CompletedVoiceSession {
        id: row.try_get("id").map_err(read_err("id"))?,
        guild_id: parse_required_u64(row, "guild_id")?,
        user_id: parse_required_u64(row, "user_id")?,
        channel_id: parse_required_u64(row, "channel_id")?,
        started_at: row.try_get("started_at").map_err(read_err("started_at"))?,
        ended_at: row.try_get("ended_at").map_err(read_err("ended_at"))?,
        duration_seconds: row
            .try_get("duration_seconds")
            .map_err(read_err("duration_seconds"))?,
        close_reason: row
            .try_get("close_reason")
            .map_err(read_err("close_reason"))?,
    })
}

fn parse_required_u64(row: &sqlx::sqlite::SqliteRow, name: &str) -> Result<u64, String> {
    row.try_get::<String, _>(name)
        .map_err(read_err(name))?
        .parse::<u64>()
        .map_err(|err| format!("failed to parse {name} as snowflake: {err}"))
}

fn duration_between_iso(started_at: &str, ended_at: &str) -> Option<i64> {
    let start = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    let end = chrono::DateTime::parse_from_rfc3339(ended_at).ok()?;
    Some((end - start).num_seconds())
}

fn read_err(name: &str) -> impl FnOnce(sqlx::Error) -> String + '_ {
    move |err| format!("failed to read {name}: {err}")
}
