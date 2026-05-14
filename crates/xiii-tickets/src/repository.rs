use crate::runtime::ticket_channel_name;
use crate::state::{
    format_utc, GoogleFormRow, ReservedTicket, Ticket, TicketRecord, TicketRuntimeState,
    TicketStatus, TicketType,
};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::path::{Path, PathBuf};

pub trait TicketRepository {
    fn counter_value(&self, name: &str) -> Result<i64, String>;
    fn tickets_for_user(&self, user_id: u64) -> Result<Vec<Ticket>, String>;
    fn form_signature_processed(&self, signature: &str) -> Result<bool, String>;
    fn writes_enabled(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyTicketRepositoryPlan {
    pub path: String,
    pub read_only: bool,
}

#[derive(Debug, Clone)]
pub struct LegacySqliteTicketRepository {
    path: PathBuf,
    pool: SqlitePool,
    writes_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketDbCounts {
    pub counters: i64,
    pub tickets: i64,
    pub processed_forms: i64,
    pub processed_form_signatures: i64,
    pub bot_state: i64,
    pub open_tickets: i64,
    pub reserved_tickets: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketCounterRow {
    pub name: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketStatusCount {
    pub status: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketBotStateRow {
    pub key: String,
    pub value: String,
}

impl LegacySqliteTicketRepository {
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
                    "failed to open ticket DB {} {}: {err}",
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
        let repo = Self {
            path: path.to_path_buf(),
            pool,
            writes_enabled: writable,
        };
        if writable {
            repo.ensure_runtime_state_schema().await?;
        }
        Ok(repo)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn writes_enabled(&self) -> bool {
        self.writes_enabled
    }

    pub async fn create_schema_for_tests(&self) -> Result<(), String> {
        self.ensure_writable()?;
        for query in [
            "CREATE TABLE IF NOT EXISTS counters (name TEXT PRIMARY KEY, value INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS tickets (
                ticket_id INTEGER PRIMARY KEY AUTOINCREMENT,
                ticket_name TEXT,
                opener_id INTEGER NOT NULL,
                ticket_type TEXT NOT NULL,
                channel_id INTEGER,
                status TEXT NOT NULL,
                created_at_ts REAL NOT NULL,
                created_at_utc TEXT NOT NULL,
                closed_at_utc TEXT,
                reopen_until_utc TEXT
            )",
            "CREATE TABLE IF NOT EXISTS processed_forms (sheet_row INTEGER PRIMARY KEY, processed_at_utc TEXT)",
            "CREATE TABLE IF NOT EXISTS processed_form_signatures (signature TEXT PRIMARY KEY, processed_at_utc TEXT)",
            "CREATE TABLE IF NOT EXISTS bot_state (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        ] {
            sqlx::query(query)
                .execute(&self.pool)
                .await
                .map_err(|err| format!("failed to create ticket test schema: {err}"))?;
        }
        self.ensure_runtime_state_schema().await?;
        Ok(())
    }

    pub async fn counts(&self) -> Result<TicketDbCounts, String> {
        Ok(TicketDbCounts {
            counters: self.count_table("counters").await?,
            tickets: self.count_table("tickets").await?,
            processed_forms: self.count_table("processed_forms").await?,
            processed_form_signatures: self.count_table("processed_form_signatures").await?,
            bot_state: self.count_table("bot_state").await?,
            open_tickets: self.count_status("open").await?,
            reserved_tickets: self.count_status("reserved").await?,
        })
    }

    pub async fn counter_value_async(&self, name: &str) -> Result<i64, String> {
        let row = sqlx::query("SELECT value FROM counters WHERE name=?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to read ticket counter {name}: {err}"))?;
        Ok(row.map(|row| row.get::<i64, _>("value")).unwrap_or(0))
    }

    pub async fn counter_rows(&self) -> Result<Vec<TicketCounterRow>, String> {
        let rows = sqlx::query("SELECT name, value FROM counters ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|err| format!("failed to read ticket counters: {err}"))?;
        rows.iter()
            .map(|row| {
                Ok(TicketCounterRow {
                    name: row.get::<String, _>("name"),
                    value: row.get::<i64, _>("value"),
                })
            })
            .collect()
    }

    pub async fn status_counts(&self) -> Result<Vec<TicketStatusCount>, String> {
        let rows = sqlx::query(
            "SELECT status, COUNT(*) AS count FROM tickets GROUP BY status ORDER BY status",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to read ticket status counts: {err}"))?;
        rows.iter()
            .map(|row| {
                Ok(TicketStatusCount {
                    status: row.get::<String, _>("status"),
                    count: row.get::<i64, _>("count"),
                })
            })
            .collect()
    }

    pub async fn bot_state_rows(&self) -> Result<Vec<TicketBotStateRow>, String> {
        let rows = sqlx::query("SELECT key, value FROM bot_state ORDER BY key")
            .fetch_all(&self.pool)
            .await
            .map_err(|err| format!("failed to read ticket bot_state rows: {err}"))?;
        rows.iter()
            .map(|row| {
                Ok(TicketBotStateRow {
                    key: row.get::<String, _>("key"),
                    value: row.get::<String, _>("value"),
                })
            })
            .collect()
    }

    pub async fn reserve_ticket(
        &self,
        opener_id: u64,
        ticket_type: TicketType,
        now: DateTime<Utc>,
        max_active_for_user: i64,
    ) -> Result<ReservedTicket, String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin ticket reservation transaction: {err}"))?;

        let active_count = active_ticket_count_for_user_in_tx(&mut tx, opener_id).await?;
        if active_count >= max_active_for_user {
            return Err(format!(
                "user {opener_id} already has {active_count} active tickets"
            ));
        }

        let counter_name = ticket_type.counter_name();
        let next = next_counter_value_in_tx(&mut tx, counter_name).await?;
        let ticket_name = ticket_channel_name(ticket_type, next);
        let result = sqlx::query(
            "INSERT INTO tickets (
                ticket_name, opener_id, ticket_type, channel_id, status,
                created_at_ts, created_at_utc, closed_at_utc, reopen_until_utc
             ) VALUES (?, ?, ?, NULL, 'reserved', ?, ?, NULL, NULL)",
        )
        .bind(&ticket_name)
        .bind(opener_id.to_string())
        .bind(ticket_type.as_legacy_value())
        .bind(now.timestamp() as f64)
        .bind(format_utc(now))
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to insert reserved ticket: {err}"))?;

        tx.commit()
            .await
            .map_err(|err| format!("failed to commit ticket reservation: {err}"))?;

        Ok(ReservedTicket {
            ticket_id: result.last_insert_rowid(),
            number: next,
            ticket_name,
            ticket_type,
        })
    }

    pub async fn finalize_ticket_open(
        &self,
        ticket_id: i64,
        ticket_name: &str,
        channel_id: u64,
    ) -> Result<bool, String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            "UPDATE tickets
             SET ticket_name=?, channel_id=?, status='open'
             WHERE ticket_id=? AND status='reserved'",
        )
        .bind(ticket_name)
        .bind(channel_id.to_string())
        .bind(ticket_id)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to finalize ticket {ticket_id}: {err}"))?
        .rows_affected();
        Ok(rows > 0)
    }

    pub async fn rollback_reserved_ticket(&self, ticket_id: i64) -> Result<bool, String> {
        self.ensure_writable()?;
        let rows = sqlx::query("DELETE FROM tickets WHERE ticket_id=? AND status='reserved'")
            .bind(ticket_id)
            .execute(&self.pool)
            .await
            .map_err(|err| format!("failed to rollback reserved ticket {ticket_id}: {err}"))?
            .rows_affected();
        Ok(rows > 0)
    }

    pub async fn get_ticket_by_channel_id(
        &self,
        channel_id: u64,
    ) -> Result<Option<TicketRecord>, String> {
        let sql = ticket_select_sql("WHERE CAST(channel_id AS TEXT)=?");
        let row = sqlx::query(&sql)
            .bind(channel_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to get ticket by channel {channel_id}: {err}"))?;
        row.as_ref().map(ticket_record_from_row).transpose()
    }

    pub async fn get_ticket_by_id(&self, ticket_id: i64) -> Result<Option<TicketRecord>, String> {
        let sql = ticket_select_sql("WHERE ticket_id=?");
        let row = sqlx::query(&sql)
            .bind(ticket_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to get ticket {ticket_id}: {err}"))?;
        row.as_ref().map(ticket_record_from_row).transpose()
    }

    pub async fn find_application_ticket_by_number(
        &self,
        ticket_number: i64,
    ) -> Result<Option<TicketRecord>, String> {
        let pattern = format!("%-{ticket_number}");
        let sql = ticket_select_sql(
            "WHERE ticket_type='application' AND ticket_name LIKE ? ORDER BY ticket_id DESC LIMIT 1",
        );
        let row = sqlx::query(&sql)
            .bind(pattern)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| {
                format!("failed to find application ticket number {ticket_number}: {err}")
            })?;
        row.as_ref().map(ticket_record_from_row).transpose()
    }

    pub async fn latest_reopenable_ticket_for_user(
        &self,
        user_id: u64,
        now: DateTime<Utc>,
    ) -> Result<Option<TicketRecord>, String> {
        let sql = ticket_select_sql(
            "WHERE CAST(opener_id AS TEXT)=?
               AND status='closed'
               AND reopen_until_utc IS NOT NULL
               AND reopen_until_utc <> ''
             ORDER BY ticket_id DESC",
        );
        let rows = sqlx::query(&sql)
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|err| {
                format!("failed to list reopenable tickets for user {user_id}: {err}")
            })?;
        for row in rows {
            let ticket = ticket_record_from_row(&row)?;
            let Some(until) = ticket.reopen_until_utc.as_deref() else {
                continue;
            };
            if DateTime::parse_from_rfc3339(until)
                .map(|until| until.with_timezone(&Utc) > now)
                .unwrap_or(false)
            {
                return Ok(Some(ticket));
            }
        }
        Ok(None)
    }

    pub async fn tickets_for_user_async(&self, user_id: u64) -> Result<Vec<TicketRecord>, String> {
        let sql = ticket_select_sql("WHERE CAST(opener_id AS TEXT)=? ORDER BY ticket_id");
        let rows = sqlx::query(&sql)
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|err| format!("failed to list tickets for user {user_id}: {err}"))?;
        rows.iter().map(ticket_record_from_row).collect()
    }

    pub async fn mark_ticket_closed_by_channel(
        &self,
        channel_id: u64,
        now: DateTime<Utc>,
        reopen_window_hours: i64,
    ) -> Result<Option<TicketRecord>, String> {
        self.ensure_writable()?;
        let closed_at = format_utc(now);
        let reopen_until = format_utc(now + Duration::hours(reopen_window_hours));
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin ticket close transaction: {err}"))?;
        let sql = ticket_select_sql("WHERE CAST(channel_id AS TEXT)=? AND status='open'");
        let existing = sqlx::query(&sql)
            .bind(channel_id.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(|err| format!("failed to select open ticket for close: {err}"))?;
        let Some(existing) = existing else {
            return Ok(None);
        };
        let ticket_id = existing.get::<i64, _>("ticket_id");
        sqlx::query(
            "UPDATE tickets
             SET status='closed', closed_at_utc=?, reopen_until_utc=?
             WHERE ticket_id=? AND status='open'",
        )
        .bind(&closed_at)
        .bind(&reopen_until)
        .bind(ticket_id)
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to mark ticket closed: {err}"))?;
        tx.commit()
            .await
            .map_err(|err| format!("failed to commit ticket close: {err}"))?;
        self.get_ticket_by_id(ticket_id).await
    }

    pub async fn reopen_ticket_record(
        &self,
        ticket_id: i64,
        new_channel_id: Option<u64>,
        new_ticket_name: Option<&str>,
    ) -> Result<bool, String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            "UPDATE tickets
             SET status='open',
                 channel_id=COALESCE(?, channel_id),
                 ticket_name=COALESCE(?, ticket_name),
                 closed_at_utc=NULL,
                 reopen_until_utc=NULL
             WHERE ticket_id=? AND status='closed'",
        )
        .bind(new_channel_id.map(|id| id.to_string()))
        .bind(new_ticket_name)
        .bind(ticket_id)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to reopen ticket {ticket_id}: {err}"))?
        .rows_affected();
        Ok(rows > 0)
    }

    pub async fn mark_ticket_deleted_by_channel(&self, channel_id: u64) -> Result<bool, String> {
        self.ensure_writable()?;
        let rows = sqlx::query(
            "UPDATE tickets
             SET status='deleted'
             WHERE CAST(channel_id AS TEXT)=? AND status IN ('closed', 'open')",
        )
        .bind(channel_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to mark ticket channel {channel_id} deleted: {err}"))?
        .rows_affected();
        Ok(rows > 0)
    }

    pub async fn processed_form_row_exists(&self, sheet_row: i64) -> Result<bool, String> {
        let row = sqlx::query("SELECT 1 FROM processed_forms WHERE sheet_row=?")
            .bind(sheet_row)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to check processed form row {sheet_row}: {err}"))?;
        Ok(row.is_some())
    }

    pub async fn form_signature_processed_async(&self, signature: &str) -> Result<bool, String> {
        let row = sqlx::query("SELECT 1 FROM processed_form_signatures WHERE signature=?")
            .bind(signature)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to check processed form signature: {err}"))?;
        Ok(row.is_some())
    }

    pub async fn mark_form_processed(
        &self,
        sheet_row: i64,
        signature: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin processed form transaction: {err}"))?;
        let processed_at = format_utc(now);
        sqlx::query(
            "INSERT OR IGNORE INTO processed_forms(sheet_row, processed_at_utc) VALUES (?, ?)",
        )
        .bind(sheet_row)
        .bind(&processed_at)
        .execute(&mut *tx)
        .await
        .map_err(|err| format!("failed to mark processed form row {sheet_row}: {err}"))?;
        sqlx::query("INSERT OR IGNORE INTO processed_form_signatures(signature, processed_at_utc) VALUES (?, ?)")
            .bind(signature)
            .bind(&processed_at)
            .execute(&mut *tx)
            .await
            .map_err(|err| format!("failed to mark processed form signature: {err}"))?;
        tx.commit()
            .await
            .map_err(|err| format!("failed to commit processed form transaction: {err}"))
    }

    pub async fn mark_form_processed_after_send(
        &self,
        send_succeeded: bool,
        sheet_row: i64,
        signature: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, String> {
        if !send_succeeded {
            return Ok(false);
        }
        self.mark_form_processed(sheet_row, signature, now).await?;
        Ok(true)
    }

    pub async fn bot_state(&self, key: &str) -> Result<Option<String>, String> {
        let row = sqlx::query("SELECT value FROM bot_state WHERE key=?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| format!("failed to read ticket bot_state {key}: {err}"))?;
        Ok(row.map(|row| row.get::<String, _>("value")))
    }

    pub async fn set_bot_state(&self, key: &str, value: &str) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query(
            "INSERT INTO bot_state(key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to set ticket bot_state {key}: {err}"))?;
        Ok(())
    }

    pub async fn try_claim_bot_state(&self, key: &str, value: &str) -> Result<bool, String> {
        self.ensure_writable()?;
        let result = sqlx::query("INSERT OR IGNORE INTO bot_state(key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await
            .map_err(|err| format!("failed to claim ticket bot_state {key}: {err}"))?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn delete_bot_state(&self, key: &str) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query("DELETE FROM bot_state WHERE key=?")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|err| format!("failed to delete ticket bot_state {key}: {err}"))?;
        Ok(())
    }

    pub async fn ticket_runtime_state(&self, ticket_id: i64) -> Result<TicketRuntimeState, String> {
        let row = sqlx::query(
            "SELECT
                ticket_id,
                transcript_channel_message_id,
                transcript_dm_message_id,
                closed_controls_message_id,
                last_transcript_hash
             FROM ticket_runtime_state
             WHERE ticket_id=?",
        )
        .bind(ticket_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| format!("failed to read ticket runtime state {ticket_id}: {err}"))?;
        row.as_ref()
            .map(ticket_runtime_state_from_row)
            .transpose()
            .map(|state| {
                state.unwrap_or_else(|| TicketRuntimeState {
                    ticket_id,
                    ..TicketRuntimeState::default()
                })
            })
    }

    pub async fn upsert_ticket_runtime_state(
        &self,
        state: &TicketRuntimeState,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        self.ensure_runtime_state_schema().await?;
        sqlx::query(
            "INSERT INTO ticket_runtime_state(
                ticket_id,
                transcript_channel_message_id,
                transcript_dm_message_id,
                closed_controls_message_id,
                last_transcript_hash
             ) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(ticket_id) DO UPDATE SET
                transcript_channel_message_id=excluded.transcript_channel_message_id,
                transcript_dm_message_id=excluded.transcript_dm_message_id,
                closed_controls_message_id=excluded.closed_controls_message_id,
                last_transcript_hash=excluded.last_transcript_hash",
        )
        .bind(state.ticket_id)
        .bind(state.transcript_channel_message_id.map(|id| id.to_string()))
        .bind(state.transcript_dm_message_id.map(|id| id.to_string()))
        .bind(state.closed_controls_message_id.map(|id| id.to_string()))
        .bind(state.last_transcript_hash.as_deref())
        .execute(&self.pool)
        .await
        .map_err(|err| {
            format!(
                "failed to upsert ticket runtime state {}: {err}",
                state.ticket_id
            )
        })?;
        Ok(())
    }

    async fn count_table(&self, table: &str) -> Result<i64, String> {
        let sql = format!("SELECT COUNT(*) AS count FROM {table}");
        let row = sqlx::query(&sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|err| format!("failed to count {table}: {err}"))?;
        Ok(row.get::<i64, _>("count"))
    }

    async fn count_status(&self, status: &str) -> Result<i64, String> {
        let row = sqlx::query("SELECT COUNT(*) AS count FROM tickets WHERE status=?")
            .bind(status)
            .fetch_one(&self.pool)
            .await
            .map_err(|err| format!("failed to count ticket status {status}: {err}"))?;
        Ok(row.get::<i64, _>("count"))
    }

    fn ensure_writable(&self) -> Result<(), String> {
        if self.writes_enabled {
            Ok(())
        } else {
            Err("ticket repository was opened read-only; write refused".to_owned())
        }
    }

    async fn ensure_runtime_state_schema(&self) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ticket_runtime_state (
                ticket_id INTEGER PRIMARY KEY,
                transcript_channel_message_id TEXT,
                transcript_dm_message_id TEXT,
                closed_controls_message_id TEXT,
                last_transcript_hash TEXT
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to ensure ticket_runtime_state schema: {err}"))?;
        Ok(())
    }
}

pub fn google_form_signature(row: &GoogleFormRow) -> String {
    let mut hasher = Sha256::new();
    hasher.update(row.sheet_row.to_string().as_bytes());
    hasher.update(b"\0");
    for value in &row.values {
        hasher.update(value.as_bytes());
        hasher.update(b"\0");
    }
    format!("{:x}", hasher.finalize())
}

async fn active_ticket_count_for_user_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    opener_id: u64,
) -> Result<i64, String> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS count
         FROM tickets
         WHERE CAST(opener_id AS TEXT)=? AND status IN ('reserved', 'open')",
    )
    .bind(opener_id.to_string())
    .fetch_one(&mut **tx)
    .await
    .map_err(|err| format!("failed to count active tickets for user {opener_id}: {err}"))?;
    Ok(row.get::<i64, _>("count"))
}

async fn next_counter_value_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    counter_name: &str,
) -> Result<i64, String> {
    let row = sqlx::query("SELECT value FROM counters WHERE name=?")
        .bind(counter_name)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|err| format!("failed to read counter {counter_name}: {err}"))?;
    let next = row.map(|row| row.get::<i64, _>("value")).unwrap_or(0) + 1;
    sqlx::query(
        "INSERT INTO counters(name, value) VALUES (?, ?)
         ON CONFLICT(name) DO UPDATE SET value=excluded.value",
    )
    .bind(counter_name)
    .bind(next)
    .execute(&mut **tx)
    .await
    .map_err(|err| format!("failed to write counter {counter_name}: {err}"))?;
    Ok(next)
}

fn ticket_select_sql(where_clause: &str) -> String {
    format!(
        "SELECT
            ticket_id,
            ticket_name,
            CAST(opener_id AS TEXT) AS opener_id,
            ticket_type,
            CAST(channel_id AS TEXT) AS channel_id,
            status,
            created_at_utc,
            closed_at_utc,
            reopen_until_utc
         FROM tickets {where_clause}"
    )
}

fn ticket_record_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TicketRecord, String> {
    let opener_id = parse_u64(row.get::<String, _>("opener_id").as_str(), "opener_id")?;
    let channel_id = row
        .get::<Option<String>, _>("channel_id")
        .as_deref()
        .map(|value| parse_u64(value, "channel_id"))
        .transpose()?;
    Ok(TicketRecord {
        ticket_id: row.get::<i64, _>("ticket_id"),
        ticket_name: row.get::<Option<String>, _>("ticket_name"),
        opener_id,
        ticket_type: TicketType::from_legacy_value(row.get::<String, _>("ticket_type").as_str()),
        channel_id,
        status: TicketStatus::from_legacy_value(row.get::<String, _>("status").as_str()),
        created_at_utc: row.get::<String, _>("created_at_utc"),
        closed_at_utc: row.get::<Option<String>, _>("closed_at_utc"),
        reopen_until_utc: row.get::<Option<String>, _>("reopen_until_utc"),
    })
}

fn ticket_runtime_state_from_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<TicketRuntimeState, String> {
    Ok(TicketRuntimeState {
        ticket_id: row.get::<i64, _>("ticket_id"),
        transcript_channel_message_id: row
            .get::<Option<String>, _>("transcript_channel_message_id")
            .as_deref()
            .map(|value| parse_u64(value, "transcript_channel_message_id"))
            .transpose()?,
        transcript_dm_message_id: row
            .get::<Option<String>, _>("transcript_dm_message_id")
            .as_deref()
            .map(|value| parse_u64(value, "transcript_dm_message_id"))
            .transpose()?,
        closed_controls_message_id: row
            .get::<Option<String>, _>("closed_controls_message_id")
            .as_deref()
            .map(|value| parse_u64(value, "closed_controls_message_id"))
            .transpose()?,
        last_transcript_hash: row.get::<Option<String>, _>("last_transcript_hash"),
    })
}

fn parse_u64(value: &str, column: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|err| format!("invalid {column} value {value}: {err}"))
}
