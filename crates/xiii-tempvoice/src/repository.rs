use crate::runtime::{TempVoiceDbMutation, TempVoiceDbTransactionPlan};
use crate::state::{HubSettings, TempVoiceChannel};
use chrono::{SecondsFormat, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};

pub trait TempVoiceRepository {
    fn guild_settings(&self, guild_id: u64) -> Result<Option<HubSettings>, String>;
    fn tracked_channels(&self, guild_id: u64) -> Result<Vec<TempVoiceChannel>, String>;
    fn writes_enabled(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySqliteTempVoiceRepositoryPlan {
    pub path: String,
    pub read_only: bool,
}

impl LegacySqliteTempVoiceRepositoryPlan {
    pub fn read_only(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            read_only: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TempVoiceChannelRecord {
    pub channel_id: u64,
    pub guild_id: u64,
    pub owner_user_id: u64,
    pub created_at: String,
    pub last_empty_at: Option<String>,
}

impl TempVoiceChannelRecord {
    pub fn to_runtime_channel(&self, member_count: usize) -> TempVoiceChannel {
        TempVoiceChannel {
            channel_id: self.channel_id,
            guild_id: self.guild_id,
            owner_user_id: self.owner_user_id,
            member_count,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LegacySqliteTempVoiceRepository {
    path: PathBuf,
    pool: SqlitePool,
    writes_enabled: bool,
}

impl LegacySqliteTempVoiceRepository {
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
                    "failed to open temp voice DB {} {}: {err}",
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

    pub async fn get_guild_hub_channel(&self, guild_id: u64) -> Result<Option<u64>, String> {
        let row = sqlx::query(
            "SELECT CAST(hub_channel_id AS TEXT) AS hub_channel_id \
             FROM guild_settings WHERE CAST(guild_id AS TEXT)=?",
        )
        .bind(guild_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| format!("failed to read guild_settings: {err}"))?;

        row.map(|row| parse_u64_column(&row, "hub_channel_id"))
            .transpose()
    }

    pub async fn list_guild_settings(&self) -> Result<Vec<HubSettings>, String> {
        let rows = sqlx::query(
            "SELECT CAST(guild_id AS TEXT) AS guild_id, \
                    CAST(hub_channel_id AS TEXT) AS hub_channel_id \
             FROM guild_settings ORDER BY CAST(guild_id AS TEXT)",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list guild_settings: {err}"))?;

        rows.iter()
            .map(|row| {
                Ok(HubSettings {
                    guild_id: parse_u64_column(row, "guild_id")?,
                    hub_channel_id: parse_u64_column(row, "hub_channel_id")?,
                })
            })
            .collect()
    }

    pub async fn set_guild_hub_channel(
        &self,
        guild_id: u64,
        hub_channel_id: u64,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        self.execute_transaction(TempVoiceDbTransactionPlan::new(vec![
            TempVoiceDbMutation::UpsertHub {
                guild_id,
                hub_channel_id,
            },
        ]))
        .await
    }

    pub async fn insert_temp_channel(
        &self,
        channel_id: u64,
        guild_id: u64,
        owner_user_id: u64,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        self.execute_transaction(TempVoiceDbTransactionPlan::new(vec![
            TempVoiceDbMutation::InsertTempChannel {
                channel_id,
                guild_id,
                owner_user_id,
            },
        ]))
        .await
    }

    pub async fn remove_temp_channel(&self, channel_id: u64) -> Result<(), String> {
        self.ensure_writable()?;
        self.execute_transaction(TempVoiceDbTransactionPlan::new(vec![
            TempVoiceDbMutation::DeleteTrackedChannel { channel_id },
        ]))
        .await
    }

    pub async fn update_last_empty_at(
        &self,
        channel_id: u64,
        last_empty_at: Option<String>,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        sqlx::query(
            "UPDATE temp_voice_channels SET last_empty_at=? \
             WHERE CAST(channel_id AS TEXT)=?",
        )
        .bind(last_empty_at)
        .bind(channel_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to update last_empty_at for {channel_id}: {err}"))?;
        Ok(())
    }

    pub async fn get_temp_channel(
        &self,
        channel_id: u64,
    ) -> Result<Option<TempVoiceChannelRecord>, String> {
        let row = sqlx::query(
            "SELECT CAST(channel_id AS TEXT) AS channel_id, \
                    CAST(guild_id AS TEXT) AS guild_id, \
                    CAST(owner_user_id AS TEXT) AS owner_user_id, \
                    CAST(created_at AS TEXT) AS created_at, \
                    CAST(last_empty_at AS TEXT) AS last_empty_at \
             FROM temp_voice_channels WHERE CAST(channel_id AS TEXT)=?",
        )
        .bind(channel_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| format!("failed to read temp_voice_channels {channel_id}: {err}"))?;

        row.map(|row| record_from_row(&row)).transpose()
    }

    pub async fn list_temp_channels(&self) -> Result<Vec<TempVoiceChannelRecord>, String> {
        let rows = sqlx::query(
            "SELECT CAST(channel_id AS TEXT) AS channel_id, \
                    CAST(guild_id AS TEXT) AS guild_id, \
                    CAST(owner_user_id AS TEXT) AS owner_user_id, \
                    CAST(created_at AS TEXT) AS created_at, \
                    CAST(last_empty_at AS TEXT) AS last_empty_at \
             FROM temp_voice_channels ORDER BY CAST(channel_id AS TEXT)",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list temp_voice_channels: {err}"))?;

        rows.iter().map(record_from_row).collect()
    }

    pub async fn list_temp_channels_by_guild(
        &self,
        guild_id: u64,
    ) -> Result<Vec<TempVoiceChannelRecord>, String> {
        let rows = sqlx::query(
            "SELECT CAST(channel_id AS TEXT) AS channel_id, \
                    CAST(guild_id AS TEXT) AS guild_id, \
                    CAST(owner_user_id AS TEXT) AS owner_user_id, \
                    CAST(created_at AS TEXT) AS created_at, \
                    CAST(last_empty_at AS TEXT) AS last_empty_at \
             FROM temp_voice_channels WHERE CAST(guild_id AS TEXT)=? \
             ORDER BY CAST(channel_id AS TEXT)",
        )
        .bind(guild_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|err| format!("failed to list temp_voice_channels for guild {guild_id}: {err}"))?;

        rows.iter().map(record_from_row).collect()
    }

    pub async fn execute_transaction(
        &self,
        plan: TempVoiceDbTransactionPlan,
    ) -> Result<(), String> {
        self.ensure_writable()?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|err| format!("failed to begin temp voice DB transaction: {err}"))?;
        let now = utc_timestamp();

        for mutation in plan.mutations {
            match mutation {
                TempVoiceDbMutation::UpsertHub {
                    guild_id,
                    hub_channel_id,
                } => {
                    sqlx::query(
                        "INSERT INTO guild_settings (guild_id, hub_channel_id, updated_at) \
                         VALUES (?, ?, ?) \
                         ON CONFLICT(guild_id) DO UPDATE SET \
                         hub_channel_id=excluded.hub_channel_id, \
                         updated_at=excluded.updated_at",
                    )
                    .bind(guild_id.to_string())
                    .bind(hub_channel_id.to_string())
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(|err| format!("failed to upsert guild_settings: {err}"))?;
                }
                TempVoiceDbMutation::InsertTempChannel {
                    channel_id,
                    guild_id,
                    owner_user_id,
                } => {
                    sqlx::query(
                        "INSERT OR REPLACE INTO temp_voice_channels \
                         (channel_id, guild_id, owner_user_id, created_at, last_empty_at) \
                         VALUES (?, ?, ?, ?, NULL)",
                    )
                    .bind(channel_id.to_string())
                    .bind(guild_id.to_string())
                    .bind(owner_user_id.to_string())
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(|err| format!("failed to insert temp_voice_channels: {err}"))?;
                }
                TempVoiceDbMutation::DeleteTrackedChannel { channel_id } => {
                    sqlx::query("DELETE FROM temp_voice_channels WHERE CAST(channel_id AS TEXT)=?")
                        .bind(channel_id.to_string())
                        .execute(&mut *tx)
                        .await
                        .map_err(|err| {
                            format!("failed to delete temp_voice_channels {channel_id}: {err}")
                        })?;
                }
            }
        }

        tx.commit()
            .await
            .map_err(|err| format!("failed to commit temp voice DB transaction: {err}"))?;
        Ok(())
    }

    fn ensure_writable(&self) -> Result<(), String> {
        if self.writes_enabled {
            Ok(())
        } else {
            Err("temp voice repository was opened read-only; writes are disabled".to_owned())
        }
    }

    #[cfg(test)]
    pub async fn create_schema_for_tests(&self) -> Result<(), String> {
        sqlx::query(
            "CREATE TABLE guild_settings (
                guild_id TEXT PRIMARY KEY,
                hub_channel_id TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create guild_settings fixture: {err}"))?;
        sqlx::query(
            "CREATE TABLE temp_voice_channels (
                channel_id TEXT PRIMARY KEY,
                guild_id TEXT NOT NULL,
                owner_user_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_empty_at TEXT
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|err| format!("failed to create temp_voice_channels fixture: {err}"))?;
        Ok(())
    }
}

fn record_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TempVoiceChannelRecord, String> {
    Ok(TempVoiceChannelRecord {
        channel_id: parse_u64_column(row, "channel_id")?,
        guild_id: parse_u64_column(row, "guild_id")?,
        owner_user_id: parse_u64_column(row, "owner_user_id")?,
        created_at: row
            .try_get::<Option<String>, _>("created_at")
            .map_err(|err| format!("failed to read created_at: {err}"))?
            .unwrap_or_default(),
        last_empty_at: row
            .try_get::<Option<String>, _>("last_empty_at")
            .map_err(|err| format!("failed to read last_empty_at: {err}"))?
            .filter(|value| !value.is_empty()),
    })
}

fn parse_u64_column(row: &sqlx::sqlite::SqliteRow, name: &str) -> Result<u64, String> {
    row.try_get::<String, _>(name)
        .map_err(|err| format!("failed to read {name}: {err}"))?
        .parse::<u64>()
        .map_err(|err| format!("failed to parse {name} as snowflake: {err}"))
}

fn utc_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}
