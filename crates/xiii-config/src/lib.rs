use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use thiserror::Error;
use xiii_core::Report;

pub mod env;
pub mod model;
pub mod redaction;
pub mod validation;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read env file {path}: {message}")]
    EnvFile { path: PathBuf, message: String },
    #[error("invalid integer for {name}: {value}")]
    InvalidInteger { name: String, value: String },
    #[error("invalid bool for {name}: {value}")]
    InvalidBool { name: String, value: String },
    #[error("missing required env var {name}")]
    MissingRequired { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretStatus {
    Missing,
    Empty,
    Set,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretValue {
    pub status: SecretStatus,
}

impl SecretValue {
    pub fn missing() -> Self {
        Self {
            status: SecretStatus::Missing,
        }
    }

    pub fn from_value(value: Option<&str>) -> Self {
        match value {
            Some(value) if value.trim().is_empty() => Self {
                status: SecretStatus::Empty,
            },
            Some(_) => Self {
                status: SecretStatus::Set,
            },
            None => Self::missing(),
        }
    }

    pub fn redacted(&self) -> &'static str {
        match self.status {
            SecretStatus::Missing => "<MISSING>",
            SecretStatus::Empty => "<EMPTY>",
            SecretStatus::Set => "<SET>",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigPath {
    pub raw: String,
    pub resolved: PathBuf,
}

impl ConfigPath {
    pub fn new(raw: impl Into<String>, base_dir: &Path) -> Self {
        let raw = raw.into();
        let raw_path = PathBuf::from(&raw);
        let resolved = if raw_path.is_absolute() {
            raw_path
        } else {
            base_dir.join(raw_path)
        };
        Self { raw, resolved }
    }

    pub fn display(&self) -> String {
        format!("{} -> {}", self.raw, self.resolved.display())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    pub discord_token: SecretValue,
    pub discord_client_id: Option<u64>,
    pub guild_id: u64,
    pub command_sync_guild_id: Option<u64>,
    pub sync_commands_on_startup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleToggles {
    pub tickets: bool,
    pub voice_activity: bool,
    pub recruit: bool,
    pub vacation: bool,
    pub discipline: bool,
    pub clanlist: bool,
    pub temp_voice: bool,
}

impl Default for ModuleToggles {
    fn default() -> Self {
        Self {
            tickets: false,
            voice_activity: false,
            recruit: false,
            vacation: false,
            discipline: false,
            clanlist: false,
            temp_voice: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyPathConfig {
    pub ticket_db: ConfigPath,
    pub voice_db: ConfigPath,
    pub recruit_db: ConfigPath,
    pub vacation_db: ConfigPath,
    pub discipline_db: ConfigPath,
    pub temp_voice_db: ConfigPath,
    pub clanlist_data_dir: ConfigPath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketConfig {
    pub panel_channel_id: u64,
    pub open_category_id: u64,
    pub transcript_channel_id: u64,
    pub officer_review_channel_id: u64,
    pub support_role_id: u64,
    pub global_moderator_role_ids: Vec<u64>,
    pub custom_command_role_ids: Vec<u64>,
    pub application_ping_role_id: u64,
    pub other_ping_role_id: u64,
    pub idea_ping_role_id: u64,
    pub accept_role_ids: Vec<u64>,
    pub google_credentials_file: SecretValue,
    pub google_sheet_id: SecretValue,
    pub google_sheet_name: SecretValue,
    pub google_poll_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceActivityConfig {
    pub stats_panel_channel_id: u64,
    pub inactive_command_channel_id: u64,
    pub auto_report_channel_id: u64,
    pub inactive_role_id: u64,
    pub vacation_marker_role_id: u64,
    pub ignored_channel_ids: Vec<u64>,
    pub heartbeat_interval_seconds: u64,
    pub public_stats_update_interval_seconds: u64,
    pub auto_report_check_interval_seconds: u64,
    pub auto_report_send_on_first_start: bool,
    pub public_stats_panel_enabled: bool,
    pub auto_reports_enabled: bool,
    pub page_size: u64,
    pub enable_prefix_commands: bool,
    pub command_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecruitConfig {
    pub recruit_role_id: u64,
    pub clan_member_role_id: u64,
    pub guest_role_id: u64,
    pub next_rank_role_id: u64,
    pub decision_channel_id: u64,
    pub decision_ping_role_ids: Vec<u64>,
    pub excluded_voice_channel_id: Option<u64>,
    pub default_days: u64,
    pub check_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacationConfig {
    pub panel_channel_id: u64,
    pub officer_channel_id: u64,
    pub active_panel_channel_id: u64,
    pub vacation_role_id: u64,
    pub officer_ping_role_id: Option<u64>,
    pub max_days: u64,
    pub brand_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisciplineConfig {
    pub board_channel_id: u64,
    pub log_channel_id: u64,
    pub main_clan_role_id: u64,
    pub composition_role_ids: Vec<u64>,
    pub guest_role_id: u64,
    pub officer_role_ids: Vec<u64>,
    pub timeout_minutes: u64,
    pub warning_expires_days: u64,
    pub verbal_expires_days: u64,
    pub board_refresh_seconds: u64,
    pub log_expirations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClanlistConfig {
    pub main_channel_id: u64,
    pub admin_channel_id: u64,
    pub steam_channel_id: u64,
    pub main_role_ids: Vec<u64>,
    pub admin_role_ids: Vec<u64>,
    pub steam_active_role_id: u64,
    pub update_debounce_seconds: u64,
    pub auto_refresh_seconds: u64,
    pub bootstrap_scan_limit: u64,
    pub edit_sleep_seconds: f64,
    pub send_sleep_seconds: f64,
    pub google_service_account_file: SecretValue,
    pub google_sheet_id: SecretValue,
    pub google_worksheet_name: SecretValue,
    pub google_fetch_min_interval_seconds: u64,
    pub google_steam_id_column: String,
    pub google_discord_id_column: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempVoiceConfig {
    pub delete_after_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub log_level: String,
    pub timezone: String,
    pub database_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperbotConfig {
    pub core: CoreConfig,
    pub modules: ModuleToggles,
    pub legacy_paths: LegacyPathConfig,
    pub tickets: TicketConfig,
    pub voice_activity: VoiceActivityConfig,
    pub recruit: RecruitConfig,
    pub vacation: VacationConfig,
    pub discipline: DisciplineConfig,
    pub clanlist: ClanlistConfig,
    pub temp_voice: TempVoiceConfig,
    pub runtime: RuntimeConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedConfigEntry {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigLoad {
    pub env_file: Option<PathBuf>,
    pub config: SuperbotConfig,
    pub entries: Vec<RedactedConfigEntry>,
    pub report: Report,
}

impl SuperbotConfig {
    pub fn skeleton() -> Self {
        let base_dir = PathBuf::from(".");
        Self {
            core: CoreConfig {
                discord_token: SecretValue::missing(),
                discord_client_id: Some(1_501_644_078_012_694_558),
                guild_id: 1_498_022_112_114_249_819,
                command_sync_guild_id: None,
                sync_commands_on_startup: false,
            },
            modules: ModuleToggles::default(),
            legacy_paths: LegacyPathConfig {
                ticket_db: ConfigPath::new("../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot/tickets.db", &base_dir),
                voice_db: ConfigPath::new("../XIII_BOTS_FULL_COPY/opt/XIII/xiii-voice-activity-bot/data/voice_activity.sqlite3", &base_dir),
                recruit_db: ConfigPath::new("../XIII_BOTS_FULL_COPY/opt/XIII/xiii-recruit-bot/data/recruits.db", &base_dir),
                vacation_db: ConfigPath::new("../XIII_BOTS_FULL_COPY/opt/XIII/xiii-vacation-bot/data/vacations.db", &base_dir),
                discipline_db: ConfigPath::new("../XIII_BOTS_FULL_COPY/opt/XIII/xiii-discipline-bot/data/discipline.sqlite", &base_dir),
                temp_voice_db: ConfigPath::new("../XIII_BOTS_FULL_COPY/opt/XIII/temp-voice-bot/data/bot.sqlite3", &base_dir),
                clanlist_data_dir: ConfigPath::new("../XIII_BOTS_FULL_COPY/opt/XIII/XIII-clanlist/data", &base_dir),
            },
            tickets: TicketConfig {
                panel_channel_id: 1_498_081_266_568_921_152,
                open_category_id: 1_498_793_152_646_086_656,
                transcript_channel_id: 1_498_088_262_483_316_817,
                officer_review_channel_id: 1_500_136_438_791_147_651,
                support_role_id: 1_498_057_076_151_422_976,
                global_moderator_role_ids: vec![
                    1_498_091_840_899_911_690,
                    1_498_022_112_131_289_217,
                    1_498_022_112_131_289_216,
                ],
                custom_command_role_ids: vec![
                    1_498_022_112_131_289_216,
                    1_498_022_112_131_289_217,
                ],
                application_ping_role_id: 1_498_057_076_151_422_976,
                other_ping_role_id: 1_498_057_076_151_422_976,
                idea_ping_role_id: 1_498_091_840_899_911_690,
                accept_role_ids: vec![1_498_022_112_114_249_828, 1_498_022_112_114_249_827],
                google_credentials_file: SecretValue::missing(),
                google_sheet_id: SecretValue::missing(),
                google_sheet_name: SecretValue::missing(),
                google_poll_seconds: 30,
            },
            voice_activity: VoiceActivityConfig {
                stats_panel_channel_id: 1_500_963_695_327_707_236,
                inactive_command_channel_id: 1_499_669_325_685_198_888,
                auto_report_channel_id: 1_499_770_822_938_722_354,
                inactive_role_id: 1_498_022_112_114_249_827,
                vacation_marker_role_id: 1_498_113_605_768_314_921,
                ignored_channel_ids: vec![1_498_022_116_682_104_914],
                heartbeat_interval_seconds: 60,
                public_stats_update_interval_seconds: 60,
                auto_report_check_interval_seconds: 600,
                auto_report_send_on_first_start: false,
                public_stats_panel_enabled: true,
                auto_reports_enabled: true,
                page_size: 10,
                enable_prefix_commands: false,
                command_prefix: "!".to_owned(),
            },
            recruit: RecruitConfig {
                recruit_role_id: 1_498_022_112_114_249_828,
                clan_member_role_id: 1_498_022_112_114_249_827,
                guest_role_id: 1_498_022_112_114_249_825,
                next_rank_role_id: 1_498_022_112_131_289_208,
                decision_channel_id: 1_500_136_438_791_147_651,
                decision_ping_role_ids: vec![
                    1_498_091_840_899_911_690,
                    1_498_057_076_151_422_976,
                ],
                excluded_voice_channel_id: Some(1_498_022_116_682_104_914),
                default_days: 14,
                check_interval_seconds: 300,
            },
            vacation: VacationConfig {
                panel_channel_id: 1_500_437_958_375_903_232,
                officer_channel_id: 1_500_438_001_514_184_714,
                active_panel_channel_id: 1_501_256_029_399_285_810,
                vacation_role_id: 1_498_022_112_131_289_214,
                officer_ping_role_id: Some(1_498_091_840_899_911_690),
                max_days: 100,
                brand_name: "XIII".to_owned(),
            },
            discipline: DisciplineConfig {
                board_channel_id: 1_501_216_257_859_649_646,
                log_channel_id: 1_501_216_301_191_004_363,
                main_clan_role_id: 1_498_022_112_114_249_827,
                composition_role_ids: vec![
                    1_498_022_112_114_249_827,
                    1_498_022_112_131_289_208,
                    1_498_022_112_131_289_209,
                    1_498_022_112_114_249_828,
                ],
                guest_role_id: 1_498_022_112_114_249_825,
                officer_role_ids: vec![
                    1_498_022_112_131_289_216,
                    1_498_022_112_131_289_217,
                ],
                timeout_minutes: 45,
                warning_expires_days: 7,
                verbal_expires_days: 14,
                board_refresh_seconds: 60,
                log_expirations: false,
            },
            clanlist: ClanlistConfig {
                main_channel_id: 1_498_762_828_666_896_535,
                admin_channel_id: 1_498_763_049_102_868_672,
                steam_channel_id: 1_500_081_418_506_862_754,
                main_role_ids: vec![
                    1_498_022_112_131_289_217,
                    1_498_022_112_131_289_216,
                    1_498_022_112_131_289_215,
                    1_498_022_112_131_289_209,
                    1_498_022_112_131_289_208,
                    1_498_022_112_114_249_828,
                ],
                admin_role_ids: vec![
                    1_498_022_112_131_289_217,
                    1_498_057_076_151_422_976,
                    1_498_091_840_899_911_690,
                    1_498_091_694_456_049_994,
                ],
                steam_active_role_id: 1_498_022_112_114_249_827,
                update_debounce_seconds: 5,
                auto_refresh_seconds: 600,
                bootstrap_scan_limit: 2000,
                edit_sleep_seconds: 0.55,
                send_sleep_seconds: 0.85,
                google_service_account_file: SecretValue::missing(),
                google_sheet_id: SecretValue::missing(),
                google_worksheet_name: SecretValue::missing(),
                google_fetch_min_interval_seconds: 60,
                google_steam_id_column: "D".to_owned(),
                google_discord_id_column: "E".to_owned(),
            },
            temp_voice: TempVoiceConfig {
                delete_after_seconds: 0,
            },
            runtime: RuntimeConfig {
                log_level: "info".to_owned(),
                timezone: "Europe/Berlin".to_owned(),
                database_url: "sqlite:data/xiii_superbot.sqlite3".to_owned(),
            },
        }
    }

    pub fn load_from_env_file(path: impl AsRef<Path>) -> Result<ConfigLoad, ConfigError> {
        let path = path.as_ref();
        let base_dir = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let env = read_dotenv_file(path)?;
        build_config(Some(path.to_path_buf()), base_dir, env)
    }

    pub fn load_from_env_str(contents: &str) -> Result<ConfigLoad, ConfigError> {
        let env = parse_env_string(contents);
        build_config(None, PathBuf::from("."), env)
    }

    pub fn redacted_discord_token_status(&self) -> &'static str {
        self.core.discord_token.redacted()
    }
}

pub fn load_local_dotenv_for_development() -> Result<(), dotenvy::Error> {
    dotenvy::dotenv().map(|_| ())
}

pub fn is_secret_like_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PRIVATE_KEY",
        "WEBHOOK",
        "CREDENTIAL",
        "API_KEY",
        "GOOGLE_SHEET",
        "GOOGLE_WORKSHEET",
        "GOOGLE_SERVICE_ACCOUNT",
    ]
    .iter()
    .any(|needle| upper.contains(needle))
}

pub fn parse_snowflake_value(name: &str, value: &str) -> Result<u64, ConfigError> {
    let trimmed = value.trim();
    let parsed = trimmed
        .parse::<u64>()
        .map_err(|_| ConfigError::InvalidInteger {
            name: name.to_owned(),
            value: value.to_owned(),
        })?;
    if parsed == 0 {
        return Err(ConfigError::InvalidInteger {
            name: name.to_owned(),
            value: value.to_owned(),
        });
    }
    Ok(parsed)
}

fn read_dotenv_file(path: &Path) -> Result<BTreeMap<String, String>, ConfigError> {
    let iter = dotenvy::from_path_iter(path).map_err(|_err| ConfigError::EnvFile {
        path: path.to_path_buf(),
        message: "unable to read or parse dotenv file; raw line suppressed".to_owned(),
    })?;
    let mut values = BTreeMap::new();
    for item in iter {
        let (key, value) = item.map_err(|_err| ConfigError::EnvFile {
            path: path.to_path_buf(),
            message: "unable to parse dotenv entry; raw line suppressed".to_owned(),
        })?;
        values.insert(key, value);
    }
    Ok(values)
}

fn parse_env_string(contents: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            values.insert(key.trim().to_owned(), unquote(value.trim()));
        }
    }
    values
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

fn build_config(
    env_file: Option<PathBuf>,
    base_dir: PathBuf,
    env: BTreeMap<String, String>,
) -> Result<ConfigLoad, ConfigError> {
    let mut report = Report::new();
    let skeleton = SuperbotConfig::skeleton();

    if env.contains_key("DATABASE_PATH") {
        report.fail(
            "config",
            "DATABASE_PATH is a generic legacy env name and must not be used globally; use LEGACY_*_DB_PATH",
        );
    }
    if env.contains_key("TICKET_ENABLED") {
        report.warn(
            "config",
            "TICKET_ENABLED was found; the unified variable is TICKETS_ENABLED",
        );
    }

    let modules = ModuleToggles {
        tickets: read_bool(&env, "TICKETS_ENABLED", skeleton.modules.tickets)?,
        voice_activity: read_bool(
            &env,
            "VOICE_ACTIVITY_ENABLED",
            skeleton.modules.voice_activity,
        )?,
        recruit: read_bool(&env, "RECRUIT_ENABLED", skeleton.modules.recruit)?,
        vacation: read_bool(&env, "VACATION_ENABLED", skeleton.modules.vacation)?,
        discipline: read_bool(&env, "DISCIPLINE_ENABLED", skeleton.modules.discipline)?,
        clanlist: read_bool(&env, "CLANLIST_ENABLED", skeleton.modules.clanlist)?,
        temp_voice: read_bool(&env, "TEMP_VOICE_ENABLED", skeleton.modules.temp_voice)?,
    };

    let core = CoreConfig {
        discord_token: SecretValue::from_value(env.get("DISCORD_TOKEN").map(String::as_str)),
        discord_client_id: read_optional_snowflake_with_default(
            &env,
            "DISCORD_CLIENT_ID",
            skeleton.core.discord_client_id,
        )?,
        guild_id: read_required_snowflake(&env, "XIII_GUILD_ID")?,
        command_sync_guild_id: read_optional_snowflake(&env, "DISCORD_COMMAND_SYNC_GUILD_ID")?,
        sync_commands_on_startup: read_bool(
            &env,
            "DISCORD_SYNC_COMMANDS_ON_STARTUP",
            skeleton.core.sync_commands_on_startup,
        )?,
    };

    if core.discord_token.status == SecretStatus::Set {
        report.ok("config", "DISCORD_TOKEN status = <SET>");
    } else {
        report.warn(
            "config",
            format!("DISCORD_TOKEN status = {}", core.discord_token.redacted()),
        );
    }
    if core.discord_client_id.is_some() {
        report.ok("config", "DISCORD_CLIENT_ID parsed");
    } else {
        report.warn(
            "config",
            "DISCORD_CLIENT_ID is not set; command registration will need it later",
        );
    }
    report.ok("config", format!("XIII_GUILD_ID = {}", core.guild_id));

    let legacy_paths = LegacyPathConfig {
        ticket_db: read_path(
            &env,
            "LEGACY_TICKET_DB_PATH",
            &skeleton.legacy_paths.ticket_db.raw,
            &base_dir,
        ),
        voice_db: read_path(
            &env,
            "LEGACY_VOICE_DB_PATH",
            &skeleton.legacy_paths.voice_db.raw,
            &base_dir,
        ),
        recruit_db: read_path(
            &env,
            "LEGACY_RECRUIT_DB_PATH",
            &skeleton.legacy_paths.recruit_db.raw,
            &base_dir,
        ),
        vacation_db: read_path(
            &env,
            "LEGACY_VACATION_DB_PATH",
            &skeleton.legacy_paths.vacation_db.raw,
            &base_dir,
        ),
        discipline_db: read_path(
            &env,
            "LEGACY_DISCIPLINE_DB_PATH",
            &skeleton.legacy_paths.discipline_db.raw,
            &base_dir,
        ),
        temp_voice_db: read_path(
            &env,
            "LEGACY_TEMP_VOICE_DB_PATH",
            &skeleton.legacy_paths.temp_voice_db.raw,
            &base_dir,
        ),
        clanlist_data_dir: read_path(
            &env,
            "LEGACY_CLANLIST_DATA_DIR",
            &skeleton.legacy_paths.clanlist_data_dir.raw,
            &base_dir,
        ),
    };

    validate_path(
        &mut report,
        "tickets",
        &legacy_paths.ticket_db,
        modules.tickets,
        false,
    );
    validate_path(
        &mut report,
        "voice_activity",
        &legacy_paths.voice_db,
        modules.voice_activity,
        false,
    );
    validate_path(
        &mut report,
        "recruit",
        &legacy_paths.recruit_db,
        modules.recruit,
        false,
    );
    validate_path(
        &mut report,
        "vacation",
        &legacy_paths.vacation_db,
        modules.vacation,
        false,
    );
    validate_path(
        &mut report,
        "discipline",
        &legacy_paths.discipline_db,
        modules.discipline,
        false,
    );
    validate_path(
        &mut report,
        "temp_voice",
        &legacy_paths.temp_voice_db,
        modules.temp_voice,
        false,
    );
    validate_path(
        &mut report,
        "clanlist",
        &legacy_paths.clanlist_data_dir,
        modules.clanlist,
        true,
    );

    let vacation_role_id = read_snowflake_or_default(
        &env,
        "VACATION_ROLE_ID",
        skeleton.vacation.vacation_role_id,
        modules.vacation,
        &mut report,
    )?;
    let voice_vacation_marker_role_id = read_snowflake_or_default(
        &env,
        "VOICE_VACATION_MARKER_ROLE_ID",
        skeleton.voice_activity.vacation_marker_role_id,
        modules.voice_activity,
        &mut report,
    )?;

    if vacation_role_id == voice_vacation_marker_role_id {
        report.fail(
            "config",
            "VACATION_ROLE_ID and VOICE_VACATION_MARKER_ROLE_ID resolve to the same value; these are distinct legacy roles",
        );
    } else {
        report.ok(
            "config",
            "VACATION_ROLE_ID and VOICE_VACATION_MARKER_ROLE_ID are distinct",
        );
    }

    let tickets = TicketConfig {
        panel_channel_id: read_snowflake_or_default(
            &env,
            "TICKET_PANEL_CHANNEL_ID",
            skeleton.tickets.panel_channel_id,
            modules.tickets,
            &mut report,
        )?,
        open_category_id: read_snowflake_or_default(
            &env,
            "TICKET_OPEN_CATEGORY_ID",
            skeleton.tickets.open_category_id,
            modules.tickets,
            &mut report,
        )?,
        transcript_channel_id: read_snowflake_or_default(
            &env,
            "TICKET_TRANSCRIPT_CHANNEL_ID",
            skeleton.tickets.transcript_channel_id,
            modules.tickets,
            &mut report,
        )?,
        officer_review_channel_id: read_snowflake_or_default(
            &env,
            "TICKET_OFFICER_REVIEW_CHANNEL_ID",
            skeleton.tickets.officer_review_channel_id,
            modules.tickets,
            &mut report,
        )?,
        support_role_id: read_snowflake_or_default(
            &env,
            "TICKET_SUPPORT_ROLE_ID",
            skeleton.tickets.support_role_id,
            modules.tickets,
            &mut report,
        )?,
        global_moderator_role_ids: read_snowflake_list_or_default(
            &env,
            "TICKET_GLOBAL_MODERATOR_ROLE_IDS",
            &skeleton.tickets.global_moderator_role_ids,
        )?,
        custom_command_role_ids: read_snowflake_list_or_default(
            &env,
            "TICKET_CUSTOM_COMMAND_ROLE_IDS",
            &skeleton.tickets.custom_command_role_ids,
        )?,
        application_ping_role_id: read_snowflake_or_default(
            &env,
            "TICKET_APPLICATION_PING_ROLE_ID",
            skeleton.tickets.application_ping_role_id,
            modules.tickets,
            &mut report,
        )?,
        other_ping_role_id: read_snowflake_or_default(
            &env,
            "TICKET_OTHER_PING_ROLE_ID",
            skeleton.tickets.other_ping_role_id,
            modules.tickets,
            &mut report,
        )?,
        idea_ping_role_id: read_snowflake_or_default(
            &env,
            "TICKET_IDEA_PING_ROLE_ID",
            skeleton.tickets.idea_ping_role_id,
            modules.tickets,
            &mut report,
        )?,
        accept_role_ids: read_snowflake_list_or_default(
            &env,
            "TICKET_ACCEPT_ROLE_IDS",
            &skeleton.tickets.accept_role_ids,
        )?,
        google_credentials_file: SecretValue::from_value(
            env.get("TICKET_GOOGLE_CREDENTIALS_FILE")
                .map(String::as_str),
        ),
        google_sheet_id: SecretValue::from_value(
            env.get("TICKET_GOOGLE_SHEET_ID").map(String::as_str),
        ),
        google_sheet_name: SecretValue::from_value(
            env.get("TICKET_GOOGLE_SHEET_NAME").map(String::as_str),
        ),
        google_poll_seconds: read_u64(
            &env,
            "TICKET_GOOGLE_POLL_SECONDS",
            skeleton.tickets.google_poll_seconds,
        )?,
    };

    let voice_activity = VoiceActivityConfig {
        stats_panel_channel_id: read_snowflake_or_default(
            &env,
            "VOICE_STATS_PANEL_CHANNEL_ID",
            skeleton.voice_activity.stats_panel_channel_id,
            modules.voice_activity,
            &mut report,
        )?,
        inactive_command_channel_id: read_snowflake_or_default(
            &env,
            "VOICE_INACTIVE_COMMAND_CHANNEL_ID",
            skeleton.voice_activity.inactive_command_channel_id,
            modules.voice_activity,
            &mut report,
        )?,
        auto_report_channel_id: read_snowflake_or_default(
            &env,
            "VOICE_AUTO_REPORT_CHANNEL_ID",
            skeleton.voice_activity.auto_report_channel_id,
            modules.voice_activity,
            &mut report,
        )?,
        inactive_role_id: read_snowflake_or_default(
            &env,
            "VOICE_INACTIVE_ROLE_ID",
            skeleton.voice_activity.inactive_role_id,
            modules.voice_activity,
            &mut report,
        )?,
        vacation_marker_role_id: voice_vacation_marker_role_id,
        ignored_channel_ids: read_snowflake_list_or_default(
            &env,
            "VOICE_IGNORED_CHANNEL_IDS",
            &skeleton.voice_activity.ignored_channel_ids,
        )?,
        heartbeat_interval_seconds: read_u64(
            &env,
            "VOICE_HEARTBEAT_INTERVAL_SECONDS",
            skeleton.voice_activity.heartbeat_interval_seconds,
        )?,
        public_stats_update_interval_seconds: read_u64(
            &env,
            "VOICE_PUBLIC_STATS_UPDATE_INTERVAL_SECONDS",
            skeleton.voice_activity.public_stats_update_interval_seconds,
        )?,
        auto_report_check_interval_seconds: read_u64(
            &env,
            "VOICE_AUTO_REPORT_CHECK_INTERVAL_SECONDS",
            skeleton.voice_activity.auto_report_check_interval_seconds,
        )?,
        auto_report_send_on_first_start: read_bool(
            &env,
            "VOICE_AUTO_REPORT_SEND_ON_FIRST_START",
            skeleton.voice_activity.auto_report_send_on_first_start,
        )?,
        public_stats_panel_enabled: read_bool(
            &env,
            "VOICE_PUBLIC_STATS_PANEL_ENABLED",
            skeleton.voice_activity.public_stats_panel_enabled,
        )?,
        auto_reports_enabled: read_bool(
            &env,
            "VOICE_AUTO_REPORTS_ENABLED",
            skeleton.voice_activity.auto_reports_enabled,
        )?,
        page_size: read_u64(&env, "VOICE_PAGE_SIZE", skeleton.voice_activity.page_size)?,
        enable_prefix_commands: read_bool(
            &env,
            "VOICE_ENABLE_PREFIX_COMMANDS",
            skeleton.voice_activity.enable_prefix_commands,
        )?,
        command_prefix: read_string(
            &env,
            "VOICE_COMMAND_PREFIX",
            &skeleton.voice_activity.command_prefix,
        ),
    };

    let recruit = RecruitConfig {
        recruit_role_id: read_snowflake_or_default(
            &env,
            "XIII_RECRUIT_ROLE_ID",
            skeleton.recruit.recruit_role_id,
            modules.recruit,
            &mut report,
        )?,
        clan_member_role_id: read_snowflake_or_default(
            &env,
            "XIII_MEMBER_ROLE_ID",
            skeleton.recruit.clan_member_role_id,
            modules.recruit,
            &mut report,
        )?,
        guest_role_id: read_snowflake_or_default(
            &env,
            "XIII_GUEST_ROLE_ID",
            skeleton.recruit.guest_role_id,
            modules.recruit,
            &mut report,
        )?,
        next_rank_role_id: read_snowflake_or_default(
            &env,
            "XIII_NEXT_RANK_ROLE_ID",
            skeleton.recruit.next_rank_role_id,
            modules.recruit,
            &mut report,
        )?,
        decision_channel_id: read_snowflake_or_default(
            &env,
            "RECRUIT_DECISION_CHANNEL_ID",
            skeleton.recruit.decision_channel_id,
            modules.recruit,
            &mut report,
        )?,
        decision_ping_role_ids: read_snowflake_list_or_default(
            &env,
            "RECRUIT_DECISION_PING_ROLE_IDS",
            &skeleton.recruit.decision_ping_role_ids,
        )?,
        excluded_voice_channel_id: read_optional_snowflake_with_default(
            &env,
            "RECRUIT_EXCLUDED_VOICE_CHANNEL_ID",
            skeleton.recruit.excluded_voice_channel_id,
        )?,
        default_days: read_u64(&env, "RECRUIT_DEFAULT_DAYS", skeleton.recruit.default_days)?,
        check_interval_seconds: read_u64(
            &env,
            "RECRUIT_CHECK_INTERVAL_SECONDS",
            skeleton.recruit.check_interval_seconds,
        )?,
    };

    let vacation = VacationConfig {
        panel_channel_id: read_snowflake_or_default(
            &env,
            "VACATION_PANEL_CHANNEL_ID",
            skeleton.vacation.panel_channel_id,
            modules.vacation,
            &mut report,
        )?,
        officer_channel_id: read_snowflake_or_default(
            &env,
            "VACATION_OFFICER_CHANNEL_ID",
            skeleton.vacation.officer_channel_id,
            modules.vacation,
            &mut report,
        )?,
        active_panel_channel_id: read_snowflake_or_default(
            &env,
            "VACATION_ACTIVE_PANEL_CHANNEL_ID",
            skeleton.vacation.active_panel_channel_id,
            modules.vacation,
            &mut report,
        )?,
        vacation_role_id,
        officer_ping_role_id: read_optional_snowflake_with_default(
            &env,
            "VACATION_OFFICER_PING_ROLE_ID",
            skeleton.vacation.officer_ping_role_id,
        )?,
        max_days: read_u64(&env, "VACATION_MAX_DAYS", skeleton.vacation.max_days)?,
        brand_name: read_string(&env, "VACATION_BRAND_NAME", &skeleton.vacation.brand_name),
    };

    let discipline = DisciplineConfig {
        board_channel_id: read_snowflake_or_default(
            &env,
            "DISCIPLINE_BOARD_CHANNEL_ID",
            skeleton.discipline.board_channel_id,
            modules.discipline,
            &mut report,
        )?,
        log_channel_id: read_snowflake_or_default(
            &env,
            "DISCIPLINE_LOG_CHANNEL_ID",
            skeleton.discipline.log_channel_id,
            modules.discipline,
            &mut report,
        )?,
        main_clan_role_id: read_snowflake_or_default(
            &env,
            "DISCIPLINE_MAIN_CLAN_ROLE_ID",
            skeleton.discipline.main_clan_role_id,
            modules.discipline,
            &mut report,
        )?,
        composition_role_ids: read_snowflake_list_or_default(
            &env,
            "DISCIPLINE_COMPOSITION_ROLE_IDS",
            &skeleton.discipline.composition_role_ids,
        )?,
        guest_role_id: read_snowflake_or_default(
            &env,
            "XIII_GUEST_ROLE_ID",
            skeleton.discipline.guest_role_id,
            modules.discipline,
            &mut report,
        )?,
        officer_role_ids: read_snowflake_list_or_default(
            &env,
            "XIII_OFFICER_ROLE_IDS",
            &skeleton.discipline.officer_role_ids,
        )?,
        timeout_minutes: read_u64(
            &env,
            "DISCIPLINE_TIMEOUT_MINUTES",
            skeleton.discipline.timeout_minutes,
        )?,
        warning_expires_days: read_u64(
            &env,
            "DISCIPLINE_WARNING_EXPIRES_DAYS",
            skeleton.discipline.warning_expires_days,
        )?,
        verbal_expires_days: read_u64(
            &env,
            "DISCIPLINE_VERBAL_EXPIRES_DAYS",
            skeleton.discipline.verbal_expires_days,
        )?,
        board_refresh_seconds: read_u64(
            &env,
            "DISCIPLINE_BOARD_REFRESH_SECONDS",
            skeleton.discipline.board_refresh_seconds,
        )?,
        log_expirations: read_bool(
            &env,
            "DISCIPLINE_LOG_EXPIRATIONS",
            skeleton.discipline.log_expirations,
        )?,
    };

    let clanlist = ClanlistConfig {
        main_channel_id: read_snowflake_or_default(
            &env,
            "CLANLIST_MAIN_CHANNEL_ID",
            skeleton.clanlist.main_channel_id,
            modules.clanlist,
            &mut report,
        )?,
        admin_channel_id: read_snowflake_or_default(
            &env,
            "CLANLIST_ADMIN_CHANNEL_ID",
            skeleton.clanlist.admin_channel_id,
            modules.clanlist,
            &mut report,
        )?,
        steam_channel_id: read_snowflake_or_default(
            &env,
            "CLANLIST_STEAM_CHANNEL_ID",
            skeleton.clanlist.steam_channel_id,
            modules.clanlist,
            &mut report,
        )?,
        main_role_ids: read_snowflake_list_or_default(
            &env,
            "CLANLIST_MAIN_ROLE_IDS",
            &skeleton.clanlist.main_role_ids,
        )?,
        admin_role_ids: read_snowflake_list_or_default(
            &env,
            "CLANLIST_ADMIN_ROLE_IDS",
            &skeleton.clanlist.admin_role_ids,
        )?,
        steam_active_role_id: read_snowflake_or_default(
            &env,
            "CLANLIST_STEAM_ACTIVE_ROLE_ID",
            skeleton.clanlist.steam_active_role_id,
            modules.clanlist,
            &mut report,
        )?,
        update_debounce_seconds: read_u64(
            &env,
            "CLANLIST_UPDATE_DEBOUNCE_SECONDS",
            skeleton.clanlist.update_debounce_seconds,
        )?,
        auto_refresh_seconds: read_u64(
            &env,
            "CLANLIST_AUTO_REFRESH_SECONDS",
            skeleton.clanlist.auto_refresh_seconds,
        )?,
        bootstrap_scan_limit: read_u64(
            &env,
            "CLANLIST_BOOTSTRAP_SCAN_LIMIT",
            skeleton.clanlist.bootstrap_scan_limit,
        )?,
        edit_sleep_seconds: read_f64(
            &env,
            "CLANLIST_EDIT_SLEEP_SECONDS",
            skeleton.clanlist.edit_sleep_seconds,
        )?,
        send_sleep_seconds: read_f64(
            &env,
            "CLANLIST_SEND_SLEEP_SECONDS",
            skeleton.clanlist.send_sleep_seconds,
        )?,
        google_service_account_file: SecretValue::from_value(
            env.get("CLANLIST_GOOGLE_SERVICE_ACCOUNT_FILE")
                .map(String::as_str),
        ),
        google_sheet_id: SecretValue::from_value(
            env.get("CLANLIST_GOOGLE_SHEET_ID").map(String::as_str),
        ),
        google_worksheet_name: SecretValue::from_value(
            env.get("CLANLIST_GOOGLE_WORKSHEET_NAME")
                .map(String::as_str),
        ),
        google_fetch_min_interval_seconds: read_u64(
            &env,
            "CLANLIST_GOOGLE_FETCH_MIN_INTERVAL_SECONDS",
            skeleton.clanlist.google_fetch_min_interval_seconds,
        )?,
        google_steam_id_column: read_string(
            &env,
            "CLANLIST_GOOGLE_STEAM_ID_COLUMN",
            &skeleton.clanlist.google_steam_id_column,
        ),
        google_discord_id_column: read_string(
            &env,
            "CLANLIST_GOOGLE_DISCORD_ID_COLUMN",
            &skeleton.clanlist.google_discord_id_column,
        ),
    };

    let temp_voice = TempVoiceConfig {
        delete_after_seconds: read_u64(
            &env,
            "TEMP_VOICE_DELETE_AFTER_SECONDS",
            skeleton.temp_voice.delete_after_seconds,
        )?,
    };

    let runtime = RuntimeConfig {
        log_level: read_string(&env, "LOG_LEVEL", &skeleton.runtime.log_level),
        timezone: read_string(&env, "TIMEZONE", &skeleton.runtime.timezone),
        database_url: read_string(&env, "DATABASE_URL", &skeleton.runtime.database_url),
    };

    for (name, enabled) in [
        ("TICKETS_ENABLED", modules.tickets),
        ("VOICE_ACTIVITY_ENABLED", modules.voice_activity),
        ("RECRUIT_ENABLED", modules.recruit),
        ("VACATION_ENABLED", modules.vacation),
        ("DISCIPLINE_ENABLED", modules.discipline),
        ("CLANLIST_ENABLED", modules.clanlist),
        ("TEMP_VOICE_ENABLED", modules.temp_voice),
    ] {
        report.ok("config", format!("{name} = {enabled}"));
    }

    let entries = redacted_entries(&env);

    Ok(ConfigLoad {
        env_file,
        config: SuperbotConfig {
            core,
            modules,
            legacy_paths,
            tickets,
            voice_activity,
            recruit,
            vacation,
            discipline,
            clanlist,
            temp_voice,
            runtime,
        },
        entries,
        report,
    })
}

fn read_required_snowflake(env: &BTreeMap<String, String>, name: &str) -> Result<u64, ConfigError> {
    let value = env.get(name).ok_or_else(|| ConfigError::MissingRequired {
        name: name.to_owned(),
    })?;
    parse_snowflake_value(name, value)
}

fn read_optional_snowflake(
    env: &BTreeMap<String, String>,
    name: &str,
) -> Result<Option<u64>, ConfigError> {
    match env.get(name) {
        Some(value) if !value.trim().is_empty() => parse_snowflake_value(name, value).map(Some),
        _ => Ok(None),
    }
}

fn read_optional_snowflake_with_default(
    env: &BTreeMap<String, String>,
    name: &str,
    default: Option<u64>,
) -> Result<Option<u64>, ConfigError> {
    match env.get(name) {
        Some(value) if !value.trim().is_empty() => parse_snowflake_value(name, value).map(Some),
        Some(_) => Ok(None),
        None => Ok(default),
    }
}

fn read_snowflake_or_default(
    env: &BTreeMap<String, String>,
    name: &str,
    default: u64,
    module_enabled: bool,
    report: &mut Report,
) -> Result<u64, ConfigError> {
    match env.get(name) {
        Some(value) if !value.trim().is_empty() => parse_snowflake_value(name, value),
        Some(_) => {
            if module_enabled {
                report.fail(
                    "config",
                    format!("{name} is empty but the module is enabled"),
                );
            } else {
                report.warn("config", format!("{name} is empty; using audit default"));
            }
            Ok(default)
        }
        None => {
            if module_enabled {
                report.fail(
                    "config",
                    format!("{name} is missing but the module is enabled"),
                );
            } else {
                report.warn("config", format!("{name} is missing; using audit default"));
            }
            Ok(default)
        }
    }
}

fn read_snowflake_list_or_default(
    env: &BTreeMap<String, String>,
    name: &str,
    default: &[u64],
) -> Result<Vec<u64>, ConfigError> {
    match env.get(name) {
        Some(value) if !value.trim().is_empty() => value
            .split(',')
            .filter(|part| !part.trim().is_empty())
            .map(|part| parse_snowflake_value(name, part))
            .collect(),
        _ => Ok(default.to_vec()),
    }
}

fn read_bool(
    env: &BTreeMap<String, String>,
    name: &str,
    default: bool,
) -> Result<bool, ConfigError> {
    match env.get(name) {
        Some(value) if !value.trim().is_empty() => match value.trim().to_ascii_lowercase().as_str()
        {
            "1" | "true" | "yes" | "y" | "on" => Ok(true),
            "0" | "false" | "no" | "n" | "off" => Ok(false),
            _ => Err(ConfigError::InvalidBool {
                name: name.to_owned(),
                value: value.to_owned(),
            }),
        },
        _ => Ok(default),
    }
}

fn read_u64(env: &BTreeMap<String, String>, name: &str, default: u64) -> Result<u64, ConfigError> {
    match env.get(name) {
        Some(value) if !value.trim().is_empty() => {
            value
                .trim()
                .parse::<u64>()
                .map_err(|_| ConfigError::InvalidInteger {
                    name: name.to_owned(),
                    value: value.to_owned(),
                })
        }
        _ => Ok(default),
    }
}

fn read_f64(env: &BTreeMap<String, String>, name: &str, default: f64) -> Result<f64, ConfigError> {
    match env.get(name) {
        Some(value) if !value.trim().is_empty() => {
            value
                .trim()
                .parse::<f64>()
                .map_err(|_| ConfigError::InvalidInteger {
                    name: name.to_owned(),
                    value: value.to_owned(),
                })
        }
        _ => Ok(default),
    }
}

fn read_string(env: &BTreeMap<String, String>, name: &str, default: &str) -> String {
    env.get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_owned())
}

fn read_path(
    env: &BTreeMap<String, String>,
    name: &str,
    default: &str,
    base_dir: &Path,
) -> ConfigPath {
    ConfigPath::new(
        env.get(name).map(String::as_str).unwrap_or(default),
        base_dir,
    )
}

fn validate_path(
    report: &mut Report,
    module: &str,
    path: &ConfigPath,
    module_enabled: bool,
    directory: bool,
) {
    let exists = if directory {
        path.resolved.is_dir()
    } else {
        path.resolved.is_file()
    };
    let kind = if directory { "directory" } else { "file" };
    if exists {
        report.ok(module, format!("legacy {kind} exists: {}", path.display()));
    } else if module_enabled {
        report.fail(
            module,
            format!(
                "legacy {kind} missing while module is enabled: {}",
                path.display()
            ),
        );
    } else {
        report.warn(
            module,
            format!(
                "legacy {kind} missing while module is disabled: {}",
                path.display()
            ),
        );
    }
}

fn redacted_entries(env: &BTreeMap<String, String>) -> Vec<RedactedConfigEntry> {
    let mut names: BTreeSet<String> = expected_env_names()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    names.extend(env.keys().cloned());
    names
        .into_iter()
        .map(|name| {
            let value = match env.get(&name) {
                Some(value) if is_secret_like_name(&name) => {
                    SecretValue::from_value(Some(value)).redacted().to_owned()
                }
                Some(value) if value.trim().is_empty() => "<EMPTY>".to_owned(),
                Some(value) => value.clone(),
                None => "<MISSING>".to_owned(),
            };
            RedactedConfigEntry { name, value }
        })
        .collect()
}

fn expected_env_names() -> &'static [&'static str] {
    &[
        "DISCORD_TOKEN",
        "DISCORD_CLIENT_ID",
        "XIII_GUILD_ID",
        "DISCORD_COMMAND_SYNC_GUILD_ID",
        "DISCORD_SYNC_COMMANDS_ON_STARTUP",
        "TICKETS_ENABLED",
        "VOICE_ACTIVITY_ENABLED",
        "RECRUIT_ENABLED",
        "VACATION_ENABLED",
        "DISCIPLINE_ENABLED",
        "CLANLIST_ENABLED",
        "TEMP_VOICE_ENABLED",
        "XIII_MEMBER_ROLE_ID",
        "XIII_GUEST_ROLE_ID",
        "XIII_RECRUIT_ROLE_ID",
        "XIII_NEXT_RANK_ROLE_ID",
        "XIII_OFFICER_ROLE_IDS",
        "TICKET_PANEL_CHANNEL_ID",
        "TICKET_OPEN_CATEGORY_ID",
        "TICKET_TRANSCRIPT_CHANNEL_ID",
        "TICKET_OFFICER_REVIEW_CHANNEL_ID",
        "TICKET_SUPPORT_ROLE_ID",
        "TICKET_GLOBAL_MODERATOR_ROLE_IDS",
        "TICKET_CUSTOM_COMMAND_ROLE_IDS",
        "TICKET_APPLICATION_PING_ROLE_ID",
        "TICKET_OTHER_PING_ROLE_ID",
        "TICKET_IDEA_PING_ROLE_ID",
        "TICKET_ACCEPT_ROLE_IDS",
        "TICKET_GOOGLE_CREDENTIALS_FILE",
        "TICKET_GOOGLE_SHEET_ID",
        "TICKET_GOOGLE_SHEET_NAME",
        "TICKET_GOOGLE_POLL_SECONDS",
        "VOICE_STATS_PANEL_CHANNEL_ID",
        "VOICE_INACTIVE_COMMAND_CHANNEL_ID",
        "VOICE_AUTO_REPORT_CHANNEL_ID",
        "VOICE_INACTIVE_ROLE_ID",
        "VOICE_VACATION_MARKER_ROLE_ID",
        "VOICE_IGNORED_CHANNEL_IDS",
        "VOICE_HEARTBEAT_INTERVAL_SECONDS",
        "VOICE_PUBLIC_STATS_UPDATE_INTERVAL_SECONDS",
        "VOICE_AUTO_REPORT_CHECK_INTERVAL_SECONDS",
        "VOICE_AUTO_REPORT_SEND_ON_FIRST_START",
        "VOICE_PUBLIC_STATS_PANEL_ENABLED",
        "VOICE_AUTO_REPORTS_ENABLED",
        "VOICE_PAGE_SIZE",
        "VOICE_ENABLE_PREFIX_COMMANDS",
        "VOICE_COMMAND_PREFIX",
        "RECRUIT_DECISION_CHANNEL_ID",
        "RECRUIT_DECISION_PING_ROLE_IDS",
        "RECRUIT_EXCLUDED_VOICE_CHANNEL_ID",
        "RECRUIT_DEFAULT_DAYS",
        "RECRUIT_CHECK_INTERVAL_SECONDS",
        "VACATION_PANEL_CHANNEL_ID",
        "VACATION_OFFICER_CHANNEL_ID",
        "VACATION_ACTIVE_PANEL_CHANNEL_ID",
        "VACATION_ROLE_ID",
        "VACATION_OFFICER_PING_ROLE_ID",
        "VACATION_MAX_DAYS",
        "VACATION_BRAND_NAME",
        "DISCIPLINE_BOARD_CHANNEL_ID",
        "DISCIPLINE_LOG_CHANNEL_ID",
        "DISCIPLINE_MAIN_CLAN_ROLE_ID",
        "DISCIPLINE_COMPOSITION_ROLE_IDS",
        "DISCIPLINE_TIMEOUT_MINUTES",
        "DISCIPLINE_WARNING_EXPIRES_DAYS",
        "DISCIPLINE_VERBAL_EXPIRES_DAYS",
        "DISCIPLINE_BOARD_REFRESH_SECONDS",
        "DISCIPLINE_LOG_EXPIRATIONS",
        "CLANLIST_MAIN_CHANNEL_ID",
        "CLANLIST_ADMIN_CHANNEL_ID",
        "CLANLIST_STEAM_CHANNEL_ID",
        "CLANLIST_MAIN_ROLE_IDS",
        "CLANLIST_ADMIN_ROLE_IDS",
        "CLANLIST_STEAM_ACTIVE_ROLE_ID",
        "CLANLIST_UPDATE_DEBOUNCE_SECONDS",
        "CLANLIST_AUTO_REFRESH_SECONDS",
        "CLANLIST_BOOTSTRAP_SCAN_LIMIT",
        "CLANLIST_EDIT_SLEEP_SECONDS",
        "CLANLIST_SEND_SLEEP_SECONDS",
        "CLANLIST_GOOGLE_SERVICE_ACCOUNT_FILE",
        "CLANLIST_GOOGLE_SHEET_ID",
        "CLANLIST_GOOGLE_WORKSHEET_NAME",
        "CLANLIST_GOOGLE_FETCH_MIN_INTERVAL_SECONDS",
        "CLANLIST_GOOGLE_STEAM_ID_COLUMN",
        "CLANLIST_GOOGLE_DISCORD_ID_COLUMN",
        "TEMP_VOICE_DELETE_AFTER_SECONDS",
        "LEGACY_TICKET_DB_PATH",
        "LEGACY_VOICE_DB_PATH",
        "LEGACY_RECRUIT_DB_PATH",
        "LEGACY_VACATION_DB_PATH",
        "LEGACY_DISCIPLINE_DB_PATH",
        "LEGACY_TEMP_VOICE_DB_PATH",
        "LEGACY_CLANLIST_DATA_DIR",
        "DATABASE_URL",
        "TIMEZONE",
        "LOG_LEVEL",
    ]
}

#[cfg(test)]
mod tests {
    use super::{is_secret_like_name, parse_snowflake_value, SecretStatus, SuperbotConfig};

    const MINIMAL_ENV: &str = r#"
DISCORD_TOKEN=not-a-real-token
DISCORD_CLIENT_ID=1501644078012694558
XIII_GUILD_ID=1498022112114249819
TICKETS_ENABLED=false
VOICE_ACTIVITY_ENABLED=false
RECRUIT_ENABLED=false
VACATION_ENABLED=false
DISCIPLINE_ENABLED=false
CLANLIST_ENABLED=false
TEMP_VOICE_ENABLED=false
VACATION_ROLE_ID=1498022112131289214
VOICE_VACATION_MARKER_ROLE_ID=1498113605768314921
"#;

    #[test]
    fn parses_valid_discord_snowflake() {
        assert_eq!(
            parse_snowflake_value("XIII_GUILD_ID", "1498022112114249819").unwrap(),
            1_498_022_112_114_249_819
        );
        assert!(parse_snowflake_value("XIII_GUILD_ID", "not-a-snowflake").is_err());
    }

    #[test]
    fn redacts_secret_like_names() {
        assert!(is_secret_like_name("DISCORD_TOKEN"));
        assert!(is_secret_like_name("GOOGLE_CREDENTIALS_FILE"));
        assert!(is_secret_like_name("CLANLIST_GOOGLE_SHEET_ID"));
        assert!(!is_secret_like_name("XIII_GUILD_ID"));
    }

    #[test]
    fn module_toggles_default_to_disabled() {
        let load = SuperbotConfig::load_from_env_str(MINIMAL_ENV).unwrap();
        assert!(!load.config.modules.tickets);
        assert!(!load.config.modules.voice_activity);
        assert!(!load.config.modules.recruit);
        assert!(!load.config.modules.vacation);
        assert!(!load.config.modules.discipline);
        assert!(!load.config.modules.clanlist);
        assert!(!load.config.modules.temp_voice);
    }

    #[test]
    fn detects_vacation_role_collapse() {
        let env = r#"
DISCORD_TOKEN=not-a-real-token
XIII_GUILD_ID=1498022112114249819
VACATION_ENABLED=true
VOICE_ACTIVITY_ENABLED=true
VACATION_ROLE_ID=1498022112131289214
VOICE_VACATION_MARKER_ROLE_ID=1498022112131289214
"#;
        let load = SuperbotConfig::load_from_env_str(env).unwrap();
        assert!(load.report.has_failures());
    }

    #[test]
    fn loads_config_from_sample_env_string_without_exposing_secret() {
        let load = SuperbotConfig::load_from_env_str(MINIMAL_ENV).unwrap();
        assert_eq!(load.config.core.discord_token.status, SecretStatus::Set);
        let token_entry = load
            .entries
            .iter()
            .find(|entry| entry.name == "DISCORD_TOKEN")
            .unwrap();
        assert_eq!(token_entry.value, "<SET>");
    }
}
