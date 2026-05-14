use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::Row;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;
use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _};
use twilight_http::error::{Error as TwilightHttpError, ErrorType as TwilightHttpErrorType};
use twilight_http::Client as DiscordHttpClient;
use twilight_model::application::command::{CommandOption, CommandOptionType};
use twilight_model::application::interaction::application_command::{
    CommandData, CommandOptionValue,
};
use twilight_model::application::interaction::{Interaction, InteractionData};
use twilight_model::channel::message::component::{
    ActionRow, Button, ButtonStyle, Component, SelectMenu, SelectMenuOption, SelectMenuType,
    TextInput, TextInputStyle,
};
use twilight_model::channel::message::embed::{Embed, EmbedAuthor, EmbedField, EmbedFooter};
use twilight_model::channel::message::AllowedMentions;
use twilight_model::channel::ChannelType;
use twilight_model::channel::Message as DiscordMessage;
use twilight_model::gateway::payload::incoming::GuildCreate;
use twilight_model::gateway::payload::incoming::VoiceStateUpdate;
use twilight_model::guild::{Member as DiscordMember, Permissions, Role as DiscordRole};
use twilight_model::id::{
    marker::{ApplicationMarker, ChannelMarker, GuildMarker, MessageMarker, UserMarker},
    Id,
};
use twilight_model::user::CurrentUser;
use twilight_model::util::Timestamp;
use xiii_config::{is_secret_like_name, SuperbotConfig};
use xiii_core::{ModuleId, ModuleManifest, Report, Severity};
use xiii_discord::{CentralRouter, DiscordRuntimePlan};
use xiii_scheduler::SchedulerRegistry;

const DISCORD_HTTP_MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Parser)]
#[command(name = "xiii-superbot")]
#[command(about = "Safe XIII Superbot planning and read-only verification CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Validate a unified env file without connecting to Discord.
    CheckConfig {
        #[arg(long)]
        env_file: PathBuf,
    },
    /// Verify legacy SQLite/JSON state in read-only mode.
    VerifyLegacy {
        #[arg(long)]
        env_file: PathBuf,
    },
    /// Build an offline read-only clanlist preview from legacy JSON/cache files.
    ClanlistPreview {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long, value_enum, default_value = "text")]
        format: PreviewFormat,
        #[arg(long, conflicts_with = "no_steam")]
        include_steam: bool,
        #[arg(long, conflicts_with = "include_steam")]
        no_steam: bool,
    },
    /// Fetch a Discord read-only clanlist role/member snapshot for diagnostics.
    DiscordReadonlyClanlistSnapshot {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: PreviewFormat,
        #[arg(long, conflicts_with = "roles_only")]
        include_members: bool,
        #[arg(long, conflicts_with = "include_members")]
        roles_only: bool,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Build a read-only Clanlist render parity preview from Discord reads and legacy JSON/cache.
    ClanlistRenderPreview {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: PreviewFormat,
        #[arg(long)]
        roles_only: bool,
        #[arg(long, conflicts_with = "no_steam")]
        include_steam: bool,
        #[arg(long, conflicts_with = "include_steam")]
        no_steam: bool,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value_t = 20)]
        max_members_per_section: usize,
    },
    /// Build a dry-run Clanlist Discord write plan without executing writes.
    ClanlistWritePlan {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
        #[arg(long)]
        allow_write_plan: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: PreviewFormat,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        require_old_service_stopped: bool,
        #[arg(long)]
        old_service_status_file: Option<PathBuf>,
    },
    /// Verify exact Clanlist target panel messages through read-only Discord GET calls.
    ClanlistTargetMessageCheck {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: PreviewFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Bootstrap fresh Clanlist panel messages; write-capable only with explicit confirmations.
    ClanlistBootstrapNewPanels {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
        #[arg(long)]
        allow_discord_write: bool,
        #[arg(long)]
        confirm_create_new_panels: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: PreviewFormat,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        state_output: Option<PathBuf>,
        #[arg(long, default_value_t = 20)]
        max_members_per_section: usize,
        #[arg(long, conflicts_with = "no_steam")]
        include_steam: bool,
        #[arg(long, conflicts_with = "include_steam")]
        no_steam: bool,
    },
    /// Update the three fresh Clanlist panel messages recorded in Superbot state.
    ClanlistUpdatePanels {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
        #[arg(long)]
        allow_discord_write: bool,
        #[arg(long)]
        confirm_update_panels: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: PreviewFormat,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        state_file: Option<PathBuf>,
        #[arg(long, default_value_t = 20)]
        max_members_per_section: usize,
        #[arg(long, conflicts_with = "no_steam")]
        include_steam: bool,
        #[arg(long, conflicts_with = "include_steam")]
        no_steam: bool,
        #[arg(long)]
        require_old_service_stopped: bool,
        #[arg(long)]
        old_service_status_file: Option<PathBuf>,
    },
    /// Run the production Clanlist refresher against the three fresh Superbot-owned panel messages.
    RunClanlist {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
        #[arg(long)]
        allow_discord_write: bool,
        #[arg(long)]
        confirm_run_clanlist: bool,
        #[arg(long)]
        state_file: Option<PathBuf>,
        #[arg(long)]
        interval_seconds: Option<u64>,
        #[arg(long)]
        once: bool,
        #[arg(long, conflicts_with = "google_readonly")]
        no_google: bool,
        #[arg(long, conflicts_with = "no_google")]
        google_readonly: bool,
        #[arg(long)]
        require_old_service_stopped: bool,
        #[arg(long)]
        old_service_status_file: Option<PathBuf>,
        #[arg(long)]
        health_output: Option<PathBuf>,
    },
    /// Plan or create missing fresh Superbot-owned panels. Dry-run is the safe default for new modules.
    BootstrapFreshPanels {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
        #[arg(long)]
        allow_discord_write: bool,
        #[arg(long)]
        confirm_bootstrap: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum, default_value = "text")]
        format: PreviewFormat,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, value_delimiter = ',')]
        modules: Vec<String>,
    },
    /// Print cutover services, DB ownership, state files, flags, and risks.
    CutoverPlan {
        #[arg(long)]
        env_file: PathBuf,
    },
    /// Print module readiness from local config/state/DB files only.
    ModuleStatus {
        #[arg(long)]
        env_file: PathBuf,
    },
    /// Verify local cutover prerequisites without Discord writes.
    VerifyCutover {
        #[arg(long)]
        env_file: PathBuf,
    },
    /// Print the concrete cutover preparation checklist for selected modules.
    PrepareCutover {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long, value_delimiter = ',')]
        modules: Vec<String>,
    },
    /// Run the future all-module Superbot runtime. Currently safe-gated and dry-run oriented.
    RunSuperbot {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
        #[arg(long)]
        allow_discord_write: bool,
        #[arg(long)]
        confirm_run_superbot: bool,
        #[arg(long, value_delimiter = ',')]
        modules: Vec<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        health_output: Option<PathBuf>,
        #[arg(long)]
        require_old_services_stopped: bool,
        #[arg(long)]
        old_services_dir: Option<PathBuf>,
    },
    /// Plan guild-scoped slash command registration; never runs automatically.
    SyncCommands {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_write: bool,
        #[arg(long)]
        confirm_sync_commands: bool,
        #[arg(long, value_delimiter = ',')]
        modules: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Local read-only voice cutover risk check.
    VoiceCutoverCheck {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
    },
    /// Explicitly close legacy active voice sessions at a single cutover timestamp.
    VoiceFinalizeCutover {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_legacy_db_write: bool,
        #[arg(long)]
        confirm_close_active_voice_sessions: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Read-only all-module production readiness check.
    FinalReadinessCheck {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
    },
    /// Read-only production preflight for a private VPS env file.
    ProductionPreflight {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long)]
        allow_discord_read: bool,
    },
    /// Read-only report proving every module is using the configured legacy DB/state source.
    DbSourceCheck {
        #[arg(long)]
        env_file: PathBuf,
    },
    /// Read-only legacy visual/text parity audit against known old bot render metadata.
    LegacyParityAudit {
        #[arg(long)]
        env_file: PathBuf,
    },
    /// Render read-only previews of Discord-facing embeds, buttons, modals, and responses.
    RenderPreview {
        #[arg(long)]
        env_file: PathBuf,
        #[arg(long, value_delimiter = ',')]
        modules: Vec<String>,
        #[arg(long, value_enum, default_value = "text")]
        format: PreviewFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Local read-only temp voice cutover risk check.
    TempVoiceCutoverCheck {
        #[arg(long)]
        env_file: PathBuf,
    },
    /// Local read-only ticket cutover risk check.
    TicketCutoverCheck {
        #[arg(long)]
        env_file: PathBuf,
    },
    /// Print module descriptors and route/job manifests.
    PrintManifest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PreviewFormat {
    Text,
    Json,
}

fn module_manifests() -> Vec<ModuleManifest> {
    vec![
        xiii_clanlist::manifest(),
        xiii_tempvoice::manifest(),
        xiii_tickets::manifest(),
        xiii_discipline::manifest(),
        xiii_vacation::manifest(),
        xiii_recruit::manifest(),
        xiii_voice_activity::manifest(),
    ]
}

pub fn run() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(io::stderr)
        .init();

    let cli = Cli::parse();
    macro_rules! run_async {
        ($future:expr) => {
            match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(Box::pin($future)),
                Err(err) => {
                    println!("[FAIL] tokio runtime {err}");
                    ExitCode::from(2)
                }
            }
        };
    }
    match cli.command {
        Commands::CheckConfig { env_file } => check_config(env_file),
        Commands::VerifyLegacy { env_file } => run_async!(verify_legacy(env_file)),
        Commands::ClanlistPreview {
            env_file,
            format,
            include_steam,
            no_steam,
        } => clanlist_preview(env_file, format, include_steam, no_steam),
        Commands::DiscordReadonlyClanlistSnapshot {
            env_file,
            allow_discord_read,
            format,
            include_members,
            roles_only,
            output,
        } => {
            run_async!(discord_readonly_clanlist_snapshot(
                env_file,
                allow_discord_read,
                format,
                include_members,
                roles_only,
                output,
            ))
        }
        Commands::ClanlistRenderPreview {
            env_file,
            allow_discord_read,
            format,
            roles_only,
            include_steam,
            no_steam,
            output,
            max_members_per_section,
        } => {
            run_async!(clanlist_render_preview(
                env_file,
                allow_discord_read,
                format,
                roles_only,
                include_steam,
                no_steam,
                output,
                max_members_per_section,
            ))
        }
        Commands::ClanlistWritePlan {
            env_file,
            allow_discord_read,
            allow_write_plan,
            format,
            output,
            require_old_service_stopped,
            old_service_status_file,
        } => {
            run_async!(clanlist_write_plan(
                env_file,
                allow_discord_read,
                allow_write_plan,
                format,
                output,
                require_old_service_stopped,
                old_service_status_file,
            ))
        }
        Commands::ClanlistTargetMessageCheck {
            env_file,
            allow_discord_read,
            format,
            output,
        } => run_async!(clanlist_target_message_check(
            env_file,
            allow_discord_read,
            format,
            output
        )),
        Commands::ClanlistBootstrapNewPanels {
            env_file,
            allow_discord_read,
            allow_discord_write,
            confirm_create_new_panels,
            dry_run,
            format,
            output,
            state_output,
            max_members_per_section,
            include_steam,
            no_steam,
        } => {
            run_async!(clanlist_bootstrap_new_panels(
                env_file,
                allow_discord_read,
                allow_discord_write,
                confirm_create_new_panels,
                dry_run,
                format,
                output,
                state_output,
                max_members_per_section,
                include_steam,
                no_steam,
            ))
        }
        Commands::ClanlistUpdatePanels {
            env_file,
            allow_discord_read,
            allow_discord_write,
            confirm_update_panels,
            dry_run,
            format,
            output,
            state_file,
            max_members_per_section,
            include_steam,
            no_steam,
            require_old_service_stopped,
            old_service_status_file,
        } => {
            run_async!(clanlist_update_panels(
                env_file,
                allow_discord_read,
                allow_discord_write,
                confirm_update_panels,
                dry_run,
                format,
                output,
                state_file,
                max_members_per_section,
                include_steam,
                no_steam,
                require_old_service_stopped,
                old_service_status_file,
            ))
        }
        Commands::RunClanlist {
            env_file,
            allow_discord_read,
            allow_discord_write,
            confirm_run_clanlist,
            state_file,
            interval_seconds,
            once,
            no_google,
            google_readonly,
            require_old_service_stopped,
            old_service_status_file,
            health_output,
        } => {
            run_async!(run_clanlist(
                env_file,
                allow_discord_read,
                allow_discord_write,
                confirm_run_clanlist,
                state_file,
                interval_seconds,
                once,
                no_google,
                google_readonly,
                require_old_service_stopped,
                old_service_status_file,
                health_output,
            ))
        }
        Commands::BootstrapFreshPanels {
            env_file,
            allow_discord_read,
            allow_discord_write,
            confirm_bootstrap,
            dry_run,
            format,
            output,
            modules,
        } => {
            run_async!(bootstrap_fresh_panels(
                env_file,
                allow_discord_read,
                allow_discord_write,
                confirm_bootstrap,
                dry_run,
                format,
                output,
                modules,
            ))
        }
        Commands::CutoverPlan { env_file } => cutover_plan(env_file, Vec::new()),
        Commands::ModuleStatus { env_file } => run_async!(module_status(env_file)),
        Commands::VerifyCutover { env_file } => run_async!(verify_cutover(env_file)),
        Commands::PrepareCutover { env_file, modules } => cutover_plan(env_file, modules),
        Commands::RunSuperbot {
            env_file,
            allow_discord_read,
            allow_discord_write,
            confirm_run_superbot,
            modules,
            dry_run,
            health_output,
            require_old_services_stopped,
            old_services_dir,
        } => {
            run_async!(run_superbot(
                env_file,
                allow_discord_read,
                allow_discord_write,
                confirm_run_superbot,
                modules,
                dry_run,
                health_output,
                require_old_services_stopped,
                old_services_dir,
            ))
        }
        Commands::SyncCommands {
            env_file,
            allow_discord_write,
            confirm_sync_commands,
            modules,
            dry_run,
        } => {
            run_async!(sync_commands(
                env_file,
                allow_discord_write,
                confirm_sync_commands,
                modules,
                dry_run,
            ))
        }
        Commands::VoiceCutoverCheck {
            env_file,
            allow_discord_read,
        } => run_async!(voice_cutover_check(env_file, allow_discord_read)),
        Commands::VoiceFinalizeCutover {
            env_file,
            allow_legacy_db_write,
            confirm_close_active_voice_sessions,
            dry_run,
        } => run_async!(voice_finalize_cutover(
            env_file,
            allow_legacy_db_write,
            confirm_close_active_voice_sessions,
            dry_run,
        )),
        Commands::FinalReadinessCheck {
            env_file,
            allow_discord_read,
        } => run_async!(final_readiness_check(env_file, allow_discord_read)),
        Commands::ProductionPreflight {
            env_file,
            allow_discord_read,
        } => run_async!(production_preflight(env_file, allow_discord_read)),
        Commands::DbSourceCheck { env_file } => run_async!(db_source_check(env_file)),
        Commands::LegacyParityAudit { env_file } => legacy_parity_audit(env_file),
        Commands::RenderPreview {
            env_file,
            modules,
            format,
            output,
        } => render_preview(env_file, modules, format, output),
        Commands::TempVoiceCutoverCheck { env_file } => {
            run_async!(temp_voice_cutover_check(env_file))
        }
        Commands::TicketCutoverCheck { env_file } => run_async!(ticket_cutover_check(env_file)),
        Commands::PrintManifest => print_manifest(),
    }
}

fn check_config(env_file: PathBuf) -> ExitCode {
    println!("XIII Superbot Config Check");
    println!("Mode: READ ONLY");
    println!("Discord login: DISABLED");
    println!("DB writes: DISABLED");
    println!("Env file: {}", env_file.display());
    println!();

    match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => {
            println!("Redacted Env Report");
            for entry in &load.entries {
                println!("{}={}", entry.name, entry.value);
            }
            println!();
            print_report("Config Validation", &load.report);
            if load.report.has_failures() {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(err) => {
            println!("[FAIL] config {err}");
            ExitCode::from(2)
        }
    }
}

fn clanlist_preview(
    env_file: PathBuf,
    format: PreviewFormat,
    include_steam: bool,
    no_steam: bool,
) -> ExitCode {
    let steam = match (include_steam, no_steam) {
        (true, false) => xiii_clanlist::SteamPreviewMode::Include,
        (false, true) => xiii_clanlist::SteamPreviewMode::Disabled,
        _ => xiii_clanlist::SteamPreviewMode::Auto,
    };

    let result = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => {
            xiii_clanlist::build_preview(&load, xiii_clanlist::ClanlistPreviewOptions { steam })
        }
        Err(err) => {
            let mut report = Report::new();
            report.fail("config", err.to_string());
            xiii_clanlist::ClanlistPreviewResult {
                report,
                model: None,
            }
        }
    };

    match format {
        PreviewFormat::Text => print!("{}", xiii_clanlist::render_text(&result)),
        PreviewFormat::Json => match xiii_clanlist::render_json(&result) {
            Ok(json) => println!("{json}"),
            Err(err) => {
                println!("{{\"failures\":[\"failed to render JSON: {err}\"]}}");
                return ExitCode::from(2);
            }
        },
    }

    if result.has_critical_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

async fn discord_readonly_clanlist_snapshot(
    env_file: PathBuf,
    allow_discord_read: bool,
    format: PreviewFormat,
    include_members_flag: bool,
    roles_only: bool,
    output: Option<PathBuf>,
) -> ExitCode {
    if let Some(report) = discord_read_permission_failure(allow_discord_read) {
        let result = xiii_clanlist::ClanlistDiscordSnapshotResult::no_discord(report);
        return print_discord_snapshot_result(&result, format, output.as_deref(), None);
    }

    let include_members = include_members_flag || !roles_only;
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            let mut report = Report::new();
            report.fail("config", err.to_string());
            let result = xiii_clanlist::ClanlistDiscordSnapshotResult::no_discord(report);
            return print_discord_snapshot_result(&result, format, output.as_deref(), None);
        }
    };

    let preview = xiii_clanlist::build_preview(
        &load,
        xiii_clanlist::ClanlistPreviewOptions {
            steam: xiii_clanlist::SteamPreviewMode::Auto,
        },
    );
    if preview.has_critical_failures() {
        let result = xiii_clanlist::ClanlistDiscordSnapshotResult::no_discord(preview.report);
        return print_discord_snapshot_result(
            &result,
            format,
            output.as_deref(),
            Some(&load.config),
        );
    }

    let token = match read_secret_from_env_file(&env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(message) => {
            let mut report = preview.report;
            report.fail("discord", message);
            let result = xiii_clanlist::ClanlistDiscordSnapshotResult::no_discord(report);
            return print_discord_snapshot_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
            );
        }
    };

    let fetch =
        fetch_discord_clanlist_snapshot(&token, load.config.core.guild_id, include_members).await;
    let result = match fetch.roles {
        Some(roles) => xiii_clanlist::build_discord_readonly_snapshot(
            preview,
            load.config.core.guild_id,
            roles,
            fetch.members,
            include_members,
            fetch.report,
        ),
        None => {
            let mut report = preview.report;
            report.extend(fetch.report);
            xiii_clanlist::ClanlistDiscordSnapshotResult {
                report,
                model: None,
                safety: xiii_clanlist::PreviewSafety::discord_http_read_only(),
            }
        }
    };

    print_discord_snapshot_result(&result, format, output.as_deref(), Some(&load.config))
}

async fn clanlist_render_preview(
    env_file: PathBuf,
    allow_discord_read: bool,
    format: PreviewFormat,
    roles_only: bool,
    include_steam: bool,
    no_steam: bool,
    output: Option<PathBuf>,
    max_members_per_section: usize,
) -> ExitCode {
    if let Some(report) = discord_read_permission_failure(allow_discord_read) {
        let result = xiii_clanlist::ClanlistRenderPreviewResult::no_discord(report);
        return print_render_preview_result(
            &result,
            format,
            output.as_deref(),
            None,
            max_members_per_section,
        );
    }

    let steam = match (include_steam, no_steam) {
        (true, false) => xiii_clanlist::SteamPreviewMode::Include,
        (false, true) => xiii_clanlist::SteamPreviewMode::Disabled,
        _ => xiii_clanlist::SteamPreviewMode::Auto,
    };
    let include_members = !roles_only;

    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            let mut report = Report::new();
            report.fail("config", err.to_string());
            let result = xiii_clanlist::ClanlistRenderPreviewResult::no_discord(report);
            return print_render_preview_result(
                &result,
                format,
                output.as_deref(),
                None,
                max_members_per_section,
            );
        }
    };

    let preview =
        xiii_clanlist::build_preview(&load, xiii_clanlist::ClanlistPreviewOptions { steam });
    if preview.has_critical_failures() {
        let result = xiii_clanlist::ClanlistRenderPreviewResult::no_discord(preview.report);
        return print_render_preview_result(
            &result,
            format,
            output.as_deref(),
            Some(&load.config),
            max_members_per_section,
        );
    }

    let token = match read_secret_from_env_file(&env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(message) => {
            let mut report = preview.report;
            report.fail("discord", message);
            let result = xiii_clanlist::ClanlistRenderPreviewResult::no_discord(report);
            return print_render_preview_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
                max_members_per_section,
            );
        }
    };

    let fetch =
        fetch_discord_clanlist_snapshot(&token, load.config.core.guild_id, include_members).await;
    let result = match fetch.roles {
        Some(roles) => xiii_clanlist::build_render_preview(
            preview,
            load.config.core.guild_id,
            roles,
            fetch.members,
            include_members,
            fetch.report,
        ),
        None => {
            let mut report = preview.report;
            report.extend(fetch.report);
            xiii_clanlist::ClanlistRenderPreviewResult {
                report,
                model: None,
                safety: xiii_clanlist::PreviewSafety::discord_http_read_only(),
            }
        }
    };

    print_render_preview_result(
        &result,
        format,
        output.as_deref(),
        Some(&load.config),
        max_members_per_section,
    )
}

async fn clanlist_write_plan(
    env_file: PathBuf,
    allow_discord_read: bool,
    allow_write_plan: bool,
    format: PreviewFormat,
    output: Option<PathBuf>,
    require_old_service_stopped: bool,
    old_service_status_file: Option<PathBuf>,
) -> ExitCode {
    if let Some(report) = write_plan_permission_failure(allow_write_plan) {
        let result = xiii_clanlist::ClanlistWritePlanResult::no_discord(report);
        return print_write_plan_result(&result, format, None, None);
    }
    if let Some(report) = discord_read_permission_failure(allow_discord_read) {
        let result = xiii_clanlist::ClanlistWritePlanResult::no_discord(report);
        return print_write_plan_result(&result, format, None, None);
    }

    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            let mut report = Report::new();
            report.fail("config", err.to_string());
            let result = xiii_clanlist::ClanlistWritePlanResult::no_discord(report);
            return print_write_plan_result(&result, format, output.as_deref(), None);
        }
    };

    let preview = xiii_clanlist::build_preview(
        &load,
        xiii_clanlist::ClanlistPreviewOptions {
            steam: xiii_clanlist::SteamPreviewMode::Auto,
        },
    );
    if preview.has_critical_failures() {
        let service_report = evaluate_old_service_guard(
            require_old_service_stopped,
            old_service_status_file.as_deref(),
        );
        let render = xiii_clanlist::ClanlistRenderPreviewResult {
            report: preview.report,
            model: None,
            safety: xiii_clanlist::PreviewSafety::discord_http_read_only(),
        };
        let result = xiii_clanlist::build_write_plan(render, service_report);
        return print_write_plan_result(&result, format, output.as_deref(), Some(&load.config));
    }

    let token = match read_secret_from_env_file(&env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(message) => {
            let mut report = preview.report;
            report.fail("discord", message);
            let result = xiii_clanlist::ClanlistWritePlanResult::no_discord(report);
            return print_write_plan_result(&result, format, output.as_deref(), Some(&load.config));
        }
    };

    let fetch = fetch_discord_clanlist_snapshot(&token, load.config.core.guild_id, true).await;
    let render = match fetch.roles {
        Some(roles) => xiii_clanlist::build_render_preview(
            preview,
            load.config.core.guild_id,
            roles,
            fetch.members,
            true,
            fetch.report,
        ),
        None => {
            let mut report = preview.report;
            report.extend(fetch.report);
            xiii_clanlist::ClanlistRenderPreviewResult {
                report,
                model: None,
                safety: xiii_clanlist::PreviewSafety::discord_http_read_only(),
            }
        }
    };
    let service_report = evaluate_old_service_guard(
        require_old_service_stopped,
        old_service_status_file.as_deref(),
    );
    let result = xiii_clanlist::build_write_plan(render, service_report);

    print_write_plan_result(&result, format, output.as_deref(), Some(&load.config))
}

async fn clanlist_target_message_check(
    env_file: PathBuf,
    allow_discord_read: bool,
    format: PreviewFormat,
    output: Option<PathBuf>,
) -> ExitCode {
    if let Some(report) = discord_read_permission_failure(allow_discord_read) {
        let result = xiii_clanlist::ClanlistTargetMessageCheckResult::no_discord(report);
        return print_target_message_check_result(&result, format, None, None);
    }

    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            let mut report = Report::new();
            report.fail("config", err.to_string());
            let result = xiii_clanlist::ClanlistTargetMessageCheckResult::no_discord(report);
            return print_target_message_check_result(&result, format, output.as_deref(), None);
        }
    };

    let preview = xiii_clanlist::build_preview(
        &load,
        xiii_clanlist::ClanlistPreviewOptions {
            steam: xiii_clanlist::SteamPreviewMode::Auto,
        },
    );
    if preview.has_critical_failures() {
        let result = xiii_clanlist::ClanlistTargetMessageCheckResult::no_discord(preview.report);
        return print_target_message_check_result(
            &result,
            format,
            output.as_deref(),
            Some(&load.config),
        );
    }

    let targets = preview
        .model
        .as_ref()
        .map(xiii_clanlist::target_message_targets_from_preview)
        .unwrap_or_default();

    let token = match read_secret_from_env_file(&env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(message) => {
            let mut report = preview.report;
            report.fail("discord", message);
            let result = xiii_clanlist::ClanlistTargetMessageCheckResult::no_discord(report);
            return print_target_message_check_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
            );
        }
    };

    let client = DiscordHttpClient::new(token);
    let mut discord_report = Report::new();
    let current_user = match fetch_current_user_with_retry(&client, &mut discord_report).await {
        Ok(user) => {
            discord_report.ok("discord", "connected to Discord read-only");
            user
        }
        Err(err) => {
            let mut report = preview.report;
            report.extend(discord_report);
            report.fail("discord", err);
            let result = xiii_clanlist::ClanlistTargetMessageCheckResult {
                report,
                model: None,
                safety: xiii_clanlist::PreviewSafety::discord_http_read_only(),
            };
            return print_target_message_check_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
            );
        }
    };

    let mut observations = Vec::with_capacity(targets.len());
    for target in &targets {
        let label = format!("{} target message", target.panel_name);
        match fetch_target_message_with_retry(
            &client,
            target.channel_id,
            target.message_id,
            &mut discord_report,
            &label,
        )
        .await
        {
            Ok(message) => observations.push(observation_from_discord_message(target, message)),
            Err(err) => observations.push(xiii_clanlist::TargetMessageObservationInput {
                panel_name: target.panel_name,
                channel_id: target.channel_id,
                message_id: target.message_id,
                exists: false,
                failure_reason: Some(err),
                author_id: None,
                embed_count: None,
                first_embed_title: None,
                first_embed_footer_text: None,
                first_embed_footer_icon_url: None,
                first_embed_marker_url: None,
            }),
        }
    }

    let result = xiii_clanlist::build_target_message_check(
        preview,
        current_user.id.get(),
        observations,
        discord_report,
    );
    print_target_message_check_result(&result, format, output.as_deref(), Some(&load.config))
}

#[allow(clippy::too_many_arguments)]
async fn clanlist_bootstrap_new_panels(
    env_file: PathBuf,
    allow_discord_read: bool,
    allow_discord_write: bool,
    confirm_create_new_panels: bool,
    dry_run: bool,
    format: PreviewFormat,
    output: Option<PathBuf>,
    state_output: Option<PathBuf>,
    _max_members_per_section: usize,
    include_steam: bool,
    no_steam: bool,
) -> ExitCode {
    if let Some(report) = bootstrap_permission_failure(
        allow_discord_read,
        allow_discord_write,
        confirm_create_new_panels,
    ) {
        let result = xiii_clanlist::ClanlistBootstrapNewPanelsResult::no_discord(report);
        return print_bootstrap_new_panels_result(&result, format, None, None);
    }

    let steam = match (include_steam, no_steam) {
        (true, false) => xiii_clanlist::SteamPreviewMode::Include,
        (false, true) => xiii_clanlist::SteamPreviewMode::Disabled,
        _ => xiii_clanlist::SteamPreviewMode::Auto,
    };

    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            let mut report = Report::new();
            report.fail("config", err.to_string());
            let result = xiii_clanlist::ClanlistBootstrapNewPanelsResult::no_discord(report);
            return print_bootstrap_new_panels_result(&result, format, output.as_deref(), None);
        }
    };

    let state_path =
        match resolve_state_output_path(state_output.as_deref(), &load.config, !dry_run) {
            Ok(path) => Some(path),
            Err(err) => {
                let mut report = Report::new();
                report.fail("state", err);
                let result = xiii_clanlist::ClanlistBootstrapNewPanelsResult::no_discord(report);
                return print_bootstrap_new_panels_result(
                    &result,
                    format,
                    output.as_deref(),
                    Some(&load.config),
                );
            }
        };

    let preview =
        xiii_clanlist::build_preview(&load, xiii_clanlist::ClanlistPreviewOptions { steam });
    let preview_targets = preview.model.as_ref().map(|model| model.targets.clone());
    if preview.has_critical_failures() {
        let result = xiii_clanlist::ClanlistBootstrapNewPanelsResult::no_discord(preview.report);
        return print_bootstrap_new_panels_result(
            &result,
            format,
            output.as_deref(),
            Some(&load.config),
        );
    }

    let token = match read_secret_from_env_file(&env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(message) => {
            let mut report = preview.report;
            report.fail("discord", message);
            let result = xiii_clanlist::ClanlistBootstrapNewPanelsResult::no_discord(report);
            return print_bootstrap_new_panels_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
            );
        }
    };

    let client = DiscordHttpClient::new(token);
    let mut identity_report = Report::new();
    let current_user = match fetch_current_user_with_retry(&client, &mut identity_report).await {
        Ok(user) => {
            identity_report.ok("discord", "connected to Discord read-only");
            user
        }
        Err(err) => {
            let mut report = preview.report;
            report.extend(identity_report);
            report.fail("discord", err);
            let result = xiii_clanlist::ClanlistBootstrapNewPanelsResult {
                report,
                model: None,
                safety: xiii_clanlist::BootstrapSafety::new(dry_run),
            };
            return print_bootstrap_new_panels_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
            );
        }
    };

    let fetch =
        fetch_discord_clanlist_snapshot_with_client(&client, load.config.core.guild_id, true).await;
    let mut discord_report = identity_report;
    discord_report.extend(fetch.report);
    let render = match fetch.roles {
        Some(roles) => xiii_clanlist::build_render_preview(
            preview,
            load.config.core.guild_id,
            roles,
            fetch.members,
            true,
            discord_report,
        ),
        None => {
            let mut report = preview.report;
            report.extend(discord_report);
            xiii_clanlist::ClanlistRenderPreviewResult {
                report,
                model: None,
                safety: xiii_clanlist::PreviewSafety::discord_http_read_only(),
            }
        }
    };

    let now = chrono::Utc::now();
    let created_at_utc = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut result = xiii_clanlist::build_bootstrap_new_panels(
        render,
        preview_targets,
        current_user.id.get(),
        dry_run,
        &created_at_utc,
    );

    if result.has_critical_failures() || dry_run {
        return print_bootstrap_new_panels_result(
            &result,
            format,
            output.as_deref(),
            Some(&load.config),
        );
    }

    let payloads = result
        .model
        .as_ref()
        .map(|model| model.payloads.clone())
        .unwrap_or_default();
    let mut outcomes = Vec::new();
    for payload in &payloads {
        match send_bootstrap_panel_message_with_retry(
            &client,
            payload,
            now.timestamp(),
            &mut result.report,
        )
        .await
        {
            Ok(message_id) => outcomes.push(xiii_clanlist::BootstrapOperationOutcome {
                panel_name: payload.panel_name,
                new_message_id: Some(message_id),
                failure_reason: None,
            }),
            Err(err) => {
                outcomes.push(xiii_clanlist::BootstrapOperationOutcome {
                    panel_name: payload.panel_name,
                    new_message_id: None,
                    failure_reason: Some(err),
                });
                break;
            }
        }
    }
    xiii_clanlist::apply_bootstrap_outcomes(&mut result, outcomes);

    let all_created = result.model.as_ref().is_some_and(|model| {
        model
            .operations
            .iter()
            .filter(|operation| operation.status == "created")
            .count()
            == 3
    });

    if all_created {
        if let Some(model) = result.model.as_ref() {
            match xiii_clanlist::build_panel_state(model, &created_at_utc)
                .and_then(|state| write_panel_state_json(state_path.as_ref().unwrap(), &state))
            {
                Ok(()) => {
                    xiii_clanlist::set_bootstrap_state_file_path(
                        &mut result,
                        state_path.as_ref().unwrap().display().to_string(),
                    );
                    result.report.ok(
                        "state",
                        format!(
                            "new Clanlist panel state written: {}",
                            state_path.as_ref().unwrap().display()
                        ),
                    );
                }
                Err(err) => result.report.fail("state", err),
            }
        }
    } else if result.model.as_ref().is_some_and(|model| {
        model
            .operations
            .iter()
            .any(|operation| operation.new_message_id.is_some())
    }) {
        let partial_path =
            partial_recovery_path(&load.config, &now.format("%Y%m%dT%H%M%SZ").to_string());
        xiii_clanlist::set_bootstrap_partial_recovery_file_path(
            &mut result,
            partial_path.display().to_string(),
        );
        match write_partial_recovery_json(&partial_path, &result) {
            Ok(()) => result.report.warn(
                "state",
                format!(
                    "partial bootstrap recovery file written: {}",
                    partial_path.display()
                ),
            ),
            Err(err) => result.report.fail("state", err),
        }
    }

    print_bootstrap_new_panels_result(&result, format, output.as_deref(), Some(&load.config))
}

#[allow(clippy::too_many_arguments)]
async fn clanlist_update_panels(
    env_file: PathBuf,
    allow_discord_read: bool,
    allow_discord_write: bool,
    confirm_update_panels: bool,
    dry_run: bool,
    format: PreviewFormat,
    output: Option<PathBuf>,
    state_file: Option<PathBuf>,
    _max_members_per_section: usize,
    include_steam: bool,
    no_steam: bool,
    require_old_service_stopped: bool,
    old_service_status_file: Option<PathBuf>,
) -> ExitCode {
    if let Some(report) = update_permission_failure(
        allow_discord_read,
        allow_discord_write,
        confirm_update_panels,
    ) {
        let result = xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report);
        return print_update_panels_result(&result, format, None, None);
    }

    let steam = match (include_steam, no_steam) {
        (true, false) => xiii_clanlist::SteamPreviewMode::Include,
        (false, true) => xiii_clanlist::SteamPreviewMode::Disabled,
        _ => xiii_clanlist::SteamPreviewMode::Auto,
    };

    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            let mut report = Report::new();
            report.fail("config", err.to_string());
            let result = xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report);
            return print_update_panels_result(&result, format, output.as_deref(), None);
        }
    };

    let state_path = match resolve_state_file_path(state_file.as_deref(), &load.config) {
        Ok(path) => path,
        Err(err) => {
            let mut report = Report::new();
            report.fail("state", err);
            let result = xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report);
            return print_update_panels_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
            );
        }
    };

    let state_text = match fs::read_to_string(&state_path) {
        Ok(text) => text,
        Err(err) => {
            let mut report = Report::new();
            report.fail(
                "state",
                format!("failed to read state file {}: {err}", state_path.display()),
            );
            let result = xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report);
            return print_update_panels_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
            );
        }
    };

    let mut state_report = Report::new();
    state_report.ok(
        "state",
        format!("state file loaded: {}", state_path.display()),
    );
    let mut state = match xiii_clanlist::parse_panel_state_json(&state_text) {
        Ok(state) => state,
        Err(err) => {
            state_report.fail("state", format!("invalid Clanlist panel state JSON: {err}"));
            let result = xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(state_report);
            return print_update_panels_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
            );
        }
    };
    xiii_clanlist::validate_panel_state(&state, load.config.core.guild_id, &mut state_report);
    if state_report.has_failures() {
        let result = xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(state_report);
        return print_update_panels_result(&result, format, output.as_deref(), Some(&load.config));
    }

    let preview =
        xiii_clanlist::build_preview(&load, xiii_clanlist::ClanlistPreviewOptions { steam });
    if preview.has_critical_failures() {
        let mut report = state_report;
        report.extend(preview.report);
        let result = xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report);
        return print_update_panels_result(&result, format, output.as_deref(), Some(&load.config));
    }

    let token = match read_secret_from_env_file(&env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(message) => {
            let mut report = state_report;
            report.extend(preview.report);
            report.fail("discord", message);
            let result = xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report);
            return print_update_panels_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
            );
        }
    };

    let client = DiscordHttpClient::new(token);
    let mut identity_report = Report::new();
    let current_user = match fetch_current_user_with_retry(&client, &mut identity_report).await {
        Ok(user) => {
            identity_report.ok("discord", "connected to Discord read-only");
            user
        }
        Err(err) => {
            let mut report = state_report;
            report.extend(preview.report);
            report.extend(identity_report);
            report.fail("discord", err);
            let result = xiii_clanlist::ClanlistUpdatePanelsResult {
                report,
                model: None,
                safety: xiii_clanlist::UpdateSafety::new(dry_run),
            };
            return print_update_panels_result(
                &result,
                format,
                output.as_deref(),
                Some(&load.config),
            );
        }
    };

    let fetch =
        fetch_discord_clanlist_snapshot_with_client(&client, load.config.core.guild_id, true).await;
    let mut discord_report = state_report;
    discord_report.extend(identity_report);
    discord_report.extend(fetch.report);
    let render = match fetch.roles {
        Some(roles) => xiii_clanlist::build_render_preview(
            preview,
            load.config.core.guild_id,
            roles,
            fetch.members,
            true,
            discord_report,
        ),
        None => {
            let mut report = preview.report;
            report.extend(discord_report);
            xiii_clanlist::ClanlistRenderPreviewResult {
                report,
                model: None,
                safety: xiii_clanlist::PreviewSafety::discord_http_read_only(),
            }
        }
    };

    let mut target_report = Report::new();
    let targets = xiii_clanlist::update_targets_from_state(&state);
    let mut observations = Vec::with_capacity(targets.len());
    for target in &targets {
        let label = format!("{} update target message", target.panel_name);
        match fetch_target_message_with_retry(
            &client,
            target.channel_id,
            target.message_id,
            &mut target_report,
            &label,
        )
        .await
        {
            Ok(message) => observations.push(observation_from_discord_message(target, message)),
            Err(err) => observations.push(xiii_clanlist::TargetMessageObservationInput {
                panel_name: target.panel_name,
                channel_id: target.channel_id,
                message_id: target.message_id,
                exists: false,
                failure_reason: Some(err),
                author_id: None,
                embed_count: None,
                first_embed_title: None,
                first_embed_footer_text: None,
                first_embed_footer_icon_url: None,
                first_embed_marker_url: None,
            }),
        }
    }

    let service_report = evaluate_old_service_guard(
        require_old_service_stopped,
        old_service_status_file.as_deref(),
    );
    let now = chrono::Utc::now();
    let updated_at_utc = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut result = xiii_clanlist::build_update_panels(
        render,
        state.clone(),
        state_path.display().to_string(),
        current_user.id.get(),
        observations,
        service_report,
        dry_run,
        &updated_at_utc,
    );
    result.report.extend(target_report);

    if result.has_critical_failures() || dry_run {
        return print_update_panels_result(&result, format, output.as_deref(), Some(&load.config));
    }

    let payloads = result
        .model
        .as_ref()
        .map(|model| model.payloads.clone())
        .unwrap_or_default();
    let mut outcomes = Vec::new();
    for payload in &payloads {
        match update_panel_message_with_retry(&client, payload, now.timestamp(), &mut result.report)
            .await
        {
            Ok(message_id) => outcomes.push(xiii_clanlist::UpdateOperationOutcome {
                panel_name: payload.panel_name,
                edited_message_id: Some(message_id),
                failure_reason: None,
            }),
            Err(err) => {
                outcomes.push(xiii_clanlist::UpdateOperationOutcome {
                    panel_name: payload.panel_name,
                    edited_message_id: None,
                    failure_reason: Some(err),
                });
                break;
            }
        }
    }
    xiii_clanlist::apply_update_outcomes(&mut result, outcomes);

    let all_edited = result.model.as_ref().is_some_and(|model| {
        model
            .operations
            .iter()
            .filter(|operation| operation.status == "edited")
            .count()
            == 3
    });

    if all_edited {
        if let Some(model) = result.model.as_ref() {
            match xiii_clanlist::apply_successful_update_to_state(
                &mut state,
                model,
                &updated_at_utc,
            )
            .and_then(|()| write_panel_state_json_atomic(&state_path, &state))
            {
                Ok(()) => {
                    xiii_clanlist::set_update_state_updated_path(
                        &mut result,
                        state_path.display().to_string(),
                    );
                    result.report.ok(
                        "state",
                        format!("Clanlist panel state updated: {}", state_path.display()),
                    );
                }
                Err(err) => result.report.fail("state", err),
            }
        }
    } else if result.model.as_ref().is_some_and(|model| {
        model
            .operations
            .iter()
            .any(|operation| operation.edited_message_id.is_some())
    }) {
        let partial_path =
            update_partial_recovery_path(&load.config, &now.format("%Y%m%dT%H%M%SZ").to_string());
        xiii_clanlist::set_update_partial_recovery_file_path(
            &mut result,
            partial_path.display().to_string(),
        );
        match write_partial_update_json(&partial_path, &result) {
            Ok(()) => result.report.warn(
                "state",
                format!(
                    "partial update recovery file written: {}",
                    partial_path.display()
                ),
            ),
            Err(err) => result.report.fail("state", err),
        }
    }

    print_update_panels_result(&result, format, output.as_deref(), Some(&load.config))
}

#[allow(clippy::too_many_arguments)]
async fn run_clanlist(
    env_file: PathBuf,
    allow_discord_read: bool,
    allow_discord_write: bool,
    confirm_run_clanlist: bool,
    state_file: Option<PathBuf>,
    interval_seconds: Option<u64>,
    once: bool,
    _no_google: bool,
    google_readonly: bool,
    require_old_service_stopped: bool,
    old_service_status_file: Option<PathBuf>,
    health_output: Option<PathBuf>,
) -> ExitCode {
    if let Some(report) = run_clanlist_permission_failure(
        allow_discord_read,
        allow_discord_write,
        confirm_run_clanlist,
    ) {
        println!("XIII Clanlist Runtime");
        println!("Mode: FAILED SAFETY GATE");
        print_report("Safety", &report);
        return ExitCode::from(2);
    }

    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("XIII Clanlist Runtime");
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    if load.report.has_failures() {
        println!("XIII Clanlist Runtime");
        print_report("Config Validation", &load.report);
        return ExitCode::from(2);
    }

    let interval = match clanlist_interval_seconds(interval_seconds, &load.config) {
        Ok(seconds) => seconds,
        Err(err) => {
            println!("XIII Clanlist Runtime");
            println!("[FAIL] runtime {err}");
            return ExitCode::from(2);
        }
    };
    let health_path = match health_output.as_deref() {
        Some(path) => match resolve_health_output_path(path, &load.config) {
            Ok(path) => Some(path),
            Err(err) => {
                println!("XIII Clanlist Runtime");
                println!("[FAIL] health {err}");
                return ExitCode::from(2);
            }
        },
        None => None,
    };

    let resolved_state_path = match resolve_state_file_path(state_file.as_deref(), &load.config) {
        Ok(path) => path,
        Err(err) => {
            println!("XIII Clanlist Runtime");
            println!("[FAIL] state {err}");
            return ExitCode::from(2);
        }
    };
    if let Err(err) = preflight_clanlist_state_file(&resolved_state_path, load.config.core.guild_id)
    {
        println!("XIII Clanlist Runtime");
        println!("[FAIL] state {err}");
        return ExitCode::from(2);
    }

    println!("XIII Clanlist Runtime");
    println!("Discord reads: ENABLED");
    println!("Discord writes: ENABLED");
    println!("Gateway: DISABLED");
    println!(
        "Google Sheets: {}",
        if google_readonly {
            "DEFERRED"
        } else {
            "DISABLED"
        }
    );
    println!("Other modules: DISABLED");
    println!("State file: {}", resolved_state_path.display());
    println!();

    if once {
        let outcome = clanlist_refresh_once(
            &env_file,
            state_file.as_deref(),
            xiii_clanlist::SteamPreviewMode::Auto,
            require_old_service_stopped,
            old_service_status_file.as_deref(),
            "once",
            google_readonly,
        )
        .await;
        print_clanlist_refresh_summary(&outcome);
        if let Some(path) = health_path.as_deref() {
            let health = clanlist_health_from_outcome(&outcome, None, None);
            if let Err(err) = write_clanlist_health(path, &health) {
                println!("[FAIL] health {err}");
                return ExitCode::from(2);
            }
            println!("[OK] health written: {}", path.display());
        }
        return if outcome.result.has_critical_failures() {
            ExitCode::from(2)
        } else {
            ExitCode::SUCCESS
        };
    }

    println!("[OK] startup complete");
    println!("[OK] refresh interval seconds = {interval}");
    println!("Press Ctrl+C for graceful shutdown.");

    let mut guard = NonOverlapGuard::default();
    let mut last_success_at_utc: Option<String> = None;
    loop {
        if !guard.try_start() {
            println!("[WARN] refresh skipped because a previous refresh is still running");
        } else {
            println!("[OK] refresh started");
            let outcome = clanlist_refresh_once(
                &env_file,
                state_file.as_deref(),
                xiii_clanlist::SteamPreviewMode::Auto,
                require_old_service_stopped,
                old_service_status_file.as_deref(),
                "daemon",
                google_readonly,
            )
            .await;
            if !outcome.result.has_critical_failures() {
                last_success_at_utc = Some(outcome.finished_at_utc.clone());
                println!("[OK] refresh success");
            } else {
                println!("[FAIL] refresh failed; will retry on the next interval");
            }
            print_clanlist_refresh_summary(&outcome);

            let next_run_at_utc = chrono::Utc::now()
                .checked_add_signed(chrono::Duration::seconds(interval as i64))
                .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
            if let Some(path) = health_path.as_deref() {
                let health = clanlist_health_from_outcome(
                    &outcome,
                    last_success_at_utc.clone(),
                    next_run_at_utc.clone(),
                );
                if let Err(err) = write_clanlist_health(path, &health) {
                    println!("[WARN] failed to write health output: {err}");
                }
            }
            if let Some(next_run_at_utc) = &next_run_at_utc {
                println!("[OK] next run at {next_run_at_utc}");
            }
            guard.finish();
        }

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(interval)) => {}
            signal = tokio::signal::ctrl_c() => {
                match signal {
                    Ok(()) => println!("[OK] shutdown requested; exiting after completed refresh"),
                    Err(err) => println!("[WARN] failed to listen for Ctrl+C: {err}; exiting"),
                }
                break;
            }
        }
    }

    println!("[OK] shutdown complete");
    ExitCode::SUCCESS
}

#[allow(clippy::too_many_arguments)]
async fn clanlist_refresh_once(
    env_file: &Path,
    state_file: Option<&Path>,
    steam: xiii_clanlist::SteamPreviewMode,
    require_old_service_stopped: bool,
    old_service_status_file: Option<&Path>,
    run_mode: &str,
    google_readonly: bool,
) -> ClanlistRefreshOutcome {
    let started_at_utc = utc_timestamp_now();
    let mut state_path_for_outcome = None;

    let load = match SuperbotConfig::load_from_env_file(env_file) {
        Ok(load) => load,
        Err(err) => {
            let mut report = Report::new();
            report.fail("config", err.to_string());
            return ClanlistRefreshOutcome::finished(
                xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report),
                state_path_for_outcome,
                started_at_utc,
            );
        }
    };

    let state_path = match resolve_state_file_path(state_file, &load.config) {
        Ok(path) => {
            state_path_for_outcome = Some(path.clone());
            path
        }
        Err(err) => {
            let mut report = Report::new();
            report.fail("state", err);
            return ClanlistRefreshOutcome::finished(
                xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report),
                state_path_for_outcome,
                started_at_utc,
            );
        }
    };

    let state_text = match fs::read_to_string(&state_path) {
        Ok(text) => text,
        Err(err) => {
            let mut report = Report::new();
            report.fail(
                "state",
                format!("failed to read state file {}: {err}", state_path.display()),
            );
            return ClanlistRefreshOutcome::finished(
                xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report),
                state_path_for_outcome,
                started_at_utc,
            );
        }
    };

    let mut state_report = Report::new();
    state_report.ok(
        "state",
        format!("state file loaded: {}", state_path.display()),
    );
    let mut state = match xiii_clanlist::parse_panel_state_json(&state_text) {
        Ok(state) => state,
        Err(err) => {
            state_report.fail("state", format!("invalid Clanlist panel state JSON: {err}"));
            return ClanlistRefreshOutcome::finished(
                xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(state_report),
                state_path_for_outcome,
                started_at_utc,
            );
        }
    };
    xiii_clanlist::validate_panel_state(&state, load.config.core.guild_id, &mut state_report);
    if state_report.has_failures() {
        return ClanlistRefreshOutcome::finished(
            xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(state_report),
            state_path_for_outcome,
            started_at_utc,
        );
    }

    let mut preview =
        xiii_clanlist::build_preview(&load, xiii_clanlist::ClanlistPreviewOptions { steam });
    if google_readonly {
        preview.report.warn(
            "google",
            "Google Sheets read-only source is accepted but not implemented yet; using legacy steam_roster_cache.json",
        );
    }
    if preview.has_critical_failures() {
        let mut report = state_report;
        report.extend(preview.report);
        return ClanlistRefreshOutcome::finished(
            xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report),
            state_path_for_outcome,
            started_at_utc,
        );
    }

    let token = match read_secret_from_env_file(env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(message) => {
            let mut report = state_report;
            report.extend(preview.report);
            report.fail("discord", message);
            return ClanlistRefreshOutcome::finished(
                xiii_clanlist::ClanlistUpdatePanelsResult::no_discord(report),
                state_path_for_outcome,
                started_at_utc,
            );
        }
    };

    let client = DiscordHttpClient::new(token);
    let mut identity_report = Report::new();
    let current_user = match fetch_current_user_with_retry(&client, &mut identity_report).await {
        Ok(user) => {
            identity_report.ok("discord", "connected to Discord read-only");
            user
        }
        Err(err) => {
            let mut report = state_report;
            report.extend(preview.report);
            report.extend(identity_report);
            report.fail("discord", err);
            return ClanlistRefreshOutcome::finished(
                xiii_clanlist::ClanlistUpdatePanelsResult {
                    report,
                    model: None,
                    safety: xiii_clanlist::UpdateSafety::new(false),
                },
                state_path_for_outcome,
                started_at_utc,
            );
        }
    };

    let fetch =
        fetch_discord_clanlist_snapshot_with_client(&client, load.config.core.guild_id, true).await;
    let mut discord_report = state_report;
    discord_report.extend(identity_report);
    discord_report.extend(fetch.report);
    let render = match fetch.roles {
        Some(roles) => xiii_clanlist::build_render_preview(
            preview,
            load.config.core.guild_id,
            roles,
            fetch.members,
            true,
            discord_report,
        ),
        None => {
            let mut report = preview.report;
            report.extend(discord_report);
            xiii_clanlist::ClanlistRenderPreviewResult {
                report,
                model: None,
                safety: xiii_clanlist::PreviewSafety::discord_http_read_only(),
            }
        }
    };

    let mut target_report = Report::new();
    let targets = xiii_clanlist::update_targets_from_state(&state);
    let mut observations = Vec::with_capacity(targets.len());
    for target in &targets {
        let label = format!("{} runtime update target message", target.panel_name);
        match fetch_target_message_with_retry(
            &client,
            target.channel_id,
            target.message_id,
            &mut target_report,
            &label,
        )
        .await
        {
            Ok(message) => observations.push(observation_from_discord_message(target, message)),
            Err(err) => observations.push(xiii_clanlist::TargetMessageObservationInput {
                panel_name: target.panel_name,
                channel_id: target.channel_id,
                message_id: target.message_id,
                exists: false,
                failure_reason: Some(err),
                author_id: None,
                embed_count: None,
                first_embed_title: None,
                first_embed_footer_text: None,
                first_embed_footer_icon_url: None,
                first_embed_marker_url: None,
            }),
        }
    }

    let service_report =
        evaluate_old_service_guard(require_old_service_stopped, old_service_status_file);
    let now = chrono::Utc::now();
    let updated_at_utc = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut result = xiii_clanlist::build_update_panels(
        render,
        state.clone(),
        state_path.display().to_string(),
        current_user.id.get(),
        observations,
        service_report,
        false,
        &updated_at_utc,
    );
    result.report.extend(target_report);

    if result.has_critical_failures() {
        return ClanlistRefreshOutcome::finished(result, state_path_for_outcome, started_at_utc);
    }

    let payloads = result
        .model
        .as_ref()
        .map(|model| model.payloads.clone())
        .unwrap_or_default();
    let mut outcomes = Vec::new();
    for payload in &payloads {
        match update_panel_message_with_retry(&client, payload, now.timestamp(), &mut result.report)
            .await
        {
            Ok(message_id) => outcomes.push(xiii_clanlist::UpdateOperationOutcome {
                panel_name: payload.panel_name,
                edited_message_id: Some(message_id),
                failure_reason: None,
            }),
            Err(err) => {
                outcomes.push(xiii_clanlist::UpdateOperationOutcome {
                    panel_name: payload.panel_name,
                    edited_message_id: None,
                    failure_reason: Some(err),
                });
                break;
            }
        }
    }
    xiii_clanlist::apply_update_outcomes(&mut result, outcomes);

    let all_edited = result.model.as_ref().is_some_and(|model| {
        model
            .operations
            .iter()
            .filter(|operation| operation.status == "edited")
            .count()
            == 3
    });

    if all_edited {
        if let Some(model) = result.model.as_ref() {
            match xiii_clanlist::apply_successful_update_to_state(
                &mut state,
                model,
                &updated_at_utc,
            )
            .and_then(|()| {
                state.last_run_mode = Some(run_mode.to_owned());
                write_panel_state_json_atomic(&state_path, &state)
            }) {
                Ok(()) => {
                    xiii_clanlist::set_update_state_updated_path(
                        &mut result,
                        state_path.display().to_string(),
                    );
                    result.report.ok(
                        "state",
                        format!("Clanlist panel state updated: {}", state_path.display()),
                    );
                }
                Err(err) => result.report.fail("state", err),
            }
        }
    } else if result.model.as_ref().is_some_and(|model| {
        model
            .operations
            .iter()
            .any(|operation| operation.edited_message_id.is_some())
    }) {
        let partial_path =
            update_partial_recovery_path(&load.config, &now.format("%Y%m%dT%H%M%SZ").to_string());
        xiii_clanlist::set_update_partial_recovery_file_path(
            &mut result,
            partial_path.display().to_string(),
        );
        match write_partial_update_json(&partial_path, &result) {
            Ok(()) => result.report.warn(
                "state",
                format!(
                    "partial update recovery file written: {}",
                    partial_path.display()
                ),
            ),
            Err(err) => result.report.fail("state", err),
        }
    }

    ClanlistRefreshOutcome::finished(result, state_path_for_outcome, started_at_utc)
}

#[derive(Debug)]
struct ClanlistRefreshOutcome {
    result: xiii_clanlist::ClanlistUpdatePanelsResult,
    state_path: Option<PathBuf>,
    started_at_utc: String,
    finished_at_utc: String,
}

impl ClanlistRefreshOutcome {
    fn finished(
        result: xiii_clanlist::ClanlistUpdatePanelsResult,
        state_path: Option<PathBuf>,
        started_at_utc: String,
    ) -> Self {
        Self {
            result,
            state_path,
            started_at_utc,
            finished_at_utc: utc_timestamp_now(),
        }
    }
}

#[derive(Debug, Default)]
struct NonOverlapGuard {
    running: bool,
}

impl NonOverlapGuard {
    fn try_start(&mut self) -> bool {
        if self.running {
            false
        } else {
            self.running = true;
            true
        }
    }

    fn finish(&mut self) {
        self.running = false;
    }
}

#[derive(Debug, Serialize)]
struct ClanlistHealth {
    module: &'static str,
    status: &'static str,
    last_refresh_started_at_utc: String,
    last_refresh_finished_at_utc: String,
    last_success_at_utc: Option<String>,
    last_error: Option<String>,
    main_message_id: Option<u64>,
    admin_message_id: Option<u64>,
    steam_message_id: Option<u64>,
    counts: Option<ClanlistHealthCounts>,
    next_run_at_utc: Option<String>,
}

#[derive(Debug, Serialize)]
struct ClanlistHealthCounts {
    main_total_members: Option<usize>,
    admin_total_members: Option<usize>,
    steam_active_records: Option<usize>,
    steam_excluded_records: Option<usize>,
    steam_unknown_member_records: Option<usize>,
}

fn clanlist_health_from_outcome(
    outcome: &ClanlistRefreshOutcome,
    last_success_at_utc: Option<String>,
    next_run_at_utc: Option<String>,
) -> ClanlistHealth {
    let counts = outcome.result.report.counts();
    let status = if counts.fail > 0 {
        "fail"
    } else if counts.warn > 0 {
        "warn"
    } else {
        "ok"
    };
    let model = outcome.result.model.as_ref();
    let render_summary = model.map(|model| &model.render_summary);
    let operations = model
        .map(|model| model.operations.as_slice())
        .unwrap_or(&[]);
    let derived_success =
        (!outcome.result.has_critical_failures()).then_some(outcome.finished_at_utc.clone());

    ClanlistHealth {
        module: "clanlist",
        status,
        last_refresh_started_at_utc: outcome.started_at_utc.clone(),
        last_refresh_finished_at_utc: outcome.finished_at_utc.clone(),
        last_success_at_utc: last_success_at_utc.or(derived_success),
        last_error: first_failure_message(&outcome.result.report),
        main_message_id: operation_message_id(operations, "main"),
        admin_message_id: operation_message_id(operations, "admin"),
        steam_message_id: operation_message_id(operations, "steam"),
        counts: render_summary.map(|summary| ClanlistHealthCounts {
            main_total_members: summary.main_total_members,
            admin_total_members: summary.admin_total_members,
            steam_active_records: summary.steam_active_records,
            steam_excluded_records: summary.steam_excluded_records,
            steam_unknown_member_records: summary.steam_unknown_member_records,
        }),
        next_run_at_utc,
    }
}

fn operation_message_id(
    operations: &[xiii_clanlist::UpdateOperation],
    panel_name: &str,
) -> Option<u64> {
    operations
        .iter()
        .find(|operation| operation.panel_name == panel_name)
        .map(|operation| operation.message_id)
}

fn first_failure_message(report: &Report) -> Option<String> {
    report
        .items
        .iter()
        .find(|item| item.severity == Severity::Fail)
        .map(|item| format!("{}: {}", item.scope, item.message))
}

fn write_clanlist_health(path: &Path, health: &ClanlistHealth) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(health)
        .map_err(|err| format!("failed to render Clanlist health JSON: {err}"))?;
    fs::write(path, json)
        .map_err(|err| format!("failed to write health output {}: {err}", path.display()))
}

fn print_clanlist_refresh_summary(outcome: &ClanlistRefreshOutcome) {
    println!("Refresh window:");
    println!("  started_at_utc: {}", outcome.started_at_utc);
    println!("  finished_at_utc: {}", outcome.finished_at_utc);
    if let Some(path) = &outcome.state_path {
        println!("  state_file: {}", path.display());
    }
    print_report("Clanlist Refresh", &outcome.result.report);
}

fn run_clanlist_permission_failure(
    allow_discord_read: bool,
    allow_discord_write: bool,
    confirm_run_clanlist: bool,
) -> Option<Report> {
    let mut report = Report::new();
    if !allow_discord_read {
        report.fail(
            "safety",
            "--allow-discord-read is required before the Clanlist runtime can load a Discord token",
        );
    }
    if !allow_discord_write {
        report.fail(
            "safety",
            "--allow-discord-write is required before the Clanlist runtime can edit panel messages",
        );
    }
    if !confirm_run_clanlist {
        report.fail(
            "safety",
            "--confirm-run-clanlist is required before loading a Discord token or starting the Clanlist runtime",
        );
    }

    report.has_failures().then_some(report)
}

fn clanlist_interval_seconds(
    override_seconds: Option<u64>,
    config: &SuperbotConfig,
) -> Result<u64, String> {
    let seconds = override_seconds.unwrap_or(config.clanlist.auto_refresh_seconds);
    if seconds == 0 {
        Err("--interval-seconds must be greater than zero".to_owned())
    } else {
        Ok(seconds)
    }
}

fn preflight_clanlist_state_file(path: &Path, guild_id: u64) -> Result<(), String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read state file {}: {err}", path.display()))?;
    let state = xiii_clanlist::parse_panel_state_json(&text)
        .map_err(|err| format!("invalid Clanlist panel state JSON: {err}"))?;
    let mut report = Report::new();
    xiii_clanlist::validate_panel_state(&state, guild_id, &mut report);
    if report.has_failures() {
        let failures = report
            .items
            .iter()
            .filter(|item| item.severity == Severity::Fail)
            .map(|item| item.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        Err(format!("invalid Clanlist panel state: {failures}"))
    } else {
        Ok(())
    }
}

fn resolve_health_output_path(path: &Path, config: &SuperbotConfig) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("--health-output path is empty".to_owned());
    }
    if path.is_dir() {
        return Err(format!(
            "--health-output points to a directory: {}",
            path.display()
        ));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|err| format!("failed to resolve current directory: {err}"))?
            .join(path)
    };
    let parent = absolute.parent().ok_or_else(|| {
        format!(
            "--health-output has no parent directory: {}",
            path.display()
        )
    })?;
    if !parent.is_dir() {
        return Err(format!(
            "--health-output parent directory must already exist: {}",
            parent.display()
        ));
    }
    let parent = parent
        .canonicalize()
        .map_err(|err| format!("failed to resolve --health-output parent directory: {err}"))?;
    let file_name = absolute
        .file_name()
        .ok_or_else(|| format!("--health-output has no file name: {}", path.display()))?;
    let resolved = parent.join(file_name);
    validate_state_output_path(&resolved, config)
        .map_err(|err| err.replace("--state-output", "--health-output"))?;
    Ok(resolved)
}

fn utc_timestamp_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

async fn bootstrap_fresh_panels(
    env_file: PathBuf,
    allow_discord_read: bool,
    allow_discord_write: bool,
    confirm_bootstrap: bool,
    dry_run: bool,
    format: PreviewFormat,
    output: Option<PathBuf>,
    modules: Vec<String>,
) -> ExitCode {
    if let Some(report) =
        bootstrap_all_permission_failure(allow_discord_read, allow_discord_write, confirm_bootstrap)
    {
        println!("XIII Superbot Fresh Panel Bootstrap");
        println!("Mode: FAILED SAFETY GATE");
        print_report("Safety", &report);
        return ExitCode::from(2);
    }

    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let selected = match selected_modules(&modules, &load.config, SelectionMode::AllWhenEmpty) {
        Ok(modules) => modules,
        Err(err) => {
            println!("[FAIL] modules {err}");
            return ExitCode::from(2);
        }
    };
    let state_dir = superbot_state_dir_from_env(&env_file);
    let needs_discord_bootstrap = !dry_run
        && selected.iter().any(|module| {
            module.readiness() == ModuleReadiness::ReadyFull
                && module != &SuperbotModuleKind::Clanlist
                && module
                    .spec()
                    .fresh_state_file
                    .map(|file| !state_dir.join(file).is_file())
                    .unwrap_or(false)
        });
    let (discord_client, current_bot_id) = if !needs_discord_bootstrap {
        (None, None)
    } else {
        let token = match read_secret_from_env_file(&env_file, "DISCORD_TOKEN") {
            Ok(token) => token,
            Err(message) => {
                println!("[FAIL] discord {message}");
                return ExitCode::from(2);
            }
        };
        let client = Arc::new(DiscordHttpClient::new(token));
        let mut discord_report = Report::new();
        let current_user =
            match fetch_current_user_with_retry(client.as_ref(), &mut discord_report).await {
                Ok(user) => user,
                Err(err) => {
                    println!("[FAIL] discord {err}");
                    return ExitCode::from(2);
                }
            };
        (Some(client), Some(current_user.id.get()))
    };

    let mut report = Report::new();
    let mut planned = Vec::new();
    for module in selected {
        let spec = module.spec();
        if let Some(file_name) = spec.fresh_state_file {
            let path = state_dir.join(file_name);
            if path.is_file() {
                report.ok(
                    module.name(),
                    format!("fresh state already exists: {}", path.display()),
                );
                planned.push(FreshPanelPlanRow {
                    module: module.name(),
                    state_file: path.display().to_string(),
                    action: "skip_existing_state".to_owned(),
                    readiness: module.readiness().as_str(),
                    note: "state file already exists".to_owned(),
                });
            } else if dry_run {
                report.warn(
                    module.name(),
                    format!(
                        "would create fresh panel/board state for {} at {}",
                        spec.panel_description.unwrap_or("module panel"),
                        path.display()
                    ),
                );
                planned.push(FreshPanelPlanRow {
                    module: module.name(),
                    state_file: path.display().to_string(),
                    action: "would_create_missing_panel_state".to_owned(),
                    readiness: module.readiness().as_str(),
                    note: spec.panel_description.unwrap_or("module panel").to_owned(),
                });
            } else if module.readiness() != ModuleReadiness::ReadyFull {
                report.fail(
                    module.name(),
                    format!(
                        "fresh panel creation is refused while module readiness is {}; use --dry-run until the module is READY_FULL",
                        module.readiness().as_str()
                    ),
                );
            } else if module == SuperbotModuleKind::Clanlist {
                report.fail(
                    module.name(),
                    "Clanlist state is missing; use clanlist-bootstrap-new-panels for the proven create path before global bootstrap takes ownership",
                );
            } else if module == SuperbotModuleKind::Vacation {
                let Some(client) = discord_client.as_ref() else {
                    report.fail(module.name(), "Discord client unavailable outside dry-run");
                    continue;
                };
                let Some(bot_user_id) = current_bot_id else {
                    report.fail(
                        module.name(),
                        "current bot user id unavailable outside dry-run",
                    );
                    continue;
                };
                match bootstrap_vacation_panels(
                    client.as_ref(),
                    &load.config,
                    &state_dir,
                    bot_user_id,
                )
                .await
                {
                    Ok(summary) => {
                        report.ok(
                            module.name(),
                            format!(
                                "created vacation panels request_message_id={} active_message_id={}",
                                summary.request_message_id, summary.active_message_id
                            ),
                        );
                        planned.push(FreshPanelPlanRow {
                            module: module.name(),
                            state_file: path.display().to_string(),
                            action: "created_missing_panel_state".to_owned(),
                            readiness: module.readiness().as_str(),
                            note: format!(
                                "request_message_id={} active_message_id={}",
                                summary.request_message_id, summary.active_message_id
                            ),
                        });
                    }
                    Err(err) => report.fail(module.name(), err),
                }
            } else if module == SuperbotModuleKind::Discipline {
                let Some(client) = discord_client.as_ref() else {
                    report.fail(module.name(), "Discord client unavailable outside dry-run");
                    continue;
                };
                let Some(bot_user_id) = current_bot_id else {
                    report.fail(
                        module.name(),
                        "current bot user id unavailable outside dry-run",
                    );
                    continue;
                };
                match bootstrap_discipline_board(
                    client.as_ref(),
                    &load.config,
                    &state_dir,
                    bot_user_id,
                )
                .await
                {
                    Ok(message_id) => {
                        report.ok(
                            module.name(),
                            format!("created discipline board message_id={message_id}"),
                        );
                        planned.push(FreshPanelPlanRow {
                            module: module.name(),
                            state_file: path.display().to_string(),
                            action: "created_missing_panel_state".to_owned(),
                            readiness: module.readiness().as_str(),
                            note: format!("board_message_id={message_id}"),
                        });
                    }
                    Err(err) => report.fail(module.name(), err),
                }
            } else if module == SuperbotModuleKind::VoiceActivity {
                let Some(client) = discord_client.as_ref() else {
                    report.fail(module.name(), "Discord client unavailable outside dry-run");
                    continue;
                };
                let Some(bot_user_id) = current_bot_id else {
                    report.fail(
                        module.name(),
                        "current bot user id unavailable outside dry-run",
                    );
                    continue;
                };
                match bootstrap_voice_activity_panel(
                    client.as_ref(),
                    &load.config,
                    &state_dir,
                    bot_user_id,
                )
                .await
                {
                    Ok(message_id) => {
                        report.ok(
                            module.name(),
                            format!("created voice activity panel message_id={message_id}"),
                        );
                        planned.push(FreshPanelPlanRow {
                            module: module.name(),
                            state_file: path.display().to_string(),
                            action: "created_missing_panel_state".to_owned(),
                            readiness: module.readiness().as_str(),
                            note: format!("public_stats_message_id={message_id}"),
                        });
                    }
                    Err(err) => report.fail(module.name(), err),
                }
            } else if module == SuperbotModuleKind::Tickets {
                let Some(client) = discord_client.as_ref() else {
                    report.fail(module.name(), "Discord client unavailable outside dry-run");
                    continue;
                };
                let Some(bot_user_id) = current_bot_id else {
                    report.fail(
                        module.name(),
                        "current bot user id unavailable outside dry-run",
                    );
                    continue;
                };
                match bootstrap_ticket_panel(client.clone(), &load.config, &state_dir, bot_user_id)
                    .await
                {
                    Ok(message_id) => {
                        report.ok(
                            module.name(),
                            format!("created ticket panel message_id={message_id}"),
                        );
                        planned.push(FreshPanelPlanRow {
                            module: module.name(),
                            state_file: path.display().to_string(),
                            action: "created_missing_panel_state".to_owned(),
                            readiness: module.readiness().as_str(),
                            note: format!("ticket_panel_message_id={message_id}"),
                        });
                    }
                    Err(err) => report.fail(module.name(), err),
                }
            } else {
                report.fail(
                    module.name(),
                    format!(
                        "{} fresh panel creation is not execution-enabled yet; dry-run plan is available and old panels are untouched",
                        module.name()
                    ),
                );
            }
        } else {
            report.ok(
                module.name(),
                "no global persistent panel is required for this module",
            );
            planned.push(FreshPanelPlanRow {
                module: module.name(),
                state_file: "-".to_owned(),
                action: "not_required".to_owned(),
                readiness: module.readiness().as_str(),
                note: "module has no global persistent panel".to_owned(),
            });
        }
    }

    let content = match format {
        PreviewFormat::Text => render_fresh_panel_plan_text(dry_run, &state_dir, &planned, &report),
        PreviewFormat::Json => render_fresh_panel_plan_json(dry_run, &state_dir, &planned, &report),
    };
    if let Err(err) = emit_output(&content, output.as_deref(), Some(&load.config)) {
        println!("[FAIL] output {err}");
        return ExitCode::from(2);
    }

    report_exit_code(&report)
}

fn cutover_plan(env_file: PathBuf, modules: Vec<String>) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let selected = match selected_modules(&modules, &load.config, SelectionMode::AllWhenEmpty) {
        Ok(modules) => modules,
        Err(err) => {
            println!("[FAIL] modules {err}");
            return ExitCode::from(2);
        }
    };
    let state_dir = superbot_state_dir_from_env(&env_file);

    println!("XIII Superbot Cutover Plan");
    println!("Mode: READ ONLY / NO WRITES");
    println!("Env file: {}", env_file.display());
    println!("State dir: {}", state_dir.display());
    println!();
    println!("Old services to stop:");
    for module in &selected {
        println!("  - {}", module.spec().service_name);
    }
    println!();
    println!("Back up legacy DB/state before enabling writers:");
    for module in &selected {
        let spec = module.spec();
        if let Some(path) = legacy_path_for_module(*module, &load.config) {
            println!(
                "  - {}: copy \"{}\" to an offline backup",
                module.name(),
                path.display()
            );
        }
        if let Some(file) = spec.fresh_state_file {
            println!(
                "  - {} fresh state: {}",
                module.name(),
                state_dir.join(file).display()
            );
        }
    }
    println!();
    println!("Env flags to enable after verification:");
    for module in &selected {
        println!("  - {}=true", module.spec().env_flag);
    }
    println!();
    println!("Risk notes:");
    for module in &selected {
        println!("  - {}: {}", module.name(), module.spec().risk_note);
    }
    println!();
    println!("Suggested sequence:");
    println!("  1. Stop selected old services.");
    println!("  2. Back up the listed DB/state files.");
    println!("  3. Run verify-legacy and module-status.");
    println!("  4. Bootstrap missing fresh panels in dry-run mode first.");
    println!("  5. Enable one module flag at a time; keep voice activity last.");
    ExitCode::SUCCESS
}

async fn module_status(env_file: PathBuf) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let state_dir = superbot_state_dir_from_env(&env_file);
    let mut report = build_module_status_report(&load.config, &state_dir, false);
    append_temp_voice_db_status(&mut report, &load.config, false).await;
    println!("XIII Superbot Module Status");
    println!("Mode: READ ONLY / NO WRITES");
    print_readiness_matrix(&load.config);
    print_module_routes_and_jobs(&SuperbotModuleKind::all());
    print_report("Module Status", &report);
    if report.has_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

async fn verify_cutover(env_file: PathBuf) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let state_dir = superbot_state_dir_from_env(&env_file);
    let mut report = build_module_status_report(&load.config, &state_dir, true);
    append_temp_voice_db_status(&mut report, &load.config, true).await;
    let enabled = SuperbotModuleKind::all()
        .into_iter()
        .filter(|module| module.enabled(&load.config))
        .collect::<Vec<_>>();
    if superbot_require_old_services_stopped_from_env(&env_file) {
        report.extend(evaluate_old_services_dir(
            &enabled,
            Some(&state_dir.join("service-status")),
        ));
    } else {
        report.warn(
            "service_guard",
            "SUPERBOT_REQUIRE_OLD_SERVICES_STOPPED=false; old service guard is not enforced",
        );
    }
    println!("XIII Superbot Cutover Verification");
    println!("Mode: READ ONLY / NO WRITES");
    print_readiness_matrix(&load.config);
    print_report("Cutover Verification", &report);
    if report.has_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_superbot(
    env_file: PathBuf,
    allow_discord_read: bool,
    allow_discord_write: bool,
    confirm_run_superbot: bool,
    modules: Vec<String>,
    dry_run: bool,
    health_output: Option<PathBuf>,
    require_old_services_stopped: bool,
    old_services_dir: Option<PathBuf>,
) -> ExitCode {
    if let Some(report) = run_superbot_permission_failure(
        allow_discord_read,
        allow_discord_write,
        confirm_run_superbot,
    ) {
        println!("XIII Superbot Runtime");
        println!("Mode: FAILED SAFETY GATE");
        print_report("Safety", &report);
        return ExitCode::from(2);
    }

    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let selected = match selected_modules(&modules, &load.config, SelectionMode::EnabledWhenEmpty) {
        Ok(modules) => modules,
        Err(err) => {
            println!("[FAIL] modules {err}");
            return ExitCode::from(2);
        }
    };
    if selected.is_empty() {
        println!("[FAIL] modules no modules selected or enabled; pass --modules or enable module env flags");
        return ExitCode::from(2);
    }

    let state_dir = superbot_state_dir_from_env(&env_file);
    let mut report = build_module_status_report(&load.config, &state_dir, true);
    for module in &selected {
        if module.readiness() != ModuleReadiness::ReadyFull {
            report.fail(
                "runtime",
                format!(
                    "selected module {} is {}; dry-run and real runtime refuse unsafe modules",
                    module.name(),
                    module.readiness().as_str()
                ),
            );
        }
    }
    if require_old_services_stopped {
        report.extend(evaluate_old_services_dir(
            &selected,
            old_services_dir.as_deref(),
        ));
    } else {
        report.warn(
            "service_guard",
            "old service stop state was not verified; production run should use --require-old-services-stopped",
        );
    }

    println!("XIII Superbot Runtime");
    println!(
        "Mode: {}",
        if dry_run {
            "DRY RUN / NO DISCORD CONNECTION"
        } else {
            "NOT STARTED"
        }
    );
    println!("Selected modules: {}", join_module_names(&selected));
    println!("Gateway: {}", if dry_run { "DISABLED" } else { "DEFERRED" });
    println!("Command sync: DISABLED by default");
    println!("Module writers: gated by readiness, explicit flags, and old-service guards");
    print_module_routes_and_jobs(&selected);
    print_report("Runtime Preflight", &report);

    if let Some(path) = health_output.as_deref() {
        match resolve_health_output_path(path, &load.config).and_then(|path| {
            let content = format!(
                "{{\n  \"module\": \"superbot\",\n  \"status\": \"{}\",\n  \"selected_modules\": \"{}\",\n  \"dry_run\": {}\n}}\n",
                if report.has_failures() { "fail" } else if report.counts().warn > 0 { "warn" } else { "ok" },
                join_module_names(&selected),
                dry_run
            );
            fs::write(&path, content.as_bytes())
                .map_err(|err| format!("failed to write health output {}: {err}", path.display()))
        }) {
            Ok(()) => println!("[OK] health output written"),
            Err(err) => println!("[WARN] health output skipped: {err}"),
        }
    }

    if report.has_failures() {
        return ExitCode::from(2);
    }

    if dry_run {
        return ExitCode::SUCCESS;
    }

    let disabled_selected = selected
        .iter()
        .copied()
        .filter(|module| !module.enabled(&load.config))
        .collect::<Vec<_>>();
    if !disabled_selected.is_empty() {
        println!(
            "[FAIL] selected real runtime modules must also be enabled in env: {}",
            join_module_names(&disabled_selected)
        );
        return ExitCode::from(2);
    }

    if selected
        .iter()
        .any(|module| module.readiness() != ModuleReadiness::ReadyFull)
    {
        println!("[FAIL] selected modules include PARTIAL/BLOCKED modules; refusing real runtime");
        return ExitCode::from(2);
    }

    if !require_old_services_stopped {
        println!(
            "[FAIL] real run-superbot mode requires --require-old-services-stopped for writer modules"
        );
        return ExitCode::from(2);
    }

    if selected.contains(&SuperbotModuleKind::TempVoice) && !load.config.modules.temp_voice {
        println!(
            "[FAIL] TEMP_VOICE_ENABLED=false; real temp_voice runtime requires the env flag as well as --modules temp_voice"
        );
        return ExitCode::from(2);
    }

    if selected.contains(&SuperbotModuleKind::TempVoice) && !require_old_services_stopped {
        println!(
            "[FAIL] temp_voice is write-capable and requires --require-old-services-stopped in real run-superbot mode"
        );
        return ExitCode::from(2);
    }

    if selected.contains(&SuperbotModuleKind::VoiceActivity) {
        match voice_activity_runtime_cutover_guard(
            &load.config,
            &superbot_state_dir_from_env(&env_file),
        )
        .await
        {
            Ok(()) => {}
            Err(err) => {
                println!("[FAIL] voice_activity cutover guard {err}");
                return ExitCode::from(2);
            }
        }
    }

    if selected == vec![SuperbotModuleKind::Clanlist] {
        println!("[OK] starting production Clanlist runtime through the proven run-clanlist path");
        let old_service_status_file = old_services_dir
            .as_deref()
            .map(|dir| dir.join("xiii-clanlist.service.txt"));
        return run_clanlist(
            env_file,
            allow_discord_read,
            allow_discord_write,
            true,
            None,
            None,
            false,
            false,
            false,
            require_old_services_stopped,
            old_service_status_file,
            health_output,
        )
        .await;
    }

    if selected == vec![SuperbotModuleKind::TempVoice] {
        println!(
            "[OK] starting production Temp Voice runtime with one Gateway connection and DB-owned deletion guard"
        );
        return run_temp_voice_runtime(env_file, load.config, health_output).await;
    }

    println!("[OK] starting mixed production Superbot runtime with one Gateway connection");
    run_mixed_superbot_runtime(env_file, load.config, selected, health_output).await
}

#[derive(Clone)]
struct MixedSuperbotRuntime {
    env_file: PathBuf,
    state_dir: PathBuf,
    config: SuperbotConfig,
    selected: Vec<SuperbotModuleKind>,
    http: Arc<DiscordHttpClient>,
    temp_voice: Option<xiii_tempvoice::runtime::TempVoiceRuntime>,
    vacation_repo: Option<xiii_vacation::repository::LegacySqliteVacationRepository>,
    vacation_discord: Option<xiii_vacation::discord_io::VacationDiscordHttp>,
    discipline_repo: Option<xiii_discipline::repository::LegacySqliteDisciplineRepository>,
    discipline_discord: Option<xiii_discipline::discord_io::DisciplineDiscordHttp>,
    recruit_repo: Option<xiii_recruit::repository::LegacySqliteRecruitRepository>,
    recruit_discord: Option<xiii_recruit::discord_io::RecruitDiscordHttp>,
    voice_repo: Option<xiii_voice_activity::repository::LegacySqliteVoiceActivityRepository>,
    ticket_repo: Option<xiii_tickets::repository::LegacySqliteTicketRepository>,
    ticket_discord: Option<xiii_tickets::discord_io::TicketDiscordHttp>,
    temp_occupancy: Arc<Mutex<TempVoiceOccupancy>>,
    voice_activity_occupancy: Arc<Mutex<TempVoiceOccupancy>>,
    voice_activity_members: Arc<Mutex<HashMap<u64, VoiceCachedMember>>>,
}

async fn run_mixed_superbot_runtime(
    env_file: PathBuf,
    config: SuperbotConfig,
    selected: Vec<SuperbotModuleKind>,
    health_output: Option<PathBuf>,
) -> ExitCode {
    let token = match read_secret_from_env_file(&env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(message) => {
            println!("[FAIL] discord {message}");
            return ExitCode::from(2);
        }
    };

    let http = Arc::new(DiscordHttpClient::new(token.clone()));
    let temp_voice = if selected.contains(&SuperbotModuleKind::TempVoice) {
        match xiii_tempvoice::repository::LegacySqliteTempVoiceRepository::open_existing_writable(
            &config.legacy_paths.temp_voice_db.resolved,
        )
        .await
        {
            Ok(repository) => Some(xiii_tempvoice::runtime::TempVoiceRuntime::new(
                repository,
                xiii_tempvoice::discord_io::TempVoiceDiscordHttp::new(http.clone()),
            )),
            Err(err) => {
                println!("[FAIL] temp_voice repository {err}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    let vacation_repo = if selected.contains(&SuperbotModuleKind::Vacation) {
        match xiii_vacation::repository::LegacySqliteVacationRepository::open_existing_writable(
            &config.legacy_paths.vacation_db.resolved,
        )
        .await
        {
            Ok(repository) => Some(repository),
            Err(err) => {
                println!("[FAIL] vacation repository {err}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    let discipline_repo = if selected.contains(&SuperbotModuleKind::Discipline) {
        match xiii_discipline::repository::LegacySqliteDisciplineRepository::open_existing_writable(
            &config.legacy_paths.discipline_db.resolved,
        )
        .await
        {
            Ok(repository) => Some(repository),
            Err(err) => {
                println!("[FAIL] discipline repository {err}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    let recruit_repo = if selected.contains(&SuperbotModuleKind::Recruit) {
        match xiii_recruit::repository::LegacySqliteRecruitRepository::open_existing_writable(
            &config.legacy_paths.recruit_db.resolved,
        )
        .await
        {
            Ok(repository) => Some(repository),
            Err(err) => {
                println!("[FAIL] recruit repository {err}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    let voice_repo = if selected.contains(&SuperbotModuleKind::VoiceActivity) {
        match xiii_voice_activity::repository::LegacySqliteVoiceActivityRepository::open_existing_writable(
            &config.legacy_paths.voice_db.resolved,
        )
        .await
        {
            Ok(repository) => Some(repository),
            Err(err) => {
                println!("[FAIL] voice_activity repository {err}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    let ticket_repo = if selected.contains(&SuperbotModuleKind::Tickets) {
        match xiii_tickets::repository::LegacySqliteTicketRepository::open_existing_writable(
            &config.legacy_paths.ticket_db.resolved,
        )
        .await
        {
            Ok(repository) => Some(repository),
            Err(err) => {
                println!("[FAIL] tickets repository {err}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };

    let state_dir = superbot_state_dir_from_env(&env_file);
    let runtime = MixedSuperbotRuntime {
        env_file,
        state_dir,
        config,
        selected,
        http: http.clone(),
        temp_voice,
        vacation_repo,
        vacation_discord: Some(xiii_vacation::discord_io::VacationDiscordHttp::new(
            http.clone(),
        )),
        discipline_repo,
        discipline_discord: Some(xiii_discipline::discord_io::DisciplineDiscordHttp::new(
            http.clone(),
        )),
        recruit_repo,
        recruit_discord: Some(xiii_recruit::discord_io::RecruitDiscordHttp::new(
            http.clone(),
        )),
        voice_repo,
        ticket_repo,
        ticket_discord: Some(xiii_tickets::discord_io::TicketDiscordHttp::new(
            http.clone(),
        )),
        temp_occupancy: Arc::new(Mutex::new(TempVoiceOccupancy::default())),
        voice_activity_occupancy: Arc::new(Mutex::new(TempVoiceOccupancy::default())),
        voice_activity_members: Arc::new(Mutex::new(HashMap::new())),
    };

    let mut identity_report = Report::new();
    match fetch_current_user_with_retry(http.as_ref(), &mut identity_report).await {
        Ok(user) => println!("[OK] current bot user id = {}", user.id.get()),
        Err(err) => {
            println!("[FAIL] discord {err}");
            return ExitCode::from(2);
        }
    }

    start_mixed_schedulers(runtime.clone(), health_output.clone());

    let mut intents = Intents::GUILDS | Intents::GUILD_VOICE_STATES | Intents::GUILD_MEMBERS;
    if runtime.selected.contains(&SuperbotModuleKind::Tickets) {
        intents |= Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT | Intents::DIRECT_MESSAGES;
    }
    let mut shard = Shard::new(ShardId::ONE, token, intents);
    println!("[OK] Superbot Gateway starting");
    println!("[OK] slash command auto-sync is disabled");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("[OK] shutdown signal received; Superbot runtime stopping gracefully");
                return ExitCode::SUCCESS;
            }
            event = shard.next_event(EventTypeFlags::all()) => {
                let Some(event) = event else {
                    println!("[FAIL] Superbot Gateway stream ended");
                    return ExitCode::from(2);
                };
                match event {
                    Ok(event) => handle_mixed_gateway_event(event, &runtime).await,
                    Err(err) => println!("[WARN] Superbot Gateway event receive failed: {err}"),
                }
            }
        }
    }
}

fn start_mixed_schedulers(runtime: MixedSuperbotRuntime, health_output: Option<PathBuf>) {
    if runtime.selected.contains(&SuperbotModuleKind::Clanlist) {
        let env_file = runtime.env_file.clone();
        let interval = runtime.config.clanlist.auto_refresh_seconds.max(60);
        tokio::spawn(async move {
            let mut guard = NonOverlapGuard::default();
            loop {
                tokio::time::sleep(Duration::from_secs(interval)).await;
                if guard.try_start() {
                    let outcome = clanlist_refresh_once(
                        &env_file,
                        None,
                        xiii_clanlist::SteamPreviewMode::Auto,
                        true,
                        None,
                        "run_superbot",
                        false,
                    )
                    .await;
                    print_clanlist_refresh_summary(&outcome);
                    guard.finish();
                }
            }
        });
    }
    if runtime.selected.contains(&SuperbotModuleKind::Vacation) {
        let expiry_runtime = runtime.clone();
        tokio::spawn(async move {
            let mut guard = NonOverlapGuard::default();
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if guard.try_start() {
                    if let Err(err) = vacation_expiration_tick(&expiry_runtime).await {
                        println!("[WARN] vacation expiration worker failed: {err}");
                    }
                    guard.finish();
                }
            }
        });
        let panel_runtime = runtime.clone();
        tokio::spawn(async move {
            let mut guard = NonOverlapGuard::default();
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if guard.try_start() {
                    if let Err(err) = vacation_active_panel_refresh_tick(&panel_runtime).await {
                        println!("[WARN] vacation active panel refresh failed: {err}");
                    }
                    guard.finish();
                }
            }
        });
    }
    if runtime.selected.contains(&SuperbotModuleKind::Discipline) {
        let expiry_runtime = runtime.clone();
        tokio::spawn(async move {
            let mut guard = NonOverlapGuard::default();
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if guard.try_start() {
                    if let Err(err) = discipline_expiration_tick(&expiry_runtime).await {
                        println!("[WARN] discipline expiration worker failed: {err}");
                    }
                    guard.finish();
                }
            }
        });
        let board_runtime = runtime.clone();
        tokio::spawn(async move {
            let mut guard = NonOverlapGuard::default();
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if guard.try_start() {
                    if let Err(err) = discipline_board_refresh_tick(&board_runtime).await {
                        println!("[WARN] discipline board refresh failed: {err}");
                    }
                    guard.finish();
                }
            }
        });
    }
    if runtime.selected.contains(&SuperbotModuleKind::Recruit) {
        let recruit_runtime = runtime.clone();
        tokio::spawn(async move {
            let mut guard = NonOverlapGuard::default();
            let interval = recruit_runtime
                .config
                .recruit
                .check_interval_seconds
                .max(60);
            loop {
                tokio::time::sleep(Duration::from_secs(interval)).await;
                if guard.try_start() {
                    if let Err(err) = recruit_due_checker_tick(&recruit_runtime).await {
                        println!("[WARN] recruit due checker failed: {err}");
                    }
                    guard.finish();
                }
            }
        });
    }
    if runtime
        .selected
        .contains(&SuperbotModuleKind::VoiceActivity)
    {
        let heartbeat_runtime = runtime.clone();
        tokio::spawn(async move {
            let mut guard = NonOverlapGuard::default();
            let interval = heartbeat_runtime
                .config
                .voice_activity
                .heartbeat_interval_seconds
                .max(60);
            loop {
                tokio::time::sleep(Duration::from_secs(interval)).await;
                if guard.try_start() {
                    if let Err(err) = voice_activity_heartbeat_tick(&heartbeat_runtime).await {
                        println!("[WARN] voice_activity heartbeat failed: {err}");
                    }
                    guard.finish();
                }
            }
        });
        let panel_runtime = runtime.clone();
        tokio::spawn(async move {
            let mut guard = NonOverlapGuard::default();
            let interval = panel_runtime
                .config
                .voice_activity
                .public_stats_update_interval_seconds
                .max(60);
            loop {
                tokio::time::sleep(Duration::from_secs(interval)).await;
                if guard.try_start() {
                    if let Err(err) = voice_activity_public_panel_refresh_tick(&panel_runtime).await
                    {
                        println!("[WARN] voice_activity public panel refresh failed: {err}");
                    }
                    guard.finish();
                }
            }
        });
        let report_runtime = runtime.clone();
        tokio::spawn(async move {
            let mut guard = NonOverlapGuard::default();
            let interval = report_runtime
                .config
                .voice_activity
                .auto_report_check_interval_seconds
                .max(60);
            loop {
                tokio::time::sleep(Duration::from_secs(interval)).await;
                if guard.try_start() {
                    if let Err(err) = voice_activity_auto_report_tick(&report_runtime).await {
                        println!("[WARN] voice_activity auto report failed: {err}");
                    }
                    guard.finish();
                }
            }
        });
    }

    if runtime.selected.contains(&SuperbotModuleKind::Tickets) {
        let ticket_runtime = runtime.clone();
        tokio::spawn(async move {
            let mut guard = NonOverlapGuard::default();
            let interval = ticket_runtime.config.tickets.google_poll_seconds.max(30);
            loop {
                tokio::time::sleep(Duration::from_secs(interval)).await;
                if guard.try_start() {
                    if let Err(err) = ticket_google_forms_poll_tick(&ticket_runtime).await {
                        println!("[WARN] ticket Google Forms poll failed: {err}");
                    }
                    guard.finish();
                }
            }
        });
    }

    if let Some(path) = health_output {
        let _ = fs::write(path, b"{\"module\":\"superbot\",\"status\":\"starting\"}\n");
    }
}

async fn handle_mixed_gateway_event(event: Event, runtime: &MixedSuperbotRuntime) {
    match event {
        Event::Ready(ready) => {
            println!(
                "[OK] Gateway READY for Superbot session user_id={}",
                ready.user.id.get()
            );
        }
        Event::GuildCreate(guild) => {
            if let GuildCreate::Available(guild) = guild.as_ref() {
                let mut occupancy = runtime.temp_occupancy.lock().await;
                occupancy.seed_guild(guild.id.get(), guild.owner_id.get(), &guild.voice_states);
                {
                    let mut voice_occupancy = runtime.voice_activity_occupancy.lock().await;
                    voice_occupancy.seed_guild(
                        guild.id.get(),
                        guild.owner_id.get(),
                        &guild.voice_states,
                    );
                }
                if runtime
                    .selected
                    .contains(&SuperbotModuleKind::VoiceActivity)
                {
                    seed_voice_activity_members(runtime, &guild.members).await;
                    if let Err(err) = voice_activity_startup_reconcile(
                        runtime,
                        guild.id.get(),
                        &guild.voice_states,
                    )
                    .await
                    {
                        println!("[WARN] voice_activity startup reconciliation failed: {err}");
                    }
                }
            }
        }
        Event::InteractionCreate(interaction) => {
            handle_mixed_interaction(&interaction.0, runtime).await;
        }
        Event::VoiceStateUpdate(update) => {
            if let Some(temp_voice) = runtime.temp_voice.as_ref() {
                handle_temp_voice_state_update(
                    update.clone(),
                    temp_voice,
                    &runtime.temp_occupancy,
                    &runtime.config,
                )
                .await;
            }
            if runtime.selected.contains(&SuperbotModuleKind::Recruit) {
                handle_recruit_voice_state_update(update.clone(), runtime).await;
            }
            if runtime
                .selected
                .contains(&SuperbotModuleKind::VoiceActivity)
            {
                handle_voice_activity_state_update(update, runtime).await;
            }
        }
        Event::MessageCreate(message) => {
            if runtime.selected.contains(&SuperbotModuleKind::Tickets) {
                handle_ticket_message_create(&message.0, runtime).await;
            }
        }
        Event::ChannelDelete(channel) => {
            if let Some(temp_voice) = runtime.temp_voice.as_ref() {
                let channel_id = channel.id.get();
                if let Err(err) = temp_voice
                    .delete_tracked_channel_if_empty(channel_id, 0)
                    .await
                {
                    println!(
                        "[WARN] temp_voice channel-delete cleanup skipped for {channel_id}: {err}"
                    );
                }
            }
        }
        _ => {}
    }
}

async fn handle_mixed_interaction(interaction: &Interaction, runtime: &MixedSuperbotRuntime) {
    if let Some(temp_voice) = runtime.temp_voice.as_ref() {
        handle_temp_voice_interaction(interaction, temp_voice, &runtime.temp_occupancy).await;
    }
    match interaction.data.as_ref() {
        Some(InteractionData::ApplicationCommand(data)) => match data.name.as_str() {
            "vacations" if runtime.selected.contains(&SuperbotModuleKind::Vacation) => {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    &xiii_vacation::commands::VACATIONS_DISABLED_RESPONSE.replace(
                        "{channel_id}",
                        &runtime.config.vacation.active_panel_channel_id.to_string(),
                    ),
                )
                .await;
            }
            "recruits" if runtime.selected.contains(&SuperbotModuleKind::Recruit) => {
                handle_recruits_command(interaction, runtime).await;
            }
            "recruit-panel" if runtime.selected.contains(&SuperbotModuleKind::Recruit) => {
                handle_recruit_panel_command(interaction, data.as_ref(), runtime).await;
            }
            "discipline" if runtime.selected.contains(&SuperbotModuleKind::Discipline) => {
                handle_discipline_command(interaction, data.as_ref(), runtime).await;
            }
            "voice-top"
                if runtime
                    .selected
                    .contains(&SuperbotModuleKind::VoiceActivity) =>
            {
                let message = xiii_voice_activity::render::voice_top_disabled_response(
                    runtime.config.voice_activity.stats_panel_channel_id,
                );
                respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &message)
                    .await;
            }
            "inactive-check"
                if runtime
                    .selected
                    .contains(&SuperbotModuleKind::VoiceActivity) =>
            {
                handle_voice_activity_inactive_command(interaction, runtime, "7d", 0).await;
            }
            "add" if runtime.selected.contains(&SuperbotModuleKind::Tickets) => {
                handle_ticket_add_remove_command(interaction, data.as_ref(), runtime, true).await;
            }
            "remove" if runtime.selected.contains(&SuperbotModuleKind::Tickets) => {
                handle_ticket_add_remove_command(interaction, data.as_ref(), runtime, false).await;
            }
            "custom-ticket" if runtime.selected.contains(&SuperbotModuleKind::Tickets) => {
                handle_ticket_custom_command(interaction, data.as_ref(), runtime).await;
            }
            _ => {}
        },
        Some(InteractionData::MessageComponent(data)) => {
            let custom_id = data.custom_id.as_str();
            if custom_id.starts_with("vacation:")
                && runtime.selected.contains(&SuperbotModuleKind::Vacation)
            {
                handle_vacation_component(interaction, custom_id, runtime).await;
            } else if custom_id.starts_with("xiii_recruit_")
                && runtime.selected.contains(&SuperbotModuleKind::Recruit)
            {
                handle_recruit_component(interaction, custom_id, runtime).await;
            } else if custom_id.starts_with("xiii:")
                && runtime.selected.contains(&SuperbotModuleKind::Discipline)
            {
                handle_discipline_component(interaction, data.as_ref(), custom_id, runtime).await;
            } else if (custom_id.starts_with("public-stats-panel:")
                || custom_id.starts_with("inactive-check:"))
                && runtime
                    .selected
                    .contains(&SuperbotModuleKind::VoiceActivity)
            {
                handle_voice_activity_component(interaction, data.as_ref(), runtime).await;
            } else if xiii_tickets::interactions::route_ticket_component(custom_id).is_some()
                && runtime.selected.contains(&SuperbotModuleKind::Tickets)
            {
                handle_ticket_component(interaction, custom_id, runtime).await;
            }
        }
        Some(InteractionData::ModalSubmit(data)) => {
            let custom_id = data.custom_id.as_str();
            if custom_id == xiii_vacation::interactions::REQUEST_MODAL_ID {
                handle_vacation_modal(interaction, data, runtime).await;
            } else if custom_id.starts_with("xiii_recruit_reject_modal:")
                || custom_id.starts_with("xiii_recruit_extend_modal:")
            {
                handle_recruit_modal(interaction, data, runtime).await;
            } else if custom_id.starts_with("xiii:issue:modal:")
                || custom_id.starts_with("xiii:remove:modal:")
            {
                handle_discipline_modal(interaction, data, runtime).await;
            } else if custom_id.starts_with("ticket_staff_notes_modal:") {
                handle_ticket_staff_notes_modal(interaction, data, runtime).await;
            }
        }
        None | Some(_) => {}
    }
}

async fn respond_interaction_ephemeral_http(
    client: &DiscordHttpClient,
    interaction: &Interaction,
    content: &str,
) {
    let response = twilight_model::http::interaction::InteractionResponse {
        kind: twilight_model::http::interaction::InteractionResponseType::ChannelMessageWithSource,
        data: Some(twilight_model::http::interaction::InteractionResponseData {
            allowed_mentions: Some(AllowedMentions::default()),
            content: Some(content.to_owned()),
            flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
            ..Default::default()
        }),
    };
    if let Err(err) = client
        .interaction(Id::<ApplicationMarker>::new(
            interaction.application_id.get(),
        ))
        .create_response(
            Id::<twilight_model::id::marker::InteractionMarker>::new(interaction.id.get()),
            interaction.token.as_str(),
            &response,
        )
        .await
    {
        println!("[WARN] failed to respond to interaction: {err}");
    }
}

async fn respond_interaction_ephemeral_components_http(
    client: &DiscordHttpClient,
    interaction: &Interaction,
    content: &str,
    components: Vec<Component>,
) {
    let response = twilight_model::http::interaction::InteractionResponse {
        kind: twilight_model::http::interaction::InteractionResponseType::ChannelMessageWithSource,
        data: Some(twilight_model::http::interaction::InteractionResponseData {
            allowed_mentions: Some(AllowedMentions::default()),
            content: Some(content.to_owned()),
            components: Some(components),
            flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
            ..Default::default()
        }),
    };
    if let Err(err) = client
        .interaction(Id::<ApplicationMarker>::new(
            interaction.application_id.get(),
        ))
        .create_response(
            Id::<twilight_model::id::marker::InteractionMarker>::new(interaction.id.get()),
            interaction.token.as_str(),
            &response,
        )
        .await
    {
        println!("[WARN] failed to respond to interaction with components: {err}");
    }
}

async fn respond_interaction_ephemeral_embeds_http(
    client: &DiscordHttpClient,
    interaction: &Interaction,
    embeds: Vec<Embed>,
    components: Option<Vec<Component>>,
) {
    let response = twilight_model::http::interaction::InteractionResponse {
        kind: twilight_model::http::interaction::InteractionResponseType::ChannelMessageWithSource,
        data: Some(twilight_model::http::interaction::InteractionResponseData {
            allowed_mentions: Some(AllowedMentions::default()),
            embeds: Some(embeds),
            components,
            flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
            ..Default::default()
        }),
    };
    if let Err(err) = client
        .interaction(Id::<ApplicationMarker>::new(
            interaction.application_id.get(),
        ))
        .create_response(
            Id::<twilight_model::id::marker::InteractionMarker>::new(interaction.id.get()),
            interaction.token.as_str(),
            &response,
        )
        .await
    {
        println!("[WARN] failed to respond to interaction with embeds: {err}");
    }
}

async fn respond_interaction_embeds_http(
    client: &DiscordHttpClient,
    interaction: &Interaction,
    embeds: Vec<Embed>,
    components: Option<Vec<Component>>,
) {
    let response = twilight_model::http::interaction::InteractionResponse {
        kind: twilight_model::http::interaction::InteractionResponseType::ChannelMessageWithSource,
        data: Some(twilight_model::http::interaction::InteractionResponseData {
            allowed_mentions: Some(AllowedMentions::default()),
            embeds: Some(embeds),
            components,
            ..Default::default()
        }),
    };
    if let Err(err) = client
        .interaction(Id::<ApplicationMarker>::new(
            interaction.application_id.get(),
        ))
        .create_response(
            Id::<twilight_model::id::marker::InteractionMarker>::new(interaction.id.get()),
            interaction.token.as_str(),
            &response,
        )
        .await
    {
        println!("[WARN] failed to respond to interaction with public embeds: {err}");
    }
}

async fn respond_interaction_update_embeds_http(
    client: &DiscordHttpClient,
    interaction: &Interaction,
    embeds: Vec<Embed>,
    components: Option<Vec<Component>>,
) {
    let response = twilight_model::http::interaction::InteractionResponse {
        kind: twilight_model::http::interaction::InteractionResponseType::UpdateMessage,
        data: Some(twilight_model::http::interaction::InteractionResponseData {
            allowed_mentions: Some(AllowedMentions::default()),
            embeds: Some(embeds),
            components,
            ..Default::default()
        }),
    };
    if let Err(err) = client
        .interaction(Id::<ApplicationMarker>::new(
            interaction.application_id.get(),
        ))
        .create_response(
            Id::<twilight_model::id::marker::InteractionMarker>::new(interaction.id.get()),
            interaction.token.as_str(),
            &response,
        )
        .await
    {
        println!("[WARN] failed to update interaction message embeds: {err}");
    }
}

async fn respond_interaction_modal_http(
    client: &DiscordHttpClient,
    interaction: &Interaction,
    custom_id: &str,
    title: &str,
    components: Vec<Component>,
) {
    let response = twilight_model::http::interaction::InteractionResponse {
        kind: twilight_model::http::interaction::InteractionResponseType::Modal,
        data: Some(twilight_model::http::interaction::InteractionResponseData {
            custom_id: Some(custom_id.to_owned()),
            title: Some(title.to_owned()),
            components: Some(components),
            ..Default::default()
        }),
    };
    if let Err(err) = client
        .interaction(Id::<ApplicationMarker>::new(
            interaction.application_id.get(),
        ))
        .create_response(
            Id::<twilight_model::id::marker::InteractionMarker>::new(interaction.id.get()),
            interaction.token.as_str(),
            &response,
        )
        .await
    {
        println!("[WARN] failed to respond with modal: {err}");
    }
}

fn action_row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { components })
}

fn button(custom_id: impl Into<String>, label: impl Into<String>, style: ButtonStyle) -> Component {
    button_with_disabled(custom_id, label, style, false)
}

fn button_with_disabled(
    custom_id: impl Into<String>,
    label: impl Into<String>,
    style: ButtonStyle,
    disabled: bool,
) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id.into()),
        disabled,
        emoji: None,
        label: Some(label.into()),
        style,
        url: None,
        sku_id: None,
    })
}

fn text_select(
    custom_id: impl Into<String>,
    placeholder: impl Into<String>,
    options: Vec<(&str, &str, bool)>,
) -> Component {
    Component::SelectMenu(SelectMenu {
        channel_types: None,
        custom_id: custom_id.into(),
        default_values: None,
        disabled: false,
        kind: SelectMenuType::Text,
        max_values: Some(1),
        min_values: Some(1),
        options: Some(
            options
                .into_iter()
                .map(|(label, value, default)| SelectMenuOption {
                    default,
                    description: None,
                    emoji: None,
                    label: label.to_owned(),
                    value: value.to_owned(),
                })
                .collect(),
        ),
        placeholder: Some(placeholder.into()),
    })
}

fn text_select_owned(
    custom_id: impl Into<String>,
    placeholder: impl Into<String>,
    options: Vec<(String, String, Option<String>, bool)>,
) -> Component {
    Component::SelectMenu(SelectMenu {
        channel_types: None,
        custom_id: custom_id.into(),
        default_values: None,
        disabled: false,
        kind: SelectMenuType::Text,
        max_values: Some(1),
        min_values: Some(1),
        options: Some(
            options
                .into_iter()
                .map(|(label, value, description, default)| SelectMenuOption {
                    default,
                    description,
                    emoji: None,
                    label,
                    value,
                })
                .collect(),
        ),
        placeholder: Some(placeholder.into()),
    })
}

fn user_select(custom_id: impl Into<String>, placeholder: impl Into<String>) -> Component {
    Component::SelectMenu(SelectMenu {
        channel_types: None,
        custom_id: custom_id.into(),
        default_values: None,
        disabled: false,
        kind: SelectMenuType::User,
        max_values: Some(1),
        min_values: Some(1),
        options: None,
        placeholder: Some(placeholder.into()),
    })
}

fn text_input(
    custom_id: impl Into<String>,
    label: impl Into<String>,
    style: TextInputStyle,
    required: bool,
) -> Component {
    Component::TextInput(TextInput {
        custom_id: custom_id.into(),
        label: label.into(),
        max_length: None,
        min_length: None,
        placeholder: None,
        required: Some(required),
        style,
        value: None,
    })
}

fn modal_value(
    data: &twilight_model::application::interaction::modal::ModalInteractionData,
    custom_id: &str,
) -> Option<String> {
    data.components
        .iter()
        .flat_map(|row| row.components.iter())
        .find(|component| component.custom_id == custom_id)
        .and_then(|component| component.value.clone())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VacationPanelStateFile {
    source: String,
    guild_id: u64,
    bot_user_id: u64,
    request_panel: PanelStateTarget,
    active_panel: PanelStateTarget,
    created_at_utc: String,
    last_updated_at_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DisciplinePanelStateFile {
    source: String,
    guild_id: u64,
    bot_user_id: u64,
    board: PanelStateTarget,
    created_at_utc: String,
    last_updated_at_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PanelStateTarget {
    channel_id: u64,
    message_id: u64,
}

fn superbot_state_file(state_dir: &Path, file_name: &str) -> PathBuf {
    state_dir.join(file_name)
}

fn load_vacation_panel_state(
    runtime: &MixedSuperbotRuntime,
) -> Result<VacationPanelStateFile, String> {
    let path = superbot_state_file(&runtime.state_dir, "vacation_panel_state.json");
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "failed to read vacation panel state {}: {err}",
            path.display()
        )
    })?;
    serde_json::from_str(&text).map_err(|err| {
        format!(
            "failed to parse vacation panel state {}: {err}",
            path.display()
        )
    })
}

fn load_discipline_panel_state(
    runtime: &MixedSuperbotRuntime,
) -> Result<DisciplinePanelStateFile, String> {
    let path = superbot_state_file(&runtime.state_dir, "discipline_panel_state.json");
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "failed to read discipline panel state {}: {err}",
            path.display()
        )
    })?;
    serde_json::from_str(&text).map_err(|err| {
        format!(
            "failed to parse discipline panel state {}: {err}",
            path.display()
        )
    })
}

fn embed_with_appearance(
    title: &str,
    description: &str,
    color: u32,
    footer: Option<&str>,
    timestamp: bool,
) -> Embed {
    Embed {
        author: None,
        color: Some(color),
        description: Some(description.to_owned()),
        fields: Vec::new(),
        footer: footer.map(|text| EmbedFooter {
            icon_url: None,
            proxy_icon_url: None,
            text: text.to_owned(),
        }),
        image: None,
        kind: "rich".to_owned(),
        provider: None,
        thumbnail: None,
        timestamp: if timestamp {
            Timestamp::from_secs(chrono::Utc::now().timestamp()).ok()
        } else {
            None
        },
        title: Some(title.to_owned()),
        url: None,
        video: None,
    }
}

fn embed_field(name: impl Into<String>, value: impl Into<String>, inline: bool) -> EmbedField {
    EmbedField {
        inline,
        name: name.into(),
        value: value.into(),
    }
}

fn embed_with_fields_appearance(
    title: &str,
    description: Option<&str>,
    fields: Vec<EmbedField>,
    color: u32,
    footer: Option<&str>,
    timestamp: Option<Timestamp>,
) -> Embed {
    Embed {
        author: None,
        color: Some(color),
        description: description.map(str::to_owned),
        fields,
        footer: footer.map(|text| EmbedFooter {
            icon_url: None,
            proxy_icon_url: None,
            text: text.to_owned(),
        }),
        image: None,
        kind: "rich".to_owned(),
        provider: None,
        thumbnail: None,
        timestamp,
        title: Some(title.to_owned()),
        url: None,
        video: None,
    }
}

fn recruit_embed_from_draft(draft: xiii_recruit::render::RecruitEmbedDraft) -> Embed {
    embed_with_fields_appearance(
        &draft.title,
        draft.description.as_deref(),
        draft
            .fields
            .into_iter()
            .map(|field| embed_field(field.name, field.value, field.inline))
            .collect(),
        draft.color,
        draft.footer.as_deref(),
        None,
    )
}

fn parse_rfc3339_timestamp(value: &str) -> Option<Timestamp> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .and_then(|time| Timestamp::from_secs(time.timestamp()).ok())
}

fn vacation_officer_review_embed(
    user_id: u64,
    days: i64,
    reason: &str,
    status: &str,
    decided_by: Option<u64>,
    _request_id: i64,
    created_at_rfc3339: Option<&str>,
) -> Embed {
    let mut fields = vec![
        embed_field(
            xiii_vacation::render::OFFICER_FIELD_USER,
            format!("<@{user_id}>\n`{user_id}`"),
            false,
        ),
        embed_field(
            xiii_vacation::render::OFFICER_FIELD_DAYS,
            days.to_string(),
            true,
        ),
        embed_field(xiii_vacation::render::OFFICER_FIELD_REASON, reason, false),
        embed_field(xiii_vacation::render::OFFICER_FIELD_STATUS, status, true),
    ];
    if let Some(decided_by) = decided_by {
        fields.push(embed_field(
            xiii_vacation::render::OFFICER_FIELD_DECIDED_BY,
            format!("<@{decided_by}>\n`{decided_by}`"),
            true,
        ));
    }
    embed_with_fields_appearance(
        xiii_vacation::render::OFFICER_REVIEW_TITLE,
        None,
        fields,
        match status {
            xiii_vacation::render::OFFICER_STATUS_APPROVED => {
                xiii_vacation::render::LEGACY_STATUS_APPROVED_COLOR
            }
            xiii_vacation::render::OFFICER_STATUS_REJECTED => {
                xiii_vacation::render::LEGACY_STATUS_REJECTED_COLOR
            }
            _ => xiii_vacation::render::LEGACY_STATUS_PENDING_COLOR,
        },
        Some(xiii_vacation::render::LEGACY_FOOTER),
        created_at_rfc3339.and_then(parse_rfc3339_timestamp),
    )
}

fn vacation_approved_dm_embed(
    vacation: &xiii_vacation::repository::VacationDbRecord,
    footer_text: &str,
) -> Embed {
    let expected_end = chrono::DateTime::parse_from_rfc3339(&vacation.expected_end_at)
        .map(|time| time.timestamp())
        .unwrap_or_default();
    embed_with_fields_appearance(
        xiii_vacation::render::APPROVED_DM_TITLE,
        Some(xiii_vacation::render::APPROVED_DM_DESCRIPTION),
        vec![
            embed_field(
                xiii_vacation::render::APPROVED_DM_DAYS_FIELD,
                vacation.days.to_string(),
                true,
            ),
            embed_field(
                xiii_vacation::render::APPROVED_DM_END_FIELD,
                xiii_vacation::render::discord_timestamp(expected_end, "f"),
                true,
            ),
        ],
        xiii_vacation::render::LEGACY_STATUS_APPROVED_COLOR,
        Some(footer_text),
        None,
    )
}

fn vacation_rejected_dm_embed(footer_text: &str) -> Embed {
    embed_with_fields_appearance(
        xiii_vacation::render::REJECTED_DM_TITLE,
        Some(xiii_vacation::render::REJECTED_DM_DESCRIPTION),
        Vec::new(),
        xiii_vacation::render::LEGACY_STATUS_REJECTED_COLOR,
        Some(footer_text),
        None,
    )
}

fn vacation_expired_dm_embed(footer_text: &str) -> Embed {
    embed_with_fields_appearance(
        xiii_vacation::render::EXPIRED_DM_TITLE,
        Some(xiii_vacation::render::EXPIRED_DM_DESCRIPTION),
        Vec::new(),
        xiii_vacation::render::LEGACY_STATUS_APPROVED_COLOR,
        Some(footer_text),
        None,
    )
}

async fn handle_vacation_component(
    interaction: &Interaction,
    custom_id: &str,
    runtime: &MixedSuperbotRuntime,
) {
    if custom_id == xiii_vacation::interactions::APPLY_BUTTON_ID {
        let components = vec![
            action_row(vec![text_input(
                "days",
                xiii_vacation::render::REQUEST_MODAL_DAYS_LABEL,
                TextInputStyle::Short,
                true,
            )]),
            action_row(vec![text_input(
                "reason",
                xiii_vacation::render::REQUEST_MODAL_REASON_LABEL,
                TextInputStyle::Paragraph,
                true,
            )]),
        ];
        respond_interaction_modal_http(
            runtime.http.as_ref(),
            interaction,
            xiii_vacation::interactions::REQUEST_MODAL_ID,
            xiii_vacation::render::REQUEST_MODAL_TITLE,
            components,
        )
        .await;
        return;
    }

    if let Some(request_id) = custom_id
        .strip_prefix("vacation:approve:")
        .and_then(|value| value.parse::<i64>().ok())
    {
        let Some(repo) = runtime.vacation_repo.as_ref() else {
            return;
        };
        let Some(discord) = runtime.vacation_discord.as_ref() else {
            return;
        };
        let actor_id = interaction
            .author_id()
            .map(|id| id.get())
            .unwrap_or_default();
        match repo
            .approve_request_and_create_vacation(
                request_id,
                actor_id,
                runtime.config.vacation.vacation_role_id,
                chrono::Utc::now(),
            )
            .await
        {
            Ok(vacation) => {
                if let Err(err) = discord
                    .add_vacation_role(vacation.guild_id, vacation.user_id, vacation.role_id)
                    .await
                {
                    println!("[WARN] vacation role add failed after DB approve: {err}");
                }
                let _ = discord
                    .dm_user_embed(
                        vacation.user_id,
                        "",
                        Some(vacation_approved_dm_embed(
                            &vacation,
                            &runtime.config.vacation.brand_name,
                        )),
                        &[action_row(vec![button(
                            xiii_vacation::interactions::end_button_id(vacation.id),
                            xiii_vacation::render::EARLY_END_BUTTON_LABEL,
                            ButtonStyle::Danger,
                        )])],
                    )
                    .await;
                if let Ok(Some(request)) = repo.get_request(request_id).await {
                    if let Err(err) = update_vacation_officer_review(
                        runtime,
                        &request,
                        xiii_vacation::render::OFFICER_STATUS_APPROVED,
                        actor_id,
                    )
                    .await
                    {
                        println!("[WARN] vacation officer review update failed: {err}");
                    }
                }
                let _ = vacation_active_panel_refresh_tick(runtime).await;
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_vacation::render::APPROVED_RESPONSE,
                )
                .await;
            }
            Err(err) => {
                respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await;
            }
        }
        return;
    }

    if let Some(request_id) = custom_id
        .strip_prefix("vacation:reject:")
        .and_then(|value| value.parse::<i64>().ok())
    {
        let Some(repo) = runtime.vacation_repo.as_ref() else {
            return;
        };
        let Some(discord) = runtime.vacation_discord.as_ref() else {
            return;
        };
        let actor_id = interaction
            .author_id()
            .map(|id| id.get())
            .unwrap_or_default();
        match repo
            .reject_request(request_id, actor_id, chrono::Utc::now())
            .await
        {
            Ok(()) => {
                if let Ok(Some(request)) = repo.get_request(request_id).await {
                    let _ = discord
                        .dm_user_embed(
                            request.user_id,
                            "",
                            Some(vacation_rejected_dm_embed(
                                &runtime.config.vacation.brand_name,
                            )),
                            &[],
                        )
                        .await;
                    if let Err(err) = update_vacation_officer_review(
                        runtime,
                        &request,
                        xiii_vacation::render::OFFICER_STATUS_REJECTED,
                        actor_id,
                    )
                    .await
                    {
                        println!("[WARN] vacation officer review update failed: {err}");
                    }
                }
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_vacation::render::REJECTED_RESPONSE,
                )
                .await
            }
            Err(err) => {
                respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await;
            }
        }
        return;
    }

    if let Some(vacation_id) = custom_id
        .strip_prefix(xiii_vacation::interactions::END_BUTTON_PREFIX)
        .and_then(|value| value.parse::<i64>().ok())
    {
        respond_interaction_ephemeral_components_http(
            runtime.http.as_ref(),
            interaction,
            xiii_vacation::render::END_PROMPT_RESPONSE,
            vec![action_row(vec![
                button(
                    xiii_vacation::interactions::end_confirm_button_id(vacation_id),
                    xiii_vacation::render::EARLY_END_CONFIRM_LABEL,
                    ButtonStyle::Danger,
                ),
                button(
                    xiii_vacation::interactions::end_cancel_button_id(vacation_id),
                    xiii_vacation::render::EARLY_END_CANCEL_LABEL,
                    ButtonStyle::Secondary,
                ),
            ])],
        )
        .await;
        return;
    }

    if custom_id.starts_with(xiii_vacation::interactions::END_CANCEL_PREFIX) {
        respond_interaction_ephemeral_http(
            runtime.http.as_ref(),
            interaction,
            xiii_vacation::render::END_CANCELLED_RESPONSE,
        )
        .await;
        return;
    }

    if let Some(vacation_id) = custom_id
        .strip_prefix(xiii_vacation::interactions::END_CONFIRM_PREFIX)
        .and_then(|value| value.parse::<i64>().ok())
    {
        let Some(repo) = runtime.vacation_repo.as_ref() else {
            return;
        };
        let Some(discord) = runtime.vacation_discord.as_ref() else {
            return;
        };
        let actor_id = interaction
            .author_id()
            .map(|id| id.get())
            .unwrap_or_default();
        match repo
            .end_vacation(vacation_id, actor_id, "EARLY_USER", chrono::Utc::now())
            .await
        {
            Ok(Some(vacation)) => {
                if let Err(err) = discord
                    .remove_vacation_role(vacation.guild_id, vacation.user_id, vacation.role_id)
                    .await
                {
                    println!("[WARN] vacation role remove failed after DB end: {err}");
                }
                let _ = vacation_active_panel_refresh_tick(runtime).await;
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_vacation::render::ENDED_RESPONSE,
                )
                .await;
            }
            Ok(None) => {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_vacation::render::ALREADY_ENDED_RESPONSE,
                )
                .await;
            }
            Err(err) => {
                respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await;
            }
        }
    }
}

async fn handle_vacation_modal(
    interaction: &Interaction,
    data: &twilight_model::application::interaction::modal::ModalInteractionData,
    runtime: &MixedSuperbotRuntime,
) {
    let Some(repo) = runtime.vacation_repo.as_ref() else {
        return;
    };
    let Some(guild_id) = interaction.guild_id.map(|id| id.get()) else {
        respond_interaction_ephemeral_http(
            runtime.http.as_ref(),
            interaction,
            "Server-only command.",
        )
        .await;
        return;
    };
    let Some(user_id) = interaction.author_id().map(|id| id.get()) else {
        return;
    };
    let days = modal_value(data, "days")
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0);
    let reason =
        modal_value(data, "reason").unwrap_or_else(|| xiii_vacation::render::NO_REASON.to_owned());
    if days <= 0 || days > runtime.config.vacation.max_days as i64 {
        respond_interaction_ephemeral_http(
            runtime.http.as_ref(),
            interaction,
            xiii_vacation::render::INVALID_DURATION_RESPONSE,
        )
        .await;
        return;
    }
    match repo
        .create_request(guild_id, user_id, days, &reason, chrono::Utc::now())
        .await
    {
        Ok(request_id) => {
            let ping = xiii_vacation::discord_io::officer_review_ping(
                runtime.config.vacation.officer_ping_role_id,
                request_id,
            );
            let created_at = repo
                .get_request(request_id)
                .await
                .ok()
                .flatten()
                .map(|request| request.created_at);
            let embed = vacation_officer_review_embed(
                user_id,
                days,
                &reason,
                xiii_vacation::render::OFFICER_STATUS_PENDING,
                None,
                request_id,
                created_at.as_deref(),
            );
            match send_vacation_officer_review(runtime, ping, embed, request_id).await {
                Ok(message_id) => {
                    let _ = repo
                        .update_officer_message(
                            request_id,
                            runtime.config.vacation.officer_channel_id,
                            message_id,
                        )
                        .await;
                }
                Err(err) => println!("[WARN] vacation officer review failed: {err}"),
            }
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                xiii_vacation::render::SUBMITTED_RESPONSE,
            )
            .await;
        }
        Err(err) => {
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await;
        }
    }
}

async fn send_vacation_officer_review(
    runtime: &MixedSuperbotRuntime,
    ping: xiii_vacation::discord_io::VacationOfficerPing,
    embed: Embed,
    request_id: i64,
) -> Result<u64, String> {
    let allowed =
        xiii_vacation::discord_io::allowed_mentions_for_roles(&ping.allowed_role_mentions);
    let components = vec![action_row(vec![
        button(
            xiii_vacation::interactions::approve_button_id(request_id),
            xiii_vacation::render::APPROVE_BUTTON_LABEL,
            ButtonStyle::Success,
        ),
        button(
            xiii_vacation::interactions::reject_button_id(request_id),
            xiii_vacation::render::REJECT_BUTTON_LABEL,
            ButtonStyle::Danger,
        ),
    ])];
    let embeds = vec![embed];
    let mut request = runtime
        .http
        .create_message(Id::<ChannelMarker>::new(
            runtime.config.vacation.officer_channel_id,
        ))
        .allowed_mentions(Some(&allowed))
        .embeds(&embeds)
        .components(&components);
    if !ping.content.is_empty() {
        request = request.content(&ping.content);
    }
    request
        .await
        .map_err(|err| format!("failed to send vacation officer review: {err}"))?
        .model()
        .await
        .map(|message| message.id.get())
        .map_err(|err| format!("failed to decode vacation officer review: {err}"))
}

async fn update_vacation_officer_review(
    runtime: &MixedSuperbotRuntime,
    request: &xiii_vacation::repository::VacationRequestRecord,
    status: &str,
    decided_by: u64,
) -> Result<(), String> {
    let Some(message_id) = request.officer_message_id else {
        return Ok(());
    };
    let channel_id = request
        .officer_channel_id
        .unwrap_or(runtime.config.vacation.officer_channel_id);
    let embed = vacation_officer_review_embed(
        request.user_id,
        request.days,
        &request.reason,
        status,
        Some(decided_by),
        request.id,
        Some(&request.created_at),
    );
    let components = vec![action_row(vec![
        button_with_disabled(
            xiii_vacation::interactions::approve_button_id(request.id),
            xiii_vacation::render::APPROVE_BUTTON_LABEL,
            ButtonStyle::Success,
            true,
        ),
        button_with_disabled(
            xiii_vacation::interactions::reject_button_id(request.id),
            xiii_vacation::render::REJECT_BUTTON_LABEL,
            ButtonStyle::Danger,
            true,
        ),
    ])];

    runtime
        .http
        .update_message(
            Id::<ChannelMarker>::new(channel_id),
            Id::<MessageMarker>::new(message_id),
        )
        .allowed_mentions(Some(&AllowedMentions::default()))
        .content(None)
        .embeds(Some(&[embed]))
        .components(Some(&components))
        .await
        .map_err(|err| format!("failed to update vacation officer review {message_id}: {err}"))?;
    Ok(())
}

async fn vacation_expiration_tick(runtime: &MixedSuperbotRuntime) -> Result<(), String> {
    let Some(repo) = runtime.vacation_repo.as_ref() else {
        return Ok(());
    };
    let Some(discord) = runtime.vacation_discord.as_ref() else {
        return Ok(());
    };
    let now = chrono::Utc::now();
    let expired = repo.list_expired_active(now, 100).await?;
    for vacation in expired {
        if repo
            .end_vacation(vacation.id, 0, "AUTO_EXPIRED", now)
            .await?
            .is_some()
        {
            if let Err(err) = discord
                .remove_vacation_role(vacation.guild_id, vacation.user_id, vacation.role_id)
                .await
            {
                println!("[WARN] vacation expiry role remove failed: {err}");
            }
            let _ = discord
                .dm_user_embed(
                    vacation.user_id,
                    "",
                    Some(vacation_expired_dm_embed(
                        &runtime.config.vacation.brand_name,
                    )),
                    &[],
                )
                .await;
        }
    }
    vacation_active_panel_refresh_tick(runtime).await
}

async fn vacation_active_panel_refresh_tick(runtime: &MixedSuperbotRuntime) -> Result<(), String> {
    let Some(repo) = runtime.vacation_repo.as_ref() else {
        return Ok(());
    };
    let Some(discord) = runtime.vacation_discord.as_ref() else {
        return Ok(());
    };
    let state = load_vacation_panel_state(runtime)?;
    let records = repo
        .list_active_vacations(runtime.config.core.guild_id)
        .await?
        .into_iter()
        .map(|record| record.to_vacation_record())
        .collect::<Vec<_>>();
    let description = xiii_vacation::render::active_panel_description(&records);
    let embed = embed_with_appearance(
        xiii_vacation::render::ACTIVE_PANEL_TITLE,
        &description,
        xiii_vacation::render::LEGACY_PANEL_COLOR,
        Some(xiii_vacation::render::LEGACY_FOOTER),
        false,
    );
    discord
        .edit_active_panel(
            state.active_panel.channel_id,
            state.active_panel.message_id,
            &[embed],
        )
        .await?;
    Ok(())
}

async fn handle_discipline_command(
    interaction: &Interaction,
    data: &CommandData,
    runtime: &MixedSuperbotRuntime,
) {
    let (subcommand, options) = discipline_subcommand_options(data);
    match subcommand {
        "health" => {
            let response = discipline_health_status(runtime).await;
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        "setup" => {
            let response = match discipline_setup_board(runtime).await {
                Ok(message) => message,
                Err(err) => format!("Discipline board setup failed: {err}"),
            };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        "member" => {
            let Some(target_user_id) = command_option_user(options, "user") else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    "Missing required user option.",
                )
                .await;
                return;
            };
            match discipline_history_embeds(runtime, target_user_id).await {
                Ok(embeds) => {
                    respond_interaction_ephemeral_embeds_http(
                        runtime.http.as_ref(),
                        interaction,
                        embeds,
                        Some(discipline_member_action_components(target_user_id)),
                    )
                    .await;
                }
                Err(err) => {
                    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err)
                        .await;
                }
            }
        }
        "issue" => {
            let Some(target_user_id) = command_option_user(options, "user") else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    "Missing required user option.",
                )
                .await;
                return;
            };
            let kind = command_option_string(options, "type")
                .and_then(|value| parse_punishment_type(value).ok())
                .unwrap_or(xiii_discipline::state::PunishmentType::Warning);
            let reason = command_option_string(options, "reason")
                .unwrap_or("No reason provided")
                .to_owned();
            let response =
                match discipline_issue(runtime, interaction, target_user_id, kind, &reason).await {
                    Ok(response) => response,
                    Err(err) => err,
                };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        "remove" => {
            let Some(punishment_id) = command_option_i64(options, "punishment_id") else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    "Missing required punishment_id option.",
                )
                .await;
                return;
            };
            let reason = command_option_string(options, "reason")
                .unwrap_or("Manual removal")
                .to_owned();
            let response =
                match discipline_remove(runtime, interaction, punishment_id, &reason).await {
                    Ok(response) => response,
                    Err(err) => err,
                };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        "history" => {
            let Some(target_user_id) = command_option_user(options, "user") else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    "Missing required user option.",
                )
                .await;
                return;
            };
            match discipline_history_embeds(runtime, target_user_id).await {
                Ok(embeds) => {
                    respond_interaction_ephemeral_embeds_http(
                        runtime.http.as_ref(),
                        interaction,
                        embeds,
                        None,
                    )
                    .await;
                }
                Err(err) => {
                    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err)
                        .await;
                }
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone)]
struct DisciplinePickerMember {
    user_id: u64,
    display_name: String,
    username: String,
}

#[derive(Debug, Clone)]
struct DisciplineIssuePickerSession {
    id: String,
    issuer_id: u64,
    page: usize,
    members: Vec<DisciplinePickerMember>,
    expires_at_unix_ms: u128,
}

const DISCIPLINE_ISSUE_PICKER_PAGE_SIZE: usize = 25;
const DISCIPLINE_ISSUE_PICKER_TTL_MS: u128 = 10 * 60 * 1000;

fn discipline_issue_picker_sessions(
) -> &'static Mutex<HashMap<String, DisciplineIssuePickerSession>> {
    static STORE: OnceLock<Mutex<HashMap<String, DisciplineIssuePickerSession>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn unix_millis_now() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default()
}

async fn discipline_issue_picker_members(
    runtime: &MixedSuperbotRuntime,
) -> Vec<DisciplinePickerMember> {
    let cache = runtime.voice_activity_members.lock().await;
    let mut members = cache
        .values()
        .filter(|member| {
            !member.is_bot
                && member
                    .role_ids
                    .contains(&runtime.config.discipline.main_clan_role_id)
        })
        .map(|member| DisciplinePickerMember {
            user_id: member.user_id,
            display_name: member.display_name.clone(),
            username: member.username.clone().unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    members.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
    members
}

async fn discipline_create_issue_picker_session(
    runtime: &MixedSuperbotRuntime,
    issuer_id: u64,
) -> DisciplineIssuePickerSession {
    let now = unix_millis_now();
    let members = discipline_issue_picker_members(runtime).await;
    let session = DisciplineIssuePickerSession {
        id: format!(
            "{:x}{:x}",
            now,
            (issuer_id ^ members.len() as u64).wrapping_mul(37)
        ),
        issuer_id,
        page: 0,
        members,
        expires_at_unix_ms: now + DISCIPLINE_ISSUE_PICKER_TTL_MS,
    };
    let store = discipline_issue_picker_sessions();
    let mut guard = store.lock().await;
    guard.retain(|_, current| current.expires_at_unix_ms > now);
    guard.insert(session.id.clone(), session.clone());
    session
}

async fn discipline_get_issue_picker_session(
    session_id: &str,
) -> Option<DisciplineIssuePickerSession> {
    let now = unix_millis_now();
    let store = discipline_issue_picker_sessions();
    let mut guard = store.lock().await;
    guard.retain(|_, current| current.expires_at_unix_ms > now);
    guard.get(session_id).cloned()
}

async fn discipline_update_issue_picker_session(
    session: DisciplineIssuePickerSession,
) -> DisciplineIssuePickerSession {
    let mut updated = session;
    updated.expires_at_unix_ms = unix_millis_now() + DISCIPLINE_ISSUE_PICKER_TTL_MS;
    let store = discipline_issue_picker_sessions();
    store
        .lock()
        .await
        .insert(updated.id.clone(), updated.clone());
    updated
}

async fn discipline_delete_issue_picker_session(session_id: &str) {
    discipline_issue_picker_sessions()
        .lock()
        .await
        .remove(session_id);
}

fn discipline_issue_picker_total_pages(session: &DisciplineIssuePickerSession) -> usize {
    session
        .members
        .len()
        .max(1)
        .div_ceil(DISCIPLINE_ISSUE_PICKER_PAGE_SIZE)
}

fn discipline_truncate_option(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= 100 {
        value.to_owned()
    } else {
        chars[..100].iter().collect::<String>()
    }
}

fn discipline_issue_picker_message(
    session: &DisciplineIssuePickerSession,
) -> (String, Vec<Component>) {
    let total_pages = discipline_issue_picker_total_pages(session);
    let clamped_page = session.page.min(total_pages.saturating_sub(1));
    let start = clamped_page * DISCIPLINE_ISSUE_PICKER_PAGE_SIZE;
    let page_members = session
        .members
        .iter()
        .skip(start)
        .take(DISCIPLINE_ISSUE_PICKER_PAGE_SIZE)
        .cloned()
        .collect::<Vec<_>>();

    let content = if page_members.is_empty() {
        xiii_discipline::render::ISSUE_PICKER_EMPTY_CONTENT.to_owned()
    } else {
        xiii_discipline::render::ISSUE_PICKER_CONTENT.to_owned()
    };

    let mut components = Vec::new();
    if !page_members.is_empty() {
        components.push(action_row(vec![text_select_owned(
            xiii_discipline::interactions::issue_member_select_id(&session.id),
            xiii_discipline::render::issue_picker_placeholder(clamped_page, total_pages),
            page_members
                .into_iter()
                .map(|member| {
                    let normalized_display = if member.display_name.trim().is_empty() {
                        "Участник XIII".to_owned()
                    } else {
                        member.display_name.trim().to_owned()
                    };
                    let username_different =
                        !member.username.is_empty() && member.username != normalized_display;
                    let label = if username_different {
                        format!("{normalized_display} ({})", member.username)
                    } else {
                        normalized_display.clone()
                    };
                    (
                        discipline_truncate_option(&label),
                        member.user_id.to_string(),
                        username_different
                            .then(|| discipline_truncate_option(&format!("@{}", member.username))),
                        false,
                    )
                })
                .collect(),
        )]));
    }
    components.push(action_row(vec![
        button_with_disabled(
            xiii_discipline::interactions::issue_picker_button_id(&session.id, "prev"),
            xiii_discipline::render::PICKER_PREV_LABEL,
            ButtonStyle::Secondary,
            clamped_page == 0,
        ),
        button_with_disabled(
            xiii_discipline::interactions::issue_picker_button_id(&session.id, "next"),
            xiii_discipline::render::PICKER_NEXT_LABEL,
            ButtonStyle::Secondary,
            clamped_page + 1 >= total_pages,
        ),
        button(
            xiii_discipline::interactions::issue_picker_button_id(&session.id, "id"),
            xiii_discipline::render::PICKER_ID_LABEL,
            ButtonStyle::Primary,
        ),
        button(
            xiii_discipline::interactions::issue_picker_button_id(&session.id, "cancel"),
            xiii_discipline::render::PICKER_CANCEL_LABEL,
            ButtonStyle::Danger,
        ),
    ]));
    (content, components)
}

fn discipline_issue_type_components(issuer_id: u64, target_user_id: u64) -> Vec<Component> {
    vec![action_row(vec![text_select(
        xiii_discipline::interactions::issue_type_select_id(issuer_id, target_user_id),
        xiii_discipline::render::ISSUE_TYPE_PLACEHOLDER,
        vec![
            (xiii_discipline::render::WARNING_LABEL, "warning", false),
            (xiii_discipline::render::VERBAL_LABEL, "verbal", false),
            (xiii_discipline::render::STRICT_LABEL, "strict", false),
        ],
    )])]
}

fn discipline_parse_issue_picker_button(custom_id: &str) -> Option<(&str, &str)> {
    let prefix = "xiii:issue:picker:";
    let rest = custom_id.strip_prefix(prefix)?;
    let (session_id, action) = rest.rsplit_once(':')?;
    Some((session_id, action))
}

async fn handle_discipline_component(
    interaction: &Interaction,
    data: &twilight_model::application::interaction::message_component::MessageComponentInteractionData,
    custom_id: &str,
    runtime: &MixedSuperbotRuntime,
) {
    let actor_id = interaction
        .author_id()
        .map(|id| id.get())
        .unwrap_or_default();
    match custom_id {
        xiii_discipline::interactions::PANEL_ISSUE => {
            let session = discipline_create_issue_picker_session(runtime, actor_id).await;
            let (content, components) = discipline_issue_picker_message(&session);
            respond_interaction_ephemeral_components_http(
                runtime.http.as_ref(),
                interaction,
                &content,
                components,
            )
            .await;
        }
        xiii_discipline::interactions::PANEL_REMOVE => {
            respond_interaction_ephemeral_components_http(
                runtime.http.as_ref(),
                interaction,
                xiii_discipline::render::REMOVE_TARGET_PROMPT,
                vec![action_row(vec![user_select(
                    xiii_discipline::interactions::remove_user_select_id(actor_id),
                    xiii_discipline::render::USER_SELECT_PLACEHOLDER,
                )])],
            )
            .await;
        }
        xiii_discipline::interactions::PANEL_HISTORY => {
            respond_interaction_ephemeral_components_http(
                runtime.http.as_ref(),
                interaction,
                xiii_discipline::render::HISTORY_TARGET_PROMPT,
                vec![action_row(vec![user_select(
                    xiii_discipline::interactions::history_user_select_id(actor_id),
                    xiii_discipline::render::USER_SELECT_PLACEHOLDER,
                )])],
            )
            .await;
        }
        xiii_discipline::interactions::BOARD_PREV | xiii_discipline::interactions::BOARD_NEXT => {
            let response = match discipline_change_board_page(
                runtime,
                custom_id == xiii_discipline::interactions::BOARD_NEXT,
            )
            .await
            {
                Ok(page) => format!("Доска наказаний обновлена: страница {}.", page + 1),
                Err(err) => err,
            };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        _ if custom_id.starts_with("xiii:issue:member:") => {
            let target_user_id = data
                .values
                .first()
                .and_then(|value| value.parse::<u64>().ok())
                .or_else(|| {
                    custom_id
                        .strip_prefix("xiii:issue:member:")
                        .and_then(|value| value.parse::<u64>().ok())
                });
            let Some(target_user_id) = target_user_id else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::INVALID_ISSUE_TARGET,
                )
                .await;
                return;
            };
            match fetch_member_checked(runtime, runtime.config.core.guild_id, target_user_id).await
            {
                Ok(target) => {
                    if let Err(err) = discipline_validate_target(
                        runtime,
                        runtime.config.core.guild_id,
                        &target,
                        interaction,
                    ) {
                        respond_interaction_ephemeral_http(
                            runtime.http.as_ref(),
                            interaction,
                            &err,
                        )
                        .await;
                        return;
                    }
                    let content = xiii_discipline::render::issue_type_content(target_user_id);
                    respond_interaction_ephemeral_components_http(
                        runtime.http.as_ref(),
                        interaction,
                        &content,
                        discipline_issue_type_components(actor_id, target_user_id),
                    )
                    .await;
                }
                Err(err) => {
                    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err)
                        .await;
                }
            }
        }
        _ if custom_id.starts_with("xiii:issue:picker:") => {
            let Some((session_id, action)) = discipline_parse_issue_picker_button(custom_id) else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::ISSUE_SESSION_EXPIRED,
                )
                .await;
                return;
            };
            let Some(session) = discipline_get_issue_picker_session(session_id).await else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::ISSUE_SESSION_EXPIRED,
                )
                .await;
                return;
            };
            if session.issuer_id != actor_id {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::ISSUE_SESSION_EXPIRED,
                )
                .await;
                return;
            }
            match action {
                "cancel" => {
                    discipline_delete_issue_picker_session(session_id).await;
                    respond_interaction_ephemeral_http(
                        runtime.http.as_ref(),
                        interaction,
                        xiii_discipline::render::ISSUE_CANCELLED,
                    )
                    .await;
                }
                "id" => {
                    respond_interaction_modal_http(
                        runtime.http.as_ref(),
                        interaction,
                        &xiii_discipline::interactions::issue_id_modal_id(session_id),
                        xiii_discipline::render::ISSUE_ID_MODAL_TITLE,
                        vec![action_row(vec![text_input(
                            "target_user_id",
                            xiii_discipline::render::ISSUE_ID_MODAL_LABEL,
                            TextInputStyle::Short,
                            true,
                        )])],
                    )
                    .await;
                }
                "prev" | "next" => {
                    let mut updated = session;
                    let total_pages = discipline_issue_picker_total_pages(&updated);
                    if action == "next" {
                        updated.page = (updated.page + 1).min(total_pages.saturating_sub(1));
                    } else {
                        updated.page = updated.page.saturating_sub(1);
                    }
                    let updated = discipline_update_issue_picker_session(updated).await;
                    let (content, components) = discipline_issue_picker_message(&updated);
                    respond_interaction_ephemeral_components_http(
                        runtime.http.as_ref(),
                        interaction,
                        &content,
                        components,
                    )
                    .await;
                }
                _ => {
                    respond_interaction_ephemeral_http(
                        runtime.http.as_ref(),
                        interaction,
                        xiii_discipline::render::ISSUE_SESSION_EXPIRED,
                    )
                    .await;
                }
            }
        }
        _ if custom_id.starts_with("xiii:issue:type:") => {
            let Some(rest) = custom_id.strip_prefix("xiii:issue:type:") else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::ISSUE_SESSION_EXPIRED,
                )
                .await;
                return;
            };
            let mut parts = rest.split(':');
            let issuer_id = parts.next().and_then(|value| value.parse::<u64>().ok());
            let target_user_id = parts.next().and_then(|value| value.parse::<u64>().ok());
            let selected_type = data.values.first().cloned().unwrap_or_default();
            let Ok(kind) = parse_punishment_type(&selected_type) else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::INVALID_TYPE_TEXT,
                )
                .await;
                return;
            };
            if issuer_id != Some(actor_id) || target_user_id.is_none() {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::ISSUE_SESSION_EXPIRED,
                )
                .await;
                return;
            }
            let kind_token = match kind {
                xiii_discipline::state::PunishmentType::Warning => "warning",
                xiii_discipline::state::PunishmentType::Verbal => "verbal",
                xiii_discipline::state::PunishmentType::Strict => "strict",
            };
            respond_interaction_modal_http(
                runtime.http.as_ref(),
                interaction,
                &xiii_discipline::interactions::issue_modal_id(
                    actor_id,
                    target_user_id.unwrap_or_default(),
                    kind_token,
                ),
                &format!("Выдать: {}", punishment_kind_name(kind)),
                vec![action_row(vec![text_input(
                    "reason",
                    xiii_discipline::render::ISSUE_REASON_LABEL,
                    TextInputStyle::Paragraph,
                    true,
                )])],
            )
            .await;
        }
        _ if custom_id.starts_with("xiii:remove:user:") => {
            let issuer_id = custom_id
                .strip_prefix("xiii:remove:user:")
                .and_then(|value| value.parse::<u64>().ok());
            let target_user_id = data
                .values
                .first()
                .and_then(|value| value.parse::<u64>().ok());
            if issuer_id != Some(actor_id) || target_user_id.is_none() {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::REMOVE_SESSION_EXPIRED,
                )
                .await;
                return;
            }
            let Some(repo) = runtime.discipline_repo.as_ref() else {
                return;
            };
            let target_user_id = target_user_id.unwrap_or_default();
            match repo
                .active_punishments(runtime.config.core.guild_id, target_user_id)
                .await
            {
                Ok(active) if active.is_empty() => {
                    respond_interaction_ephemeral_http(
                        runtime.http.as_ref(),
                        interaction,
                        &xiii_discipline::render::remove_no_active_message(target_user_id),
                    )
                    .await;
                }
                Ok(active) => {
                    let shown = active.len().min(25);
                    let content = xiii_discipline::render::remove_selection_content(
                        target_user_id,
                        shown,
                        active.len(),
                    );
                    let options = active
                        .iter()
                        .take(25)
                        .map(|record| {
                            let expires = record
                                .expires_at
                                .map(|unix| format!("<t:{unix}:d>"))
                                .unwrap_or_else(|| "не истекает".to_owned());
                            (
                                discipline_truncate_option(&format!(
                                    "#{} • {} • <t:{}:d>",
                                    record.id,
                                    punishment_kind_name(record.kind),
                                    record.issued_at
                                )),
                                record.id.to_string(),
                                Some(discipline_truncate_option(&format!(
                                    "{} • {}",
                                    expires, record.reason
                                ))),
                                false,
                            )
                        })
                        .collect::<Vec<_>>();
                    respond_interaction_ephemeral_components_http(
                        runtime.http.as_ref(),
                        interaction,
                        &content,
                        vec![action_row(vec![text_select_owned(
                            xiii_discipline::interactions::remove_punishment_select_id(
                                actor_id,
                                target_user_id,
                            ),
                            xiii_discipline::render::REMOVE_SELECT_PLACEHOLDER,
                            options,
                        )])],
                    )
                    .await;
                }
                Err(err) => {
                    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err)
                        .await;
                }
            }
        }
        _ if custom_id.starts_with("xiii:remove:punishment:") => {
            let Some(rest) = custom_id.strip_prefix("xiii:remove:punishment:") else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::REMOVE_SESSION_EXPIRED,
                )
                .await;
                return;
            };
            let mut parts = rest.split(':');
            let issuer_id = parts.next().and_then(|value| value.parse::<u64>().ok());
            let target_user_id = parts.next().and_then(|value| value.parse::<u64>().ok());
            let punishment_id = data
                .values
                .first()
                .and_then(|value| value.parse::<i64>().ok());
            if issuer_id != Some(actor_id) || target_user_id.is_none() || punishment_id.is_none() {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::REMOVE_SESSION_EXPIRED,
                )
                .await;
                return;
            }
            respond_interaction_modal_http(
                runtime.http.as_ref(),
                interaction,
                &xiii_discipline::interactions::remove_modal_id(
                    actor_id,
                    target_user_id.unwrap_or_default(),
                    punishment_id.unwrap_or_default(),
                ),
                xiii_discipline::render::REMOVE_MODAL_TITLE,
                vec![action_row(vec![text_input(
                    "reason",
                    xiii_discipline::render::REMOVE_REASON_LABEL,
                    TextInputStyle::Paragraph,
                    true,
                )])],
            )
            .await;
        }
        _ if custom_id.starts_with("xiii:remove:member:") => {
            let Some(target_user_id) = custom_id
                .strip_prefix("xiii:remove:member:")
                .and_then(|value| value.parse::<u64>().ok())
            else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::INVALID_REMOVE_TARGET,
                )
                .await;
                return;
            };
            let Some(repo) = runtime.discipline_repo.as_ref() else {
                return;
            };
            match repo
                .active_punishments(runtime.config.core.guild_id, target_user_id)
                .await
            {
                Ok(active) if active.is_empty() => {
                    respond_interaction_ephemeral_http(
                        runtime.http.as_ref(),
                        interaction,
                        &xiii_discipline::render::remove_no_active_message(target_user_id),
                    )
                    .await;
                }
                Ok(active) => {
                    let shown = active.len().min(25);
                    let content = xiii_discipline::render::remove_selection_content(
                        target_user_id,
                        shown,
                        active.len(),
                    );
                    let options = active
                        .iter()
                        .take(25)
                        .map(|record| {
                            let expires = record
                                .expires_at
                                .map(|unix| format!("<t:{unix}:d>"))
                                .unwrap_or_else(|| "не истекает".to_owned());
                            (
                                discipline_truncate_option(&format!(
                                    "#{} • {} • <t:{}:d>",
                                    record.id,
                                    punishment_kind_name(record.kind),
                                    record.issued_at
                                )),
                                record.id.to_string(),
                                Some(discipline_truncate_option(&format!(
                                    "{} • {}",
                                    expires, record.reason
                                ))),
                                false,
                            )
                        })
                        .collect::<Vec<_>>();
                    respond_interaction_ephemeral_components_http(
                        runtime.http.as_ref(),
                        interaction,
                        &content,
                        vec![action_row(vec![text_select_owned(
                            xiii_discipline::interactions::remove_punishment_select_id(
                                actor_id,
                                target_user_id,
                            ),
                            xiii_discipline::render::REMOVE_SELECT_PLACEHOLDER,
                            options,
                        )])],
                    )
                    .await;
                }
                Err(err) => {
                    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err)
                        .await;
                }
            }
        }
        _ if custom_id.starts_with("xiii:history:user:") => {
            let issuer_id = custom_id
                .strip_prefix("xiii:history:user:")
                .and_then(|value| value.parse::<u64>().ok());
            let target_user_id = data
                .values
                .first()
                .and_then(|value| value.parse::<u64>().ok());
            if issuer_id != Some(actor_id) || target_user_id.is_none() {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::HISTORY_SESSION_EXPIRED,
                )
                .await;
                return;
            }
            match discipline_history_embeds(runtime, target_user_id.unwrap_or_default()).await {
                Ok(embeds) => {
                    respond_interaction_ephemeral_embeds_http(
                        runtime.http.as_ref(),
                        interaction,
                        embeds,
                        None,
                    )
                    .await;
                }
                Err(err) => {
                    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err)
                        .await;
                }
            }
        }
        _ if custom_id.starts_with("xiii:history:member:") => {
            if let Some(target_user_id) = custom_id
                .strip_prefix("xiii:history:member:")
                .and_then(|value| value.parse::<u64>().ok())
            {
                match discipline_history_embeds(runtime, target_user_id).await {
                    Ok(embeds) => {
                        respond_interaction_ephemeral_embeds_http(
                            runtime.http.as_ref(),
                            interaction,
                            embeds,
                            None,
                        )
                        .await;
                    }
                    Err(err) => {
                        respond_interaction_ephemeral_http(
                            runtime.http.as_ref(),
                            interaction,
                            &err,
                        )
                        .await;
                    }
                }
            } else {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::INVALID_HISTORY_TARGET,
                )
                .await;
            }
        }
        _ if custom_id.starts_with("xiii:issue:") => {
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                "Элемент устарел. Открой выдачу наказания заново.",
            )
            .await;
        }
        _ if custom_id.starts_with("xiii:remove:") => {
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                "Элемент устарел. Открой снятие наказания заново.",
            )
            .await;
        }
        _ if custom_id.starts_with("xiii:history:") => {
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                "Элемент устарел. Открой историю заново.",
            )
            .await;
        }
        _ => {}
    }
}

async fn handle_discipline_modal(
    interaction: &Interaction,
    data: &twilight_model::application::interaction::modal::ModalInteractionData,
    runtime: &MixedSuperbotRuntime,
) {
    let actor_id = interaction
        .author_id()
        .map(|id| id.get())
        .unwrap_or_default();
    if let Some(session_id) = data.custom_id.strip_prefix("xiii:issue:idmodal:") {
        let Some(session) = discipline_get_issue_picker_session(session_id).await else {
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                xiii_discipline::render::ISSUE_SESSION_EXPIRED,
            )
            .await;
            return;
        };
        if session.issuer_id != actor_id {
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                xiii_discipline::render::ISSUE_SESSION_EXPIRED,
            )
            .await;
            return;
        }
        let Some(target_user_id) =
            modal_value(data, "target_user_id").and_then(|value| value.parse::<u64>().ok())
        else {
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                xiii_discipline::render::INVALID_MEMBER_ID,
            )
            .await;
            return;
        };
        match fetch_member_checked(runtime, runtime.config.core.guild_id, target_user_id).await {
            Ok(target) => {
                if let Err(err) = discipline_validate_target(
                    runtime,
                    runtime.config.core.guild_id,
                    &target,
                    interaction,
                ) {
                    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err)
                        .await;
                    return;
                }
                let content = xiii_discipline::render::issue_type_content(target_user_id);
                respond_interaction_ephemeral_components_http(
                    runtime.http.as_ref(),
                    interaction,
                    &content,
                    discipline_issue_type_components(session.issuer_id, target_user_id),
                )
                .await;
            }
            Err(err) => {
                respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await;
            }
        }
        return;
    }
    if let Some(rest) = data.custom_id.strip_prefix("xiii:issue:modal:") {
        let parts = rest.split(':').collect::<Vec<_>>();
        if parts.len() == 3 {
            let issuer_id = parts[0].parse::<u64>().ok();
            let target_user_id = parts[1].parse::<u64>().ok();
            let kind = parse_punishment_type(parts[2]).ok();
            if issuer_id != Some(actor_id) || target_user_id.is_none() || kind.is_none() {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::ISSUE_SESSION_EXPIRED,
                )
                .await;
                return;
            }
            let reason =
                modal_value(data, "reason").unwrap_or_else(|| "Причина не указана".to_owned());
            let response = discipline_issue(
                runtime,
                interaction,
                target_user_id.unwrap_or_default(),
                kind.unwrap_or(xiii_discipline::state::PunishmentType::Warning),
                &reason,
            )
            .await
            .unwrap_or_else(|err| err);
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
            return;
        }
    }
    if let Some(rest) = data.custom_id.strip_prefix("xiii:remove:modal:") {
        let parts = rest.split(':').collect::<Vec<_>>();
        if parts.len() == 3 {
            let issuer_id = parts[0].parse::<u64>().ok();
            let punishment_id = parts[2].parse::<i64>().ok();
            if issuer_id != Some(actor_id) || punishment_id.is_none() {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::REMOVE_SESSION_EXPIRED,
                )
                .await;
                return;
            }
            let reason = modal_value(data, "reason").unwrap_or_else(|| "Ручное снятие".to_owned());
            let response = discipline_remove(
                runtime,
                interaction,
                punishment_id.unwrap_or_default(),
                &reason,
            )
            .await
            .unwrap_or_else(|err| err);
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
            return;
        }
    }
    let response = if data.custom_id == "xiii:issue:modal"
        || data.custom_id.starts_with("xiii:issue:modal:")
    {
        let target_user_id = if let Some(id) = data
            .custom_id
            .strip_prefix("xiii:issue:modal:")
            .and_then(|value| value.parse::<u64>().ok())
        {
            id
        } else {
            match modal_value(data, "target_user_id").and_then(|value| value.parse::<u64>().ok()) {
                Some(id) => id,
                None => {
                    respond_interaction_ephemeral_http(
                        runtime.http.as_ref(),
                        interaction,
                        xiii_discipline::render::INVALID_MEMBER_ID,
                    )
                    .await;
                    return;
                }
            }
        };
        let kind = match modal_value(data, "type")
            .as_deref()
            .map(parse_punishment_type)
        {
            Some(Ok(kind)) => kind,
            _ => {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_discipline::render::INVALID_TYPE_TEXT,
                )
                .await;
                return;
            }
        };
        let reason = modal_value(data, "reason").unwrap_or_else(|| "Причина не указана".to_owned());
        discipline_issue(runtime, interaction, target_user_id, kind, &reason)
            .await
            .unwrap_or_else(|err| err)
    } else if data.custom_id == "xiii:remove:modal" {
        let Some(punishment_id) =
            modal_value(data, "punishment_id").and_then(|value| value.parse::<i64>().ok())
        else {
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                xiii_discipline::render::INVALID_PUNISHMENT_ID,
            )
            .await;
            return;
        };
        let reason = modal_value(data, "reason").unwrap_or_else(|| "Ручное снятие".to_owned());
        discipline_remove(runtime, interaction, punishment_id, &reason)
            .await
            .unwrap_or_else(|err| err)
    } else if data.custom_id == "xiii:history:modal" {
        match modal_value(data, "target_user_id").and_then(|value| value.parse::<u64>().ok()) {
            Some(target_user_id) => discipline_history_text(runtime, target_user_id)
                .await
                .unwrap_or_else(|err| err),
            None => xiii_discipline::render::INVALID_MEMBER_ID.to_owned(),
        }
    } else {
        "Неподдерживаемая форма дисциплины.".to_owned()
    };
    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
}

fn discipline_subcommand_options(
    data: &CommandData,
) -> (
    &str,
    &[twilight_model::application::interaction::application_command::CommandDataOption],
) {
    let Some(option) = data.options.first() else {
        return ("health", &[]);
    };
    match &option.value {
        CommandOptionValue::SubCommand(options) => (option.name.as_str(), options.as_slice()),
        _ => (option.name.as_str(), data.options.as_slice()),
    }
}

fn command_option_user(
    options: &[twilight_model::application::interaction::application_command::CommandDataOption],
    name: &str,
) -> Option<u64> {
    options.iter().find_map(|option| {
        if option.name != name {
            return None;
        }
        match &option.value {
            CommandOptionValue::User(id) => Some(id.get()),
            CommandOptionValue::String(value) => value.parse::<u64>().ok(),
            CommandOptionValue::Integer(value) => u64::try_from(*value).ok(),
            _ => None,
        }
    })
}

fn command_option_string<'a>(
    options: &'a [twilight_model::application::interaction::application_command::CommandDataOption],
    name: &str,
) -> Option<&'a str> {
    options.iter().find_map(|option| {
        if option.name != name {
            return None;
        }
        match &option.value {
            CommandOptionValue::String(value) => Some(value.as_str()),
            _ => None,
        }
    })
}

fn command_option_i64(
    options: &[twilight_model::application::interaction::application_command::CommandDataOption],
    name: &str,
) -> Option<i64> {
    options.iter().find_map(|option| {
        if option.name != name {
            return None;
        }
        match &option.value {
            CommandOptionValue::Integer(value) => Some(*value),
            CommandOptionValue::String(value) => value.parse::<i64>().ok(),
            _ => None,
        }
    })
}

fn parse_punishment_type(value: &str) -> Result<xiii_discipline::state::PunishmentType, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "warning" | "warn" | "предупреждение" => {
            Ok(xiii_discipline::state::PunishmentType::Warning)
        }
        "verbal" | "устный" | "устное" => {
            Ok(xiii_discipline::state::PunishmentType::Verbal)
        }
        "strict" | "строгий" | "строгое" => {
            Ok(xiii_discipline::state::PunishmentType::Strict)
        }
        other => Err(format!("unknown punishment type {other:?}")),
    }
}

fn punishment_kind_name(kind: xiii_discipline::state::PunishmentType) -> &'static str {
    match kind {
        xiii_discipline::state::PunishmentType::Warning => xiii_discipline::render::WARNING_LABEL,
        xiii_discipline::state::PunishmentType::Verbal => xiii_discipline::render::VERBAL_LABEL,
        xiii_discipline::state::PunishmentType::Strict => xiii_discipline::render::STRICT_LABEL,
    }
}

fn discipline_member_action_components(target_user_id: u64) -> Vec<Component> {
    vec![action_row(vec![
        button(
            format!("xiii:issue:member:{target_user_id}"),
            xiii_discipline::render::PANEL_ISSUE_LABEL,
            ButtonStyle::Danger,
        ),
        button(
            format!("xiii:remove:member:{target_user_id}"),
            xiii_discipline::render::PANEL_REMOVE_LABEL,
            ButtonStyle::Success,
        ),
        button(
            format!("xiii:history:member:{target_user_id}"),
            xiii_discipline::render::PANEL_HISTORY_LABEL,
            ButtonStyle::Secondary,
        ),
    ])]
}

async fn discipline_setup_board(runtime: &MixedSuperbotRuntime) -> Result<String, String> {
    match load_discipline_panel_state(runtime) {
        Ok(state) => {
            discipline_board_refresh_tick(runtime).await?;
            Ok(format!(
                "Discipline board verified and refreshed: channel_id={} message_id={}",
                state.board.channel_id, state.board.message_id
            ))
        }
        Err(_) => {
            let mut identity_report = Report::new();
            let current_user =
                fetch_current_user_with_retry(runtime.http.as_ref(), &mut identity_report)
                    .await
                    .map_err(|err| {
                        format!("unable to identify current bot for board setup: {err}")
                    })?;
            let message_id = bootstrap_discipline_board(
                runtime.http.as_ref(),
                &runtime.config,
                &runtime.state_dir,
                current_user.id.get(),
            )
            .await?;
            Ok(format!(
                "Discipline board created from current Superbot token: message_id={message_id}"
            ))
        }
    }
}

async fn discipline_health_status(runtime: &MixedSuperbotRuntime) -> String {
    let db_status = if runtime.discipline_repo.is_some() {
        "ok"
    } else {
        "missing"
    };
    let state_status = match load_discipline_panel_state(runtime) {
        Ok(state) => format!(
            "ok channel_id={} message_id={}",
            state.board.channel_id, state.board.message_id
        ),
        Err(err) => format!("missing_or_invalid: {err}"),
    };
    format!(
        "Discipline health\nDB: {db_status}\nBoard state: {state_status}\nSchedulers: discipline_expiration_worker, discipline_board_refresh\nWriter eligible: {}",
        runtime.selected.contains(&SuperbotModuleKind::Discipline)
            && runtime.config.modules.discipline
    )
}

async fn discipline_history_embeds(
    runtime: &MixedSuperbotRuntime,
    target_user_id: u64,
) -> Result<Vec<Embed>, String> {
    let Some(repo) = runtime.discipline_repo.as_ref() else {
        return Err("Discipline repository is not available.".to_owned());
    };
    let rows = repo
        .punishment_history(runtime.config.core.guild_id, target_user_id, 100)
        .await?;
    if rows.is_empty() {
        return Ok(vec![embed_with_fields_appearance(
            xiii_discipline::render::HISTORY_TITLE,
            Some(&xiii_discipline::render::history_empty_description(
                target_user_id,
            )),
            Vec::new(),
            xiii_discipline::render::LEGACY_HISTORY_EMPTY_COLOR,
            None,
            Timestamp::from_secs(chrono::Utc::now().timestamp()).ok(),
        )]);
    }

    let pages = xiii_discipline::render::history_pages(&rows);
    let timestamp = Timestamp::from_secs(chrono::Utc::now().timestamp()).ok();
    Ok(pages
        .iter()
        .enumerate()
        .map(|(index, description)| {
            embed_with_fields_appearance(
                xiii_discipline::render::HISTORY_TITLE,
                Some(description),
                Vec::new(),
                xiii_discipline::render::LEGACY_BOARD_COLOR,
                Some(&xiii_discipline::render::history_footer(
                    target_user_id,
                    index,
                    pages.len(),
                )),
                timestamp,
            )
        })
        .collect())
}

async fn discipline_history_text(
    runtime: &MixedSuperbotRuntime,
    target_user_id: u64,
) -> Result<String, String> {
    let Some(repo) = runtime.discipline_repo.as_ref() else {
        return Err("Discipline repository is not available.".to_owned());
    };
    let rows = repo
        .punishment_history(runtime.config.core.guild_id, target_user_id, 15)
        .await?;
    if rows.is_empty() {
        return Ok(format!("No punishment history for <@{target_user_id}>."));
    }
    let lines = rows
        .iter()
        .map(|record| {
            format!(
                "#{} {} status={} reason={} issued=<t:{}:f>",
                record.id,
                punishment_kind_name(record.kind),
                record.status,
                record.reason,
                record.issued_at
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "Punishment history for <@{target_user_id}>:\n{lines}"
    ))
}

async fn discipline_issue(
    runtime: &MixedSuperbotRuntime,
    interaction: &Interaction,
    target_user_id: u64,
    requested_kind: xiii_discipline::state::PunishmentType,
    reason: &str,
) -> Result<String, String> {
    let Some(repo) = runtime.discipline_repo.as_ref() else {
        return Err("Discipline repository is not available.".to_owned());
    };
    let Some(discord) = runtime.discipline_discord.as_ref() else {
        return Err("Discipline Discord adapter is not available.".to_owned());
    };
    let guild_id = interaction
        .guild_id
        .map(|id| id.get())
        .ok_or_else(|| "Discipline actions can only run in a server.".to_owned())?;
    let issuer_id = interaction
        .author_id()
        .map(|id| id.get())
        .ok_or_else(|| "Unable to determine moderator user ID.".to_owned())?;
    discipline_require_moderator(interaction, runtime)?;
    let target = fetch_member_checked(runtime, guild_id, target_user_id).await?;
    discipline_validate_target(runtime, guild_id, &target, interaction)?;

    let now = chrono::Utc::now().timestamp();
    let lock_key = xiii_discipline::state::action_lock_key("issue", target_user_id);
    if !repo.acquire_action_lock(&lock_key, now + 45, now).await? {
        return Err("A discipline issue action for this member is already in progress.".to_owned());
    }

    let result = async {
        let active = repo.active_punishments(guild_id, target_user_id).await?;
        let active_model = active.iter().map(|record| record.to_punishment()).collect::<Vec<_>>();
        let escalation = xiii_discipline::runtime::escalation_for_new_punishment(
            &active_model,
            requested_kind,
            target_user_id,
        );
        let final_kind = match escalation {
            xiii_discipline::state::EscalationOutcome::Issue(kind) => kind,
            xiii_discipline::state::EscalationOutcome::ClanRemoval => {
                xiii_discipline::state::PunishmentType::Strict
            }
        };
        let convert_active_ids = active
            .iter()
            .filter(|record| record.kind == requested_kind)
            .map(|record| record.id)
            .collect::<Vec<_>>();
        let expires_at = xiii_discipline::runtime::expires_after_days(
            final_kind,
            runtime.config.discipline.warning_expires_days,
            runtime.config.discipline.verbal_expires_days,
        )
        .map(|days| now + (days as i64 * 86_400));
        let punishment_id = repo
            .issue_punishment_with_log(xiii_discipline::repository::IssuePunishmentDraft {
                guild_id,
                user_id: target_user_id,
                kind: final_kind,
                reason: reason.to_owned(),
                issuer_id: Some(issuer_id),
                issued_at: now,
                expires_at,
                convert_active_ids,
                action_type: match escalation {
                    xiii_discipline::state::EscalationOutcome::ClanRemoval => {
                        "issue_clan_removal".to_owned()
                    }
                    _ => "issue".to_owned(),
                },
                payload_json: serde_json::json!({
                    "requested_type": punishment_kind_name(requested_kind),
                    "final_type": punishment_kind_name(final_kind),
                    "reason": reason,
                    "clan_removal": matches!(escalation, xiii_discipline::state::EscalationOutcome::ClanRemoval)
                })
                .to_string(),
            })
            .await?;

        let timeout = xiii_discipline::discord_io::TimeoutRequest {
            user_id: target_user_id,
            timeout_minutes: runtime.config.discipline.timeout_minutes,
            reason: format!("XIII discipline: {} - {}", punishment_kind_name(final_kind), reason),
        };
        if let Err(err) = discord.apply_timeout(guild_id, &timeout, now).await {
            println!("[WARN] discipline timeout failed: {err}");
        }
        let dm = xiii_discipline::discord_io::punishment_dm(
            target_user_id,
            &format!("XIII discipline {}", punishment_kind_name(final_kind)),
            reason,
        );
        if let Err(err) = discord.dm_user(&dm).await {
            println!("[WARN] discipline DM failed: {err}");
        }
        if matches!(escalation, xiii_discipline::state::EscalationOutcome::ClanRemoval) {
            let current_roles = target.roles.iter().map(|id| id.get()).collect::<Vec<_>>();
            let remove_role_ids = current_roles
                .iter()
                .copied()
                .filter(|role_id| runtime.config.discipline.composition_role_ids.contains(role_id))
                .collect::<Vec<_>>();
            let request = xiii_discipline::discord_io::ClanRemovalRequest {
                user_id: target_user_id,
                remove_role_ids,
                add_guest_role_id: runtime.config.discipline.guest_role_id,
            };
            if let Err(err) = discord.execute_clan_removal(guild_id, &request).await {
                println!("[WARN] discipline clan removal role update failed: {err}");
            }
        }
        let log = format!(
            "Discipline {} issued to <@{}> by <@{}>: {}",
            punishment_kind_name(final_kind),
            target_user_id,
            issuer_id,
            reason
        );
        let _ = discord
            .send_admin_log(runtime.config.discipline.log_channel_id, &log, None)
            .await;
        discipline_board_refresh_tick(runtime).await?;
        Ok::<String, String>(format!(
            "Issued {} punishment #{} for <@{}>.",
            punishment_kind_name(final_kind),
            punishment_id,
            target_user_id
        ))
    }
    .await;

    let _ = repo.release_action_lock(&lock_key).await;
    result
}

async fn discipline_remove(
    runtime: &MixedSuperbotRuntime,
    interaction: &Interaction,
    punishment_id: i64,
    reason: &str,
) -> Result<String, String> {
    let Some(repo) = runtime.discipline_repo.as_ref() else {
        return Err("Discipline repository is not available.".to_owned());
    };
    let Some(discord) = runtime.discipline_discord.as_ref() else {
        return Err("Discipline Discord adapter is not available.".to_owned());
    };
    let issuer_id = interaction
        .author_id()
        .map(|id| id.get())
        .ok_or_else(|| "Unable to determine moderator user ID.".to_owned())?;
    discipline_require_moderator(interaction, runtime)?;
    let record = repo
        .get_punishment(punishment_id)
        .await?
        .ok_or_else(|| format!("Punishment #{punishment_id} was not found."))?;
    let now = chrono::Utc::now().timestamp();
    let lock_key = xiii_discipline::state::action_lock_key("remove", record.user_id);
    if !repo.acquire_action_lock(&lock_key, now + 45, now).await? {
        return Err(
            "A discipline remove action for this member is already in progress.".to_owned(),
        );
    }
    let result = async {
        let removed = repo
            .remove_punishment_with_log(punishment_id, issuer_id, reason, now)
            .await?;
        if !removed {
            return Err(format!(
                "Punishment #{punishment_id} is not active or was already removed."
            ));
        }
        let dm = xiii_discipline::discord_io::punishment_dm(
            record.user_id,
            "XIII discipline punishment removed",
            &format!("Punishment #{punishment_id} was removed. Reason: {reason}"),
        );
        if let Err(err) = discord.dm_user(&dm).await {
            println!("[WARN] discipline removal DM failed: {err}");
        }
        let log = format!(
            "Discipline punishment #{} removed by <@{}>: {}",
            punishment_id, issuer_id, reason
        );
        let _ = discord
            .send_admin_log(runtime.config.discipline.log_channel_id, &log, None)
            .await;
        discipline_board_refresh_tick(runtime).await?;
        Ok::<String, String>(format!("Removed punishment #{punishment_id}."))
    }
    .await;
    let _ = repo.release_action_lock(&lock_key).await;
    result
}

fn discipline_require_moderator(
    interaction: &Interaction,
    runtime: &MixedSuperbotRuntime,
) -> Result<(), String> {
    let member = interaction
        .member
        .as_ref()
        .ok_or_else(|| "Moderator permissions are unavailable for this interaction.".to_owned())?;
    let permissions = member.permissions.unwrap_or_else(Permissions::empty);
    let role_ids = member.roles.iter().map(|id| id.get()).collect::<Vec<_>>();
    match xiii_discipline::commands::can_moderate(
        permissions.contains(Permissions::ADMINISTRATOR),
        permissions.contains(Permissions::MANAGE_GUILD),
        &role_ids,
        &runtime.config.discipline.officer_role_ids,
    ) {
        xiii_discipline::commands::DisciplinePermission::Allowed => Ok(()),
        xiii_discipline::commands::DisciplinePermission::Denied => {
            Err("You do not have permission to use Discipline actions.".to_owned())
        }
    }
}

async fn fetch_member_checked(
    runtime: &MixedSuperbotRuntime,
    guild_id: u64,
    user_id: u64,
) -> Result<DiscordMember, String> {
    runtime
        .http
        .guild_member(
            Id::<GuildMarker>::new(guild_id),
            Id::<UserMarker>::new(user_id),
        )
        .await
        .map_err(|err| format!("failed to fetch target member {user_id}: {err}"))?
        .model()
        .await
        .map_err(|err| format!("failed to decode target member {user_id}: {err}"))
}

fn discipline_validate_target(
    runtime: &MixedSuperbotRuntime,
    guild_id: u64,
    target: &DiscordMember,
    interaction: &Interaction,
) -> Result<(), String> {
    let target_user_id = target.user.id.get();
    let owner_id = runtime
        .temp_occupancy
        .try_lock()
        .ok()
        .and_then(|occupancy| occupancy.guild_owner_id(guild_id));
    let has_main = target
        .roles
        .iter()
        .any(|role| role.get() == runtime.config.discipline.main_clan_role_id);
    xiii_discipline::commands::valid_target(
        target.user.bot,
        owner_id == Some(target_user_id),
        has_main,
    )?;
    let target_is_officer = target.roles.iter().any(|role| {
        runtime
            .config
            .discipline
            .officer_role_ids
            .contains(&role.get())
    });
    let moderator_is_admin = interaction
        .member
        .as_ref()
        .and_then(|member| member.permissions)
        .map(|permissions| permissions.contains(Permissions::ADMINISTRATOR))
        .unwrap_or(false);
    if target_is_officer && !moderator_is_admin {
        return Err("target has a protected officer role".to_owned());
    }
    Ok(())
}

async fn discipline_change_board_page(
    runtime: &MixedSuperbotRuntime,
    next: bool,
) -> Result<i64, String> {
    let Some(repo) = runtime.discipline_repo.as_ref() else {
        return Err("Discipline repository is not available.".to_owned());
    };
    let current = repo
        .get_setting("discipline_board_page")
        .await?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let page = if next {
        current + 1
    } else {
        current.saturating_sub(1)
    };
    repo.set_setting("discipline_board_page", &page.to_string())
        .await?;
    discipline_board_refresh_tick(runtime).await?;
    Ok(page)
}

async fn discipline_expiration_tick(runtime: &MixedSuperbotRuntime) -> Result<(), String> {
    let Some(repo) = runtime.discipline_repo.as_ref() else {
        return Ok(());
    };
    let Some(discord) = runtime.discipline_discord.as_ref() else {
        return Ok(());
    };
    let now = chrono::Utc::now().timestamp();
    let expired = repo
        .expire_due_punishments_with_logs(runtime.config.core.guild_id, now, 100)
        .await?;
    if runtime.config.discipline.log_expirations {
        for punishment_id in expired {
            let content = format!("Discipline punishment #{punishment_id} expired.");
            let _ = discord
                .send_admin_log(runtime.config.discipline.log_channel_id, &content, None)
                .await;
        }
    }
    discipline_board_refresh_tick(runtime).await
}

async fn discipline_board_refresh_tick(runtime: &MixedSuperbotRuntime) -> Result<(), String> {
    let Some(repo) = runtime.discipline_repo.as_ref() else {
        return Ok(());
    };
    let Some(discord) = runtime.discipline_discord.as_ref() else {
        return Ok(());
    };
    let state = load_discipline_panel_state(runtime)?;
    let page = repo
        .get_setting("discipline_board_page")
        .await?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let records = repo
        .list_active_for_board_page(runtime.config.core.guild_id, 1000, 0)
        .await?;
    let raw_page = page as usize;
    let (description, total_pages) = xiii_discipline::render::board_description(&records, raw_page);
    let clamped_page = raw_page.min(total_pages.saturating_sub(1));
    if clamped_page as i64 != page {
        repo.set_setting("discipline_board_page", &clamped_page.to_string())
            .await?;
    }
    let footer = xiii_discipline::render::board_footer(
        clamped_page,
        total_pages,
        &chrono::Local::now()
            .format("%d.%m.%Y, %H:%M:%S")
            .to_string(),
    );
    let mut components = vec![action_row(vec![
        button(
            xiii_discipline::interactions::PANEL_ISSUE,
            xiii_discipline::render::PANEL_ISSUE_LABEL,
            ButtonStyle::Danger,
        ),
        button(
            xiii_discipline::interactions::PANEL_REMOVE,
            xiii_discipline::render::PANEL_REMOVE_LABEL,
            ButtonStyle::Success,
        ),
        button(
            xiii_discipline::interactions::PANEL_HISTORY,
            xiii_discipline::render::PANEL_HISTORY_LABEL,
            ButtonStyle::Secondary,
        ),
    ])];
    if total_pages > 1 {
        components.push(action_row(vec![
            button_with_disabled(
                xiii_discipline::interactions::BOARD_PREV,
                xiii_discipline::render::BOARD_PREV_LABEL,
                ButtonStyle::Secondary,
                clamped_page == 0,
            ),
            button_with_disabled(
                xiii_discipline::interactions::BOARD_NEXT,
                xiii_discipline::render::BOARD_NEXT_LABEL,
                ButtonStyle::Secondary,
                clamped_page + 1 >= total_pages,
            ),
        ]));
    }
    let embed = embed_with_appearance(
        xiii_discipline::render::board_title(),
        &description,
        xiii_discipline::render::LEGACY_BOARD_COLOR,
        Some(&footer),
        true,
    );
    discord
        .edit_board(
            state.board.channel_id,
            state.board.message_id,
            &[embed],
            &components,
        )
        .await?;
    Ok(())
}

async fn handle_recruits_command(interaction: &Interaction, runtime: &MixedSuperbotRuntime) {
    let Some(repo) = runtime.recruit_repo.as_ref() else {
        return;
    };
    match repo
        .list_active_recruits(runtime.config.core.guild_id)
        .await
    {
        Ok(recruits) => {
            let content = if recruits.is_empty() {
                "No active recruits.".to_owned()
            } else {
                recruits
                    .iter()
                    .map(|recruit| {
                        format!(
                            "#{} <@{}> due <t:{}:f>",
                            recruit.id,
                            recruit.user_id,
                            recruit.to_recruit().due_unix
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &content).await;
        }
        Err(err) => {
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await;
        }
    }
}

async fn handle_recruit_panel_command(
    interaction: &Interaction,
    data: &CommandData,
    runtime: &MixedSuperbotRuntime,
) {
    let mut target_user_id = None;
    let mut recruit_id = None;
    for option in &data.options {
        match (option.name.as_str(), &option.value) {
            ("user", CommandOptionValue::User(id)) => target_user_id = Some(id.get()),
            ("recruit_id", CommandOptionValue::Integer(value)) => recruit_id = Some(*value),
            ("recruit_id", CommandOptionValue::String(value)) => {
                recruit_id = value.parse::<i64>().ok();
            }
            _ => {}
        }
    }
    let recruit_id = if let Some(recruit_id) = recruit_id {
        recruit_id
    } else if let Some(user_id) = target_user_id {
        let Some(repo) = runtime.recruit_repo.as_ref() else {
            return;
        };
        match repo
            .list_active_recruits(runtime.config.core.guild_id)
            .await
        {
            Ok(recruits) => recruits
                .into_iter()
                .find(|recruit| recruit.user_id == user_id)
                .map(|recruit| recruit.id)
                .unwrap_or_default(),
            Err(err) => {
                respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await;
                return;
            }
        }
    } else {
        0
    };
    if recruit_id <= 0 {
        respond_interaction_ephemeral_http(
            runtime.http.as_ref(),
            interaction,
            "Missing recruit_id option.",
        )
        .await;
        return;
    }
    match send_recruit_decision_panel(runtime, recruit_id, false).await {
        Ok(message_id) => {
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                &format!("Recruit decision panel sent: {message_id}"),
            )
            .await;
        }
        Err(err) => {
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await;
        }
    }
}

async fn handle_recruit_component(
    interaction: &Interaction,
    custom_id: &str,
    runtime: &MixedSuperbotRuntime,
) {
    if let Some(recruit_id) = custom_id
        .strip_prefix("xiii_recruit_accept:")
        .and_then(|value| value.parse::<i64>().ok())
    {
        let Some(repo) = runtime.recruit_repo.as_ref() else {
            return;
        };
        let Some(discord) = runtime.recruit_discord.as_ref() else {
            return;
        };
        let actor_id = interaction
            .author_id()
            .map(|id| id.get())
            .unwrap_or_default();
        match repo.get_active_recruit_by_id(recruit_id).await {
            Ok(Some(recruit)) => {
                let transition = xiii_recruit::discord_io::accept_transition(
                    recruit.user_id,
                    runtime.config.recruit.recruit_role_id,
                    runtime.config.recruit.next_rank_role_id,
                );
                if let Err(err) = discord
                    .apply_role_transition(recruit.guild_id, &transition)
                    .await
                {
                    println!("[WARN] recruit accept role transition failed: {err}");
                }
                let _ = repo
                    .complete_with_decision(
                        recruit_id,
                        "accepted",
                        "accepted",
                        actor_id,
                        None,
                        chrono::Utc::now(),
                    )
                    .await;
                let final_recruit = repo
                    .get_recruit_by_id(recruit_id)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(recruit.clone());
                let voice_seconds = repo
                    .voice_seconds_for_recruit(&final_recruit, chrono::Utc::now())
                    .await
                    .unwrap_or_default();
                let panel_embed =
                    recruit_embed_from_draft(xiii_recruit::render::processed_decision_embed(
                        &final_recruit,
                        voice_seconds,
                        xiii_recruit::render::accept_decision_label(),
                        actor_id,
                        xiii_recruit::render::LEGACY_ACCEPT_COLOR,
                        None,
                        None,
                        &[],
                    ));
                if let Some((channel_id, message_id)) = interaction
                    .message
                    .as_ref()
                    .map(|message| (interaction_channel_id(interaction), Some(message.id.get())))
                    .and_then(|(channel_id, message_id)| channel_id.zip(message_id))
                    .or_else(|| {
                        recruit
                            .last_decision_channel_id
                            .zip(recruit.last_decision_message_id)
                    })
                {
                    let _ = discord
                        .edit_decision_panel(channel_id, message_id, &panel_embed)
                        .await;
                }
                let _ = discord
                    .dm_user(&xiii_recruit::discord_io::decision_dm_embed(
                        recruit.user_id,
                        recruit_embed_from_draft(xiii_recruit::render::accepted_dm_embed()),
                    ))
                    .await;
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_recruit::render::ACCEPT_SUCCESS,
                )
                .await;
            }
            Ok(None) => {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    "Recruit is no longer active.",
                )
                .await;
            }
            Err(err) => {
                respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await;
            }
        }
        return;
    }

    if let Some(recruit_id) = custom_id.strip_prefix("xiii_recruit_reject:") {
        respond_interaction_modal_http(
            runtime.http.as_ref(),
            interaction,
            &format!("xiii_recruit_reject_modal:{recruit_id}"),
            xiii_recruit::render::REJECT_MODAL_TITLE,
            vec![action_row(vec![text_input(
                "reason",
                xiii_recruit::render::REJECT_REASON_LABEL,
                TextInputStyle::Paragraph,
                true,
            )])],
        )
        .await;
        return;
    }

    if let Some(recruit_id) = custom_id.strip_prefix("xiii_recruit_extend:") {
        respond_interaction_modal_http(
            runtime.http.as_ref(),
            interaction,
            &format!("xiii_recruit_extend_modal:{recruit_id}"),
            xiii_recruit::render::EXTEND_MODAL_TITLE,
            vec![
                action_row(vec![text_input(
                    "days",
                    xiii_recruit::render::EXTEND_DAYS_LABEL,
                    TextInputStyle::Short,
                    true,
                )]),
                action_row(vec![text_input(
                    "reason",
                    xiii_recruit::render::EXTEND_REASON_LABEL,
                    TextInputStyle::Paragraph,
                    true,
                )]),
            ],
        )
        .await;
    }
}

async fn handle_recruit_modal(
    interaction: &Interaction,
    data: &twilight_model::application::interaction::modal::ModalInteractionData,
    runtime: &MixedSuperbotRuntime,
) {
    let Some(repo) = runtime.recruit_repo.as_ref() else {
        return;
    };
    let Some(discord) = runtime.recruit_discord.as_ref() else {
        return;
    };
    let actor_id = interaction
        .author_id()
        .map(|id| id.get())
        .unwrap_or_default();
    if let Some(recruit_id) = data
        .custom_id
        .strip_prefix("xiii_recruit_reject_modal:")
        .and_then(|value| value.parse::<i64>().ok())
    {
        let reason = modal_value(data, "reason")
            .unwrap_or_else(|| xiii_vacation::render::NO_REASON.to_owned());
        match repo.get_active_recruit_by_id(recruit_id).await {
            Ok(Some(recruit)) => {
                let transition = xiii_recruit::discord_io::reject_transition(
                    recruit.user_id,
                    runtime.config.recruit.recruit_role_id,
                    runtime.config.recruit.clan_member_role_id,
                    runtime.config.recruit.guest_role_id,
                );
                if let Err(err) = discord
                    .apply_role_transition(recruit.guild_id, &transition)
                    .await
                {
                    println!("[WARN] recruit reject role transition failed: {err}");
                }
                let _ = repo
                    .complete_with_decision(
                        recruit_id,
                        "rejected",
                        "rejected",
                        actor_id,
                        Some(&reason),
                        chrono::Utc::now(),
                    )
                    .await;
                let final_recruit = repo
                    .get_recruit_by_id(recruit_id)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(recruit.clone());
                let voice_seconds = repo
                    .voice_seconds_for_recruit(&final_recruit, chrono::Utc::now())
                    .await
                    .unwrap_or_default();
                let panel_embed =
                    recruit_embed_from_draft(xiii_recruit::render::processed_decision_embed(
                        &final_recruit,
                        voice_seconds,
                        xiii_recruit::render::reject_decision_label(),
                        actor_id,
                        xiii_recruit::render::LEGACY_REJECT_COLOR,
                        Some(&reason),
                        None,
                        &[],
                    ));
                if let Some((channel_id, message_id)) = recruit
                    .last_decision_channel_id
                    .zip(recruit.last_decision_message_id)
                    .or_else(|| {
                        interaction
                            .message
                            .as_ref()
                            .map(|message| {
                                (interaction_channel_id(interaction), Some(message.id.get()))
                            })
                            .and_then(|(channel_id, message_id)| channel_id.zip(message_id))
                    })
                {
                    let _ = discord
                        .edit_decision_panel(channel_id, message_id, &panel_embed)
                        .await;
                }
                let _ = discord
                    .dm_user(&xiii_recruit::discord_io::decision_dm_embed(
                        recruit.user_id,
                        recruit_embed_from_draft(xiii_recruit::render::rejected_dm_embed(&reason)),
                    ))
                    .await;
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_recruit::render::REJECT_SUCCESS,
                )
                .await;
            }
            _ => {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    "Recruit is no longer active.",
                )
                .await
            }
        }
        return;
    }
    if let Some(recruit_id) = data
        .custom_id
        .strip_prefix("xiii_recruit_extend_modal:")
        .and_then(|value| value.parse::<i64>().ok())
    {
        let days = modal_value(data, "days")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(runtime.config.recruit.default_days as i64);
        let reason = modal_value(data, "reason").unwrap_or_else(|| "Продлено".to_owned());
        let due_at = chrono::Utc::now() + chrono::Duration::days(days.max(1));
        let prior_recruit = repo
            .get_active_recruit_by_id(recruit_id)
            .await
            .ok()
            .flatten();
        match repo
            .extend_with_decision(
                recruit_id,
                actor_id,
                due_at,
                &reason,
                days.max(1),
                chrono::Utc::now(),
            )
            .await
        {
            Ok(true) => {
                if let Ok(Some(recruit)) = repo.get_active_recruit_by_id(recruit_id).await {
                    let voice_seconds = repo
                        .voice_seconds_for_recruit(&recruit, chrono::Utc::now())
                        .await
                        .unwrap_or_default();
                    let panel_embed =
                        recruit_embed_from_draft(xiii_recruit::render::processed_decision_embed(
                            &recruit,
                            voice_seconds,
                            xiii_recruit::render::extend_decision_label(),
                            actor_id,
                            xiii_recruit::render::LEGACY_EXTEND_COLOR,
                            Some(&reason),
                            Some(days.max(1)),
                            &[],
                        ));
                    if let Some((channel_id, message_id)) = prior_recruit
                        .as_ref()
                        .and_then(|value| {
                            value
                                .last_decision_channel_id
                                .zip(value.last_decision_message_id)
                        })
                        .or_else(|| {
                            interaction
                                .message
                                .as_ref()
                                .map(|message| {
                                    (interaction_channel_id(interaction), Some(message.id.get()))
                                })
                                .and_then(|(channel_id, message_id)| channel_id.zip(message_id))
                        })
                    {
                        let _ = discord
                            .edit_decision_panel(channel_id, message_id, &panel_embed)
                            .await;
                    }
                    let _ = discord
                        .dm_user(&xiii_recruit::discord_io::decision_dm_embed(
                            recruit.user_id,
                            recruit_embed_from_draft(xiii_recruit::render::extended_dm_embed(
                                days.max(1),
                                &reason,
                            )),
                        ))
                        .await;
                }
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    xiii_recruit::render::EXTEND_SUCCESS,
                )
                .await
            }
            Ok(false) => {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    "Recruit is no longer active.",
                )
                .await
            }
            Err(err) => {
                respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await
            }
        }
    }
}

async fn handle_recruit_voice_state_update(
    update: Box<VoiceStateUpdate>,
    runtime: &MixedSuperbotRuntime,
) {
    let Some(repo) = runtime.recruit_repo.as_ref() else {
        return;
    };
    let Some(guild_id) = update.guild_id.map(|id| id.get()) else {
        return;
    };
    let user_id = update.user_id.get();
    let channel_id = update.channel_id.map(|id| id.get());
    let active = match repo.list_active_recruits(guild_id).await {
        Ok(active) => active.into_iter().any(|recruit| recruit.user_id == user_id),
        Err(err) => {
            println!("[WARN] recruit voice tracking failed to list active recruits: {err}");
            return;
        }
    };
    if !active {
        return;
    }
    if let Some(channel_id) = channel_id {
        if xiii_recruit::runtime::voice_channel_is_tracked(
            channel_id,
            runtime.config.recruit.excluded_voice_channel_id,
        ) {
            let _ = repo
                .open_voice_session(guild_id, user_id, channel_id, chrono::Utc::now())
                .await;
        }
    } else {
        let _ = repo
            .close_open_voice_sessions(guild_id, user_id, chrono::Utc::now(), false)
            .await;
    }
}

async fn recruit_due_checker_tick(runtime: &MixedSuperbotRuntime) -> Result<(), String> {
    let Some(repo) = runtime.recruit_repo.as_ref() else {
        return Ok(());
    };
    let due = repo
        .list_due_active_without_panel(runtime.config.core.guild_id, chrono::Utc::now())
        .await?;
    for recruit in due {
        let _ = send_recruit_decision_panel(runtime, recruit.id, true).await?;
    }
    Ok(())
}

async fn send_recruit_decision_panel(
    runtime: &MixedSuperbotRuntime,
    recruit_id: i64,
    automatic: bool,
) -> Result<u64, String> {
    let Some(repo) = runtime.recruit_repo.as_ref() else {
        return Err("recruit repository is not initialized".to_owned());
    };
    let Some(discord) = runtime.recruit_discord.as_ref() else {
        return Err("recruit Discord adapter is not initialized".to_owned());
    };
    let Some(recruit) = repo.get_active_recruit_by_id(recruit_id).await? else {
        return Err(format!("recruit {recruit_id} is not active"));
    };
    if automatic
        && recruit.last_decision_message_id.is_some()
        && recruit.last_decision_channel_id.is_some()
    {
        return Ok(recruit.last_decision_message_id.unwrap_or_default());
    }
    let content = if automatic && xiii_recruit::runtime::should_ping_decision_roles(true) {
        runtime
            .config
            .recruit
            .decision_ping_role_ids
            .iter()
            .map(|role_id| format!("<@&{role_id}>"))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        String::new()
    };
    let components = vec![action_row(vec![
        button(
            xiii_recruit::interactions::accept_button_id(recruit_id),
            xiii_recruit::render::ACCEPT_BUTTON_LABEL,
            ButtonStyle::Success,
        ),
        button(
            xiii_recruit::interactions::reject_button_id(recruit_id),
            xiii_recruit::render::REJECT_BUTTON_LABEL,
            ButtonStyle::Danger,
        ),
        button(
            xiii_recruit::interactions::extend_button_id(recruit_id),
            xiii_recruit::render::EXTEND_BUTTON_LABEL,
            ButtonStyle::Secondary,
        ),
    ])];
    let voice_seconds = repo
        .voice_seconds_for_recruit(&recruit, chrono::Utc::now())
        .await
        .unwrap_or_default();
    let embed = recruit_embed_from_draft(xiii_recruit::render::decision_panel_embed(
        &recruit,
        voice_seconds,
    ));
    let message = discord
        .send_decision_panel(
            runtime.config.recruit.decision_channel_id,
            &content,
            if automatic {
                &runtime.config.recruit.decision_ping_role_ids
            } else {
                &[]
            },
            Some(embed),
            &components,
        )
        .await?;
    repo.set_decision_message(
        recruit_id,
        runtime.config.recruit.decision_channel_id,
        message.id.get(),
        chrono::Utc::now(),
    )
    .await?;
    Ok(message.id.get())
}

async fn seed_voice_activity_members(runtime: &MixedSuperbotRuntime, members: &[DiscordMember]) {
    let mut cache = runtime.voice_activity_members.lock().await;
    for member in members {
        cache.insert(
            member.user.id.get(),
            VoiceCachedMember {
                user_id: member.user.id.get(),
                display_name: display_name_from_member(member),
                username: Some(member.user.name.clone()),
                role_ids: member.roles.iter().map(|role| role.get()).collect(),
                is_bot: member.user.bot,
            },
        );
    }
}

async fn voice_activity_startup_reconcile(
    runtime: &MixedSuperbotRuntime,
    guild_id: u64,
    states: &[twilight_model::voice::VoiceState],
) -> Result<(), String> {
    let Some(repo) = runtime.voice_repo.as_ref() else {
        return Ok(());
    };
    let members = runtime.voice_activity_members.lock().await.clone();
    let live = states
        .iter()
        .filter_map(|state| {
            let channel_id = state.channel_id.map(|id| id.get())?;
            let cached = members.get(&state.user_id.get())?;
            Some(xiii_voice_activity::state::LiveVoiceMember {
                user_id: cached.user_id,
                channel_id,
                display_name: cached.display_name.clone(),
                username: cached.username.clone(),
                is_bot: cached.is_bot,
            })
        })
        .collect::<Vec<_>>();
    let active = repo.list_active_sessions(guild_id).await?;
    let now = chrono::Utc::now();
    let actions = xiii_voice_activity::runtime::reconcile_actions(
        guild_id,
        &active,
        &live,
        &runtime.config.voice_activity.ignored_channel_ids,
        now,
    );
    for action in actions {
        apply_voice_reconcile_action(repo, action).await?;
    }
    repo.set_bot_state(
        "last_heartbeat",
        &now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    )
    .await?;
    println!(
        "[OK] voice_activity startup reconciliation active_db={} live_tracked={}",
        active.len(),
        live.len()
    );
    Ok(())
}

async fn apply_voice_reconcile_action(
    repo: &xiii_voice_activity::repository::LegacySqliteVoiceActivityRepository,
    action: xiii_voice_activity::runtime::ReconcileAction,
) -> Result<(), String> {
    match action {
        xiii_voice_activity::runtime::ReconcileAction::CloseStale {
            guild_id,
            user_id,
            ended_at,
            reason,
        } => {
            let _ = repo
                .close_active_session(guild_id, user_id, &ended_at, &reason)
                .await?;
        }
        xiii_voice_activity::runtime::ReconcileAction::UpdateChannel {
            guild_id,
            user_id,
            channel_id,
            last_seen_at,
        } => {
            if !repo
                .update_active_session_channel(guild_id, user_id, channel_id, &last_seen_at)
                .await?
            {
                return Err(format!(
                    "voice active session missing while updating channel for user {user_id}"
                ));
            }
        }
        xiii_voice_activity::runtime::ReconcileAction::Touch {
            guild_id,
            user_id,
            last_seen_at,
        } => {
            let _ = repo
                .update_active_session_last_seen(guild_id, user_id, &last_seen_at)
                .await?;
        }
        xiii_voice_activity::runtime::ReconcileAction::OpenRecovered { session, user } => {
            repo.upsert_user(&user).await?;
            repo.create_or_replace_active_session(&session).await?;
        }
    }
    Ok(())
}

async fn handle_voice_activity_state_update(
    update: Box<twilight_model::gateway::payload::incoming::VoiceStateUpdate>,
    runtime: &MixedSuperbotRuntime,
) {
    let Some(repo) = runtime.voice_repo.as_ref() else {
        return;
    };
    let Some(guild_id) = update.guild_id.map(|id| id.get()) else {
        return;
    };
    let user_id = update.user_id.get();
    let is_bot = update
        .member
        .as_ref()
        .map(|member| member.user.bot)
        .unwrap_or(false);
    if is_bot {
        return;
    }
    if let Some(member) = update.member.as_ref() {
        let mut cache = runtime.voice_activity_members.lock().await;
        cache.insert(
            user_id,
            VoiceCachedMember {
                user_id,
                display_name: display_name_from_member(member),
                username: Some(member.user.name.clone()),
                role_ids: member.roles.iter().map(|role| role.get()).collect(),
                is_bot,
            },
        );
    }

    let new_channel_id = update.channel_id.map(|id| id.get());
    let transition = {
        let mut occupancy = runtime.voice_activity_occupancy.lock().await;
        occupancy.apply_voice_update(guild_id, user_id, new_channel_id)
    };
    let action = xiii_voice_activity::runtime::classify_voice_update(
        transition.old_channel_id,
        new_channel_id,
        &runtime.config.voice_activity.ignored_channel_ids,
    );
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let cached = runtime
        .voice_activity_members
        .lock()
        .await
        .get(&user_id)
        .cloned();
    let display_name = cached
        .as_ref()
        .map(|member| member.display_name.clone())
        .unwrap_or_else(|| user_id.to_string());
    let username = cached.and_then(|member| member.username);
    let result = match action {
        xiii_voice_activity::runtime::VoiceTrackingAction::Ignored => Ok(()),
        xiii_voice_activity::runtime::VoiceTrackingAction::Join { channel_id } => {
            match repo
                .upsert_user(&xiii_voice_activity::state::StoredVoiceUser {
                    user_id,
                    display_name,
                    username,
                    last_seen_at: now.clone(),
                })
                .await
            {
                Ok(()) => {
                    repo.create_or_replace_active_session(
                        &xiii_voice_activity::state::ActiveVoiceSession {
                            guild_id,
                            user_id,
                            channel_id,
                            started_at: now.clone(),
                            last_seen_at: now.clone(),
                            recovered: false,
                        },
                    )
                    .await
                }
                Err(err) => Err(err),
            }
        }
        xiii_voice_activity::runtime::VoiceTrackingAction::Leave { .. } => repo
            .close_active_session(guild_id, user_id, &now, "normal")
            .await
            .map(|_| ()),
        xiii_voice_activity::runtime::VoiceTrackingAction::Move { to_channel_id, .. } => match repo
            .update_active_session_channel(guild_id, user_id, to_channel_id, &now)
            .await
        {
            Ok(true) => Ok(()),
            Ok(false) => {
                repo.create_or_replace_active_session(
                    &xiii_voice_activity::state::ActiveVoiceSession {
                        guild_id,
                        user_id,
                        channel_id: to_channel_id,
                        started_at: now.clone(),
                        last_seen_at: now.clone(),
                        recovered: true,
                    },
                )
                .await
            }
            Err(err) => Err(err),
        },
        xiii_voice_activity::runtime::VoiceTrackingAction::Stay { .. } => repo
            .update_active_session_last_seen(guild_id, user_id, &now)
            .await
            .map(|_| ()),
    };
    if let Err(err) = result {
        println!("[WARN] voice_activity voice-state write failed for user {user_id}: {err}");
    }
}

async fn voice_activity_report_inputs(
    runtime: &MixedSuperbotRuntime,
    period_key: &str,
) -> Result<VoiceActivityReportInputs, String> {
    let Some(repo) = runtime.voice_repo.as_ref() else {
        return Err("voice_activity repository is not initialized".to_owned());
    };
    let now = chrono::Utc::now();
    let since = xiii_voice_activity::runtime::period_start(period_key, now)
        .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let users = repo.list_users().await?;
    let completed = repo
        .fetch_completed_sessions_since(runtime.config.core.guild_id, since.as_deref())
        .await?;
    let active = repo
        .list_active_sessions(runtime.config.core.guild_id)
        .await?;
    Ok(VoiceActivityReportInputs {
        users,
        completed,
        active,
        now,
    })
}

struct VoiceActivityReportInputs {
    users: Vec<xiii_voice_activity::state::StoredVoiceUser>,
    completed: Vec<xiii_voice_activity::state::CompletedVoiceSession>,
    active: Vec<xiii_voice_activity::state::ActiveVoiceSession>,
    now: chrono::DateTime<chrono::Utc>,
}

async fn handle_voice_activity_inactive_command(
    interaction: &Interaction,
    runtime: &MixedSuperbotRuntime,
    period_key: &str,
    page: usize,
) {
    match voice_activity_inactive_view(runtime, period_key, page, false).await {
        Ok((embed, components, _, _)) => {
            respond_interaction_embeds_http(
                runtime.http.as_ref(),
                interaction,
                vec![embed],
                Some(components),
            )
            .await;
        }
        Err(err) => {
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                &format!("Voice inactive check failed: {err}"),
            )
            .await;
        }
    }
}

async fn handle_voice_activity_component(
    interaction: &Interaction,
    data: &twilight_model::application::interaction::message_component::MessageComponentInteractionData,
    runtime: &MixedSuperbotRuntime,
) {
    let custom_id = data.custom_id.as_str();
    if custom_id.starts_with("public-stats-panel:") {
        let (current_period, current_page, _) =
            voice_activity_public_panel_state(interaction.message.as_ref());
        let period = if custom_id == xiii_voice_activity::interactions::PUBLIC_STATS_PERIOD {
            data.values
                .first()
                .map(String::as_str)
                .unwrap_or(current_period.as_str())
        } else {
            current_period.as_str()
        };
        let page = if custom_id == xiii_voice_activity::interactions::PUBLIC_STATS_PREVIOUS {
            current_page.saturating_sub(1)
        } else if custom_id == xiii_voice_activity::interactions::PUBLIC_STATS_NEXT {
            current_page.saturating_add(1)
        } else {
            0
        };
        match voice_activity_public_view(runtime, period, page).await {
            Ok((embed, components, _, _)) => {
                respond_interaction_update_embeds_http(
                    runtime.http.as_ref(),
                    interaction,
                    vec![embed],
                    Some(components),
                )
                .await;
            }
            Err(err) => {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    &format!("Voice stats panel refresh failed: {err}"),
                )
                .await;
            }
        }
    } else if custom_id.starts_with("inactive-check:") {
        let (current_period, current_page, auto_title) = voice_activity_inactive_panel_state(
            interaction.message.as_ref(),
            runtime.config.voice_activity.page_size as usize,
        );
        let period = if custom_id == xiii_voice_activity::interactions::INACTIVE_PERIOD {
            data.values
                .first()
                .map(String::as_str)
                .unwrap_or(current_period.as_str())
        } else {
            current_period.as_str()
        };
        let page = if custom_id == xiii_voice_activity::interactions::INACTIVE_PREVIOUS {
            current_page.saturating_sub(1)
        } else if custom_id == xiii_voice_activity::interactions::INACTIVE_NEXT {
            current_page.saturating_add(1)
        } else {
            0
        };
        match voice_activity_inactive_view(runtime, period, page, auto_title).await {
            Ok((embed, components, _, _)) => {
                respond_interaction_update_embeds_http(
                    runtime.http.as_ref(),
                    interaction,
                    vec![embed],
                    Some(components),
                )
                .await;
            }
            Err(err) => {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    &format!("Voice inactive check failed: {err}"),
                )
                .await;
            }
        }
    }
}

async fn voice_activity_public_view(
    runtime: &MixedSuperbotRuntime,
    period_key: &str,
    page: usize,
) -> Result<(Embed, Vec<Component>, usize, usize), String> {
    let inputs = voice_activity_report_inputs(runtime, period_key).await?;
    let total_pages = xiii_voice_activity::render::leaderboard_total_pages(
        &inputs.completed,
        &inputs.active,
        period_key,
        runtime.config.voice_activity.page_size as usize,
        inputs.now,
    );
    let clamped_page = page.min(total_pages.saturating_sub(1));
    let entries = xiii_voice_activity::render::leaderboard_entries(
        &inputs.users,
        &inputs.completed,
        &inputs.active,
        period_key,
        clamped_page,
        runtime.config.voice_activity.page_size as usize,
        inputs.now,
    );
    let description = xiii_voice_activity::render::render_leaderboard_description(
        period_key,
        clamped_page,
        total_pages,
        &entries,
    );
    let embed = embed_with_appearance(
        "XIII Voice Activity",
        &description,
        xiii_voice_activity::render::LEGACY_EMBED_COLOR,
        Some(xiii_voice_activity::render::LEGACY_FOOTER),
        true,
    );
    let components = voice_activity_public_stats_components(period_key, clamped_page, total_pages);
    Ok((embed, components, clamped_page, total_pages))
}

async fn voice_activity_inactive_view(
    runtime: &MixedSuperbotRuntime,
    period_key: &str,
    page: usize,
    auto_title: bool,
) -> Result<(Embed, Vec<Component>, usize, usize), String> {
    let inputs = voice_activity_report_inputs(runtime, period_key).await?;
    let members = runtime
        .voice_activity_members
        .lock()
        .await
        .values()
        .map(|member| xiii_voice_activity::state::VoiceMemberForReport {
            user_id: member.user_id,
            display_name: member.display_name.clone(),
            role_ids: member.role_ids.clone(),
            is_bot: member.is_bot,
        })
        .collect::<Vec<_>>();
    let total_pages = xiii_voice_activity::render::inactive_total_pages(
        &members,
        &inputs.completed,
        &inputs.active,
        runtime.config.voice_activity.inactive_role_id,
        runtime.config.voice_activity.vacation_marker_role_id,
        period_key,
        runtime.config.voice_activity.page_size as usize,
        inputs.now,
    );
    let clamped_page = page.min(total_pages.saturating_sub(1));
    let entries = xiii_voice_activity::render::inactive_entries(
        &members,
        &inputs.completed,
        &inputs.active,
        runtime.config.voice_activity.inactive_role_id,
        runtime.config.voice_activity.vacation_marker_role_id,
        period_key,
        clamped_page,
        runtime.config.voice_activity.page_size as usize,
        inputs.now,
    );
    let description =
        xiii_voice_activity::render::render_inactive_description(period_key, &entries);
    let title = if auto_title {
        format!(
            "XIII Inactivity Check · {}",
            xiii_voice_activity::render::inactive_period_label(period_key)
        )
    } else {
        "XIII Inactivity Check".to_owned()
    };
    let embed = embed_with_appearance(
        &title,
        &description,
        xiii_voice_activity::render::LEGACY_EMBED_COLOR,
        Some(xiii_voice_activity::render::LEGACY_FOOTER),
        true,
    );
    let components = voice_activity_inactive_components(period_key, clamped_page, total_pages);
    Ok((embed, components, clamped_page, total_pages))
}

async fn voice_activity_public_panel_refresh_tick(
    runtime: &MixedSuperbotRuntime,
) -> Result<(), String> {
    voice_activity_public_panel_refresh_tick_for_period(runtime, "7d", 0).await
}

async fn voice_activity_public_panel_refresh_tick_for_period(
    runtime: &MixedSuperbotRuntime,
    period_key: &str,
    page: usize,
) -> Result<(), String> {
    if !runtime.config.voice_activity.public_stats_panel_enabled {
        return Ok(());
    }
    let state = load_voice_activity_panel_state(runtime)?;
    let (embed, components, _, _) = voice_activity_public_view(runtime, period_key, page).await?;
    runtime
        .http
        .update_message(
            Id::<ChannelMarker>::new(state.public_stats_panel.channel_id),
            Id::<MessageMarker>::new(state.public_stats_panel.message_id),
        )
        .allowed_mentions(Some(&AllowedMentions::default()))
        .embeds(Some(&[embed]))
        .components(Some(&components))
        .await
        .map_err(|err| format!("failed to edit voice activity panel: {err}"))?;
    Ok(())
}

async fn voice_activity_heartbeat_tick(runtime: &MixedSuperbotRuntime) -> Result<(), String> {
    let Some(repo) = runtime.voice_repo.as_ref() else {
        return Ok(());
    };
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    repo.set_bot_state("last_heartbeat", &now).await
}

async fn voice_activity_auto_report_tick(runtime: &MixedSuperbotRuntime) -> Result<(), String> {
    if !runtime.config.voice_activity.auto_reports_enabled {
        return Ok(());
    }
    let Some(repo) = runtime.voice_repo.as_ref() else {
        return Ok(());
    };
    let now = chrono::Utc::now();
    let last_sent = repo.get_bot_state("auto_report_last_sent_at").await?;
    if last_sent.is_some()
        || !runtime
            .config
            .voice_activity
            .auto_report_send_on_first_start
    {
        return Ok(());
    }
    let (embed, _, _, _) = voice_activity_inactive_view(runtime, "7d", 0, true).await?;
    runtime
        .http
        .create_message(Id::<ChannelMarker>::new(
            runtime.config.voice_activity.auto_report_channel_id,
        ))
        .allowed_mentions(Some(&AllowedMentions::default()))
        .embeds(&[embed])
        .await
        .map_err(|err| format!("failed to send voice auto inactive report: {err}"))?;
    repo.set_bot_state(
        "auto_report_last_sent_at",
        &now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    )
    .await?;
    Ok(())
}

fn load_voice_activity_panel_state(
    runtime: &MixedSuperbotRuntime,
) -> Result<xiii_voice_activity::state::VoicePanelState, String> {
    let path = superbot_state_file(&runtime.state_dir, "voice_activity_panel_state.json");
    let content = fs::read_to_string(&path).map_err(|err| {
        format!(
            "failed to read voice activity panel state {}: {err}",
            path.display()
        )
    })?;
    serde_json::from_str(&content).map_err(|err| {
        format!(
            "failed to parse voice activity panel state {}: {err}",
            path.display()
        )
    })
}

fn voice_activity_public_stats_components(
    period_key: &str,
    page: usize,
    total_pages: usize,
) -> Vec<Component> {
    vec![
        action_row(vec![text_select(
            xiii_voice_activity::interactions::PUBLIC_STATS_PERIOD,
            xiii_voice_activity::render::PERIOD_SELECT_PLACEHOLDER,
            vec![
                (
                    xiii_voice_activity::runtime::period_label("7d"),
                    "7d",
                    period_key == "7d",
                ),
                (
                    xiii_voice_activity::runtime::period_label("14d"),
                    "14d",
                    period_key == "14d",
                ),
                (
                    xiii_voice_activity::runtime::period_label("30d"),
                    "30d",
                    period_key == "30d",
                ),
                (
                    xiii_voice_activity::runtime::period_label("all"),
                    "all",
                    period_key == "all",
                ),
            ],
        )]),
        action_row(vec![
            button_with_disabled(
                xiii_voice_activity::interactions::PUBLIC_STATS_PREVIOUS,
                xiii_voice_activity::render::PREVIOUS_LABEL,
                ButtonStyle::Secondary,
                page == 0,
            ),
            button_with_disabled(
                xiii_voice_activity::interactions::PUBLIC_STATS_NEXT,
                xiii_voice_activity::render::NEXT_LABEL,
                ButtonStyle::Secondary,
                page + 1 >= total_pages.max(1),
            ),
        ]),
    ]
}

fn voice_activity_inactive_components(
    period_key: &str,
    page: usize,
    total_pages: usize,
) -> Vec<Component> {
    vec![
        action_row(vec![text_select(
            xiii_voice_activity::interactions::INACTIVE_PERIOD,
            xiii_voice_activity::render::PERIOD_SELECT_PLACEHOLDER,
            vec![
                (
                    xiii_voice_activity::render::inactive_period_label("7d"),
                    "7d",
                    period_key == "7d",
                ),
                (
                    xiii_voice_activity::render::inactive_period_label("14d"),
                    "14d",
                    period_key == "14d",
                ),
                (
                    xiii_voice_activity::render::inactive_period_label("30d"),
                    "30d",
                    period_key == "30d",
                ),
                (
                    xiii_voice_activity::render::inactive_period_label("60d"),
                    "60d",
                    period_key == "60d",
                ),
            ],
        )]),
        action_row(vec![
            button_with_disabled(
                xiii_voice_activity::interactions::INACTIVE_PREVIOUS,
                xiii_voice_activity::render::PREVIOUS_LABEL,
                ButtonStyle::Secondary,
                page == 0,
            ),
            button_with_disabled(
                xiii_voice_activity::interactions::INACTIVE_NEXT,
                xiii_voice_activity::render::NEXT_LABEL,
                ButtonStyle::Secondary,
                page + 1 >= total_pages.max(1),
            ),
        ]),
    ]
}

fn voice_activity_public_panel_state(message: Option<&DiscordMessage>) -> (String, usize, usize) {
    let Some(description) = message
        .and_then(|message| message.embeds.first())
        .and_then(|embed| embed.description.as_deref())
    else {
        return ("7d".to_owned(), 0, 1);
    };
    let Some(first_line) = description.lines().next() else {
        return ("7d".to_owned(), 0, 1);
    };
    let Some(rest) = first_line.strip_prefix("Период: ") else {
        return ("7d".to_owned(), 0, 1);
    };
    let (label, page_part) = rest.split_once(" · Страница ").unwrap_or((rest, "1/1"));
    let period_key = voice_activity_period_key_from_label(label.trim())
        .unwrap_or("7d")
        .to_owned();
    let (page_raw, total_raw) = page_part.split_once('/').unwrap_or(("1", "1"));
    let page = page_raw
        .trim()
        .parse::<usize>()
        .ok()
        .unwrap_or(1)
        .saturating_sub(1);
    let total_pages = total_raw.trim().parse::<usize>().ok().unwrap_or(1).max(1);
    (period_key, page, total_pages)
}

fn voice_activity_period_key_from_label(label: &str) -> Option<&'static str> {
    ["7d", "14d", "30d", "all"]
        .into_iter()
        .find(|key| xiii_voice_activity::runtime::period_label(key) == label)
}

fn voice_activity_inactive_panel_state(
    message: Option<&DiscordMessage>,
    page_size: usize,
) -> (String, usize, bool) {
    let Some(embed) = message.and_then(|message| message.embeds.first()) else {
        return ("7d".to_owned(), 0, false);
    };
    let description = embed.description.as_deref().unwrap_or_default();
    let title = embed.title.as_deref().unwrap_or_default();
    let mut period_key = "7d".to_owned();
    let mut page = 0usize;
    let mut saw_rank = false;
    for line in description.lines() {
        let trimmed = line.trim();
        if let Some(label) = trimmed.strip_prefix("Период: ") {
            period_key = match label.trim() {
                "7 дней / 10ч" => "7d",
                "14 дней / 20ч" => "14d",
                "30 дней / 40ч" => "30d",
                "60 дней / 80ч" => "60d",
                _ => "7d",
            }
            .to_owned();
            continue;
        }
        if saw_rank {
            continue;
        }
        let Some((prefix, _)) = trimmed.split_once('.') else {
            continue;
        };
        let Ok(rank) = prefix.parse::<usize>() else {
            continue;
        };
        page = rank.saturating_sub(1) / page_size.max(1);
        saw_rank = true;
    }
    (
        period_key,
        page,
        title.starts_with("XIII Inactivity Check · "),
    )
}

async fn run_temp_voice_runtime(
    env_file: PathBuf,
    config: SuperbotConfig,
    health_output: Option<PathBuf>,
) -> ExitCode {
    let token = match read_secret_from_env_file(&env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(message) => {
            println!("[FAIL] discord {message}");
            return ExitCode::from(2);
        }
    };

    let repository =
        match xiii_tempvoice::repository::LegacySqliteTempVoiceRepository::open_existing_writable(
            &config.legacy_paths.temp_voice_db.resolved,
        )
        .await
        {
            Ok(repository) => repository,
            Err(err) => {
                println!("[FAIL] temp_voice repository {err}");
                return ExitCode::from(2);
            }
        };

    let http = Arc::new(DiscordHttpClient::new(token.clone()));
    let discord = xiii_tempvoice::discord_io::TempVoiceDiscordHttp::new(http.clone());
    let runtime = xiii_tempvoice::runtime::TempVoiceRuntime::new(repository, discord);
    let occupancy = Arc::new(Mutex::new(TempVoiceOccupancy::default()));

    let mut identity_report = Report::new();
    match fetch_current_user_with_retry(http.as_ref(), &mut identity_report).await {
        Ok(user) => println!("[OK] current bot user id = {}", user.id.get()),
        Err(err) => {
            println!("[FAIL] discord {err}");
            return ExitCode::from(2);
        }
    }

    if let Some(path) = health_output.as_deref() {
        let _ = write_superbot_runtime_health(
            path,
            &config,
            "temp_voice",
            "starting",
            "gateway connecting",
        );
    }

    let intents = Intents::GUILDS | Intents::GUILD_VOICE_STATES;
    let mut shard = Shard::new(ShardId::ONE, token, intents);
    println!("[OK] temp_voice Gateway starting");
    println!("[OK] Discord writes are limited to setup-hub responses, temp channel create/move/delete, and DB-owned channel cleanup");
    println!("[OK] slash command auto-sync is disabled");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("[OK] shutdown signal received; temp_voice runtime stopping gracefully");
                if let Some(path) = health_output.as_deref() {
                    let _ = write_superbot_runtime_health(path, &config, "temp_voice", "stopped", "graceful shutdown");
                }
                return ExitCode::SUCCESS;
            }
            event = shard.next_event(EventTypeFlags::all()) => {
                let Some(event) = event else {
                    println!("[FAIL] temp_voice Gateway stream ended");
                    if let Some(path) = health_output.as_deref() {
                        let _ = write_superbot_runtime_health(path, &config, "temp_voice", "fail", "gateway stream ended");
                    }
                    return ExitCode::from(2);
                };
                match event {
                    Ok(event) => {
                        handle_temp_voice_gateway_event(
                            event,
                            &runtime,
                            &occupancy,
                            &config,
                        ).await;
                    }
                    Err(err) => {
                        println!("[WARN] temp_voice Gateway event receive failed: {err}");
                    }
                }
            }
        }
    }
}

async fn handle_temp_voice_gateway_event(
    event: Event,
    runtime: &xiii_tempvoice::runtime::TempVoiceRuntime,
    occupancy: &Arc<Mutex<TempVoiceOccupancy>>,
    config: &SuperbotConfig,
) {
    match event {
        Event::Ready(ready) => {
            println!(
                "[OK] Gateway READY for temp_voice session user_id={}",
                ready.user.id.get()
            );
        }
        Event::GuildCreate(guild) => {
            if let GuildCreate::Available(guild) = guild.as_ref() {
                {
                    let mut occupancy = occupancy.lock().await;
                    occupancy.seed_guild(guild.id.get(), guild.owner_id.get(), &guild.voice_states);
                }
                let counts = occupancy.lock().await.channel_counts();
                match runtime
                    .startup_reconcile(|channel_id| counts.get(&channel_id).copied().unwrap_or(0))
                    .await
                {
                    Ok(outcomes) => {
                        for outcome in outcomes {
                            println!("[OK] temp_voice startup reconciliation {outcome:?}");
                        }
                    }
                    Err(err) => println!("[WARN] temp_voice startup reconciliation skipped: {err}"),
                }
            }
        }
        Event::InteractionCreate(interaction) => {
            handle_temp_voice_interaction(&interaction.0, runtime, occupancy).await;
        }
        Event::VoiceStateUpdate(update) => {
            handle_temp_voice_state_update(update, runtime, occupancy, config).await;
        }
        Event::ChannelDelete(channel) => {
            let channel_id = channel.id.get();
            if let Err(err) = runtime.delete_tracked_channel_if_empty(channel_id, 0).await {
                println!(
                    "[WARN] temp_voice channel-delete cleanup skipped for {channel_id}: {err}"
                );
            }
        }
        _ => {}
    }
}

async fn handle_temp_voice_interaction(
    interaction: &Interaction,
    runtime: &xiii_tempvoice::runtime::TempVoiceRuntime,
    occupancy: &Arc<Mutex<TempVoiceOccupancy>>,
) {
    let Some(InteractionData::ApplicationCommand(data)) = interaction.data.as_ref() else {
        return;
    };
    if data.name != "setup-voice-hub" {
        return;
    }

    let response =
        match setup_voice_hub_interaction_target(interaction, data.as_ref(), occupancy).await {
            Ok((guild_id, target_channel_id)) => {
                match runtime.setup_voice_hub(guild_id, target_channel_id).await {
                    Ok(_) => "Voice hub channel has been set successfully.".to_owned(),
                    Err(err) if err.contains("regular voice") => {
                        "This channel is not a regular voice channel.".to_owned()
                    }
                    Err(err) if err.contains("server") => {
                        "This channel does not belong to this server.".to_owned()
                    }
                    Err(_) => "Invalid voice channel ID.".to_owned(),
                }
            }
            Err(message) => message,
        };

    if let Err(err) = runtime
        .respond_interaction_ephemeral(
            interaction.application_id.get(),
            interaction.id.get(),
            interaction.token.as_str(),
            &response,
        )
        .await
    {
        println!("[WARN] temp_voice failed to respond to /setup-voice-hub: {err}");
    }
}

async fn setup_voice_hub_interaction_target(
    interaction: &Interaction,
    data: &CommandData,
    occupancy: &Arc<Mutex<TempVoiceOccupancy>>,
) -> Result<(u64, u64), String> {
    let Some(guild_id) = interaction.guild_id.map(|id| id.get()) else {
        return Err("This command can only be used in a server.".to_owned());
    };
    let Some(user_id) = interaction.author_id().map(|id| id.get()) else {
        return Err("You do not have permission to use this command.".to_owned());
    };

    let is_owner = occupancy.lock().await.guild_owner_id(guild_id) == Some(user_id);
    let permissions = interaction
        .member
        .as_ref()
        .and_then(|member| member.permissions);
    if !xiii_tempvoice::discord_io::user_can_setup_hub(is_owner, permissions) {
        return Err("You do not have permission to use this command.".to_owned());
    }

    let Some(target_channel_id) = data.options.iter().find_map(|option| {
        if option.name != "channel_id" {
            return None;
        }
        match &option.value {
            CommandOptionValue::Channel(id) => Some(id.get()),
            CommandOptionValue::String(value) => value.parse::<u64>().ok(),
            CommandOptionValue::Integer(value) => u64::try_from(*value).ok(),
            _ => None,
        }
    }) else {
        return Err("Invalid channel ID format.".to_owned());
    };

    Ok((guild_id, target_channel_id))
}

async fn handle_temp_voice_state_update(
    update: Box<twilight_model::gateway::payload::incoming::VoiceStateUpdate>,
    runtime: &xiii_tempvoice::runtime::TempVoiceRuntime,
    occupancy: &Arc<Mutex<TempVoiceOccupancy>>,
    config: &SuperbotConfig,
) {
    let Some(guild_id) = update.guild_id.map(|id| id.get()) else {
        return;
    };
    let user_id = update.user_id.get();
    let is_bot = update
        .member
        .as_ref()
        .map(|member| member.user.bot)
        .unwrap_or(false);
    if is_bot {
        return;
    }

    let new_channel_id = update.channel_id.map(|id| id.get());
    let display_name = update
        .member
        .as_ref()
        .map(display_name_from_member)
        .unwrap_or_else(|| user_id.to_string());

    let transition = {
        let mut occupancy = occupancy.lock().await;
        occupancy.apply_voice_update(guild_id, user_id, new_channel_id)
    };

    if let Some(old_channel_id) = transition.old_channel_id {
        if transition.old_channel_member_count == 0 {
            handle_temp_voice_empty_candidate(
                runtime.clone(),
                occupancy.clone(),
                old_channel_id,
                config.temp_voice.delete_after_seconds,
            )
            .await;
        }
    }

    let hub = match runtime_hub_for_guild(runtime, guild_id).await {
        Ok(hub) => hub,
        Err(err) => {
            println!("[WARN] temp_voice failed to read hub setting for guild {guild_id}: {err}");
            return;
        }
    };
    if new_channel_id == hub {
        let Some(hub_channel_id) = hub else {
            return;
        };
        match runtime
            .create_room_for_hub_join(guild_id, hub_channel_id, user_id, &display_name)
            .await
        {
            Ok(outcome) => println!(
                "[OK] temp_voice created channel {} for user {}",
                outcome.created_channel_id, user_id
            ),
            Err(err) => println!("[WARN] temp_voice failed hub join for user {user_id}: {err}"),
        }
    }
}

async fn runtime_hub_for_guild(
    runtime: &xiii_tempvoice::runtime::TempVoiceRuntime,
    guild_id: u64,
) -> Result<Option<u64>, String> {
    runtime.repository_hub_for_guild(guild_id).await
}

async fn handle_temp_voice_empty_candidate(
    runtime: xiii_tempvoice::runtime::TempVoiceRuntime,
    occupancy: Arc<Mutex<TempVoiceOccupancy>>,
    channel_id: u64,
    delete_after_seconds: u64,
) {
    if delete_after_seconds == 0 {
        match runtime.delete_tracked_channel_if_empty(channel_id, 0).await {
            Ok(outcome) => println!("[OK] temp_voice empty-channel action {outcome:?}"),
            Err(err) => println!("[WARN] temp_voice empty-channel action failed: {err}"),
        }
        return;
    }

    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    if let Err(err) = runtime
        .mark_last_empty_at(channel_id, Some(timestamp))
        .await
    {
        println!("[WARN] temp_voice failed to mark last_empty_at for {channel_id}: {err}");
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(delete_after_seconds)).await;
        let member_count = occupancy.lock().await.member_count(channel_id);
        if member_count == 0 {
            match runtime
                .delete_tracked_channel_if_empty(channel_id, member_count)
                .await
            {
                Ok(outcome) => println!("[OK] temp_voice delayed empty-channel action {outcome:?}"),
                Err(err) => println!("[WARN] temp_voice delayed delete failed: {err}"),
            }
        }
    });
}

fn display_name_from_member(member: &DiscordMember) -> String {
    member
        .nick
        .clone()
        .or_else(|| member.user.global_name.clone())
        .unwrap_or_else(|| member.user.name.clone())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VoiceTransition {
    old_channel_id: Option<u64>,
    old_channel_member_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VoiceCachedMember {
    user_id: u64,
    display_name: String,
    username: Option<String>,
    role_ids: Vec<u64>,
    is_bot: bool,
}

#[derive(Debug, Default)]
struct TempVoiceOccupancy {
    guild_owner_ids: HashMap<u64, u64>,
    user_channels: HashMap<(u64, u64), u64>,
    channel_members: HashMap<u64, BTreeSet<u64>>,
}

impl TempVoiceOccupancy {
    fn seed_guild(
        &mut self,
        guild_id: u64,
        owner_id: u64,
        states: &[twilight_model::voice::VoiceState],
    ) {
        self.guild_owner_ids.insert(guild_id, owner_id);
        for state in states {
            if let Some(channel_id) = state.channel_id.map(|id| id.get()) {
                self.user_channels
                    .insert((guild_id, state.user_id.get()), channel_id);
                self.channel_members
                    .entry(channel_id)
                    .or_default()
                    .insert(state.user_id.get());
            }
        }
    }

    fn guild_owner_id(&self, guild_id: u64) -> Option<u64> {
        self.guild_owner_ids.get(&guild_id).copied()
    }

    fn apply_voice_update(
        &mut self,
        guild_id: u64,
        user_id: u64,
        new_channel_id: Option<u64>,
    ) -> VoiceTransition {
        let key = (guild_id, user_id);
        let old_channel_id = self.user_channels.remove(&key);
        if let Some(old_channel_id) = old_channel_id {
            if let Some(members) = self.channel_members.get_mut(&old_channel_id) {
                members.remove(&user_id);
            }
        }

        if let Some(new_channel_id) = new_channel_id {
            self.user_channels.insert(key, new_channel_id);
            self.channel_members
                .entry(new_channel_id)
                .or_default()
                .insert(user_id);
        }

        VoiceTransition {
            old_channel_id,
            old_channel_member_count: old_channel_id
                .map(|channel_id| self.member_count(channel_id))
                .unwrap_or(0),
        }
    }

    fn member_count(&self, channel_id: u64) -> usize {
        self.channel_members
            .get(&channel_id)
            .map(BTreeSet::len)
            .unwrap_or(0)
    }

    fn channel_counts(&self) -> HashMap<u64, usize> {
        self.channel_members
            .iter()
            .map(|(channel_id, members)| (*channel_id, members.len()))
            .collect()
    }
}

fn write_superbot_runtime_health(
    path: &Path,
    config: &SuperbotConfig,
    module: &str,
    status: &str,
    detail: &str,
) -> Result<(), String> {
    let path = resolve_health_output_path(path, config)?;
    let content = serde_json::json!({
        "module": module,
        "status": status,
        "detail": detail,
        "updated_at_utc": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    });
    fs::write(
        &path,
        serde_json::to_string_pretty(&content)
            .map_err(|err| format!("failed to render health JSON: {err}"))?
            .as_bytes(),
    )
    .map_err(|err| format!("failed to write health output {}: {err}", path.display()))
}

async fn sync_commands(
    env_file: PathBuf,
    allow_discord_write: bool,
    confirm_sync_commands: bool,
    modules: Vec<String>,
    dry_run: bool,
) -> ExitCode {
    if !allow_discord_write || !confirm_sync_commands {
        println!("XIII Superbot Command Sync");
        println!("[FAIL] safety --allow-discord-write and --confirm-sync-commands are required before command registration");
        return ExitCode::from(2);
    }
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let selected = match selected_modules(&modules, &load.config, SelectionMode::EnabledWhenEmpty) {
        Ok(modules) => modules,
        Err(err) => {
            println!("[FAIL] modules {err}");
            return ExitCode::from(2);
        }
    };
    let plan = sync_command_plan(&selected);
    println!("XIII Superbot Command Sync");
    println!(
        "Mode: {}",
        if dry_run {
            "DRY RUN / NO WRITES"
        } else {
            "PLANNED ONLY"
        }
    );
    println!(
        "Guild scoped command target: {:?}",
        load.config.core.command_sync_guild_id
    );
    println!("Command readiness:");
    for row in &plan {
        match row.status {
            SyncPlanStatus::Planned => println!(
                "  - [OK] {} readiness={} planned {}",
                row.module,
                row.readiness,
                row.commands.join(", ")
            ),
            SyncPlanStatus::NoCommands => println!(
                "  - [OK] {} readiness={} no slash commands",
                row.module, row.readiness
            ),
            SyncPlanStatus::Unsafe => println!(
                "  - [WARN] {} readiness={} skipped: {}",
                row.module,
                row.readiness,
                row.reason.unwrap_or("module is not safe for command sync")
            ),
        }
    }
    if dry_run {
        ExitCode::SUCCESS
    } else if plan.iter().any(|row| row.status == SyncPlanStatus::Unsafe) {
        println!("[FAIL] refusing to register commands because at least one requested module is PARTIAL/BLOCKED");
        ExitCode::from(2)
    } else if !plan.iter().any(|row| row.status == SyncPlanStatus::Planned) {
        println!("[FAIL] no selected READY_FULL module exposes slash commands to register");
        ExitCode::from(2)
    } else {
        Box::pin(sync_commands_real(env_file, load.config, selected)).await
    }
}

async fn sync_commands_real(
    env_file: PathBuf,
    config: SuperbotConfig,
    selected: Vec<SuperbotModuleKind>,
) -> ExitCode {
    let Some(application_id) = config.core.discord_client_id else {
        println!("[FAIL] config DISCORD_CLIENT_ID is required to sync guild commands");
        return ExitCode::from(2);
    };
    let guild_id = config
        .core
        .command_sync_guild_id
        .unwrap_or(config.core.guild_id);
    let token = match read_secret_from_env_file(&env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(message) => {
            println!("[FAIL] discord {message}");
            return ExitCode::from(2);
        }
    };
    let client = DiscordHttpClient::new(token);

    let mut failed = false;
    for module in &selected {
        match module {
            SuperbotModuleKind::Clanlist => {
                println!("[OK] clanlist exposes no slash commands to register");
            }
            SuperbotModuleKind::TempVoice => {
                let options = vec![command_option(
                    "channel_id",
                    "The permanent voice channel used to create temporary channels.",
                    CommandOptionType::Channel,
                    true,
                    None,
                    Some(vec![ChannelType::GuildVoice]),
                )];
                if !register_guild_chat_command(
                    &client,
                    application_id,
                    guild_id,
                    "setup-voice-hub",
                    "Set the temporary voice hub channel.",
                    Some(Permissions::ADMINISTRATOR),
                    &options,
                )
                .await
                {
                    failed = true;
                }
            }
            SuperbotModuleKind::Vacation => {
                if !register_guild_chat_command(
                    &client,
                    application_id,
                    guild_id,
                    "vacations",
                    "Show the active vacation panel.",
                    None,
                    &[],
                )
                .await
                {
                    failed = true;
                }
            }
            SuperbotModuleKind::Discipline => {
                let options = vec![
                    command_option(
                        "setup",
                        "Create or refresh the Discipline board.",
                        CommandOptionType::SubCommand,
                        false,
                        Some(Vec::new()),
                        None,
                    ),
                    command_option(
                        "member",
                        "Show DB-backed Discipline status for a member.",
                        CommandOptionType::SubCommand,
                        false,
                        Some(vec![command_option(
                            "user",
                            "Member to inspect.",
                            CommandOptionType::User,
                            true,
                            None,
                            None,
                        )]),
                        None,
                    ),
                    command_option(
                        "health",
                        "Check Discipline DB, board state, and scheduler readiness.",
                        CommandOptionType::SubCommand,
                        false,
                        Some(Vec::new()),
                        None,
                    ),
                ];
                if !register_guild_chat_command(
                    &client,
                    application_id,
                    guild_id,
                    "discipline",
                    "Manage XIII discipline board and member status.",
                    Some(Permissions::MANAGE_GUILD),
                    &options,
                )
                .await
                {
                    failed = true;
                }
            }
            SuperbotModuleKind::Recruit => {
                if !register_guild_chat_command(
                    &client,
                    application_id,
                    guild_id,
                    "recruits",
                    "List active recruits.",
                    None,
                    &[],
                )
                .await
                {
                    failed = true;
                }
                let options = vec![command_option(
                    "user",
                    "Recruit to send a decision panel for.",
                    CommandOptionType::User,
                    true,
                    None,
                    None,
                )];
                if !register_guild_chat_command(
                    &client,
                    application_id,
                    guild_id,
                    "recruit-panel",
                    "Send a recruit decision panel.",
                    Some(Permissions::MANAGE_GUILD),
                    &options,
                )
                .await
                {
                    failed = true;
                }
            }
            SuperbotModuleKind::VoiceActivity => {
                if !register_guild_chat_command(
                    &client,
                    application_id,
                    guild_id,
                    "voice-top",
                    "Show where XIII voice stats are published.",
                    None,
                    &[],
                )
                .await
                {
                    failed = true;
                }
                if !register_guild_chat_command(
                    &client,
                    application_id,
                    guild_id,
                    "inactive-check",
                    "Run the XIII inactivity report.",
                    Some(Permissions::MANAGE_GUILD),
                    &[],
                )
                .await
                {
                    failed = true;
                }
            }
            SuperbotModuleKind::Tickets => {
                let member_options = vec![command_option(
                    "member",
                    "Member to add or remove from the current ticket.",
                    CommandOptionType::User,
                    true,
                    None,
                    None,
                )];
                if !register_guild_chat_command(
                    &client,
                    application_id,
                    guild_id,
                    "add",
                    "Add a member to the current ticket.",
                    Some(Permissions::MANAGE_CHANNELS),
                    &member_options,
                )
                .await
                {
                    failed = true;
                }
                if !register_guild_chat_command(
                    &client,
                    application_id,
                    guild_id,
                    "remove",
                    "Remove a member from the current ticket.",
                    Some(Permissions::MANAGE_CHANNELS),
                    &member_options,
                )
                .await
                {
                    failed = true;
                }
                let custom_options = vec![
                    command_option(
                        "name",
                        "Ticket channel name.",
                        CommandOptionType::String,
                        true,
                        None,
                        None,
                    ),
                    command_option(
                        "user",
                        "Optional ticket opener.",
                        CommandOptionType::User,
                        false,
                        None,
                        None,
                    ),
                    command_option(
                        "reason",
                        "Optional reason to include in the ticket.",
                        CommandOptionType::String,
                        false,
                        None,
                        None,
                    ),
                ];
                if !register_guild_chat_command(
                    &client,
                    application_id,
                    guild_id,
                    "custom-ticket",
                    "Create a custom ticket.",
                    Some(Permissions::MANAGE_CHANNELS),
                    &custom_options,
                )
                .await
                {
                    failed = true;
                }
            }
        }
    }

    if failed {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum SyncPlanStatus {
    Planned,
    NoCommands,
    Unsafe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SyncCommandPlanRow {
    module: &'static str,
    readiness: &'static str,
    status: SyncPlanStatus,
    commands: Vec<&'static str>,
    reason: Option<&'static str>,
}

fn sync_command_plan(modules: &[SuperbotModuleKind]) -> Vec<SyncCommandPlanRow> {
    modules
        .iter()
        .copied()
        .map(sync_command_plan_for_module)
        .collect()
}

fn sync_command_plan_for_module(module: SuperbotModuleKind) -> SyncCommandPlanRow {
    let readiness = module.readiness();
    if readiness != ModuleReadiness::ReadyFull {
        return SyncCommandPlanRow {
            module: module.name(),
            readiness: readiness.as_str(),
            status: SyncPlanStatus::Unsafe,
            commands: Vec::new(),
            reason: Some(sync_unsafe_reason(module)),
        };
    }

    match module {
        SuperbotModuleKind::Clanlist => SyncCommandPlanRow {
            module: module.name(),
            readiness: readiness.as_str(),
            status: SyncPlanStatus::NoCommands,
            commands: Vec::new(),
            reason: Some("clanlist exposes no slash commands"),
        },
        SuperbotModuleKind::TempVoice => SyncCommandPlanRow {
            module: module.name(),
            readiness: readiness.as_str(),
            status: SyncPlanStatus::Planned,
            commands: vec!["/setup-voice-hub"],
            reason: None,
        },
        SuperbotModuleKind::Vacation => SyncCommandPlanRow {
            module: module.name(),
            readiness: readiness.as_str(),
            status: SyncPlanStatus::Planned,
            commands: vec!["/vacations"],
            reason: None,
        },
        SuperbotModuleKind::Discipline => SyncCommandPlanRow {
            module: module.name(),
            readiness: readiness.as_str(),
            status: SyncPlanStatus::Planned,
            commands: vec!["/discipline"],
            reason: None,
        },
        SuperbotModuleKind::Recruit => SyncCommandPlanRow {
            module: module.name(),
            readiness: readiness.as_str(),
            status: SyncPlanStatus::Planned,
            commands: vec!["/recruits", "/recruit-panel"],
            reason: None,
        },
        SuperbotModuleKind::VoiceActivity => SyncCommandPlanRow {
            module: module.name(),
            readiness: readiness.as_str(),
            status: SyncPlanStatus::Planned,
            commands: vec!["/voice-top", "/inactive-check"],
            reason: None,
        },
        SuperbotModuleKind::Tickets => SyncCommandPlanRow {
            module: module.name(),
            readiness: readiness.as_str(),
            status: SyncPlanStatus::Planned,
            commands: vec!["/add", "/remove", "/custom-ticket"],
            reason: Some(
                "legacy text commands are handled at runtime through MESSAGE_CONTENT intent",
            ),
        },
    }
}

fn sync_unsafe_reason(module: SuperbotModuleKind) -> &'static str {
    match module {
        SuperbotModuleKind::VoiceActivity => "voice_activity is not READY_FULL",
        SuperbotModuleKind::Tickets => "tickets is not READY_FULL",
        _ => "module is not READY_FULL",
    }
}

fn command_option(
    name: &str,
    description: &str,
    kind: CommandOptionType,
    required: bool,
    options: Option<Vec<CommandOption>>,
    channel_types: Option<Vec<ChannelType>>,
) -> CommandOption {
    CommandOption {
        autocomplete: None,
        channel_types,
        choices: None,
        description: description.to_owned(),
        description_localizations: None,
        kind,
        max_length: None,
        max_value: None,
        min_length: None,
        min_value: None,
        name: name.to_owned(),
        name_localizations: None,
        options,
        required: if required { Some(true) } else { None },
    }
}

async fn register_guild_chat_command(
    client: &DiscordHttpClient,
    application_id: u64,
    guild_id: u64,
    name: &str,
    description: &str,
    permissions: Option<Permissions>,
    options: &[CommandOption],
) -> bool {
    let interaction = client.interaction(Id::<ApplicationMarker>::new(application_id));
    let mut request = interaction
        .create_guild_command(Id::<GuildMarker>::new(guild_id))
        .chat_input(name, description);
    if let Some(permissions) = permissions {
        request = request.default_member_permissions(permissions);
    }
    if !options.is_empty() {
        request = request.command_options(options);
    }
    match request.await {
        Ok(response) => match response.model().await {
            Ok(command) => {
                println!("[OK] registered guild command {}", command.name);
                true
            }
            Err(err) => {
                println!("[FAIL] failed to decode command sync response for /{name}: {err}");
                false
            }
        },
        Err(err) => {
            println!("[FAIL] failed to register /{name}: {err}");
            false
        }
    }
}

async fn voice_cutover_check(env_file: PathBuf, allow_discord_read: bool) -> ExitCode {
    if !allow_discord_read {
        println!("XIII Voice Activity Cutover Check");
        println!("[FAIL] safety --allow-discord-read is required before comparing DB active sessions with live Discord voice state");
        return ExitCode::from(2);
    }
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let state_dir = superbot_state_dir_from_env(&env_file);
    let cutover_state_path = state_dir.join("voice_activity_cutover_state.json");
    let cutover_state = read_voice_cutover_state(&cutover_state_path)
        .ok()
        .flatten()
        .filter(|state| valid_voice_cutover_state(state, load.config.core.guild_id));
    let active_count = read_voice_active_session_count(&load.config.legacy_paths.voice_db.resolved)
        .await
        .unwrap_or_else(|err| {
            println!("[FAIL] voice_activity failed to read active_voice_sessions: {err}");
            -1
        });
    println!("XIII Voice Activity Cutover Check");
    println!("Mode: READ ONLY / NO WRITES");
    println!(
        "Legacy DB: {}",
        load.config.legacy_paths.voice_db.resolved.display()
    );
    println!(
        "Fresh panel state: {}",
        state_dir.join("voice_activity_panel_state.json").display()
    );
    println!("Cutover state: {}", cutover_state_path.display());
    if active_count >= 0 {
        println!("[OK] active_voice_sessions rows = {active_count}");
    }
    if let Some(state) = cutover_state.as_ref() {
        println!(
            "[OK] voice cutover policy={} cutover_at_utc={} active_sessions_before={}",
            state.policy, state.cutover_at_utc, state.active_sessions_before
        );
    } else {
        println!("[WARN] voice cutover state is not present; active/live sessions must be zero or explicitly finalized before enabling VOICE_ACTIVITY_ENABLED");
    }
    let live_count = match fetch_live_voice_state_count_for_cutover(&env_file, &load.config).await {
        Ok(count) => {
            println!("[OK] live Discord voice states fetched = {count}");
            count
        }
        Err(err) => {
            println!("[FAIL] Discord read-only live voice-state fetch failed: {err}");
            -1
        }
    };
    println!("[OK] no database mutations were performed");
    if active_count > 0 {
        println!(
            "[FAIL] active DB voice sessions still exist; run `cargo run -- voice-finalize-cutover --env-file {} --allow-legacy-db-write --confirm-close-active-voice-sessions` during cutover to intentionally close them once",
            env_file.display()
        );
        ExitCode::from(2)
    } else if live_count > 0 && cutover_state.is_none() {
        println!("[FAIL] live Discord voice sessions exist and no finalized cutover state was found; either wait for voice to empty or intentionally split sessions with `voice-finalize-cutover`");
        ExitCode::from(2)
    } else if live_count > 0 {
        println!("[OK] live sessions exist but policy=closed_active_at_cutover is recorded; Superbot startup will open fresh active sessions after cutover");
        ExitCode::SUCCESS
    } else if active_count < 0 || live_count < 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

async fn voice_finalize_cutover(
    env_file: PathBuf,
    allow_legacy_db_write: bool,
    confirm_close_active_voice_sessions: bool,
    dry_run: bool,
) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let state_dir = superbot_state_dir_from_env(&env_file);
    let state_path = state_dir.join("voice_activity_cutover_state.json");
    let cutover_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    println!("XIII Voice Activity Finalize Cutover");
    println!(
        "Mode: {}",
        if dry_run {
            "DRY RUN / NO WRITES"
        } else {
            "GATED LEGACY DB WRITE"
        }
    );
    println!("Discord login: DISABLED");
    println!("Discord writes: DISABLED");
    println!("Google calls: DISABLED");
    println!(
        "Legacy DB: {}",
        load.config.legacy_paths.voice_db.resolved.display()
    );
    println!("Cutover state: {}", state_path.display());
    println!("Cutover timestamp: {cutover_at}");

    if !dry_run && (!allow_legacy_db_write || !confirm_close_active_voice_sessions) {
        println!("[FAIL] safety --allow-legacy-db-write and --confirm-close-active-voice-sessions are required before closing active voice sessions");
        return ExitCode::from(2);
    }

    if !dry_run {
        if let Ok(Some(state)) = read_voice_cutover_state(&state_path) {
            let active_count =
                match read_voice_active_session_count(&load.config.legacy_paths.voice_db.resolved)
                    .await
                {
                    Ok(count) => count,
                    Err(err) => {
                        println!("[FAIL] voice_activity active session check failed: {err}");
                        return ExitCode::from(2);
                    }
                };
            if valid_voice_cutover_state(&state, load.config.core.guild_id) && active_count == 0 {
                println!(
                    "[OK] cutover state already exists and active_voice_sessions rows = 0; no-op"
                );
                return ExitCode::SUCCESS;
            }
            if valid_voice_cutover_state(&state, load.config.core.guild_id) && active_count > 0 {
                println!("[FAIL] cutover state already exists but active_voice_sessions rows = {active_count}; refusing ambiguous second finalize");
                return ExitCode::from(2);
            }
        }
    }

    let repo = if dry_run {
        match xiii_voice_activity::repository::LegacySqliteVoiceActivityRepository::open_existing_read_only(
            &load.config.legacy_paths.voice_db.resolved,
        )
        .await
        {
            Ok(repo) => repo,
            Err(err) => {
                println!("[FAIL] voice_activity repository {err}");
                return ExitCode::from(2);
            }
        }
    } else {
        match xiii_voice_activity::repository::LegacySqliteVoiceActivityRepository::open_existing_writable(
            &load.config.legacy_paths.voice_db.resolved,
        )
        .await
        {
            Ok(repo) => repo,
            Err(err) => {
                println!("[FAIL] voice_activity repository {err}");
                return ExitCode::from(2);
            }
        }
    };

    let active = match repo.list_active_sessions(load.config.core.guild_id).await {
        Ok(active) => active,
        Err(err) => {
            println!("[FAIL] failed to read active voice sessions: {err}");
            return ExitCode::from(2);
        }
    };
    println!("[OK] active_voice_sessions rows = {}", active.len());
    for session in &active {
        let duration = duration_between_iso_clamped(&session.started_at, &cutover_at);
        println!(
            "[OK] would_close user_id={} channel_id={} started_at={} duration_seconds={}",
            session.user_id, session.channel_id, session.started_at, duration
        );
    }

    if dry_run {
        println!(
            "[OK] dry-run only; no legacy DB rows were changed and no cutover state was written"
        );
        return ExitCode::SUCCESS;
    }

    let result = match repo
        .close_all_active_sessions_at_cutover(load.config.core.guild_id, &cutover_at)
        .await
    {
        Ok(result) => result,
        Err(err) => {
            println!("[FAIL] failed to close active voice sessions: {err}");
            return ExitCode::from(2);
        }
    };
    let state = xiii_voice_activity::state::VoiceActivityCutoverState {
        source: "voice-finalize-cutover".to_owned(),
        policy: "closed_active_at_cutover".to_owned(),
        guild_id: load.config.core.guild_id,
        cutover_at_utc: result.cutover_at_utc.clone(),
        active_sessions_before: result.active_sessions_before,
        closed_sessions: result.closed_sessions.clone(),
        note: "Historical completed voice stats are preserved; active rows were intentionally closed once at cutover.".to_owned(),
    };
    if let Err(err) = write_voice_cutover_state(&state_path, &state) {
        println!("[FAIL] failed to write cutover state: {err}");
        return ExitCode::from(2);
    }
    println!(
        "[OK] closed active voice sessions = {}",
        result.closed_sessions.len()
    );
    println!("[OK] cutover state written: {}", state_path.display());
    println!("[OK] completed historical voice stats were preserved");
    ExitCode::SUCCESS
}

async fn final_readiness_check(env_file: PathBuf, allow_discord_read: bool) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let state_dir = superbot_state_dir_from_env(&env_file);
    let mut report = Report::new();

    println!("XIII Superbot Final Readiness Check");
    println!("Mode: READ ONLY / NO WRITES");
    println!(
        "Discord reads: {}",
        if allow_discord_read {
            "ENABLED"
        } else {
            "DISABLED"
        }
    );
    println!("Discord writes: DISABLED");
    println!("Legacy DB writes: DISABLED");
    println!("Google calls: DISABLED");
    println!("State dir: {}", state_dir.display());
    println!();
    print_readiness_matrix(&load.config);

    for module in SuperbotModuleKind::all() {
        if module.readiness() == ModuleReadiness::ReadyFull {
            report.ok(module.name(), "readiness=READY_FULL");
        } else {
            report.fail(
                module.name(),
                format!(
                    "readiness={} blocks deployment",
                    module.readiness().as_str()
                ),
            );
        }
    }

    let state_expectations = [
        ("clanlist", "clanlist_panel_state.json", 3usize),
        ("vacation", "vacation_panel_state.json", 2usize),
        ("discipline", "discipline_panel_state.json", 1usize),
        ("voice_activity", "voice_activity_panel_state.json", 1usize),
        ("tickets", "ticket_panel_state.json", 1usize),
    ];
    for (scope, file, min_targets) in state_expectations {
        let path = state_dir.join(file);
        match validate_fresh_state_json(&path, load.config.core.guild_id, min_targets) {
            Ok(targets) => report.ok(
                scope,
                format!(
                    "fresh state {} guild_id matches; message targets={}",
                    path.display(),
                    targets.len()
                ),
            ),
            Err(err) => report.fail(scope, err),
        }
    }

    add_db_source_report(
        &mut report,
        "clanlist",
        &load.config.legacy_paths.clanlist_data_dir.resolved,
        &[],
    )
    .await;
    add_db_source_report(
        &mut report,
        "temp_voice",
        &load.config.legacy_paths.temp_voice_db.resolved,
        &[("guild_settings", true), ("temp_voice_channels", true)],
    )
    .await;
    add_db_source_report(
        &mut report,
        "vacation",
        &load.config.legacy_paths.vacation_db.resolved,
        &[("vacation_requests", true), ("vacations", true)],
    )
    .await;
    add_db_source_report(
        &mut report,
        "discipline",
        &load.config.legacy_paths.discipline_db.resolved,
        &[
            ("settings", true),
            ("punishments", true),
            ("action_logs", true),
            ("action_locks", false),
        ],
    )
    .await;
    add_db_source_report(
        &mut report,
        "recruit",
        &load.config.legacy_paths.recruit_db.resolved,
        &[
            ("recruits", true),
            ("voice_sessions", true),
            ("decisions", true),
        ],
    )
    .await;
    add_db_source_report(
        &mut report,
        "voice_activity",
        &load.config.legacy_paths.voice_db.resolved,
        &[
            ("users", true),
            ("voice_sessions", true),
            ("active_voice_sessions", true),
            ("bot_state", true),
        ],
    )
    .await;
    add_db_source_report(
        &mut report,
        "tickets",
        &load.config.legacy_paths.ticket_db.resolved,
        &[
            ("counters", true),
            ("tickets", true),
            ("processed_forms", true),
            ("processed_form_signatures", true),
            ("bot_state", true),
        ],
    )
    .await;

    add_ticket_final_readiness(&mut report, &load.config).await;
    add_voice_final_readiness(&mut report, &load.config, &state_dir, &env_file).await;

    report.warn(
        "tickets",
        "operator check required: MESSAGE_CONTENT intent must be enabled before ticket cutover for !panel, !accept/!принять, and !reject/!отклонить",
    );

    if allow_discord_read {
        add_discord_panel_ownership_checks(&mut report, &env_file, &load.config, &state_dir).await;
    } else {
        report.warn(
            "discord",
            "fresh panel message existence/ownership was not checked; pass --allow-discord-read for optional HTTP GET verification",
        );
    }

    print_report("Final Readiness", &report);
    if report.has_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

async fn production_preflight(env_file: PathBuf, allow_discord_read: bool) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let state_dir = superbot_state_dir_from_env(&env_file);
    let mut report = Report::new();

    println!("XIII Superbot Production Preflight");
    println!("Mode: READ ONLY / NO WRITES");
    println!("Env file: {}", env_file.display());
    println!(
        "Discord reads: {}",
        if allow_discord_read {
            "ENABLED"
        } else {
            "DISABLED"
        }
    );
    println!("Discord writes: DISABLED");
    println!("Legacy DB writes: DISABLED");
    println!("Google calls: DISABLED");
    println!();

    match env_file.file_name().and_then(|name| name.to_str()) {
        Some(".env.production") => report.ok("config", "env file name looks production-shaped"),
        Some(".env.example") => report.warn(
            "config",
            "env file is the tracked example template; replace placeholders and real VPS paths before deployment",
        ),
        Some(name) => report.warn(
            "config",
            format!("env file name is {name}; expected a private production file such as .env.production"),
        ),
        None => report.warn(
            "config",
            "env file name could not be derived; verify this is the intended private production env",
        ),
    }

    if superbot_require_old_services_stopped_from_env(&env_file) {
        report.ok(
            "service_guard",
            "SUPERBOT_REQUIRE_OLD_SERVICES_STOPPED=true",
        );
    } else {
        report.fail(
            "service_guard",
            "SUPERBOT_REQUIRE_OLD_SERVICES_STOPPED must stay true for production",
        );
    }

    if read_env_value(&env_file, "DISCORD_SYNC_COMMANDS_ON_STARTUP")
        .map(|value| parse_truthy_bool(&value))
        .unwrap_or(false)
    {
        report.fail(
            "discord",
            "DISCORD_SYNC_COMMANDS_ON_STARTUP must stay false; production command sync should remain explicit",
        );
    } else {
        report.ok(
            "discord",
            "DISCORD_SYNC_COMMANDS_ON_STARTUP=false (explicit sync only)",
        );
    }

    add_production_path_report(&mut report, "state", "SUPERBOT_STATE_DIR", &state_dir);
    if let Some(health_output) =
        read_env_value(&env_file, "SUPERBOT_HEALTH_OUTPUT").filter(|value| !value.trim().is_empty())
    {
        add_production_path_report(
            &mut report,
            "state",
            "SUPERBOT_HEALTH_OUTPUT",
            Path::new(&health_output),
        );
    }

    let service_status_dir = state_dir.join("service-status");
    add_production_path_report(
        &mut report,
        "service_guard",
        "service-status directory",
        &service_status_dir,
    );
    let missing_service_files = SuperbotModuleKind::all()
        .into_iter()
        .filter_map(|module| {
            let path = service_status_dir.join(format!("{}.txt", module.spec().service_name));
            (!path.is_file()).then_some(path)
        })
        .collect::<Vec<_>>();
    if missing_service_files.is_empty() {
        report.ok(
            "service_guard",
            format!(
                "service-status directory contains all {} expected old-service snapshots",
                SuperbotModuleKind::all().len()
            ),
        );
    } else {
        report.warn(
            "service_guard",
            format!(
                "capture old-service status files before final enablement; missing {} entries under {}",
                missing_service_files.len(),
                service_status_dir.display()
            ),
        );
    }

    for module in SuperbotModuleKind::all() {
        if module.readiness() == ModuleReadiness::ReadyFull {
            report.ok(module.name(), "readiness=READY_FULL");
        } else {
            report.fail(
                module.name(),
                format!(
                    "readiness={} blocks production enablement",
                    module.readiness().as_str()
                ),
            );
        }
    }

    add_production_path_report(
        &mut report,
        "clanlist",
        "LEGACY_CLANLIST_DATA_DIR",
        &load.config.legacy_paths.clanlist_data_dir.resolved,
    );
    add_production_path_report(
        &mut report,
        "temp_voice",
        "LEGACY_TEMP_VOICE_DB_PATH",
        &load.config.legacy_paths.temp_voice_db.resolved,
    );
    add_production_path_report(
        &mut report,
        "vacation",
        "LEGACY_VACATION_DB_PATH",
        &load.config.legacy_paths.vacation_db.resolved,
    );
    add_production_path_report(
        &mut report,
        "discipline",
        "LEGACY_DISCIPLINE_DB_PATH",
        &load.config.legacy_paths.discipline_db.resolved,
    );
    add_production_path_report(
        &mut report,
        "recruit",
        "LEGACY_RECRUIT_DB_PATH",
        &load.config.legacy_paths.recruit_db.resolved,
    );
    add_production_path_report(
        &mut report,
        "voice_activity",
        "LEGACY_VOICE_DB_PATH",
        &load.config.legacy_paths.voice_db.resolved,
    );
    add_production_path_report(
        &mut report,
        "tickets",
        "LEGACY_TICKET_DB_PATH",
        &load.config.legacy_paths.ticket_db.resolved,
    );

    let state_expectations = [
        ("clanlist", "clanlist_panel_state.json", 3usize),
        ("vacation", "vacation_panel_state.json", 2usize),
        ("discipline", "discipline_panel_state.json", 1usize),
        ("voice_activity", "voice_activity_panel_state.json", 1usize),
        ("tickets", "ticket_panel_state.json", 1usize),
    ];
    for (scope, file, min_targets) in state_expectations {
        let path = state_dir.join(file);
        match validate_fresh_state_json(&path, load.config.core.guild_id, min_targets) {
            Ok(targets) => report.ok(
                scope,
                format!(
                    "fresh state {} guild_id matches; message targets={}",
                    path.display(),
                    targets.len()
                ),
            ),
            Err(err) => report.fail(scope, err),
        }
    }

    add_db_source_report(
        &mut report,
        "clanlist",
        &load.config.legacy_paths.clanlist_data_dir.resolved,
        &[],
    )
    .await;
    add_db_source_report(
        &mut report,
        "temp_voice",
        &load.config.legacy_paths.temp_voice_db.resolved,
        &[("guild_settings", true), ("temp_voice_channels", true)],
    )
    .await;
    add_db_source_report(
        &mut report,
        "vacation",
        &load.config.legacy_paths.vacation_db.resolved,
        &[("vacation_requests", true), ("vacations", true)],
    )
    .await;
    add_db_source_report(
        &mut report,
        "discipline",
        &load.config.legacy_paths.discipline_db.resolved,
        &[
            ("settings", true),
            ("punishments", true),
            ("action_logs", true),
            ("action_locks", false),
        ],
    )
    .await;
    add_db_source_report(
        &mut report,
        "recruit",
        &load.config.legacy_paths.recruit_db.resolved,
        &[
            ("recruits", true),
            ("voice_sessions", true),
            ("decisions", true),
        ],
    )
    .await;
    add_db_source_report(
        &mut report,
        "voice_activity",
        &load.config.legacy_paths.voice_db.resolved,
        &[
            ("users", true),
            ("voice_sessions", true),
            ("active_voice_sessions", true),
            ("bot_state", true),
        ],
    )
    .await;
    add_db_source_report(
        &mut report,
        "tickets",
        &load.config.legacy_paths.ticket_db.resolved,
        &[
            ("counters", true),
            ("tickets", true),
            ("processed_forms", true),
            ("processed_form_signatures", true),
            ("bot_state", true),
        ],
    )
    .await;

    add_ticket_final_readiness(&mut report, &load.config).await;
    add_voice_final_readiness(&mut report, &load.config, &state_dir, &env_file).await;

    report.warn(
        "tickets",
        "operator check required: MESSAGE_CONTENT intent must be enabled before ticket cutover for !panel, !accept/!принять, and !reject/!отклонить",
    );

    if allow_discord_read {
        add_discord_panel_ownership_checks(&mut report, &env_file, &load.config, &state_dir).await;
    } else {
        report.warn(
            "discord",
            "fresh panel message existence/ownership was not checked; pass --allow-discord-read for optional HTTP GET verification",
        );
    }

    print_report("Production Preflight", &report);
    if report.has_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

async fn db_source_check(env_file: PathBuf) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let mut report = Report::new();
    println!("XIII Superbot DB Source Check");
    println!("Mode: READ ONLY / NO WRITES");
    println!("Discord login: DISABLED");
    println!("Google calls: DISABLED");
    println!("Legacy DB writes: DISABLED");
    println!("Legacy DB/state paths are the source of truth; no new empty DBs are accepted.");
    println!();

    add_db_source_report(
        &mut report,
        "clanlist",
        &load.config.legacy_paths.clanlist_data_dir.resolved,
        &[],
    )
    .await;
    add_db_source_report(
        &mut report,
        "temp_voice",
        &load.config.legacy_paths.temp_voice_db.resolved,
        &[("guild_settings", true), ("temp_voice_channels", true)],
    )
    .await;
    add_db_source_report(
        &mut report,
        "vacation",
        &load.config.legacy_paths.vacation_db.resolved,
        &[("vacation_requests", true), ("vacations", true)],
    )
    .await;
    add_db_source_report(
        &mut report,
        "discipline",
        &load.config.legacy_paths.discipline_db.resolved,
        &[
            ("settings", true),
            ("punishments", true),
            ("action_logs", true),
            ("action_locks", false),
            ("schema_migrations", false),
        ],
    )
    .await;
    add_db_source_report(
        &mut report,
        "recruit",
        &load.config.legacy_paths.recruit_db.resolved,
        &[
            ("recruits", true),
            ("voice_sessions", true),
            ("decisions", true),
        ],
    )
    .await;
    add_db_source_report(
        &mut report,
        "voice_activity",
        &load.config.legacy_paths.voice_db.resolved,
        &[
            ("users", true),
            ("voice_sessions", true),
            ("active_voice_sessions", true),
            ("bot_state", true),
        ],
    )
    .await;
    add_db_source_report(
        &mut report,
        "tickets",
        &load.config.legacy_paths.ticket_db.resolved,
        &[
            ("counters", true),
            ("tickets", true),
            ("processed_forms", true),
            ("processed_form_signatures", true),
            ("bot_state", true),
        ],
    )
    .await;

    print_report("DB Source Check", &report);
    if report.has_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn legacy_parity_audit(env_file: PathBuf) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let modules = legacy_parity_modules();
    let mut report = Report::new();
    for module in &modules {
        match module.status {
            LegacyParityStatus::Exact => report.ok(
                module.module,
                "known render metadata matches the audited legacy source",
            ),
            LegacyParityStatus::AcceptedDifference => report.warn(
                module.module,
                "known render metadata differs intentionally; audit output lists the accepted difference",
            ),
            LegacyParityStatus::Partial => report.warn(
                module.module,
                "known render metadata is partial; audit output lists the mismatches",
            ),
        }
        for finding in &module.findings {
            match finding.status {
                LegacyParityStatus::Exact => report.ok(
                    module.module,
                    format!("{}: {}", finding.category, finding.note),
                ),
                LegacyParityStatus::AcceptedDifference | LegacyParityStatus::Partial => report
                    .warn(
                        module.module,
                        format!(
                            "{}: {} legacy={} superbot={}",
                            finding.category, finding.note, finding.legacy, finding.superbot
                        ),
                    ),
            }
        }
    }

    println!("XIII Superbot Legacy Parity Audit");
    println!("Mode: READ ONLY / NO WRITES");
    println!("Discord writes: DISABLED");
    println!("Legacy DB writes: DISABLED");
    println!("Google calls: DISABLED");
    println!("Legacy source root: ../XIII_BOTS_FULL_COPY");
    println!();
    for module in &modules {
        println!("{} parity={}", module.module, module.status.as_str());
        println!("  legacy_source: {}", module.legacy_source);
        let legacy_source_exists = module
            .legacy_source
            .split(" ; ")
            .filter(|entry| !entry.trim().is_empty())
            .all(|entry| Path::new(entry.trim()).exists());
        println!("  legacy_source_exists: {}", legacy_source_exists);
        for finding in &module.findings {
            println!(
                "  - [{}] {}: {}",
                finding.status.as_str(),
                finding.category,
                finding.note
            );
            println!("    legacy: {}", finding.legacy);
            println!("    superbot: {}", finding.superbot);
        }
        println!();
    }
    print_report("Legacy Parity", &report);
    let _ = load;
    ExitCode::SUCCESS
}

fn render_preview(
    env_file: PathBuf,
    modules: Vec<String>,
    format: PreviewFormat,
    output: Option<PathBuf>,
) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let selected =
        match selected_modules_allow_all(&modules, &load.config, SelectionMode::AllWhenEmpty) {
            Ok(modules) => modules,
            Err(err) => {
                println!("[FAIL] modules {err}");
                return ExitCode::from(2);
            }
        };
    let previews = selected
        .into_iter()
        .map(module_render_preview)
        .collect::<Vec<_>>();
    let content = match format {
        PreviewFormat::Text => render_preview_text(&previews),
        PreviewFormat::Json => render_preview_json(&previews),
    };
    if let Err(err) = emit_output(&content, output.as_deref(), Some(&load.config)) {
        println!("[FAIL] output {err}");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum LegacyParityStatus {
    Exact,
    AcceptedDifference,
    Partial,
}

impl LegacyParityStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "EXACT",
            Self::AcceptedDifference => "ACCEPTED_DIFFERENCE",
            Self::Partial => "PARTIAL",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct LegacyParityFinding {
    category: &'static str,
    status: LegacyParityStatus,
    legacy: &'static str,
    superbot: &'static str,
    note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct LegacyParityModule {
    module: &'static str,
    status: LegacyParityStatus,
    legacy_source: &'static str,
    findings: Vec<LegacyParityFinding>,
}

#[derive(Debug, Clone, Serialize)]
struct RenderPreviewModule {
    module: &'static str,
    parity_status: LegacyParityStatus,
    embeds: Vec<RenderPreviewEmbed>,
    buttons: Vec<RenderPreviewButton>,
    modals: Vec<RenderPreviewModal>,
    commands: Vec<&'static str>,
    text_responses: Vec<&'static str>,
    allowed_mentions: Vec<&'static str>,
    warnings: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
struct RenderPreviewEmbed {
    surface: &'static str,
    title: &'static str,
    description: &'static str,
    fields: Vec<&'static str>,
    color: &'static str,
    footer: &'static str,
    timestamp_behavior: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct RenderPreviewButton {
    surface: &'static str,
    custom_id: &'static str,
    label: &'static str,
    style: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct RenderPreviewModal {
    surface: &'static str,
    custom_id: &'static str,
    title: &'static str,
    fields: Vec<&'static str>,
    placeholders: Vec<&'static str>,
}

fn selected_modules_allow_all(
    requested: &[String],
    config: &SuperbotConfig,
    mode: SelectionMode,
) -> Result<Vec<SuperbotModuleKind>, String> {
    if requested
        .iter()
        .any(|item| item.eq_ignore_ascii_case("all"))
    {
        return Ok(SuperbotModuleKind::all());
    }
    selected_modules(requested, config, mode)
}

fn legacy_parity_modules() -> Vec<LegacyParityModule> {
    legacy_parity_modules_curated()
}

fn legacy_parity_modules_curated() -> Vec<LegacyParityModule> {
    vec![
        LegacyParityModule {
            module: "clanlist",
            status: LegacyParityStatus::AcceptedDifference,
            legacy_source: "../XIII_BOTS_FULL_COPY/opt/XIII/XIII-clanlist",
            findings: vec![
                exact(
                    "embed color",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/XIII-clanlist legacy builder EMBED_COLOR=#0066FF",
                    "#0066FF",
                    "Clanlist embed stripe color matches the old builder.",
                ),
                exact(
                    "footer",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/XIII-clanlist legacy footer 'Обновлено: {Europe/Berlin local timestamp}'",
                    "Обновлено: {timestamp}",
                    "Footer wording matches; timestamp is rendered at runtime.",
                ),
                exact(
                    "titles",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/XIII-clanlist legacy titles for main/admin/Steam panels",
                    "Список участников XIII / Административный состав XIII / Список Steam ID XIII",
                    "Panel titles match legacy constants.",
                ),
                close(
                    "live layout verification",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/XIII-clanlist: header embed plus role chunk embeds with marker author URL",
                    "crates/xiii-clanlist/src/lib.rs plus clanlist-render-preview and clanlist-target-message-check",
                    "Accepted difference: parity-audit is read-only metadata; exact live message chunking is verified with clanlist-render-preview and target-message checks.",
                ),
            ],
        },
        LegacyParityModule {
            module: "temp_voice",
            status: LegacyParityStatus::AcceptedDifference,
            legacy_source: "../XIII_BOTS_FULL_COPY/opt/XIII/temp-voice-bot",
            findings: vec![
                exact(
                    "slash command",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/temp-voice-bot: /setup-voice-hub channel_id",
                    "/setup-voice-hub channel_id",
                    "Command name and required option are preserved.",
                ),
                exact(
                    "runtime behavior",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/temp-voice-bot: create room on hub join, move member, delete tracked empty temp rooms",
                    "same DB-owned channel guard and Gateway route",
                    "Temp voice has no persistent panel surface.",
                ),
                close(
                    "response wording",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/temp-voice-bot command responses",
                    "src/app.rs temp voice interaction responses",
                    "Accepted difference: no persistent user-facing panel; short Superbot responses intentionally stay concise while preserving behavior.",
                ),
            ],
        },
        LegacyParityModule {
            module: "vacation",
            status: LegacyParityStatus::Exact,
            legacy_source: "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-vacation-bot/internal/bot/embeds.go ; ../XIII_BOTS_FULL_COPY/opt/XIII/xiii-vacation-bot/internal/bot/interactions.go",
            findings: vec![
                exact(
                    "request panel",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-vacation-bot/internal/bot/embeds.go::panelEmbed + panelComponents",
                    "crates/xiii-vacation/src/render.rs::REQUEST_PANEL_* and src/app.rs::bootstrap_vacation_panels",
                    "Request panel title, body, color, footer, and button label match legacy.",
                ),
                exact(
                    "request modal",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-vacation-bot/internal/bot/interactions.go modal submit flow",
                    "crates/xiii-vacation/src/render.rs::REQUEST_MODAL_* and src/app.rs::handle_vacation_component",
                    "Modal title, labels, and placeholders match legacy.",
                ),
                exact(
                    "officer review",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-vacation-bot/internal/bot/embeds.go::officerRequestEmbed + officerRequestComponents",
                    "src/app.rs::vacation_officer_review_embed, send_vacation_officer_review, update_vacation_officer_review",
                    "Officer review title, color, footer, timestamp, field set, and buttons match legacy.",
                ),
                exact(
                    "active vacations panel",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-vacation-bot/internal/bot/embeds.go::activeVacationsEmbed",
                    "crates/xiii-vacation/src/render.rs::active_panel_description and src/app.rs::vacation_active_panel_refresh_tick",
                    "Active rows include member mention, start date, end date, relative end, reason, and truncation text like legacy.",
                ),
                exact(
                    "DM embeds and early-end prompts",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-vacation-bot/internal/bot/embeds.go::approvalDMEmbed, rejectionDMEmbed, activeVacationsEmbed ; interactions.go early-end flow",
                    "src/app.rs::vacation_approved_dm_embed, vacation_rejected_dm_embed, vacation_expired_dm_embed and crates/xiii-vacation/src/render.rs::*_RESPONSE",
                    "Approval/rejection/expiry DM embeds and early-end confirmation wording/buttons match legacy.",
                ),
            ],
        },
        LegacyParityModule {
            module: "discipline",
            status: LegacyParityStatus::Exact,
            legacy_source: "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-discipline-bot/src/services/boardService.ts ; ../XIII_BOTS_FULL_COPY/opt/XIII/xiii-discipline-bot/src/interactions/panel.ts ; ../XIII_BOTS_FULL_COPY/opt/XIII/xiii-discipline-bot/src/interactions/historyFlow.ts",
            findings: vec![
                exact(
                    "board surface",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-discipline-bot/src/services/boardService.ts",
                    "crates/xiii-discipline/src/render.rs and src/app.rs::discipline_board_refresh_tick",
                    "Board title, summary, color, empty state, footer format, and pagination labels now match the legacy board surface closely.",
                ),
                exact(
                    "buttons, modal copy, and history wording",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-discipline-bot/src/interactions/panel.ts and src/interactions/historyFlow.ts",
                    "crates/xiii-discipline/src/render.rs and src/app.rs::discipline_history_embeds",
                    "Issue/remove/history labels, modal copy, history empty state, oversize note, and navigation labels match legacy wording.",
                ),
                exact(
                    "interaction entry flow",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-discipline-bot/src/interactions/panel.ts and src/interactions/historyFlow.ts use picker-first and select-first member selection flows",
                    "src/app.rs::handle_discipline_component and handle_discipline_modal now mirror picker/select-first flows for issue/remove/history",
                    "Issue/remove/history now start with picker or select controls before modal submission, matching legacy visible interaction flow.",
                ),
            ],
        },
        LegacyParityModule {
            module: "recruit",
            status: LegacyParityStatus::Exact,
            legacy_source: "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-recruit-bot/app/services/embed_service.py ; ../XIII_BOTS_FULL_COPY/opt/XIII/xiii-recruit-bot/app/discord_ui/views.py ; ../XIII_BOTS_FULL_COPY/opt/XIII/xiii-recruit-bot/app/discord_ui/modals.py ; ../XIII_BOTS_FULL_COPY/opt/XIII/xiii-recruit-bot/app/services/decision_service.py",
            findings: vec![
                exact(
                    "decision panel title/buttons",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-recruit-bot/app/services/embed_service.py and app/discord_ui/views.py",
                    "crates/xiii-recruit/src/render.rs::DECISION_PANEL_TITLE and button labels",
                    "Decision title and button labels/styles now match legacy.",
                ),
                exact(
                    "modals/responses",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-recruit-bot/app/discord_ui/modals.py and app/services/decision_service.py",
                    "crates/xiii-recruit/src/render.rs modal constants and success copy",
                    "Modal titles/labels and success response constants are now ported.",
                ),
                exact(
                    "decision embed fields",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-recruit-bot/app/services/embed_service.py::build_decision_embed",
                    "crates/xiii-recruit/src/render.rs::decision_panel_embed / processed_decision_embed and src/app.rs recruit decision handlers",
                    "Decision embeds now include the legacy summary description, deadlines, voice duration, extension count, decision block, optional extension days, reason, warnings, and recruit-id footer.",
                ),
                exact(
                    "DM body detail",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-recruit-bot/app/services/embed_service.py accepted/rejected/extended DM embeds",
                    "crates/xiii-recruit/src/render.rs::accepted_dm_embed / rejected_dm_embed / extended_dm_embed plus crates/xiii-recruit/src/discord_io.rs::decision_dm_embed",
                    "Accepted/rejected/extended DM embeds now carry the legacy title/body/field/footer copy rather than simplified placeholder text.",
                ),
            ],
        },
        LegacyParityModule {
            module: "voice_activity",
            status: LegacyParityStatus::Exact,
            legacy_source: "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-voice-activity-bot/app/views/public_stats_view.py ; ../XIII_BOTS_FULL_COPY/opt/XIII/xiii-voice-activity-bot/app/views/inactive_view.py ; ../XIII_BOTS_FULL_COPY/opt/XIII/xiii-voice-activity-bot/app/cogs/public_stats.py",
            findings: vec![
                exact(
                    "visible text surfaces",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-voice-activity-bot/app/views/public_stats_view.py and inactive_view.py",
                    "crates/xiii-voice-activity/src/render.rs",
                    "Footer, refresh notice, period labels, empty states, row formatting, and Russian wording now match the legacy render model closely.",
                ),
                exact(
                    "voice-top disabled text",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-voice-activity-bot/app/cogs/public_stats.py::_disabled_voice_top_message",
                    "crates/xiii-voice-activity/src/render.rs::voice_top_disabled_response",
                    "Disabled /voice-top response now matches legacy wording.",
                ),
                exact(
                    "interactive views",
                    "../XIII_BOTS_FULL_COPY/opt/XIII/xiii-voice-activity-bot/app/views/public_stats_view.py / inactive_view.py and app/cogs/inactivity.py",
                    "src/app.rs::handle_voice_activity_component / handle_voice_activity_inactive_command plus public/inactive component builders",
                    "Public stats and inactive-check now answer with legacy-style embed/view updates, disable previous/next at boundaries, preserve period/page state, and use the period-labelled inactivity title for automatic reports.",
                ),
            ],
        },
        LegacyParityModule {
            module: "tickets",
            status: LegacyParityStatus::AcceptedDifference,
            legacy_source: "../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot/app/services/ticket_service.py ; ../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot/app/discord_app/views/close.py ; ../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot/app/services/transcript_service.py",
            findings: vec![
                exact(
                    "panel title/body/buttons/color",
                    "../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot/app/config/constants.py and app/discord_app/views/panel.py",
                    "crates/xiii-tickets/src/render.rs::LEGACY_PANEL_* and ticket_panel_button_specs()",
                    "Ticket panel title, description, color, labels, and styles match legacy constants.",
                ),
                close(
                    "transcript",
                    "../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot/app/services/transcript_service.py uses Python chat_exporter",
                    "crates/xiii-tickets/src/render.rs::transcript_html",
                    "Accepted difference: the Superbot keeps a safe Rust HTML transcript substitute instead of the Python chat_exporter markup, while preserving author/timestamp/content/attachment reviewability without pings.",
                ),
                exact(
                    "ticket opening messages",
                    "../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot/app/services/ticket_service.py application_form_message / promotion_request_message / complaint embed",
                    "src/app.rs ticket creation path plus crates/xiii-tickets/src/discord_io.rs::ticket_open_payload",
                    "Application, complaint, promotion, and custom ticket opening copy now follows the legacy per-ticket-type bodies instead of a unified placeholder payload.",
                ),
                exact(
                    "officer review copy",
                    "../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot officer review application flow",
                    "crates/xiii-tickets/src/render.rs::officer_review_description and src/app.rs Google review sender",
                    "Officer review wording now follows the legacy application review checklist and result text.",
                ),
                exact(
                    "close summary / reopen lifecycle surface",
                    "../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot/app/discord_app/views/close.py ; app/discord_app/views/after_close.py ; app/services/transcript_service.py",
                    "crates/xiii-tickets/src/render.rs ; crates/xiii-tickets/src/discord_io.rs ; src/app.rs::handle_ticket_component / ticket_close_current_channel / ticket_reopen_current_channel",
                    "The Superbot now uses the legacy public close confirmation embed, close-result summary embed with after-close reopen/delete view, transcript summary embed delivery, and DM reopen surface.",
                ),
            ],
        },
    ]
}

fn exact(
    category: &'static str,
    legacy: &'static str,
    superbot: &'static str,
    note: &'static str,
) -> LegacyParityFinding {
    LegacyParityFinding {
        category,
        status: LegacyParityStatus::Exact,
        legacy,
        superbot,
        note,
    }
}

fn close(
    category: &'static str,
    legacy: &'static str,
    superbot: &'static str,
    note: &'static str,
) -> LegacyParityFinding {
    LegacyParityFinding {
        category,
        status: LegacyParityStatus::AcceptedDifference,
        legacy,
        superbot,
        note,
    }
}

fn module_render_preview(module: SuperbotModuleKind) -> RenderPreviewModule {
    module_render_preview_curated(module)
}

fn module_render_preview_curated(module: SuperbotModuleKind) -> RenderPreviewModule {
    let parity = legacy_parity_modules()
        .into_iter()
        .find(|item| item.module == module.name())
        .map(|item| item.status)
        .unwrap_or(LegacyParityStatus::Partial);
    match module {
        SuperbotModuleKind::Clanlist => RenderPreviewModule {
            module: module.name(),
            parity_status: parity,
            embeds: vec![
                embed_preview_with_details(
                    "main roster",
                    "Список участников XIII",
                    "**Количество участников: N**",
                    vec!["role sections are emitted as follow-up embeds in legacy order"],
                    "#0066FF",
                    "Обновлено: {timestamp}",
                    "Europe/Berlin local time",
                ),
                embed_preview_with_details(
                    "admin roster",
                    "Административный состав XIII",
                    "**Количество участников: N**",
                    vec!["role sections are emitted as follow-up embeds in legacy order"],
                    "#0066FF",
                    "Обновлено: {timestamp}",
                    "Europe/Berlin local time",
                ),
                embed_preview_with_details(
                    "steam roster",
                    "Список Steam ID XIII",
                    "**Количество Discord участников:** N\n**Количество Steam ID:** N",

                    vec!["Steam legacy cache only; no Google reads in preview"],
                    "#0066FF",
                    "Обновлено: {timestamp}",
                    "Europe/Berlin local time",
                ),
            ],
            buttons: Vec::new(),
            modals: Vec::new(),
            commands: Vec::new(),
            text_responses: vec!["allowed_mentions disabled; no normal text content"],
            allowed_mentions: vec!["disabled"],
            warnings: vec!["ACCEPTED_DIFFERENCE: live chunking is verified by clanlist-render-preview and target-message checks, not by metadata audit alone."],
        },
        SuperbotModuleKind::TempVoice => RenderPreviewModule {
            module: module.name(),
            parity_status: parity,
            embeds: Vec::new(),
            buttons: Vec::new(),
            modals: Vec::new(),
            commands: vec!["/setup-voice-hub channel_id"],
            text_responses: vec!["ephemeral setup success/error responses"],
            allowed_mentions: vec!["disabled"],
            warnings: vec!["ACCEPTED_DIFFERENCE: no persistent visual panel exists for temp voice."],
        },
        SuperbotModuleKind::Vacation => RenderPreviewModule {
            module: module.name(),
            parity_status: parity,
            embeds: vec![
                embed_preview_with_details(
                    "request panel",
                    xiii_vacation::render::request_panel_title(),
                    xiii_vacation::render::REQUEST_PANEL_DESCRIPTION,
                    vec![],
                    "#5865F2",
                    xiii_vacation::render::LEGACY_FOOTER,
                    "no timestamp",
                ),
                embed_preview_with_details(
                    "active vacations",
                    xiii_vacation::render::ACTIVE_PANEL_TITLE,
                    "Сейчас в отпуске: N\n\n**1.** <@42> • <t:1715335200:d> • <t:1715594400:d> • <t:1715594400:R>\n> Причина: family trip",



                    vec!["truncation line: Показаны первые N отпусков из M."],
                    "#5865F2",
                    xiii_vacation::render::LEGACY_FOOTER,
                    "no timestamp",
                ),
                embed_preview_with_details(
                    "officer review",
                    xiii_vacation::render::OFFICER_REVIEW_TITLE,
                    xiii_vacation::render::OFFICER_REVIEW_PREVIEW,
                    vec![
                        xiii_vacation::render::OFFICER_FIELD_USER,
                        xiii_vacation::render::OFFICER_FIELD_DAYS,
                        xiii_vacation::render::OFFICER_FIELD_REASON,
                        xiii_vacation::render::OFFICER_FIELD_STATUS,
                        xiii_vacation::render::OFFICER_FIELD_DECIDED_BY,
                    ],
                    "#FEE75C",
                    xiii_vacation::render::LEGACY_FOOTER,
                    "request created_at RFC3339 timestamp",
                ),
                embed_preview_with_details(
                    "approval DM",
                    xiii_vacation::render::APPROVED_DM_TITLE,
                    xiii_vacation::render::APPROVED_DM_DESCRIPTION,
                    vec![
                        xiii_vacation::render::APPROVED_DM_DAYS_FIELD,
                        xiii_vacation::render::APPROVED_DM_END_FIELD,
                    ],
                    "#57F287",
                    xiii_vacation::render::LEGACY_FOOTER,
                    "no timestamp",
                ),
                embed_preview_with_details(
                    "rejection DM",
                    xiii_vacation::render::REJECTED_DM_TITLE,
                    xiii_vacation::render::REJECTED_DM_DESCRIPTION,
                    vec![],
                    "#ED4245",
                    xiii_vacation::render::LEGACY_FOOTER,
                    "no timestamp",
                ),
                embed_preview_with_details(
                    "expired DM",
                    xiii_vacation::render::EXPIRED_DM_TITLE,
                    xiii_vacation::render::EXPIRED_DM_DESCRIPTION,
                    vec![],
                    "#57F287",
                    xiii_vacation::render::LEGACY_FOOTER,
                    "no timestamp",
                ),
            ],
            buttons: vec![
                button_preview(
                    "request panel",
                    "vacation:apply",
                    xiii_vacation::render::REQUEST_BUTTON_LABEL,
                    "Primary",
                ),
                button_preview(
                    "officer review",
                    "vacation:approve:{request_id}",
                    xiii_vacation::render::APPROVE_BUTTON_LABEL,
                    "Success",
                ),
                button_preview(
                    "officer review",
                    "vacation:reject:{request_id}",
                    xiii_vacation::render::REJECT_BUTTON_LABEL,
                    "Danger",
                ),
                button_preview(
                    "early end",
                    "vacation:end:{vacation_id}",
                    xiii_vacation::render::EARLY_END_BUTTON_LABEL,
                    "Danger",
                ),
                button_preview(
                    "early end confirm",
                    "vacation:end_confirm:{vacation_id}",
                    xiii_vacation::render::EARLY_END_CONFIRM_LABEL,
                    "Danger",
                ),
                button_preview(
                    "early end confirm",
                    "vacation:end_cancel:{vacation_id}",
                    xiii_vacation::render::EARLY_END_CANCEL_LABEL,
                    "Secondary",
                ),
            ],
            modals: vec![modal_preview(
                "request",
                "vacation:modal",
                xiii_vacation::render::REQUEST_MODAL_TITLE,
                vec![
                    xiii_vacation::render::REQUEST_MODAL_DAYS_LABEL,
                    xiii_vacation::render::REQUEST_MODAL_REASON_LABEL,
                ],
                vec![
                    xiii_vacation::render::REQUEST_MODAL_DAYS_PLACEHOLDER,
                    xiii_vacation::render::REQUEST_MODAL_REASON_PLACEHOLDER,
                ],
            )],
            commands: vec!["/vacations"],
            text_responses: vec![
                xiii_vacation::commands::VACATIONS_DISABLED_RESPONSE,
                xiii_vacation::render::SUBMITTED_RESPONSE,
                xiii_vacation::render::APPROVED_RESPONSE,
                xiii_vacation::render::REJECTED_RESPONSE,
                xiii_vacation::render::ENDED_RESPONSE,
            ],
            allowed_mentions: vec![
                "disabled by default",
                "officer ping limited to VACATION_OFFICER_PING_ROLE_ID",
            ],
            warnings: Vec::new(),
        },
        SuperbotModuleKind::Discipline => RenderPreviewModule {
            module: module.name(),
            parity_status: parity,
            embeds: vec![
                embed_preview_with_details(
                    "board",
                    xiii_discipline::render::board_title(),
                    xiii_discipline::render::BOARD_SUMMARY_PREVIEW,
                    vec!["active punishments are rendered from the legacy DB source of truth"],
                    "#2F80ED",
                    xiii_discipline::render::BOARD_FOOTER_PREVIEW,
                    "ru-RU formatted update footer",
                ),
                embed_preview_with_details(
                    "history",
                    xiii_discipline::render::HISTORY_TITLE,
                    "<@42> • Страница 1/2\n\n1. Предупреждение • активно\nПричина: AFK",



                    vec![
                        xiii_discipline::render::EMPTY_HISTORY_TEMPLATE,
                        xiii_discipline::render::HISTORY_OVERSIZE_NOTE,
                    ],
                    "#42B883",
                    "<@42> • Страница 1/2",
                    "no timestamp",
                ),
            ],
            buttons: vec![
                button_preview(
                    "board",
                    "xiii:panel:issue",
                    xiii_discipline::render::PANEL_ISSUE_LABEL,
                    "Danger",
                ),
                button_preview(
                    "board",
                    "xiii:panel:remove",
                    xiii_discipline::render::PANEL_REMOVE_LABEL,
                    "Success",
                ),
                button_preview(
                    "board",
                    "xiii:panel:history",
                    xiii_discipline::render::PANEL_HISTORY_LABEL,
                    "Secondary",
                ),
                button_preview(
                    "board",
                    "xiii:board:page:prev",
                    xiii_discipline::render::BOARD_PREV_LABEL,
                    "Secondary",
                ),
                button_preview(
                    "board",
                    "xiii:board:page:next",
                    xiii_discipline::render::BOARD_NEXT_LABEL,
                    "Secondary",
                ),
            ],
            modals: vec![
                modal_preview(
                    "issue id",
                    "xiii:issue:idmodal:{session}",
                    xiii_discipline::render::ISSUE_ID_MODAL_TITLE,
                    vec![xiii_discipline::render::ISSUE_ID_MODAL_LABEL],




                    vec![],
                ),
                modal_preview(
                    "remove",
                    "xiii:remove:modal:{issuer}:{target}:{punishment_id}",
                    xiii_discipline::render::REMOVE_MODAL_TITLE,
                    vec![xiii_discipline::render::REMOVE_REASON_LABEL],



                    vec![],
                ),
                modal_preview(
                    "issue reason",
                    "xiii:issue:modal:{issuer}:{target}:{type}",
                    "\u{0412}\u{044b}\u{0434}\u{0430}\u{0442}\u{044c}: \u{041f}\u{0440}\u{0435}\u{0434}\u{0443}\u{043f}\u{0440}\u{0435}\u{0436}\u{0434}\u{0435}\u{043d}\u{0438}\u{0435}",
                    vec![xiii_discipline::render::ISSUE_REASON_LABEL],
                    vec![],
                ),
            ],
            commands: vec!["/discipline setup", "/discipline member", "/discipline health"],
            text_responses: vec!["ephemeral moderation responses; admin log embeds; DM notifications"],
            allowed_mentions: vec!["disabled"],
            warnings: vec![],
        },
        SuperbotModuleKind::Recruit => RenderPreviewModule {
            module: module.name(),
            parity_status: parity,
            embeds: vec![embed_preview_with_details(
                "decision panel",
                xiii_recruit::render::DECISION_PANEL_TITLE,
                xiii_recruit::render::DECISION_PREVIEW_DESCRIPTION,
                vec![
                    "description: стажёр mention, Discord ID, статус",
                    "fields: сроки / голос / продлений",
                    "processed decision adds решение, optional продление, причина, предупреждения",
                ],
                "#F1C40F",
                xiii_recruit::render::DECISION_FOOTER_PREVIEW,
                "footer uses recruit id",
            )],
            buttons: vec![
                button_preview(
                    "decision panel",
                    "xiii_recruit_accept:{id}",
                    xiii_recruit::render::ACCEPT_BUTTON_LABEL,
                    "Success",
                ),
                button_preview(
                    "decision panel",
                    "xiii_recruit_reject:{id}",
                    xiii_recruit::render::REJECT_BUTTON_LABEL,
                    "Danger",
                ),
                button_preview(
                    "decision panel",
                    "xiii_recruit_extend:{id}",
                    xiii_recruit::render::EXTEND_BUTTON_LABEL,
                    "Secondary",
                ),
            ],
            modals: vec![
                modal_preview(
                    "reject",
                    "xiii_recruit_reject_modal:{id}",
                    xiii_recruit::render::REJECT_MODAL_TITLE,
                    vec![xiii_recruit::render::REJECT_REASON_LABEL],
                    vec![],
                ),
                modal_preview(
                    "extend",
                    "xiii_recruit_extend_modal:{id}",
                    xiii_recruit::render::EXTEND_MODAL_TITLE,
                    vec![
                        xiii_recruit::render::EXTEND_DAYS_LABEL,
                        xiii_recruit::render::EXTEND_REASON_LABEL,
                    ],
                    vec![],
                ),
            ],
            commands: vec!["/recruits", "/recruit-panel user"],
            text_responses: vec![
                xiii_recruit::render::ACCEPT_SUCCESS,
                xiii_recruit::render::REJECT_SUCCESS,
                xiii_recruit::render::EXTEND_SUCCESS,
            ],
            allowed_mentions: vec![
                "disabled by default",
                "automatic due panel limited to RECRUIT_DECISION_PING_ROLE_IDS",
            ],
            warnings: vec![],
        },
        SuperbotModuleKind::VoiceActivity => RenderPreviewModule {
            module: module.name(),
            parity_status: parity,
            embeds: vec![
                embed_preview_with_details(
                    "public stats",
                    "XIII Voice Activity",
                    xiii_voice_activity::render::PUBLIC_STATS_PREVIEW_DESCRIPTION,
                    vec!["leaderboard rows: top-three medals, escaped display name, compact duration, Russian points suffix"],
                    "#5865F2",
                    xiii_voice_activity::render::LEGACY_FOOTER,
                    "current UTC timestamp",
                ),
                embed_preview_with_details(
                    "inactive check",
                    "XIII Inactivity Check",
                    xiii_voice_activity::render::INACTIVE_PREVIEW_DESCRIPTION,
                    vec!["inactive rows: rank, status icon, escaped display name, vacation marker, compact duration, Russian points suffix"],
                    "#5865F2",
                    xiii_voice_activity::render::LEGACY_FOOTER,
                    "current UTC timestamp",
                ),
            ],
            buttons: vec![
                button_preview(
                    "public stats",
                    "public-stats-panel:previous",
                    xiii_voice_activity::render::PREVIOUS_LABEL,
                    "Secondary",
                ),
                button_preview(
                    "public stats",
                    "public-stats-panel:next",
                    xiii_voice_activity::render::NEXT_LABEL,
                    "Secondary",
                ),
                button_preview(
                    "inactive check",
                    "inactive-check:previous",
                    xiii_voice_activity::render::PREVIOUS_LABEL,
                    "Secondary",
                ),
                button_preview(
                    "inactive check",
                    "inactive-check:next",
                    xiii_voice_activity::render::NEXT_LABEL,
                    "Secondary",
                ),
            ],
            modals: Vec::new(),
            commands: vec!["/voice-top", "/inactive-check"],
            text_responses: vec!["/voice-top returns the legacy disabled message pointing to the public stats panel"],
            allowed_mentions: vec!["disabled"],
            warnings: vec![],
        },
        SuperbotModuleKind::Tickets => RenderPreviewModule {
            module: module.name(),
            parity_status: parity,
            embeds: vec![
                embed_preview_with_details(
                    "ticket panel",
                    xiii_tickets::render::panel_title(),
                    xiii_tickets::render::panel_description(),
                    vec!["application / complaint / promotion buttons in legacy order"],
                    "#3498DB",
                    "none",
                    "no timestamp",
                ),
                embed_preview_with_details(
                    "ticket close confirmation",
                    xiii_tickets::render::CLOSE_CONFIRM_TITLE,
                    xiii_tickets::render::CLOSE_CONFIRM_DESCRIPTION,
                    vec![],
                    "#E74C3C",
                    "none",
                    "no timestamp",
                ),
                embed_preview_with_details(
                    "ticket transcript summary",
                    xiii_tickets::render::TRANSCRIPT_SUMMARY_TITLE,
                    "",
                    vec![
                        xiii_tickets::render::TRANSCRIPT_FIELD_TICKET,
                        xiii_tickets::render::TRANSCRIPT_FIELD_NUMBER,
                        xiii_tickets::render::TRANSCRIPT_FIELD_TYPE,
                        xiii_tickets::render::TRANSCRIPT_FIELD_OPENED_BY,
                        xiii_tickets::render::TRANSCRIPT_FIELD_CLOSED_BY,
                        xiii_tickets::render::TRANSCRIPT_FIELD_PARTICIPANTS,
                        xiii_tickets::render::TRANSCRIPT_FIELD_OPENED_AT,
                        xiii_tickets::render::TRANSCRIPT_FIELD_CLOSED_AT,
                    ],
                    "#607D8B",
                    "none",
                    "no timestamp",
                ),
            ],
            buttons: {
                let mut buttons = xiii_tickets::discord_io::ticket_panel_button_specs()
                    .iter()
                    .map(|button| {
                        button_preview(
                            "ticket panel",
                            button.custom_id,
                            button.label,
                            button_style_name(button.style),
                        )
                    })
                    .collect::<Vec<_>>();
                buttons.extend([
                    button_preview(
                        "ticket lifecycle",
                        xiii_tickets::interactions::TICKET_CLOSE,
                        xiii_tickets::render::TICKET_CLOSE_LABEL,
                        "Danger",
                    ),
                    button_preview(
                        "ticket close confirmation",
                        xiii_tickets::interactions::TICKET_CLOSE_CONFIRM,
                        xiii_tickets::render::TICKET_CLOSE_CONFIRM_LABEL,
                        "Danger",
                    ),
                    button_preview(
                        "ticket close confirmation",
                        xiii_tickets::interactions::TICKET_CLOSE_CANCEL,
                        xiii_tickets::render::TICKET_CLOSE_CANCEL_LABEL,
                        "Secondary",
                    ),
                    button_preview(
                        "ticket after-close view",
                        xiii_tickets::interactions::TICKET_DELETE,
                        xiii_tickets::render::TICKET_DELETE_LABEL,
                        "Danger",
                    ),
                    button_preview(
                        "ticket after-close view",
                        xiii_tickets::interactions::TICKET_REOPEN_MOD,
                        xiii_tickets::render::TICKET_REOPEN_LABEL,
                        "Success",
                    ),
                    button_preview(
                        "ticket DM reopen",
                        xiii_tickets::interactions::DM_REOPEN_GENERIC,
                        xiii_tickets::render::TICKET_REOPEN_LABEL,
                        "Success",
                    ),
                ]);
                buttons
            },
            modals: Vec::new(),
            commands: vec![
                "/add member",
                "/remove member",
                "/custom-ticket name user? reason?",
                "!panel",
                "!accept|!принять",
                "!reject|!отклонить",
            ],
            text_responses: vec![
                "public: ticket channel lifecycle messages, close-summary embeds, and officer review embeds",
                "dm: transcript summary embed, reopen button, and ticket status updates",
            ],
            allowed_mentions: vec!["disabled by default", "configured ticket ping roles only"],
            warnings: vec!["ACCEPTED_DIFFERENCE: transcript is safe Rust HTML, not exact chat_exporter markup."],
        },
    }
}

fn embed_preview_with_details(
    surface: &'static str,
    title: &'static str,
    description: &'static str,
    fields: Vec<&'static str>,
    color: &'static str,
    footer: &'static str,
    timestamp_behavior: &'static str,
) -> RenderPreviewEmbed {
    RenderPreviewEmbed {
        surface,
        title,
        description,
        fields,
        color,
        footer,
        timestamp_behavior,
    }
}

fn button_preview(
    surface: &'static str,
    custom_id: &'static str,
    label: &'static str,
    style: &'static str,
) -> RenderPreviewButton {
    RenderPreviewButton {
        surface,
        custom_id,
        label,
        style,
    }
}

fn modal_preview(
    surface: &'static str,
    custom_id: &'static str,
    title: &'static str,
    fields: Vec<&'static str>,
    placeholders: Vec<&'static str>,
) -> RenderPreviewModal {
    RenderPreviewModal {
        surface,
        custom_id,
        title,
        fields,
        placeholders,
    }
}

fn button_style_name(style: ButtonStyle) -> &'static str {
    match style {
        ButtonStyle::Primary => "Primary",
        ButtonStyle::Secondary => "Secondary",
        ButtonStyle::Success => "Success",
        ButtonStyle::Danger => "Danger",
        ButtonStyle::Link => "Link",
        ButtonStyle::Premium => "Premium",
        _ => "Unknown",
    }
}

fn render_preview_text(previews: &[RenderPreviewModule]) -> String {
    let mut text = String::new();
    text.push_str("XIII Superbot Render Preview\n");
    text.push_str("Mode: READ ONLY / NO WRITES\n");
    text.push_str("Discord writes: DISABLED\n");
    text.push_str("Legacy DB writes: DISABLED\n");
    text.push_str("Google calls: DISABLED\n\n");
    for preview in previews {
        text.push_str(&format!(
            "{} parity={}\n",
            preview.module,
            preview.parity_status.as_str()
        ));
        for embed in &preview.embeds {
            text.push_str(&format!(
                "  embed {} color={} title={} footer={} timestamp={}\n",
                embed.surface, embed.color, embed.title, embed.footer, embed.timestamp_behavior
            ));
            text.push_str(&format!("    description: {}\n", embed.description));
            if !embed.fields.is_empty() {
                text.push_str(&format!("    fields: {}\n", embed.fields.join(" / ")));
            }
        }
        for button in &preview.buttons {
            text.push_str(&format!(
                "  button {} custom_id={} style={} label={}\n",
                button.surface, button.custom_id, button.style, button.label
            ));
        }
        for modal in &preview.modals {
            text.push_str(&format!(
                "  modal {} custom_id={} title={} fields={}\n",
                modal.surface,
                modal.custom_id,
                modal.title,
                modal.fields.join(" / ")
            ));
            if !modal.placeholders.is_empty() {
                text.push_str(&format!(
                    "    placeholders: {}\n",
                    modal.placeholders.join(" / ")
                ));
            }
        }
        if !preview.commands.is_empty() {
            text.push_str(&format!("  commands: {}\n", preview.commands.join(", ")));
        }
        if !preview.text_responses.is_empty() {
            text.push_str(&format!(
                "  responses: {}\n",
                preview.text_responses.join(" | ")
            ));
        }
        if !preview.allowed_mentions.is_empty() {
            text.push_str(&format!(
                "  allowed_mentions: {}\n",
                preview.allowed_mentions.join("; ")
            ));
        }
        for warning in &preview.warnings {
            text.push_str(&format!("  [WARN] {warning}\n"));
        }
        text.push('\n');
    }
    text
}

fn render_preview_json(previews: &[RenderPreviewModule]) -> String {
    let value = serde_json::json!({
        "mode": "read_only",
        "safety": {
            "discord_writes": false,
            "legacy_db_writes": false,
            "google_calls": false,
            "secrets_redacted": true
        },
        "modules": previews,
    });
    serde_json::to_string_pretty(&value)
        .unwrap_or_else(|err| format!("{{\"failures\":[\"failed to render JSON: {err}\"]}}"))
}

async fn fetch_live_voice_state_count_for_cutover(
    env_file: &Path,
    config: &SuperbotConfig,
) -> Result<i64, String> {
    let token = read_secret_from_env_file(env_file, "DISCORD_TOKEN")?;
    let intents = Intents::GUILDS | Intents::GUILD_VOICE_STATES;
    let mut shard = Shard::new(ShardId::ONE, token, intents);
    let deadline = tokio::time::sleep(Duration::from_secs(20));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => {
                return Err("timed out waiting for read-only GuildCreate voice snapshot".to_owned());
            }
            event = shard.next_event(EventTypeFlags::GUILD_CREATE | EventTypeFlags::READY) => {
                let Some(event) = event else {
                    return Err("Gateway stream ended before GuildCreate".to_owned());
                };
                match event {
                    Ok(Event::GuildCreate(guild)) => {
                        if let GuildCreate::Available(guild) = guild.as_ref() {
                            if guild.id.get() == config.core.guild_id {
                                let count = guild
                                    .voice_states
                                    .iter()
                                    .filter(|state| {
                                        state.channel_id
                                            .map(|id| {
                                                xiii_voice_activity::runtime::should_track_channel(
                                                    id.get(),
                                                    &config.voice_activity.ignored_channel_ids,
                                                )
                                            })
                                            .unwrap_or(false)
                                    })
                                    .count();
                                return Ok(count as i64);
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(err) => return Err(format!("Gateway event failed: {err}")),
                }
            }
        }
    }
}

async fn temp_voice_cutover_check(env_file: PathBuf) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    println!("XIII Temp Voice Cutover Check");
    println!("Mode: READ ONLY / NO WRITES");
    println!("Discord login: DISABLED");
    println!("Legacy DB writes: DISABLED");
    println!(
        "Legacy DB: {}",
        load.config.legacy_paths.temp_voice_db.resolved.display()
    );

    match read_temp_voice_cutover_state(&load.config.legacy_paths.temp_voice_db.resolved).await {
        Ok(state) => {
            if state.guild_settings_count == 0 {
                println!("[FAIL] guild_settings rows = 0; /setup-voice-hub must be recovered before cutover");
                return ExitCode::from(2);
            }
            println!("[OK] guild_settings rows = {}", state.guild_settings_count);
            println!(
                "[OK] temp_voice_channels rows = {}",
                state.temp_voice_channels_count
            );
            for (guild_id, hub_channel_id) in state.hubs {
                println!("[OK] hub guild_id={guild_id} hub_channel_id={hub_channel_id}");
            }
            if state.temp_voice_channels_count > 0 {
                println!("[WARN] tracked temp voice channels exist; startup reconciliation must compare them to live Discord channels before deleting anything");
            }
            println!(
                "[WARN] old temp-voice-bot.service must be stopped before TEMP_VOICE_ENABLED=true"
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            println!("[FAIL] temp_voice {err}");
            ExitCode::from(2)
        }
    }
}

async fn handle_ticket_component(
    interaction: &Interaction,
    custom_id: &str,
    runtime: &MixedSuperbotRuntime,
) {
    let Some(route) = xiii_tickets::interactions::route_ticket_component(custom_id) else {
        return;
    };
    use xiii_tickets::interactions::TicketComponentRoute;
    match route {
        TicketComponentRoute::OpenApplication => {
            handle_ticket_create_from_interaction(
                interaction,
                xiii_tickets::state::TicketType::Application,
                None,
                runtime,
            )
            .await;
        }
        TicketComponentRoute::OpenQuestion => {
            handle_ticket_create_from_interaction(
                interaction,
                xiii_tickets::state::TicketType::Complaint,
                None,
                runtime,
            )
            .await;
        }
        TicketComponentRoute::OpenIdea => {
            handle_ticket_create_from_interaction(
                interaction,
                xiii_tickets::state::TicketType::Idea,
                None,
                runtime,
            )
            .await;
        }
        TicketComponentRoute::Close => {
            respond_interaction_embeds_http(
                runtime.http.as_ref(),
                interaction,
                vec![xiii_tickets::discord_io::embed_from_draft(
                    &xiii_tickets::render::close_confirmation_embed(),
                )],
                Some(xiii_tickets::discord_io::close_confirmation_components()),
            )
            .await;
        }
        TicketComponentRoute::CloseConfirm => {
            match ticket_close_current_channel(interaction, runtime).await {
                Ok(embed) => {
                    respond_interaction_update_embeds_http(
                        runtime.http.as_ref(),
                        interaction,
                        vec![embed],
                        Some(xiii_tickets::discord_io::after_close_components()),
                    )
                    .await;
                }
                Err(err) => {
                    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err)
                        .await;
                }
            }
        }
        TicketComponentRoute::CloseCancel => {
            respond_interaction_update_embeds_http(
                runtime.http.as_ref(),
                interaction,
                vec![xiii_tickets::discord_io::embed_from_draft(
                    &xiii_tickets::render::close_cancelled_embed(),
                )],
                None,
            )
            .await;
        }
        TicketComponentRoute::StaffNotes => {
            let channel_id = interaction_channel_id(interaction).unwrap_or_default();
            respond_interaction_modal_http(
                runtime.http.as_ref(),
                interaction,
                &format!("ticket_staff_notes_modal:{channel_id}"),
                xiii_tickets::render::STAFF_NOTES_MODAL_TITLE,
                vec![action_row(vec![text_input(
                    "note",
                    xiii_tickets::render::STAFF_NOTES_MODAL_LABEL,
                    TextInputStyle::Paragraph,
                    true,
                )])],
            )
            .await;
        }
        TicketComponentRoute::NotesDelete => {
            let response = match interaction.message.as_ref() {
                Some(message) => match runtime.ticket_discord.as_ref() {
                    Some(discord) => match discord
                        .delete_message(message.channel_id.get(), message.id.get())
                        .await
                    {
                        Ok(()) => xiii_tickets::render::STAFF_NOTE_DELETED_TEXT.to_owned(),
                        Err(err) => err,
                    },
                    None => "ticket Discord adapter is unavailable".to_owned(),
                },
                None => "No note message was attached to this interaction.".to_owned(),
            };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        TicketComponentRoute::Delete => {
            let response = match ticket_delete_current_channel(interaction, runtime).await {
                Ok(message) => message,
                Err(err) => err,
            };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        TicketComponentRoute::ReopenMod => {
            let response = match ticket_reopen_current_channel(interaction, runtime).await {
                Ok(message) => message,
                Err(err) => err,
            };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        TicketComponentRoute::DmReopen => {
            let response = match ticket_reopen_from_dm(interaction, runtime).await {
                Ok(message) => message,
                Err(err) => err,
            };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        TicketComponentRoute::ApplicationAccept => {
            let response = match ticket_application_decision(interaction, runtime, true).await {
                Ok(message) => message,
                Err(err) => err,
            };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        TicketComponentRoute::ApplicationReject => {
            let response = match ticket_application_decision(interaction, runtime, false).await {
                Ok(message) => message,
                Err(err) => err,
            };
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
    }
}

async fn handle_ticket_custom_command(
    interaction: &Interaction,
    data: &CommandData,
    runtime: &MixedSuperbotRuntime,
) {
    let roles = interaction_member_role_ids(interaction);
    if !ticket_can_custom_command(interaction, &roles, runtime) {
        respond_interaction_ephemeral_http(
            runtime.http.as_ref(),
            interaction,
            "Access denied for /custom-ticket.",
        )
        .await;
        return;
    }
    let name = command_option_string(&data.options, "name")
        .unwrap_or("ticket")
        .to_owned();
    let opener = command_option_user(&data.options, "user")
        .or_else(|| interaction.author_id().map(|id| id.get()))
        .unwrap_or_default();
    if opener == 0 {
        respond_interaction_ephemeral_http(
            runtime.http.as_ref(),
            interaction,
            "Could not determine ticket opener.",
        )
        .await;
        return;
    }
    handle_ticket_create_for_user(
        opener,
        xiii_tickets::state::TicketType::Custom,
        Some(name),
        runtime,
        interaction,
    )
    .await;
}

async fn handle_ticket_add_remove_command(
    interaction: &Interaction,
    data: &CommandData,
    runtime: &MixedSuperbotRuntime,
    add: bool,
) {
    let Some(channel_id) = interaction_channel_id(interaction) else {
        respond_interaction_ephemeral_http(
            runtime.http.as_ref(),
            interaction,
            "This command must be used in a ticket channel.",
        )
        .await;
        return;
    };
    let Some(target_user_id) = command_option_user(&data.options, "member")
        .or_else(|| command_option_user(&data.options, "user"))
    else {
        respond_interaction_ephemeral_http(
            runtime.http.as_ref(),
            interaction,
            "Missing member option.",
        )
        .await;
        return;
    };
    let roles = interaction_member_role_ids(interaction);
    if !ticket_can_moderate(interaction, &roles, runtime) {
        respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, "Access denied.")
            .await;
        return;
    }
    let Some(repo) = runtime.ticket_repo.as_ref() else {
        return;
    };
    let Some(discord) = runtime.ticket_discord.as_ref() else {
        return;
    };
    match repo.get_ticket_by_channel_id(channel_id).await {
        Ok(Some(ticket)) => {
            if !add && target_user_id == ticket.opener_id {
                respond_interaction_ephemeral_http(
                    runtime.http.as_ref(),
                    interaction,
                    "The ticket opener cannot be removed with /remove.",
                )
                .await;
                return;
            }
            let result = if add {
                discord
                    .set_channel_permissions(
                        channel_id,
                        &xiii_tickets::discord_io::reopen_ticket_owner_overwrite(target_user_id),
                    )
                    .await
                    .map(|_| format!("<@{target_user_id}> added to the ticket."))
            } else {
                discord
                    .delete_member_channel_permission(channel_id, target_user_id)
                    .await
                    .map(|_| format!("<@{target_user_id}> removed from the ticket."))
            };
            let response = result.unwrap_or_else(|err| err);
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
        }
        Ok(None) => {
            respond_interaction_ephemeral_http(
                runtime.http.as_ref(),
                interaction,
                "This channel is not a tracked ticket channel.",
            )
            .await;
        }
        Err(err) => {
            respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &err).await;
        }
    }
}

async fn handle_ticket_create_from_interaction(
    interaction: &Interaction,
    ticket_type: xiii_tickets::state::TicketType,
    custom_name: Option<String>,
    runtime: &MixedSuperbotRuntime,
) {
    let Some(opener_id) = interaction.author_id().map(|id| id.get()) else {
        respond_interaction_ephemeral_http(
            runtime.http.as_ref(),
            interaction,
            "Could not determine ticket opener.",
        )
        .await;
        return;
    };
    handle_ticket_create_for_user(opener_id, ticket_type, custom_name, runtime, interaction).await;
}

async fn handle_ticket_create_for_user(
    opener_id: u64,
    ticket_type: xiii_tickets::state::TicketType,
    custom_name: Option<String>,
    runtime: &MixedSuperbotRuntime,
    interaction: &Interaction,
) {
    let response = match ticket_create_for_user(opener_id, ticket_type, custom_name, runtime).await
    {
        Ok(channel_id) => format!("Тикет создан: <#{channel_id}>"),
        Err(err) => err,
    };
    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
}

async fn ticket_create_for_user(
    opener_id: u64,
    ticket_type: xiii_tickets::state::TicketType,
    custom_name: Option<String>,
    runtime: &MixedSuperbotRuntime,
) -> Result<u64, String> {
    let repo = runtime
        .ticket_repo
        .as_ref()
        .ok_or_else(|| "ticket repository is unavailable".to_owned())?;
    let discord = runtime
        .ticket_discord
        .as_ref()
        .ok_or_else(|| "ticket Discord adapter is unavailable".to_owned())?;
    let mut reserved = repo
        .reserve_ticket(
            opener_id,
            ticket_type,
            chrono::Utc::now(),
            xiii_tickets::runtime::DEFAULT_MAX_OPEN_TICKETS_PER_USER,
        )
        .await?;
    if let Some(name) = custom_name {
        reserved.ticket_name =
            xiii_tickets::runtime::custom_ticket_channel_name(&name, reserved.number);
    }
    let ping_role_id = xiii_tickets::runtime::ping_role_for_ticket_type(
        ticket_type,
        runtime.config.tickets.application_ping_role_id,
        runtime.config.tickets.other_ping_role_id,
        runtime.config.tickets.idea_ping_role_id,
    );
    let plan =
        xiii_tickets::runtime::build_creation_plan(reserved.clone(), opener_id, ping_role_id);
    let request = xiii_tickets::discord_io::TicketChannelCreateRequest::from_plan(
        runtime.config.core.guild_id,
        runtime.config.tickets.open_category_id,
        runtime.config.tickets.support_role_id,
        runtime.config.tickets.global_moderator_role_ids.clone(),
        &plan,
    );
    let channel = match discord.create_ticket_channel(&request).await {
        Ok(channel) => channel,
        Err(err) => {
            let _ = repo.rollback_reserved_ticket(reserved.ticket_id).await;
            return Err(err);
        }
    };
    if let Err(err) = discord
        .send_ticket_open_message(
            channel.id.get(),
            &xiii_tickets::discord_io::ticket_open_payload(&plan, ticket_type),
        )
        .await
    {
        let _ = repo.rollback_reserved_ticket(reserved.ticket_id).await;
        return Err(err);
    }
    if !repo
        .finalize_ticket_open(reserved.ticket_id, &reserved.ticket_name, channel.id.get())
        .await?
    {
        return Err("ticket reservation was no longer available for finalization".to_owned());
    }
    Ok(channel.id.get())
}

async fn ticket_close_current_channel(
    interaction: &Interaction,
    runtime: &MixedSuperbotRuntime,
) -> Result<Embed, String> {
    let channel_id = interaction_channel_id(interaction)
        .ok_or_else(|| "This action must be used in a ticket channel.".to_owned())?;
    let closer_id = interaction
        .author_id()
        .map(|id| id.get())
        .unwrap_or_default();
    let repo = runtime
        .ticket_repo
        .as_ref()
        .ok_or_else(|| "ticket repository is unavailable".to_owned())?;
    let discord = runtime
        .ticket_discord
        .as_ref()
        .ok_or_else(|| "ticket Discord adapter is unavailable".to_owned())?;
    let ticket = repo
        .get_ticket_by_channel_id(channel_id)
        .await?
        .ok_or_else(|| "This channel is not a tracked ticket channel.".to_owned())?;
    let closed_at = chrono::Utc::now();
    let messages = discord
        .fetch_transcript_messages(channel_id, xiii_tickets::runtime::TRANSCRIPT_FETCH_LIMIT)
        .await
        .unwrap_or_default();
    let participant_count = (!messages.is_empty()).then(|| {
        messages
            .iter()
            .map(|message| message.author_id)
            .collect::<BTreeSet<_>>()
            .len()
    });
    let html = xiii_tickets::render::transcript_html(&ticket, &messages);
    let ticket_name = ticket
        .ticket_name
        .clone()
        .unwrap_or_else(|| format!("ticket-{}", ticket.ticket_id));
    let transcript_summary = xiii_tickets::render::transcript_summary_embed(
        &ticket,
        closer_id,
        closed_at,
        participant_count,
    );
    let transcript_summary_embed = xiii_tickets::discord_io::embed_from_draft(&transcript_summary);
    let transcript_payload = xiii_tickets::discord_io::transcript_payload(
        runtime.config.tickets.transcript_channel_id,
        &ticket_name,
        html,
    );
    let transcript_saved = if discord
        .send_channel_embed_message(
            runtime.config.tickets.transcript_channel_id,
            &transcript_summary_embed,
            None,
        )
        .await
        .is_ok()
    {
        discord.send_transcript(&transcript_payload).await.is_ok()
    } else {
        false
    };
    let closed = repo
        .mark_ticket_closed_by_channel(
            channel_id,
            closed_at,
            xiii_tickets::runtime::DEFAULT_REOPEN_WINDOW_HOURS,
        )
        .await?;
    if let Some(closed) = closed {
        let closed_name = format!("closed-{}", ticket_name.trim_start_matches("closed-"));
        let _ = discord.rename_channel(channel_id, &closed_name).await;
        let _ = discord
            .set_channel_permissions(
                channel_id,
                &xiii_tickets::discord_io::closed_ticket_owner_overwrite(closed.opener_id),
            )
            .await;
        let dm_sent = if transcript_saved {
            discord
                .send_dm_embed_message(
                    closed.opener_id,
                    &transcript_summary_embed,
                    Some(xiii_tickets::discord_io::dm_reopen_components()),
                )
                .await
                .is_ok()
                && discord
                    .send_dm_transcript(closed.opener_id, &transcript_payload)
                    .await
                    .is_ok()
        } else {
            false
        };
        Ok(xiii_tickets::discord_io::embed_from_draft(
            &xiii_tickets::render::close_result_embed(transcript_saved, dm_sent),
        ))
    } else {
        Err("Ticket was already closed or is not open.".to_owned())
    }
}

async fn ticket_delete_current_channel(
    interaction: &Interaction,
    runtime: &MixedSuperbotRuntime,
) -> Result<String, String> {
    let channel_id = interaction_channel_id(interaction)
        .ok_or_else(|| "This action must be used in a ticket channel.".to_owned())?;
    let repo = runtime
        .ticket_repo
        .as_ref()
        .ok_or_else(|| "ticket repository is unavailable".to_owned())?;
    let discord = runtime
        .ticket_discord
        .as_ref()
        .ok_or_else(|| "ticket Discord adapter is unavailable".to_owned())?;
    let ticket = repo
        .get_ticket_by_channel_id(channel_id)
        .await?
        .ok_or_else(|| "This channel is not a tracked ticket channel.".to_owned())?;
    let ticket_name = ticket
        .ticket_name
        .clone()
        .unwrap_or_else(|| format!("ticket-{}", ticket.ticket_id));
    if ticket.status != xiii_tickets::state::TicketStatus::Closed {
        let messages = discord
            .fetch_transcript_messages(channel_id, xiii_tickets::runtime::TRANSCRIPT_FETCH_LIMIT)
            .await
            .unwrap_or_default();
        let html = xiii_tickets::render::transcript_html(&ticket, &messages);
        discord
            .send_transcript(&xiii_tickets::discord_io::transcript_payload(
                runtime.config.tickets.transcript_channel_id,
                &ticket_name,
                html,
            ))
            .await?;
    }
    if repo.mark_ticket_deleted_by_channel(channel_id).await? {
        discord.delete_channel(channel_id).await?;
        Ok(xiii_tickets::render::DELETE_SUCCESS_TEXT.to_owned())
    } else {
        Ok("Ticket was already deleted or not in a deletable state.".to_owned())
    }
}

async fn ticket_reopen_current_channel(
    interaction: &Interaction,
    runtime: &MixedSuperbotRuntime,
) -> Result<String, String> {
    let channel_id = interaction_channel_id(interaction)
        .ok_or_else(|| "This action must be used in a ticket channel.".to_owned())?;
    let repo = runtime
        .ticket_repo
        .as_ref()
        .ok_or_else(|| "ticket repository is unavailable".to_owned())?;
    let discord = runtime
        .ticket_discord
        .as_ref()
        .ok_or_else(|| "ticket Discord adapter is unavailable".to_owned())?;
    let ticket = repo
        .get_ticket_by_channel_id(channel_id)
        .await?
        .ok_or_else(|| "This channel is not a tracked ticket channel.".to_owned())?;
    let base_name = ticket
        .ticket_name
        .clone()
        .unwrap_or_else(|| format!("ticket-{}", ticket.ticket_id))
        .trim_start_matches("closed-")
        .to_owned();
    if repo
        .reopen_ticket_record(ticket.ticket_id, Some(channel_id), Some(&base_name))
        .await?
    {
        let actor_id = interaction
            .author_id()
            .map(|id| id.get())
            .unwrap_or_default();
        let _ = discord.rename_channel(channel_id, &base_name).await;
        let _ = discord
            .set_channel_permissions(
                channel_id,
                &xiii_tickets::discord_io::reopen_ticket_owner_overwrite(ticket.opener_id),
            )
            .await;
        let _ = discord
            .send_channel_message(
                channel_id,
                &xiii_tickets::render::reopen_channel_message(actor_id),
                &[],
            )
            .await;
        let _ = discord
            .dm_user(&xiii_tickets::discord_io::dm_payload(
                ticket.opener_id,
                xiii_tickets::runtime::reopen_dm_text(&base_name),
            ))
            .await;
        Ok(xiii_tickets::render::REOPEN_SUCCESS_TEXT.to_owned())
    } else {
        Ok("Ticket was not in a reopenable closed state.".to_owned())
    }
}

async fn ticket_reopen_from_dm(
    interaction: &Interaction,
    runtime: &MixedSuperbotRuntime,
) -> Result<String, String> {
    let user_id = interaction
        .author_id()
        .map(|id| id.get())
        .ok_or_else(|| "Could not determine DM user.".to_owned())?;
    let repo = runtime
        .ticket_repo
        .as_ref()
        .ok_or_else(|| "ticket repository is unavailable".to_owned())?;
    let ticket = repo
        .latest_reopenable_ticket_for_user(user_id, chrono::Utc::now())
        .await?
        .ok_or_else(|| "No reopenable closed ticket was found for this user.".to_owned())?;
    let channel_id = ticket_create_for_user(
        user_id,
        ticket.ticket_type,
        ticket.ticket_name.clone(),
        runtime,
    )
    .await?;
    let _ = repo
        .reopen_ticket_record(
            ticket.ticket_id,
            Some(channel_id),
            ticket.ticket_name.as_deref(),
        )
        .await?;
    Ok(format!("Тикет переоткрыт: <#{channel_id}>"))
}

async fn ticket_application_decision(
    interaction: &Interaction,
    runtime: &MixedSuperbotRuntime,
    accept: bool,
) -> Result<String, String> {
    let roles = interaction_member_role_ids(interaction);
    let allowed = roles.contains(&runtime.config.tickets.application_ping_role_id)
        || ticket_can_moderate(interaction, &roles, runtime);
    if !allowed {
        return Err("Access denied for application decision.".to_owned());
    }
    let channel_id = parse_ticket_target_channel_from_interaction(interaction)
        .or_else(|| interaction_channel_id(interaction))
        .ok_or_else(|| "Could not resolve target ticket channel.".to_owned())?;
    let repo = runtime
        .ticket_repo
        .as_ref()
        .ok_or_else(|| "ticket repository is unavailable".to_owned())?;
    let discord = runtime
        .ticket_discord
        .as_ref()
        .ok_or_else(|| "ticket Discord adapter is unavailable".to_owned())?;
    let ticket = repo
        .get_ticket_by_channel_id(channel_id)
        .await?
        .ok_or_else(|| "Target ticket was not found in the ticket DB.".to_owned())?;
    if ticket.ticket_type != xiii_tickets::state::TicketType::Application {
        return Err("Target ticket is not an application ticket.".to_owned());
    }
    if accept {
        for role_id in &runtime.config.tickets.accept_role_ids {
            let _ = discord
                .add_member_role(runtime.config.core.guild_id, ticket.opener_id, *role_id)
                .await;
        }
        let _ = discord
            .send_channel_message(
                channel_id,
                xiii_tickets::runtime::accept_application_text(),
                &[],
            )
            .await;
        Ok("Готово: игрок принят ✅".to_owned())
    } else {
        let _ = discord
            .send_channel_message(
                channel_id,
                xiii_tickets::runtime::reject_application_text(),
                &[],
            )
            .await;
        Ok("Готово: игрок отклонён ❌".to_owned())
    }
}

async fn handle_ticket_staff_notes_modal(
    interaction: &Interaction,
    data: &twilight_model::application::interaction::modal::ModalInteractionData,
    runtime: &MixedSuperbotRuntime,
) {
    let channel_id = data
        .custom_id
        .strip_prefix("ticket_staff_notes_modal:")
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| interaction_channel_id(interaction))
        .unwrap_or_default();
    let note = modal_value(data, "note").unwrap_or_default();
    let response = if channel_id == 0 || note.trim().is_empty() {
        "Missing ticket channel or note text.".to_owned()
    } else if let Some(discord) = runtime.ticket_discord.as_ref() {
        discord
            .send_channel_message(
                channel_id,
                &format!(
                    "{} {}",
                    xiii_tickets::render::STAFF_NOTE_PREFIX,
                    note.trim()
                ),
                &[],
            )
            .await
            .map(|_| xiii_tickets::render::STAFF_NOTE_ADDED_TEXT.to_owned())
            .unwrap_or_else(|err| err)
    } else {
        "ticket Discord adapter is unavailable".to_owned()
    };
    respond_interaction_ephemeral_http(runtime.http.as_ref(), interaction, &response).await;
}

async fn handle_ticket_message_create(message: &DiscordMessage, runtime: &MixedSuperbotRuntime) {
    if message.author.bot {
        return;
    }
    let Some(route) = xiii_tickets::commands::route_text_command(&message.content) else {
        return;
    };
    match route {
        xiii_tickets::commands::TicketTextCommand::Panel => {
            if !message_author_can_custom(message, runtime) {
                return;
            }
            if let Some(discord) = runtime.ticket_discord.as_ref() {
                if let Err(err) = discord.send_ticket_panel(message.channel_id.get()).await {
                    println!("[WARN] ticket !panel failed: {err}");
                }
            }
        }
        xiii_tickets::commands::TicketTextCommand::Accept => {
            if let Err(err) = ticket_application_decision_from_message(message, runtime, true).await
            {
                println!("[WARN] ticket !accept failed: {err}");
            }
        }
        xiii_tickets::commands::TicketTextCommand::Reject => {
            if let Err(err) =
                ticket_application_decision_from_message(message, runtime, false).await
            {
                println!("[WARN] ticket !reject failed: {err}");
            }
        }
    }
}

async fn ticket_application_decision_from_message(
    message: &DiscordMessage,
    runtime: &MixedSuperbotRuntime,
    accept: bool,
) -> Result<(), String> {
    if !message_author_can_accept(message, runtime) {
        return Err("message author lacks application decision role".to_owned());
    }
    let repo = runtime
        .ticket_repo
        .as_ref()
        .ok_or_else(|| "ticket repository is unavailable".to_owned())?;
    let discord = runtime
        .ticket_discord
        .as_ref()
        .ok_or_else(|| "ticket Discord adapter is unavailable".to_owned())?;
    let ticket = repo
        .get_ticket_by_channel_id(message.channel_id.get())
        .await?
        .ok_or_else(|| "message channel is not a tracked ticket".to_owned())?;
    if ticket.ticket_type != xiii_tickets::state::TicketType::Application {
        return Err("message channel is not an application ticket".to_owned());
    }
    if accept {
        for role_id in &runtime.config.tickets.accept_role_ids {
            let _ = discord
                .add_member_role(runtime.config.core.guild_id, ticket.opener_id, *role_id)
                .await;
        }
        discord
            .send_channel_message(
                message.channel_id.get(),
                xiii_tickets::runtime::accept_application_text(),
                &[],
            )
            .await?;
    } else {
        discord
            .send_channel_message(
                message.channel_id.get(),
                xiii_tickets::runtime::reject_application_text(),
                &[],
            )
            .await?;
    }
    Ok(())
}

async fn ticket_google_forms_poll_tick(runtime: &MixedSuperbotRuntime) -> Result<(), String> {
    let repo = runtime
        .ticket_repo
        .as_ref()
        .ok_or_else(|| "ticket repository is unavailable".to_owned())?;
    let discord = runtime
        .ticket_discord
        .as_ref()
        .ok_or_else(|| "ticket Discord adapter is unavailable".to_owned())?;
    let config = ticket_google_poll_config(&runtime.env_file)?;
    let google = xiii_tickets::google::GoogleSheetsReadonlyClient::new();
    let mut rows = google.fetch_rows(&config).await?;
    rows.sort_by_key(|row| row.sheet_row);
    for row in rows {
        if row.values.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        let signature = xiii_tickets::repository::google_form_signature(&row);
        if repo.processed_form_row_exists(row.sheet_row).await?
            || repo.form_signature_processed_async(&signature).await?
        {
            continue;
        }
        let Some(ticket_number) = xiii_tickets::runtime::ticket_number_from_google_row(&row) else {
            continue;
        };
        let Some(ticket) = repo
            .find_application_ticket_by_number(ticket_number)
            .await?
        else {
            continue;
        };
        let Some(channel_id) = ticket.channel_id else {
            continue;
        };
        let description =
            xiii_tickets::runtime::officer_review_description(&row, Some(ticket_number));
        let payload = xiii_tickets::discord_io::officer_review_payload(
            runtime.config.tickets.officer_review_channel_id,
            channel_id,
            description,
            if runtime.config.tickets.application_ping_role_id == 0 {
                Vec::new()
            } else {
                vec![runtime.config.tickets.application_ping_role_id]
            },
        );
        discord.send_officer_review(&payload).await?;
        repo.mark_form_processed_after_send(true, row.sheet_row, &signature, chrono::Utc::now())
            .await?;
    }
    Ok(())
}

fn ticket_google_poll_config(
    env_file: &Path,
) -> Result<xiii_tickets::google::GoogleSheetsPollConfig, String> {
    let credentials_file = read_env_value(env_file, "TICKET_GOOGLE_CREDENTIALS_FILE")
        .ok_or_else(|| "TICKET_GOOGLE_CREDENTIALS_FILE is missing".to_owned())?;
    let sheet_id = read_env_value(env_file, "TICKET_GOOGLE_SHEET_ID")
        .ok_or_else(|| "TICKET_GOOGLE_SHEET_ID is missing".to_owned())?;
    let sheet_name = read_env_value(env_file, "TICKET_GOOGLE_SHEET_NAME")
        .unwrap_or_else(|| "Form Responses 1".to_owned());
    Ok(xiii_tickets::google::GoogleSheetsPollConfig {
        credentials_file: PathBuf::from(credentials_file),
        sheet_id,
        sheet_name,
        start_row: xiii_tickets::runtime::LEGACY_GOOGLE_START_ROW,
        end_column: xiii_tickets::runtime::LEGACY_GOOGLE_END_COLUMN.to_owned(),
    })
}

#[allow(deprecated)]
fn interaction_channel_id(interaction: &Interaction) -> Option<u64> {
    interaction
        .channel
        .as_ref()
        .map(|channel| channel.id.get())
        .or_else(|| interaction.channel_id.map(|id| id.get()))
}

fn interaction_member_role_ids(interaction: &Interaction) -> Vec<u64> {
    interaction
        .member
        .as_ref()
        .map(|member| member.roles.iter().map(|id| id.get()).collect())
        .unwrap_or_default()
}

fn ticket_can_moderate(
    interaction: &Interaction,
    roles: &[u64],
    runtime: &MixedSuperbotRuntime,
) -> bool {
    interaction
        .member
        .as_ref()
        .and_then(|member| member.permissions)
        .map(|permissions| {
            permissions.contains(Permissions::ADMINISTRATOR)
                || permissions.contains(Permissions::MANAGE_CHANNELS)
        })
        .unwrap_or(false)
        || xiii_tickets::commands::can_moderate_tickets(
            roles,
            &runtime.config.tickets.global_moderator_role_ids,
        )
        || roles.contains(&runtime.config.tickets.support_role_id)
}

fn ticket_can_custom_command(
    interaction: &Interaction,
    roles: &[u64],
    runtime: &MixedSuperbotRuntime,
) -> bool {
    interaction
        .member
        .as_ref()
        .and_then(|member| member.permissions)
        .map(|permissions| permissions.contains(Permissions::ADMINISTRATOR))
        .unwrap_or(false)
        || xiii_tickets::commands::can_use_custom_ticket_command(
            roles,
            &runtime.config.tickets.custom_command_role_ids,
        )
}

fn message_author_roles(message: &DiscordMessage) -> Vec<u64> {
    message
        .member
        .as_ref()
        .map(|member| member.roles.iter().map(|id| id.get()).collect())
        .unwrap_or_default()
}

fn message_author_can_custom(message: &DiscordMessage, runtime: &MixedSuperbotRuntime) -> bool {
    let roles = message_author_roles(message);
    xiii_tickets::commands::can_use_custom_ticket_command(
        &roles,
        &runtime.config.tickets.custom_command_role_ids,
    ) || xiii_tickets::commands::can_moderate_tickets(
        &roles,
        &runtime.config.tickets.global_moderator_role_ids,
    )
}

fn message_author_can_accept(message: &DiscordMessage, runtime: &MixedSuperbotRuntime) -> bool {
    let roles = message_author_roles(message);
    roles.contains(&runtime.config.tickets.application_ping_role_id)
        || xiii_tickets::commands::can_moderate_tickets(
            &roles,
            &runtime.config.tickets.global_moderator_role_ids,
        )
}

fn parse_ticket_target_channel_from_interaction(interaction: &Interaction) -> Option<u64> {
    if let Some(custom_id) = interaction_component_custom_id(interaction) {
        if let Some(channel_id) =
            xiii_tickets::interactions::parse_application_decision_target_channel(custom_id)
        {
            return Some(channel_id);
        }
    }
    let message = interaction.message.as_ref()?;
    for embed in &message.embeds {
        if let Some(description) = embed.description.as_deref() {
            if let Some(channel_id) = parse_target_channel_from_text(description) {
                return Some(channel_id);
            }
        }
    }
    None
}

fn parse_target_channel_from_text(text: &str) -> Option<u64> {
    let marker = "Target channel";
    let index = text.find(marker)?;
    text[index + marker.len()..]
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse::<u64>()
        .ok()
}

fn interaction_component_custom_id(interaction: &Interaction) -> Option<&str> {
    match interaction.data.as_ref()? {
        twilight_model::application::interaction::InteractionData::MessageComponent(data) => {
            Some(data.custom_id.as_str())
        }
        _ => None,
    }
}

async fn ticket_cutover_check(env_file: PathBuf) -> ExitCode {
    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };
    let config = load.config;
    let state_dir = superbot_state_dir_from_env(&env_file);
    let state_path = state_dir.join("ticket_panel_state.json");

    println!("XIII Ticket Cutover Check");
    println!("Mode: READ ONLY / NO WRITES");
    println!("Discord login: DISABLED");
    println!("Discord writes: DISABLED");
    println!("Google calls: DISABLED");
    println!("Legacy DB writes: DISABLED");
    println!(
        "Legacy DB: {}",
        config.legacy_paths.ticket_db.resolved.display()
    );
    println!("Fresh state: {}", state_path.display());
    println!(
        "Google credentials: {}",
        config.tickets.google_credentials_file.redacted()
    );
    println!(
        "Google sheet id: {}",
        config.tickets.google_sheet_id.redacted()
    );
    println!(
        "Google sheet name: {}",
        config.tickets.google_sheet_name.redacted()
    );

    let mut report = Report::new();
    if state_path.is_file() {
        match fs::read_to_string(&state_path)
            .map_err(|err| format!("failed to read ticket panel state: {err}"))
            .and_then(|text| {
                serde_json::from_str::<xiii_tickets::state::TicketPanelState>(&text)
                    .map_err(|err| format!("failed to parse ticket panel state: {err}"))
            }) {
            Ok(state) => {
                report.ok("tickets", "fresh ticket panel state exists");
                if state.guild_id == config.core.guild_id {
                    report.ok(
                        "tickets",
                        format!("fresh panel guild_id = {}", state.guild_id),
                    );
                } else {
                    report.fail(
                        "tickets",
                        format!(
                            "fresh panel guild_id {} does not match XIII_GUILD_ID {}",
                            state.guild_id, config.core.guild_id
                        ),
                    );
                }
                if state.channel_id == 0 || state.panel_message_id == 0 {
                    report.fail("tickets", "fresh panel channel/message id must be non-zero");
                } else {
                    report.ok(
                        "tickets",
                        format!(
                            "fresh panel target channel_id={} message_id={}",
                            state.channel_id, state.panel_message_id
                        ),
                    );
                }
                if state.source != "fresh_bootstrap" {
                    report.warn(
                        "tickets",
                        format!(
                            "fresh panel source is {}; expected fresh_bootstrap",
                            state.source
                        ),
                    );
                }
            }
            Err(err) => report.fail("tickets", err),
        }
    } else {
        report.warn(
            "tickets",
            "fresh ticket panel state missing; run bootstrap-fresh-panels --modules tickets before enabling TICKETS_ENABLED",
        );
    }

    for (name, value) in [
        ("TICKET_PANEL_CHANNEL_ID", config.tickets.panel_channel_id),
        ("TICKET_OPEN_CATEGORY_ID", config.tickets.open_category_id),
        (
            "TICKET_TRANSCRIPT_CHANNEL_ID",
            config.tickets.transcript_channel_id,
        ),
        (
            "TICKET_OFFICER_REVIEW_CHANNEL_ID",
            config.tickets.officer_review_channel_id,
        ),
        ("TICKET_SUPPORT_ROLE_ID", config.tickets.support_role_id),
    ] {
        if value == 0 {
            report.fail("tickets", format!("{name} is missing or zero"));
        } else {
            report.ok("tickets", format!("{name} = {value}"));
        }
    }
    for (name, values) in [
        (
            "TICKET_GLOBAL_MODERATOR_ROLE_IDS",
            config.tickets.global_moderator_role_ids.as_slice(),
        ),
        (
            "TICKET_CUSTOM_COMMAND_ROLE_IDS",
            config.tickets.custom_command_role_ids.as_slice(),
        ),
        (
            "TICKET_ACCEPT_ROLE_IDS",
            config.tickets.accept_role_ids.as_slice(),
        ),
    ] {
        if values.is_empty() {
            report.warn("tickets", format!("{name} is empty"));
        } else {
            report.ok("tickets", format!("{name} count = {}", values.len()));
        }
    }
    report.warn(
        "tickets",
        "legacy text commands require MESSAGE_CONTENT intent; confirm this is enabled for the Superbot application before ticket cutover",
    );

    match xiii_tickets::repository::LegacySqliteTicketRepository::open_existing_read_only(
        &config.legacy_paths.ticket_db.resolved,
    )
    .await
    {
        Ok(repository) => match repository.counts().await {
            Ok(counts) => {
                report.ok("tickets", format!("counters rows = {}", counts.counters));
                report.ok("tickets", format!("tickets rows = {}", counts.tickets));
                report.ok(
                    "tickets",
                    format!("processed_forms rows = {}", counts.processed_forms),
                );
                report.ok(
                    "tickets",
                    format!(
                        "processed_form_signatures rows = {}",
                        counts.processed_form_signatures
                    ),
                );
                report.ok("tickets", format!("bot_state rows = {}", counts.bot_state));
                report.ok("tickets", format!("open tickets = {}", counts.open_tickets));
                if counts.reserved_tickets > 0 {
                    report.warn(
                        "tickets",
                        format!(
                            "reserved tickets = {}; cutover should verify no half-open ticket creation is in progress",
                            counts.reserved_tickets
                        ),
                    );
                } else {
                    report.ok("tickets", "reserved tickets = 0");
                }
                match repository.counter_rows().await {
                    Ok(rows) if rows.is_empty() => {
                        report.warn("tickets", "counters table has no rows")
                    }
                    Ok(rows) => {
                        for row in rows {
                            report.ok("tickets", format!("counter {} = {}", row.name, row.value));
                        }
                    }
                    Err(err) => report.fail("tickets", err),
                }
                match repository.status_counts().await {
                    Ok(rows) if rows.is_empty() => {
                        report.warn("tickets", "tickets table has no status rows")
                    }
                    Ok(rows) => {
                        for row in rows {
                            report.ok(
                                "tickets",
                                format!("status {} rows = {}", row.status, row.count),
                            );
                        }
                    }
                    Err(err) => report.fail("tickets", err),
                }
                match repository.bot_state_rows().await {
                    Ok(rows) if rows.is_empty() => report.warn("tickets", "bot_state has no rows"),
                    Ok(rows) => {
                        for row in rows {
                            let value = if is_secret_like_name(&row.key) {
                                "<SET>".to_owned()
                            } else {
                                row.value
                            };
                            report.ok("tickets", format!("bot_state {} = {}", row.key, value));
                        }
                    }
                    Err(err) => report.fail("tickets", err),
                }
            }
            Err(err) => report.fail("tickets", err),
        },
        Err(err) => report.fail("tickets", err),
    }

    report.ok(
        "tickets",
        "runtime wiring is READY_FULL: ticket handlers, Discord IO, Google officer-review poller, and safe HTML transcripts are wired behind runtime gates",
    );
    print_report("Ticket Cutover Check", &report);
    report_exit_code(&report)
}

async fn append_temp_voice_db_status(
    report: &mut Report,
    config: &SuperbotConfig,
    _strict_enabled: bool,
) {
    match read_temp_voice_cutover_state(&config.legacy_paths.temp_voice_db.resolved).await {
        Ok(state) => {
            if state.guild_settings_count > 0 {
                report.ok(
                    "temp_voice",
                    format!(
                        "hub configured in guild_settings rows={}",
                        state.guild_settings_count
                    ),
                );
                for (guild_id, hub_channel_id) in state.hubs {
                    report.ok(
                        "temp_voice",
                        format!("hub guild_id={guild_id} hub_channel_id={hub_channel_id}"),
                    );
                }
            } else if config.modules.temp_voice {
                report.fail("temp_voice", "hub is not configured in guild_settings");
            } else {
                report.warn(
                    "temp_voice",
                    "hub is not configured in guild_settings while module is disabled",
                );
            }
            report.ok(
                "temp_voice",
                format!(
                    "tracked temp_voice_channels rows={}",
                    state.temp_voice_channels_count
                ),
            );
        }
        Err(err) if config.modules.temp_voice => {
            report.fail(
                "temp_voice",
                format!("failed to inspect temp voice DB: {err}"),
            );
        }
        Err(err) => {
            report.warn(
                "temp_voice",
                format!("temp voice DB inspection skipped while disabled: {err}"),
            );
        }
    }
}

#[derive(Debug, Clone)]
struct TempVoiceCutoverState {
    guild_settings_count: i64,
    temp_voice_channels_count: i64,
    hubs: Vec<(String, String)>,
}

fn bootstrap_all_permission_failure(
    allow_discord_read: bool,
    allow_discord_write: bool,
    confirm_bootstrap: bool,
) -> Option<Report> {
    let mut report = Report::new();
    if !allow_discord_read {
        report.fail(
            "safety",
            "--allow-discord-read is required before bootstrap can inspect Discord targets",
        );
    }
    if !allow_discord_write {
        report.fail(
            "safety",
            "--allow-discord-write is required before any fresh panel can be created",
        );
    }
    if !confirm_bootstrap {
        report.fail("safety", "--confirm-bootstrap is required before loading a Discord token or creating panel messages");
    }
    report.has_failures().then_some(report)
}

fn run_superbot_permission_failure(
    allow_discord_read: bool,
    allow_discord_write: bool,
    confirm_run_superbot: bool,
) -> Option<Report> {
    let mut report = Report::new();
    if !allow_discord_read {
        report.fail(
            "safety",
            "--allow-discord-read is required before run-superbot can load a Discord token",
        );
    }
    if !allow_discord_write {
        report.fail(
            "safety",
            "--allow-discord-write is required before run-superbot can start write-capable modules",
        );
    }
    if !confirm_run_superbot {
        report.fail(
            "safety",
            "--confirm-run-superbot is required before the Superbot runtime can start",
        );
    }
    report.has_failures().then_some(report)
}

async fn read_voice_active_session_count(path: &Path) -> Result<i64, String> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|err| format!("failed to open {} read-only: {err}", path.display()))?;
    sqlx::query("PRAGMA query_only=ON")
        .execute(&pool)
        .await
        .map_err(|err| format!("failed to set PRAGMA query_only=ON: {err}"))?;
    let row = sqlx::query("SELECT COUNT(*) AS count FROM active_voice_sessions")
        .fetch_one(&pool)
        .await
        .map_err(|err| format!("failed to count active_voice_sessions: {err}"))?;
    Ok(row.get::<i64, _>("count"))
}

fn read_voice_cutover_state(
    path: &Path,
) -> Result<Option<xiii_voice_activity::state::VoiceActivityCutoverState>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let state =
        serde_json::from_str::<xiii_voice_activity::state::VoiceActivityCutoverState>(&text)
            .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    Ok(Some(state))
}

fn valid_voice_cutover_state(
    state: &xiii_voice_activity::state::VoiceActivityCutoverState,
    guild_id: u64,
) -> bool {
    state.guild_id == guild_id
        && state.source == "voice-finalize-cutover"
        && state.policy == "closed_active_at_cutover"
        && !state.cutover_at_utc.trim().is_empty()
}

fn write_voice_cutover_state(
    path: &Path,
    state: &xiii_voice_activity::state::VoiceActivityCutoverState,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(|err| format!("failed to render cutover state JSON: {err}"))?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, json.as_bytes())
        .map_err(|err| format!("failed to write {}: {err}", temp.display()))?;
    fs::rename(&temp, path)
        .map_err(|err| format!("failed to replace {}: {err}", path.display()))?;
    Ok(())
}

fn duration_between_iso_clamped(started_at: &str, ended_at: &str) -> i64 {
    let Some(start) = chrono::DateTime::parse_from_rfc3339(started_at).ok() else {
        return 0;
    };
    let Some(end) = chrono::DateTime::parse_from_rfc3339(ended_at).ok() else {
        return 0;
    };
    (end - start).num_seconds().max(0)
}

async fn voice_activity_runtime_cutover_guard(
    config: &SuperbotConfig,
    state_dir: &Path,
) -> Result<(), String> {
    let active_count = read_voice_active_session_count(&config.legacy_paths.voice_db.resolved)
        .await
        .map_err(|err| format!("failed to inspect active sessions: {err}"))?;
    if active_count <= 0 {
        return Ok(());
    }
    let state_path = state_dir.join("voice_activity_cutover_state.json");
    match read_voice_cutover_state(&state_path)? {
        Some(state) if valid_voice_cutover_state(&state, config.core.guild_id) => Ok(()),
        _ => Err(format!(
            "active_voice_sessions rows = {active_count}; run voice-finalize-cutover with --allow-legacy-db-write --confirm-close-active-voice-sessions before enabling voice_activity"
        )),
    }
}

async fn add_db_source_report(
    report: &mut Report,
    scope: &str,
    path: &Path,
    tables: &[(&str, bool)],
) {
    report.ok(scope, format!("source path = {}", path.display()));
    if path.display().to_string().contains("XIII_BOTS_FULL_COPY") {
        report.warn(
            scope,
            "source path points at XIII_BOTS_FULL_COPY; OK for local validation, replace with VPS legacy path before production",
        );
    }
    if tables.is_empty() {
        if path.is_dir() {
            report.ok(scope, "legacy data directory exists");
        } else {
            report.fail(
                scope,
                format!("legacy data directory missing: {}", path.display()),
            );
        }
        return;
    }
    if !path.is_file() {
        report.fail(scope, format!("legacy DB missing: {}", path.display()));
        return;
    }
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.len() == 0 {
            report.fail(
                scope,
                "legacy DB file is zero bytes; refusing likely new empty DB",
            );
        } else {
            report.ok(scope, format!("legacy DB size bytes = {}", metadata.len()));
        }
    }
    let pool = match open_sqlite_read_only_pool(path).await {
        Ok(pool) => pool,
        Err(err) => {
            report.fail(scope, err);
            return;
        }
    };
    let mut total_rows = 0i64;
    for (table, required) in tables {
        match sqlite_table_count(&pool, table).await {
            Ok(Some(count)) => {
                total_rows += count;
                report.ok(scope, format!("{table} rows = {count}"));
            }
            Ok(None) if *required => report.fail(scope, format!("required table {table} missing")),
            Ok(None) => report.warn(scope, format!("optional table {table} missing")),
            Err(err) => report.fail(scope, err),
        }
    }
    if total_rows == 0 {
        report.fail(
            scope,
            "all inspected legacy tables are empty; this looks like a fresh DB, not legacy source-of-truth",
        );
    }
}

fn add_production_path_report(report: &mut Report, scope: &str, label: &str, path: &Path) {
    report.ok(scope, format!("{label} = {}", path.display()));
    match production_path_issue(path) {
        Some(issue) => report.fail(scope, format!("{label} {issue}")),
        None => report.ok(scope, format!("{label} looks production-shaped")),
    }
}

fn production_path_issue(path: &Path) -> Option<&'static str> {
    let normalized = normalized_path_string(path);
    if normalized.contains("xiii_bots_full_copy") {
        Some("points at XIII_BOTS_FULL_COPY; replace it with the real VPS legacy path before production")
    } else if normalized.contains(":\\") {
        Some("looks like a Windows path; production env files must use Linux VPS paths")
    } else if !normalized.starts_with('\\') {
        Some("is relative; production env files should use explicit VPS paths")
    } else {
        None
    }
}

async fn open_sqlite_read_only_pool(path: &Path) -> Result<sqlx::SqlitePool, String> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|err| format!("failed to open {} read-only: {err}", path.display()))?;
    sqlx::query("PRAGMA query_only=ON")
        .execute(&pool)
        .await
        .map_err(|err| {
            format!(
                "failed to set PRAGMA query_only=ON for {}: {err}",
                path.display()
            )
        })?;
    Ok(pool)
}

async fn sqlite_table_count(pool: &sqlx::SqlitePool, table: &str) -> Result<Option<i64>, String> {
    let exists = sqlx::query(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=? COLLATE NOCASE LIMIT 1",
    )
    .bind(table)
    .fetch_optional(pool)
    .await
    .map_err(|err| format!("failed to inspect sqlite_master for table {table}: {err}"))?
    .is_some();
    if !exists {
        return Ok(None);
    }
    let sql = format!(
        "SELECT COUNT(*) AS count FROM \"{}\"",
        table.replace('"', "\"\"")
    );
    let row = sqlx::query(&sql)
        .fetch_one(pool)
        .await
        .map_err(|err| format!("failed to count table {table}: {err}"))?;
    Ok(Some(row.get::<i64, _>("count")))
}

fn validate_fresh_state_json(
    path: &Path,
    expected_guild_id: u64,
    min_message_targets: usize,
) -> Result<Vec<(String, u64, u64)>, String> {
    if !path.is_file() {
        return Err(format!("fresh state missing: {}", path.display()));
    }
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read fresh state {}: {err}", path.display()))?;
    let value = serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|err| format!("failed to parse fresh state {}: {err}", path.display()))?;
    let guild_id = json_u64(&value, "guild_id")
        .ok_or_else(|| format!("fresh state {} missing numeric guild_id", path.display()))?;
    if guild_id != expected_guild_id {
        return Err(format!(
            "fresh state {} guild_id {guild_id} does not match XIII_GUILD_ID {expected_guild_id}",
            path.display()
        ));
    }
    let mut targets = Vec::new();
    collect_json_message_targets("$", &value, &mut targets);
    if targets.len() < min_message_targets {
        return Err(format!(
            "fresh state {} has {} message targets, expected at least {min_message_targets}",
            path.display(),
            targets.len()
        ));
    }
    if targets
        .iter()
        .any(|(_, channel_id, message_id)| *channel_id == 0 || *message_id == 0)
    {
        return Err(format!(
            "fresh state {} contains zero channel/message id",
            path.display()
        ));
    }
    Ok(targets)
}

fn json_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(|item| item.as_u64().or_else(|| item.as_str()?.parse().ok()))
}

fn collect_json_message_targets(
    label: &str,
    value: &serde_json::Value,
    targets: &mut Vec<(String, u64, u64)>,
) {
    match value {
        serde_json::Value::Object(map) => {
            let channel_id = value
                .get("channel_id")
                .and_then(|item| item.as_u64().or_else(|| item.as_str()?.parse().ok()));
            let message_id = value
                .get("message_id")
                .or_else(|| value.get("panel_message_id"))
                .and_then(|item| item.as_u64().or_else(|| item.as_str()?.parse().ok()));
            if let (Some(channel_id), Some(message_id)) = (channel_id, message_id) {
                targets.push((label.to_owned(), channel_id, message_id));
            }
            for (key, child) in map {
                collect_json_message_targets(&format!("{label}.{key}"), child, targets);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_json_message_targets(&format!("{label}[{index}]"), child, targets);
            }
        }
        _ => {}
    }
}

async fn add_ticket_final_readiness(report: &mut Report, config: &SuperbotConfig) {
    match xiii_tickets::repository::LegacySqliteTicketRepository::open_existing_read_only(
        &config.legacy_paths.ticket_db.resolved,
    )
    .await
    {
        Ok(repo) => {
            match repo.counter_rows().await {
                Ok(counters) => {
                    let nonzero = counters.iter().filter(|row| row.value > 0).count();
                    if nonzero == 0 {
                        report.fail(
                            "tickets",
                            "ticket counters are all zero; refusing possible reset",
                        );
                    } else {
                        report.ok("tickets", format!("nonzero ticket counters = {nonzero}"));
                    }
                }
                Err(err) => report.fail("tickets", err),
            }
            match repo.counts().await {
                Ok(counts) => {
                    if counts.open_tickets > 0 || counts.reserved_tickets > 0 {
                        report.warn(
                            "tickets",
                            format!(
                                "open/reserved tickets at cutover: open={} reserved={}",
                                counts.open_tickets, counts.reserved_tickets
                            ),
                        );
                    } else {
                        report.ok("tickets", "open/reserved tickets = 0/0");
                    }
                }
                Err(err) => report.fail("tickets", err),
            }
        }
        Err(err) => report.fail("tickets", err),
    }
}

async fn add_voice_final_readiness(
    report: &mut Report,
    config: &SuperbotConfig,
    state_dir: &Path,
    env_file: &Path,
) {
    let active_count =
        match read_voice_active_session_count(&config.legacy_paths.voice_db.resolved).await {
            Ok(count) => count,
            Err(err) => {
                report.fail("voice_activity", err);
                return;
            }
        };
    let state_path = state_dir.join("voice_activity_cutover_state.json");
    let cutover_state = read_voice_cutover_state(&state_path)
        .ok()
        .flatten()
        .filter(|state| valid_voice_cutover_state(state, config.core.guild_id));
    if active_count == 0 {
        if let Some(state) = cutover_state {
            report.ok(
                "voice_activity",
                format!(
                    "active_voice_sessions rows = 0; finalized cutover policy={} at {}",
                    state.policy, state.cutover_at_utc
                ),
            );
        } else {
            report.ok("voice_activity", "active_voice_sessions rows = 0");
        }
    } else if cutover_state.is_some() {
        report.fail(
            "voice_activity",
            format!(
                "active_voice_sessions rows = {active_count} even though cutover state exists; inspect manually before deployment"
            ),
        );
    } else {
        report.fail(
            "voice_activity",
            format!(
                "active_voice_sessions rows = {active_count}; intentionally close them with `cargo run -- voice-finalize-cutover --env-file {} --allow-legacy-db-write --confirm-close-active-voice-sessions`",
                env_file.display()
            ),
        );
    }
}

async fn add_discord_panel_ownership_checks(
    report: &mut Report,
    env_file: &Path,
    config: &SuperbotConfig,
    state_dir: &Path,
) {
    let token = match read_secret_from_env_file(env_file, "DISCORD_TOKEN") {
        Ok(token) => token,
        Err(err) => {
            report.fail("discord", err);
            return;
        }
    };
    let client = DiscordHttpClient::new(token);
    let current = match fetch_current_user_with_retry(&client, report).await {
        Ok(user) => user,
        Err(err) => {
            report.fail("discord", err);
            return;
        }
    };
    let files = [
        "clanlist_panel_state.json",
        "vacation_panel_state.json",
        "discipline_panel_state.json",
        "voice_activity_panel_state.json",
        "ticket_panel_state.json",
    ];
    for file in files {
        let path = state_dir.join(file);
        let Ok(targets) = validate_fresh_state_json(&path, config.core.guild_id, 1) else {
            continue;
        };
        for (label, channel_id, message_id) in targets {
            match fetch_target_message_with_retry(
                &client,
                channel_id,
                message_id,
                report,
                &format!("{file}:{label}"),
            )
            .await
            {
                Ok(message) if message.author.id == current.id => report.ok(
                    "discord",
                    format!("{file}:{label} message exists and is authored by current bot"),
                ),
                Ok(message) => report.fail(
                    "discord",
                    format!(
                        "{file}:{label} message author {} does not match current bot {}",
                        message.author.id.get(),
                        current.id.get()
                    ),
                ),
                Err(err) => report.fail("discord", err),
            }
        }
    }
}

async fn read_temp_voice_cutover_state(path: &Path) -> Result<TempVoiceCutoverState, String> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|err| format!("failed to open {} read-only: {err}", path.display()))?;
    sqlx::query("PRAGMA query_only=ON")
        .execute(&pool)
        .await
        .map_err(|err| format!("failed to set PRAGMA query_only=ON: {err}"))?;

    let guild_settings_count = sqlx::query("SELECT COUNT(*) AS count FROM guild_settings")
        .fetch_one(&pool)
        .await
        .map_err(|err| format!("failed to count guild_settings: {err}"))?
        .get::<i64, _>("count");
    let temp_voice_channels_count =
        sqlx::query("SELECT COUNT(*) AS count FROM temp_voice_channels")
            .fetch_one(&pool)
            .await
            .map_err(|err| format!("failed to count temp_voice_channels: {err}"))?
            .get::<i64, _>("count");
    let rows = sqlx::query(
        "SELECT CAST(guild_id AS TEXT) AS guild_id, CAST(hub_channel_id AS TEXT) AS hub_channel_id FROM guild_settings ORDER BY guild_id",
    )
    .fetch_all(&pool)
    .await
    .map_err(|err| format!("failed to read guild_settings hubs: {err}"))?;
    let hubs = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("guild_id"),
                row.get::<String, _>("hub_channel_id"),
            )
        })
        .collect();

    Ok(TempVoiceCutoverState {
        guild_settings_count,
        temp_voice_channels_count,
        hubs,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SuperbotModuleKind {
    Clanlist,
    TempVoice,
    Vacation,
    Discipline,
    Recruit,
    VoiceActivity,
    Tickets,
}

impl SuperbotModuleKind {
    fn all() -> Vec<Self> {
        vec![
            Self::Clanlist,
            Self::TempVoice,
            Self::Vacation,
            Self::Discipline,
            Self::Recruit,
            Self::VoiceActivity,
            Self::Tickets,
        ]
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "clanlist" => Some(Self::Clanlist),
            "tempvoice" | "temp_voice" => Some(Self::TempVoice),
            "vacation" => Some(Self::Vacation),
            "discipline" | "punishments" => Some(Self::Discipline),
            "recruit" => Some(Self::Recruit),
            "voice" | "voice_activity" => Some(Self::VoiceActivity),
            "tickets" | "ticket" => Some(Self::Tickets),
            _ => None,
        }
    }

    fn id(self) -> ModuleId {
        match self {
            Self::Clanlist => ModuleId::Clanlist,
            Self::TempVoice => ModuleId::TempVoice,
            Self::Vacation => ModuleId::Vacation,
            Self::Discipline => ModuleId::Discipline,
            Self::Recruit => ModuleId::Recruit,
            Self::VoiceActivity => ModuleId::VoiceActivity,
            Self::Tickets => ModuleId::Tickets,
        }
    }

    fn name(self) -> &'static str {
        self.id().as_str()
    }

    fn enabled(self, config: &SuperbotConfig) -> bool {
        match self {
            Self::Clanlist => config.modules.clanlist,
            Self::TempVoice => config.modules.temp_voice,
            Self::Vacation => config.modules.vacation,
            Self::Discipline => config.modules.discipline,
            Self::Recruit => config.modules.recruit,
            Self::VoiceActivity => config.modules.voice_activity,
            Self::Tickets => config.modules.tickets,
        }
    }

    fn readiness(self) -> ModuleReadiness {
        match self {
            Self::Clanlist
            | Self::TempVoice
            | Self::Vacation
            | Self::Discipline
            | Self::Recruit
            | Self::VoiceActivity
            | Self::Tickets => ModuleReadiness::ReadyFull,
        }
    }

    fn blockers(self) -> &'static [&'static str] {
        match self {
            Self::Clanlist => &[
                "Google Sheets Steam source remains legacy-cache fallback unless explicitly implemented later",
            ],
            Self::TempVoice => &[
                "requires temp-voice-bot.service stopped before TEMP_VOICE_ENABLED=true",
                "deletes only channel IDs present in legacy temp_voice_channels",
            ],
            Self::Vacation => &[
                "requires xiii-vacation-bot.service stopped before VACATION_ENABLED=true",
                "uses fresh Superbot-owned vacation panel state only; old panels are reference-only",
            ],
            Self::Discipline => &[
                "requires xiii-discipline-bot.service stopped before DISCIPLINE_ENABLED=true",
                "uses fresh Superbot-owned discipline board state only; old board is reference-only",
                "issue/remove/history flows use legacy discipline.sqlite transactionally with Discord side effects behind runtime gates",
            ],
            Self::Recruit => &[
                "requires xiii-recruit-bot.service stopped before RECRUIT_ENABLED=true",
                "decision panels are idempotent through last_decision_message_id/channel_id in recruits.db",
            ],
            Self::VoiceActivity => &[
                "requires xiii-voice-activity-bot.service stopped before VOICE_ACTIVITY_ENABLED=true",
                "voice-cutover-check must show zero active DB sessions or a finalized voice_activity_cutover_state.json from voice-finalize-cutover",
                "uses fresh Superbot-owned voice_activity_panel_state.json only; old stats panel is reference-only",
            ],
            Self::Tickets => &[
                "requires xiii-ticketbot.service stopped before TICKETS_ENABLED=true",
                "requires MESSAGE_CONTENT intent for legacy !panel, !accept/!принять, and !reject/!отклонить commands",
                "uses safe Rust HTML transcript attachments as the production substitute for Python chat_exporter output",
                "Google Sheets poller is read-only and marks processed rows only after officer-review send succeeds",
            ],
        }
    }

    fn spec(self) -> ModuleSpec {
        match self {
            Self::Clanlist => ModuleSpec {
                env_flag: "CLANLIST_ENABLED",
                service_name: "xiii-clanlist.service",
                fresh_state_file: Some("clanlist_panel_state.json"),
                panel_description: Some("main/admin/Steam roster panels"),
                risk_note: "working migrated module; edits only fresh Superbot-owned panel IDs",
            },
            Self::TempVoice => ModuleSpec {
                env_flag: "TEMP_VOICE_ENABLED",
                service_name: "temp-voice-bot.service",
                fresh_state_file: None,
                panel_description: None,
                risk_note: "can delete voice channels; only DB-owned channel IDs may be deleted",
            },
            Self::Vacation => ModuleSpec {
                env_flag: "VACATION_ENABLED",
                service_name: "xiii-vacation-bot.service",
                fresh_state_file: Some("vacation_panel_state.json"),
                panel_description: Some("vacation request and active vacations panels"),
                risk_note: "role add/remove and expiry DMs require single writer and VACATION_ROLE_ID split",
            },
            Self::Discipline => ModuleSpec {
                env_flag: "DISCIPLINE_ENABLED",
                service_name: "xiii-discipline-bot.service",
                fresh_state_file: Some("discipline_panel_state.json"),
                panel_description: Some("discipline board"),
                risk_note: "policy-sensitive escalation, timeout, removal, logs, and DMs",
            },
            Self::Recruit => ModuleSpec {
                env_flag: "RECRUIT_ENABLED",
                service_name: "xiii-recruit-bot.service",
                fresh_state_file: None,
                panel_description: None,
                risk_note: "active recruit decision state and automatic pings must remain idempotent",
            },
            Self::VoiceActivity => ModuleSpec {
                env_flag: "VOICE_ACTIVITY_ENABLED",
                service_name: "xiii-voice-activity-bot.service",
                fresh_state_file: Some("voice_activity_panel_state.json"),
                panel_description: Some("voice public stats panel"),
                risk_note: "highest-risk active voice session cutover; migrate last",
            },
            Self::Tickets => ModuleSpec {
                env_flag: "TICKETS_ENABLED",
                service_name: "xiii-ticketbot.service",
                fresh_state_file: Some("ticket_panel_state.json"),
                panel_description: Some("ticket panel"),
                risk_note: "ticket counters, channels, permissions, transcripts, and Google dedupe must not reset",
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ModuleSpec {
    env_flag: &'static str,
    service_name: &'static str,
    fresh_state_file: Option<&'static str>,
    panel_description: Option<&'static str>,
    risk_note: &'static str,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum ModuleReadiness {
    ReadyFull,
    Partial,
    Blocked,
}

impl ModuleReadiness {
    fn as_str(self) -> &'static str {
        match self {
            Self::ReadyFull => "READY_FULL",
            Self::Partial => "PARTIAL",
            Self::Blocked => "BLOCKED",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct FreshPanelPlanRow {
    module: &'static str,
    state_file: String,
    action: String,
    readiness: &'static str,
    note: String,
}

#[derive(Debug, Clone, Copy)]
enum SelectionMode {
    AllWhenEmpty,
    EnabledWhenEmpty,
}

fn selected_modules(
    requested: &[String],
    config: &SuperbotConfig,
    mode: SelectionMode,
) -> Result<Vec<SuperbotModuleKind>, String> {
    if requested.is_empty() {
        return Ok(match mode {
            SelectionMode::AllWhenEmpty => SuperbotModuleKind::all(),
            SelectionMode::EnabledWhenEmpty => SuperbotModuleKind::all()
                .into_iter()
                .filter(|module| module.enabled(config))
                .collect(),
        });
    }
    let mut selected = Vec::new();
    for item in requested {
        let module =
            SuperbotModuleKind::parse(item).ok_or_else(|| format!("unknown module '{item}'"))?;
        if !selected.contains(&module) {
            selected.push(module);
        }
    }
    Ok(selected)
}

fn build_module_status_report(
    config: &SuperbotConfig,
    state_dir: &Path,
    strict_enabled: bool,
) -> Report {
    let mut report = Report::new();
    let manifests = module_manifests();
    for module in SuperbotModuleKind::all() {
        let enabled = module.enabled(config);
        let readiness = module.readiness();
        let manifest = manifests.iter().find(|manifest| manifest.id == module.id());
        if enabled {
            report.ok(module.name(), format!("{}=true", module.spec().env_flag));
        } else {
            report.warn(module.name(), format!("{}=false", module.spec().env_flag));
        }
        match readiness {
            ModuleReadiness::ReadyFull => {
                report.ok(module.name(), format!("readiness={}", readiness.as_str()))
            }
            ModuleReadiness::Partial | ModuleReadiness::Blocked if enabled && strict_enabled => {
                report.fail(
                    module.name(),
                    format!(
                        "readiness={} so this module must not be enabled for cutover",
                        readiness.as_str()
                    ),
                );
            }
            ModuleReadiness::Partial | ModuleReadiness::Blocked => {
                report.warn(module.name(), format!("readiness={}", readiness.as_str()))
            }
        }
        report.warn(
            module.name(),
            format!(
                "writer_allowed={}",
                enabled && readiness == ModuleReadiness::ReadyFull
            ),
        );
        for blocker in module.blockers() {
            if readiness == ModuleReadiness::ReadyFull {
                report.warn(module.name(), format!("operational note: {blocker}"));
            } else if enabled && strict_enabled {
                report.fail(module.name(), format!("blocker: {blocker}"));
            } else {
                report.warn(module.name(), format!("blocker: {blocker}"));
            }
        }
        if let Some(manifest) = manifest {
            let commands = manifest
                .slash_commands
                .iter()
                .map(|command| command.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let jobs = manifest
                .scheduler_jobs
                .iter()
                .map(|job| job.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            report.ok(
                module.name(),
                format!(
                    "slash_commands={}",
                    if commands.is_empty() { "-" } else { &commands }
                ),
            );
            report.ok(
                module.name(),
                format!(
                    "scheduled_jobs={}",
                    if jobs.is_empty() { "-" } else { &jobs }
                ),
            );
        }
        if let Some(path) = legacy_path_for_module(module, config) {
            if path.exists() {
                report.ok(
                    module.name(),
                    format!("legacy state exists: {}", path.display()),
                );
            } else if enabled || strict_enabled {
                report.fail(
                    module.name(),
                    format!("legacy state missing: {}", path.display()),
                );
            } else {
                report.warn(
                    module.name(),
                    format!("legacy state missing while disabled: {}", path.display()),
                );
            }
        }
        if let Some(file) = module.spec().fresh_state_file {
            let path = state_dir.join(file);
            if path.is_file() {
                report.ok(
                    module.name(),
                    format!("fresh state exists: {}", path.display()),
                );
            } else if enabled {
                report.fail(
                    module.name(),
                    format!("fresh state missing: {}", path.display()),
                );
            } else {
                report.warn(
                    module.name(),
                    format!("fresh state missing while disabled: {}", path.display()),
                );
            }
        }
    }
    if config.vacation.vacation_role_id == config.voice_activity.vacation_marker_role_id {
        report.fail(
            "config",
            "VACATION_ROLE_ID and VOICE_VACATION_MARKER_ROLE_ID are collapsed; they must stay separate",
        );
    } else {
        report.ok(
            "config",
            "VACATION_ROLE_ID and VOICE_VACATION_MARKER_ROLE_ID are distinct",
        );
    }
    report
}

fn legacy_path_for_module(module: SuperbotModuleKind, config: &SuperbotConfig) -> Option<&Path> {
    Some(match module {
        SuperbotModuleKind::Clanlist => config.legacy_paths.clanlist_data_dir.resolved.as_path(),
        SuperbotModuleKind::TempVoice => config.legacy_paths.temp_voice_db.resolved.as_path(),
        SuperbotModuleKind::Vacation => config.legacy_paths.vacation_db.resolved.as_path(),
        SuperbotModuleKind::Discipline => config.legacy_paths.discipline_db.resolved.as_path(),
        SuperbotModuleKind::Recruit => config.legacy_paths.recruit_db.resolved.as_path(),
        SuperbotModuleKind::VoiceActivity => config.legacy_paths.voice_db.resolved.as_path(),
        SuperbotModuleKind::Tickets => config.legacy_paths.ticket_db.resolved.as_path(),
    })
}

fn print_readiness_matrix(config: &SuperbotConfig) {
    println!("Readiness Matrix");
    println!("Module          Enabled  Readiness   Writer Allowed");
    for module in SuperbotModuleKind::all() {
        let enabled = module.enabled(config);
        let readiness = module.readiness();
        println!(
            "{:<15} {:<7} {:<11} {}",
            module.name(),
            enabled,
            readiness.as_str(),
            enabled && readiness == ModuleReadiness::ReadyFull
        );
    }
    println!();
}

fn print_module_routes_and_jobs(modules: &[SuperbotModuleKind]) {
    let manifests = module_manifests();
    println!("Module Routes And Jobs");
    for module in modules {
        let Some(manifest) = manifests.iter().find(|manifest| manifest.id == module.id()) else {
            println!("  - {}: manifest missing", module.name());
            continue;
        };
        let commands = manifest
            .slash_commands
            .iter()
            .map(|command| command.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let components = manifest
            .component_routes
            .iter()
            .map(|route| route.custom_id_pattern.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let jobs = manifest
            .scheduler_jobs
            .iter()
            .map(|job| job.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  - {} readiness={} slash_commands={} component_routes={} scheduler_jobs={}",
            module.name(),
            module.readiness().as_str(),
            if commands.is_empty() { "-" } else { &commands },
            if components.is_empty() {
                "-"
            } else {
                &components
            },
            if jobs.is_empty() { "-" } else { &jobs }
        );
        for blocker in module.blockers() {
            if module.readiness() == ModuleReadiness::ReadyFull {
                println!("      note: {blocker}");
            } else {
                println!("      blocker: {blocker}");
            }
        }
    }
    println!();
}

fn evaluate_old_services_dir(
    modules: &[SuperbotModuleKind],
    old_services_dir: Option<&Path>,
) -> Report {
    let mut report = Report::new();
    let Some(dir) = old_services_dir else {
        report.fail(
            "service_guard",
            "--old-services-dir is required when --require-old-services-stopped is used",
        );
        return report;
    };
    for module in modules {
        let file = dir.join(format!("{}.txt", module.spec().service_name));
        let service_report = evaluate_old_service_guard(true, Some(&file));
        report.extend(service_report);
    }
    report
}

fn superbot_state_dir_from_env(env_file: &Path) -> PathBuf {
    read_env_value(env_file, "SUPERBOT_STATE_DIR")
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"))
}

fn superbot_require_old_services_stopped_from_env(env_file: &Path) -> bool {
    read_env_value(env_file, "SUPERBOT_REQUIRE_OLD_SERVICES_STOPPED")
        .map(|value| parse_truthy_bool(&value))
        .unwrap_or(true)
}

fn parse_truthy_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn read_env_value(path: &Path, name: &str) -> Option<String> {
    let iter = dotenvy::from_path_iter(path).ok()?;
    for item in iter.flatten() {
        let (key, value) = item;
        if key == name {
            return Some(value);
        }
    }
    None
}

fn join_module_names(modules: &[SuperbotModuleKind]) -> String {
    modules
        .iter()
        .map(|module| module.name())
        .collect::<Vec<_>>()
        .join(",")
}

fn render_fresh_panel_plan_text(
    dry_run: bool,
    state_dir: &Path,
    rows: &[FreshPanelPlanRow],
    report: &Report,
) -> String {
    let mut text = String::new();
    text.push_str("XIII Superbot Fresh Panel Bootstrap\n");
    text.push_str(&format!(
        "Mode: {}\n",
        if dry_run {
            "DRY RUN / NO WRITES"
        } else {
            "GATED WRITE PLAN"
        }
    ));
    text.push_str(&format!(
        "Discord writes: {}\n",
        if dry_run { "DISABLED" } else { "GATED" }
    ));
    text.push_str("Old panel deletion: DISABLED\n");
    text.push_str("Legacy DB/JSON writes: DISABLED\n");
    text.push_str(&format!("State dir: {}\n\n", state_dir.display()));
    text.push_str("Planned panels:\n");
    for row in rows {
        text.push_str(&format!(
            "  - {} readiness={} action={} state={} note={}\n",
            row.module, row.readiness, row.action, row.state_file, row.note
        ));
    }
    text.push('\n');
    text.push_str(&render_report_text("Fresh Panel Bootstrap", report));
    text
}

fn render_fresh_panel_plan_json(
    dry_run: bool,
    state_dir: &Path,
    rows: &[FreshPanelPlanRow],
    report: &Report,
) -> String {
    let value = serde_json::json!({
        "mode": if dry_run { "dry_run" } else { "gated_write_plan" },
        "safety": {
            "discord_writes": !dry_run,
            "old_panel_deletion": false,
            "legacy_db_json_writes": false
        },
        "state_dir": state_dir.display().to_string(),
        "planned_panels": rows,
        "warnings": report.items.iter().filter(|item| item.severity == Severity::Warn).map(|item| format!("{}: {}", item.scope, item.message)).collect::<Vec<_>>(),
        "failures": report.items.iter().filter(|item| item.severity == Severity::Fail).map(|item| format!("{}: {}", item.scope, item.message)).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&value)
        .unwrap_or_else(|err| format!("{{\"failures\":[\"failed to render JSON: {err}\"]}}"))
}

struct VacationBootstrapSummary {
    request_message_id: u64,
    active_message_id: u64,
}

async fn bootstrap_vacation_panels(
    client: &DiscordHttpClient,
    config: &SuperbotConfig,
    state_dir: &Path,
    bot_user_id: u64,
) -> Result<VacationBootstrapSummary, String> {
    let state_path = state_dir.join("vacation_panel_state.json");
    validate_state_output_path(&state_path, config)?;
    fs::create_dir_all(state_dir)
        .map_err(|err| format!("failed to create state dir {}: {err}", state_dir.display()))?;

    let request_embed = embed_with_appearance(
        xiii_vacation::render::request_panel_title(),
        xiii_vacation::render::REQUEST_PANEL_DESCRIPTION,
        xiii_vacation::render::LEGACY_PANEL_COLOR,
        Some(xiii_vacation::render::LEGACY_FOOTER),
        false,
    );
    let request_components = vec![action_row(vec![button(
        xiii_vacation::interactions::APPLY_BUTTON_ID,
        xiii_vacation::render::REQUEST_BUTTON_LABEL,
        ButtonStyle::Primary,
    )])];
    let request_message = client
        .create_message(Id::<ChannelMarker>::new(config.vacation.panel_channel_id))
        .allowed_mentions(Some(&AllowedMentions::default()))
        .embeds(&[request_embed])
        .components(&request_components)
        .await
        .map_err(|err| format!("failed to create vacation request panel: {err}"))?
        .model()
        .await
        .map_err(|err| format!("failed to decode vacation request panel message: {err}"))?;

    let active_embed = embed_with_appearance(
        xiii_vacation::render::ACTIVE_PANEL_TITLE,
        xiii_vacation::render::ACTIVE_PANEL_EMPTY,
        xiii_vacation::render::LEGACY_PANEL_COLOR,
        Some(xiii_vacation::render::LEGACY_FOOTER),
        false,
    );
    let active_message = client
        .create_message(Id::<ChannelMarker>::new(
            config.vacation.active_panel_channel_id,
        ))
        .allowed_mentions(Some(&AllowedMentions::default()))
        .embeds(&[active_embed])
        .await
        .map_err(|err| format!("failed to create active vacations panel: {err}"))?
        .model()
        .await
        .map_err(|err| format!("failed to decode active vacations panel message: {err}"))?;

    let state = VacationPanelStateFile {
        source: "fresh_bootstrap".to_owned(),
        guild_id: config.core.guild_id,
        bot_user_id,
        request_panel: PanelStateTarget {
            channel_id: config.vacation.panel_channel_id,
            message_id: request_message.id.get(),
        },
        active_panel: PanelStateTarget {
            channel_id: config.vacation.active_panel_channel_id,
            message_id: active_message.id.get(),
        },
        created_at_utc: utc_timestamp_now(),
        last_updated_at_utc: None,
    };
    let content = serde_json::to_string_pretty(&state)
        .map_err(|err| format!("failed to render vacation panel state: {err}"))?;
    fs::write(&state_path, content.as_bytes()).map_err(|err| {
        format!(
            "failed to write vacation panel state {}: {err}",
            state_path.display()
        )
    })?;

    Ok(VacationBootstrapSummary {
        request_message_id: state.request_panel.message_id,
        active_message_id: state.active_panel.message_id,
    })
}

async fn bootstrap_discipline_board(
    client: &DiscordHttpClient,
    config: &SuperbotConfig,
    state_dir: &Path,
    bot_user_id: u64,
) -> Result<u64, String> {
    let state_path = state_dir.join("discipline_panel_state.json");
    validate_state_output_path(&state_path, config)?;
    fs::create_dir_all(state_dir)
        .map_err(|err| format!("failed to create state dir {}: {err}", state_dir.display()))?;

    let embed = embed_with_appearance(
        xiii_discipline::render::board_title(),
        xiii_discipline::render::EMPTY_BOARD_DESCRIPTION,
        xiii_discipline::render::LEGACY_BOARD_COLOR,
        Some(xiii_discipline::render::BOARD_FOOTER_PREVIEW),
        true,
    );
    let components = vec![action_row(vec![
        button(
            xiii_discipline::interactions::PANEL_ISSUE,
            xiii_discipline::render::PANEL_ISSUE_LABEL,
            ButtonStyle::Danger,
        ),
        button(
            xiii_discipline::interactions::PANEL_REMOVE,
            xiii_discipline::render::PANEL_REMOVE_LABEL,
            ButtonStyle::Success,
        ),
        button(
            xiii_discipline::interactions::PANEL_HISTORY,
            xiii_discipline::render::PANEL_HISTORY_LABEL,
            ButtonStyle::Secondary,
        ),
    ])];
    let message = client
        .create_message(Id::<ChannelMarker>::new(config.discipline.board_channel_id))
        .allowed_mentions(Some(&AllowedMentions::default()))
        .embeds(&[embed])
        .components(&components)
        .await
        .map_err(|err| format!("failed to create discipline board: {err}"))?
        .model()
        .await
        .map_err(|err| format!("failed to decode discipline board message: {err}"))?;

    let state = DisciplinePanelStateFile {
        source: "fresh_bootstrap".to_owned(),
        guild_id: config.core.guild_id,
        bot_user_id,
        board: PanelStateTarget {
            channel_id: config.discipline.board_channel_id,
            message_id: message.id.get(),
        },
        created_at_utc: utc_timestamp_now(),
        last_updated_at_utc: None,
    };
    let content = serde_json::to_string_pretty(&state)
        .map_err(|err| format!("failed to render discipline panel state: {err}"))?;
    fs::write(&state_path, content.as_bytes()).map_err(|err| {
        format!(
            "failed to write discipline panel state {}: {err}",
            state_path.display()
        )
    })?;

    Ok(state.board.message_id)
}

async fn bootstrap_voice_activity_panel(
    client: &DiscordHttpClient,
    config: &SuperbotConfig,
    state_dir: &Path,
    bot_user_id: u64,
) -> Result<u64, String> {
    let state_path = state_dir.join("voice_activity_panel_state.json");
    validate_state_output_path(&state_path, config)?;
    if state_path.exists() {
        return Err(format!(
            "voice activity state already exists at {}; refusing duplicate panel creation",
            state_path.display()
        ));
    }
    fs::create_dir_all(state_dir)
        .map_err(|err| format!("failed to create state dir {}: {err}", state_dir.display()))?;

    let embed = embed_with_appearance(
        "XIII Voice Activity",
        &format!(
            "Период: 7 дней · Страница 1/1\n{}\n\n{}",
            xiii_voice_activity::render::PANEL_REFRESH_NOTICE,
            xiii_voice_activity::render::PUBLIC_EMPTY_MESSAGE
        ),
        xiii_voice_activity::render::LEGACY_EMBED_COLOR,
        Some(xiii_voice_activity::render::LEGACY_FOOTER),
        true,
    );
    let components = voice_activity_public_stats_components("7d", 0, 1);
    let message = client
        .create_message(Id::<ChannelMarker>::new(
            config.voice_activity.stats_panel_channel_id,
        ))
        .allowed_mentions(Some(&AllowedMentions::default()))
        .embeds(&[embed])
        .components(&components)
        .await
        .map_err(|err| format!("failed to create voice activity panel: {err}"))?
        .model()
        .await
        .map_err(|err| format!("failed to decode voice activity panel message: {err}"))?;

    let state = xiii_voice_activity::state::VoicePanelState {
        source: "fresh_bootstrap".to_owned(),
        guild_id: config.core.guild_id,
        bot_user_id,
        public_stats_panel: xiii_voice_activity::state::VoicePanelTarget {
            channel_id: config.voice_activity.stats_panel_channel_id,
            message_id: message.id.get(),
        },
        created_at_utc: utc_timestamp_now(),
        last_updated_at_utc: None,
    };
    let content = serde_json::to_string_pretty(&state)
        .map_err(|err| format!("failed to render voice activity panel state: {err}"))?;
    fs::write(&state_path, content.as_bytes()).map_err(|err| {
        format!(
            "failed to write voice activity panel state {}: {err}",
            state_path.display()
        )
    })?;

    Ok(state.public_stats_panel.message_id)
}

async fn bootstrap_ticket_panel(
    client: Arc<DiscordHttpClient>,
    config: &SuperbotConfig,
    state_dir: &Path,
    bot_user_id: u64,
) -> Result<u64, String> {
    let state_path = state_dir.join("ticket_panel_state.json");
    validate_state_output_path(&state_path, config)?;
    if state_path.exists() {
        return Err(format!(
            "ticket panel state already exists at {}; refusing duplicate panel creation",
            state_path.display()
        ));
    }
    fs::create_dir_all(state_dir)
        .map_err(|err| format!("failed to create state dir {}: {err}", state_dir.display()))?;
    let discord = xiii_tickets::discord_io::TicketDiscordHttp::new(client);
    let message = discord
        .send_ticket_panel(config.tickets.panel_channel_id)
        .await?;
    let legacy_panel_message_id =
        match xiii_tickets::repository::LegacySqliteTicketRepository::open_existing_read_only(
            &config.legacy_paths.ticket_db.resolved,
        )
        .await
        {
            Ok(repo) => repo
                .bot_state("ticket_panel_message_id")
                .await
                .ok()
                .flatten()
                .and_then(|value| value.parse::<u64>().ok()),
            Err(_) => None,
        };
    let state = xiii_tickets::state::TicketPanelState {
        source: "fresh_bootstrap".to_owned(),
        guild_id: config.core.guild_id,
        bot_user_id,
        channel_id: config.tickets.panel_channel_id,
        panel_message_id: message.id.get(),
        created_at_utc: utc_timestamp_now(),
        legacy_panel_message_id,
    };
    let content = serde_json::to_string_pretty(&state)
        .map_err(|err| format!("failed to render ticket panel state: {err}"))?;
    fs::write(&state_path, content.as_bytes()).map_err(|err| {
        format!(
            "failed to write ticket panel state {}: {err}",
            state_path.display()
        )
    })?;
    Ok(state.panel_message_id)
}

fn render_report_text(title: &str, report: &Report) -> String {
    let mut text = String::new();
    text.push_str(title);
    text.push('\n');
    for item in &report.items {
        text.push_str(&format!(
            "[{}] {} {}\n",
            item.severity, item.scope, item.message
        ));
    }
    let counts = report.counts();
    text.push_str(&format!(
        "Summary: OK={} WARN={} FAIL={}\n",
        counts.ok, counts.warn, counts.fail
    ));
    text
}

fn report_exit_code(report: &Report) -> ExitCode {
    if report.has_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn discord_read_permission_failure(allow_discord_read: bool) -> Option<Report> {
    if allow_discord_read {
        return None;
    }

    let mut report = Report::new();
    report.fail(
        "safety",
        "--allow-discord-read is required before any Discord HTTP read can be attempted",
    );
    Some(report)
}

fn write_plan_permission_failure(allow_write_plan: bool) -> Option<Report> {
    if allow_write_plan {
        return None;
    }

    let mut report = Report::new();
    report.fail(
        "safety",
        "--allow-write-plan is required before a Clanlist write plan can load a Discord token or create a Discord HTTP client",
    );
    Some(report)
}

fn bootstrap_permission_failure(
    allow_discord_read: bool,
    allow_discord_write: bool,
    confirm_create_new_panels: bool,
) -> Option<Report> {
    let mut report = Report::new();
    if !allow_discord_read {
        report.fail(
            "safety",
            "--allow-discord-read is required before any Discord HTTP read can be attempted",
        );
    }
    if !allow_discord_write {
        report.fail(
            "safety",
            "--allow-discord-write is required before fresh Clanlist panels can be created",
        );
    }
    if !confirm_create_new_panels {
        report.fail(
            "safety",
            "--confirm-create-new-panels is required before loading a Discord token or creating a Discord HTTP client",
        );
    }

    report.has_failures().then_some(report)
}

fn update_permission_failure(
    allow_discord_read: bool,
    allow_discord_write: bool,
    confirm_update_panels: bool,
) -> Option<Report> {
    let mut report = Report::new();
    if !allow_discord_read {
        report.fail(
            "safety",
            "--allow-discord-read is required before any Discord HTTP read can be attempted",
        );
    }
    if !allow_discord_write {
        report.fail(
            "safety",
            "--allow-discord-write is required before Clanlist panels can be edited",
        );
    }
    if !confirm_update_panels {
        report.fail(
            "safety",
            "--confirm-update-panels is required before loading a Discord token or editing panel messages",
        );
    }

    report.has_failures().then_some(report)
}

fn evaluate_old_service_guard(
    require_old_service_stopped: bool,
    old_service_status_file: Option<&Path>,
) -> Report {
    let mut report = Report::new();
    if !require_old_service_stopped {
        report.warn(
            "service_guard",
            "old service stop state was not verified; production write execution must require it",
        );
        return report;
    }

    let Some(path) = old_service_status_file else {
        report.fail(
            "service_guard",
            "--old-service-status-file is required when --require-old-service-stopped is used",
        );
        return report;
    };

    match fs::read_to_string(path) {
        Ok(text) => match parse_old_service_status(&text) {
            OldServiceStatus::Stopped(evidence) => report.ok(
                "service_guard",
                format!("old service appears stopped: {evidence}"),
            ),
            OldServiceStatus::Running(evidence) => report.fail(
                "service_guard",
                format!("old service appears active/running: {evidence}"),
            ),
            OldServiceStatus::Unknown => report.fail(
                "service_guard",
                "old service stop state could not be determined from status file",
            ),
        },
        Err(err) => report.fail(
            "service_guard",
            format!(
                "failed to read old service status file {}: {err}",
                path.display()
            ),
        ),
    }
    report
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OldServiceStatus {
    Stopped(&'static str),
    Running(&'static str),
    Unknown,
}

fn parse_old_service_status(text: &str) -> OldServiceStatus {
    let lower = text.to_ascii_lowercase();
    if lower.contains("active: active (running)") || lower.contains("active: active") {
        return OldServiceStatus::Running("Active: active");
    }
    if lower.contains("active: inactive") {
        return OldServiceStatus::Stopped("Active: inactive");
    }
    if lower.contains("active: failed") {
        return OldServiceStatus::Stopped("Active: failed");
    }
    if lower.contains("loaded: not-found") {
        return OldServiceStatus::Stopped("Loaded: not-found");
    }
    if lower.contains("could not be found") {
        return OldServiceStatus::Stopped("unit could not be found");
    }
    OldServiceStatus::Unknown
}

fn print_discord_snapshot_result(
    result: &xiii_clanlist::ClanlistDiscordSnapshotResult,
    format: PreviewFormat,
    output: Option<&Path>,
    config: Option<&SuperbotConfig>,
) -> ExitCode {
    let content = match format {
        PreviewFormat::Text => xiii_clanlist::render_discord_snapshot_text(result),
        PreviewFormat::Json => match xiii_clanlist::render_discord_snapshot_json(result) {
            Ok(json) => json,
            Err(err) => {
                println!("{{\"failures\":[\"failed to render JSON: {err}\"]}}");
                return ExitCode::from(2);
            }
        },
    };

    if let Err(err) = emit_output(&content, output, config) {
        let mut failed = result.clone();
        failed.report.fail("output", err);
        return print_discord_snapshot_result(&failed, format, None, None);
    }

    if result.has_critical_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_render_preview_result(
    result: &xiii_clanlist::ClanlistRenderPreviewResult,
    format: PreviewFormat,
    output: Option<&Path>,
    config: Option<&SuperbotConfig>,
    max_members_per_section: usize,
) -> ExitCode {
    let content = match format {
        PreviewFormat::Text => xiii_clanlist::render_render_preview_text_with_options(
            result,
            xiii_clanlist::RenderTextOptions {
                max_members_per_section,
            },
        ),
        PreviewFormat::Json => match xiii_clanlist::render_render_preview_json(result) {
            Ok(json) => json,
            Err(err) => {
                println!("{{\"failures\":[\"failed to render JSON: {err}\"]}}");
                return ExitCode::from(2);
            }
        },
    };

    if let Err(err) = emit_output(&content, output, config) {
        let mut failed = result.clone();
        failed.report.fail("output", err);
        return print_render_preview_result(&failed, format, None, None, max_members_per_section);
    }

    if result.has_critical_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_write_plan_result(
    result: &xiii_clanlist::ClanlistWritePlanResult,
    format: PreviewFormat,
    output: Option<&Path>,
    config: Option<&SuperbotConfig>,
) -> ExitCode {
    let content = match format {
        PreviewFormat::Text => xiii_clanlist::render_write_plan_text(result),
        PreviewFormat::Json => match xiii_clanlist::render_write_plan_json(result) {
            Ok(json) => json,
            Err(err) => {
                println!("{{\"failures\":[\"failed to render JSON: {err}\"]}}");
                return ExitCode::from(2);
            }
        },
    };

    if let Err(err) = emit_output(&content, output, config) {
        let mut failed = result.clone();
        failed.report.fail("output", err);
        return print_write_plan_result(&failed, format, None, None);
    }

    if result.has_critical_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_target_message_check_result(
    result: &xiii_clanlist::ClanlistTargetMessageCheckResult,
    format: PreviewFormat,
    output: Option<&Path>,
    config: Option<&SuperbotConfig>,
) -> ExitCode {
    let content = match format {
        PreviewFormat::Text => xiii_clanlist::render_target_message_check_text(result),
        PreviewFormat::Json => match xiii_clanlist::render_target_message_check_json(result) {
            Ok(json) => json,
            Err(err) => {
                println!("{{\"failures\":[\"failed to render JSON: {err}\"]}}");
                return ExitCode::from(2);
            }
        },
    };

    if let Err(err) = emit_output(&content, output, config) {
        let mut failed = result.clone();
        failed.report.fail("output", err);
        return print_target_message_check_result(&failed, format, None, None);
    }

    if result.has_critical_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_bootstrap_new_panels_result(
    result: &xiii_clanlist::ClanlistBootstrapNewPanelsResult,
    format: PreviewFormat,
    output: Option<&Path>,
    config: Option<&SuperbotConfig>,
) -> ExitCode {
    let content = match format {
        PreviewFormat::Text => xiii_clanlist::render_bootstrap_new_panels_text(result),
        PreviewFormat::Json => match xiii_clanlist::render_bootstrap_new_panels_json(result) {
            Ok(json) => json,
            Err(err) => {
                println!("{{\"failures\":[\"failed to render JSON: {err}\"]}}");
                return ExitCode::from(2);
            }
        },
    };

    if let Err(err) = emit_output(&content, output, config) {
        let mut failed = result.clone();
        failed.report.fail("output", err);
        return print_bootstrap_new_panels_result(&failed, format, None, None);
    }

    if result.has_critical_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_update_panels_result(
    result: &xiii_clanlist::ClanlistUpdatePanelsResult,
    format: PreviewFormat,
    output: Option<&Path>,
    config: Option<&SuperbotConfig>,
) -> ExitCode {
    let content = match format {
        PreviewFormat::Text => xiii_clanlist::render_update_panels_text(result),
        PreviewFormat::Json => match xiii_clanlist::render_update_panels_json(result) {
            Ok(json) => json,
            Err(err) => {
                println!("{{\"failures\":[\"failed to render JSON: {err}\"]}}");
                return ExitCode::from(2);
            }
        },
    };

    if let Err(err) = emit_output(&content, output, config) {
        let mut failed = result.clone();
        failed.report.fail("output", err);
        return print_update_panels_result(&failed, format, None, None);
    }

    if result.has_critical_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn emit_output(
    content: &str,
    output: Option<&Path>,
    config: Option<&SuperbotConfig>,
) -> Result<(), String> {
    match output {
        Some(path) => {
            let config = config.ok_or_else(|| {
                "cannot validate --output against legacy paths before config is loaded".to_owned()
            })?;
            let safe_path = validate_output_path(path, config)?;
            fs::write(&safe_path, content.as_bytes())
                .map_err(|err| format!("failed to write {} as UTF-8: {err}", safe_path.display()))
        }
        None => {
            print!("{content}");
            Ok(())
        }
    }
}

fn validate_output_path(path: &Path, config: &SuperbotConfig) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("--output path is empty".to_owned());
    }
    if path.is_dir() {
        return Err(format!(
            "--output points to a directory: {}",
            path.display()
        ));
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|err| format!("failed to resolve current directory: {err}"))?
            .join(path)
    };
    let parent = absolute
        .parent()
        .ok_or_else(|| format!("--output has no parent directory: {}", path.display()))?;
    if !parent.is_dir() {
        return Err(format!(
            "--output parent directory must already exist: {}",
            parent.display()
        ));
    }
    let parent = parent
        .canonicalize()
        .map_err(|err| format!("failed to resolve --output parent directory: {err}"))?;
    let file_name = absolute
        .file_name()
        .ok_or_else(|| format!("--output has no file name: {}", path.display()))?;
    let resolved_output = parent.join(file_name);

    let clanlist_dir = resolve_existing_path(&config.legacy_paths.clanlist_data_dir.resolved);
    if path_is_equal_or_inside(&resolved_output, &clanlist_dir) {
        return Err(format!(
            "--output must not be inside LEGACY_CLANLIST_DATA_DIR: {}",
            clanlist_dir.display()
        ));
    }

    for legacy_file in [
        &config.legacy_paths.ticket_db.resolved,
        &config.legacy_paths.voice_db.resolved,
        &config.legacy_paths.recruit_db.resolved,
        &config.legacy_paths.vacation_db.resolved,
        &config.legacy_paths.discipline_db.resolved,
        &config.legacy_paths.temp_voice_db.resolved,
    ] {
        let legacy_file = resolve_existing_path(legacy_file);
        if paths_equivalent(&resolved_output, &legacy_file) {
            return Err(format!(
                "--output must not overwrite a legacy state file: {}",
                legacy_file.display()
            ));
        }
    }

    Ok(resolved_output)
}

fn resolve_state_output_path(
    path: Option<&Path>,
    config: &SuperbotConfig,
    create_default_dir: bool,
) -> Result<PathBuf, String> {
    let absolute = match path {
        Some(path) => {
            if path.as_os_str().is_empty() {
                return Err("--state-output path is empty".to_owned());
            }
            if path.is_dir() {
                return Err(format!(
                    "--state-output points to a directory: {}",
                    path.display()
                ));
            }
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|err| format!("failed to resolve current directory: {err}"))?
                    .join(path)
            }
        }
        None => {
            let data_dir = std::env::current_dir()
                .map_err(|err| format!("failed to resolve current directory: {err}"))?
                .join("data");
            if create_default_dir {
                fs::create_dir_all(&data_dir)
                    .map_err(|err| format!("failed to create {}: {err}", data_dir.display()))?;
            }
            data_dir.join("clanlist_panel_state.json")
        }
    };

    let parent = absolute.parent().ok_or_else(|| {
        format!(
            "--state-output has no parent directory: {}",
            absolute.display()
        )
    })?;
    if !parent.is_dir() {
        return Err(format!(
            "--state-output parent directory must already exist: {}",
            parent.display()
        ));
    }
    let parent = parent
        .canonicalize()
        .map_err(|err| format!("failed to resolve --state-output parent directory: {err}"))?;
    let file_name = absolute
        .file_name()
        .ok_or_else(|| format!("--state-output has no file name: {}", absolute.display()))?;
    let resolved = parent.join(file_name);
    validate_state_output_path(&resolved, config)?;
    Ok(resolved)
}

fn resolve_state_file_path(
    path: Option<&Path>,
    config: &SuperbotConfig,
) -> Result<PathBuf, String> {
    let absolute = match path {
        Some(path) => {
            if path.as_os_str().is_empty() {
                return Err("--state-file path is empty".to_owned());
            }
            if path.is_dir() {
                return Err(format!(
                    "--state-file points to a directory: {}",
                    path.display()
                ));
            }
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|err| format!("failed to resolve current directory: {err}"))?
                    .join(path)
            }
        }
        None => std::env::current_dir()
            .map_err(|err| format!("failed to resolve current directory: {err}"))?
            .join("data")
            .join("clanlist_panel_state.json"),
    };

    let parent = absolute.parent().ok_or_else(|| {
        format!(
            "--state-file has no parent directory: {}",
            absolute.display()
        )
    })?;
    if !parent.is_dir() {
        return Err(format!(
            "--state-file parent directory must already exist: {}",
            parent.display()
        ));
    }
    let parent = parent
        .canonicalize()
        .map_err(|err| format!("failed to resolve --state-file parent directory: {err}"))?;
    let file_name = absolute
        .file_name()
        .ok_or_else(|| format!("--state-file has no file name: {}", absolute.display()))?;
    let resolved = parent.join(file_name);
    validate_state_output_path(&resolved, config)?;
    if !resolved.is_file() {
        return Err(format!("--state-file missing: {}", resolved.display()));
    }
    Ok(resolved)
}

fn validate_state_output_path(path: &Path, config: &SuperbotConfig) -> Result<(), String> {
    let clanlist_dir = resolve_existing_path(&config.legacy_paths.clanlist_data_dir.resolved);
    if path_is_equal_or_inside(path, &clanlist_dir) {
        return Err(format!(
            "--state-output must not be inside LEGACY_CLANLIST_DATA_DIR: {}",
            clanlist_dir.display()
        ));
    }

    if let Some(old_clanlist_root) = clanlist_dir.parent() {
        if path_is_equal_or_inside(path, old_clanlist_root) {
            return Err(format!(
                "--state-output must not be inside the old Clanlist bot directory: {}",
                old_clanlist_root.display()
            ));
        }
    }

    for legacy_path in legacy_state_paths(config) {
        let resolved_legacy = resolve_existing_path(legacy_path);
        if path_is_equal_or_inside(path, &resolved_legacy) {
            return Err(format!(
                "--state-output must not overwrite or live inside a legacy state path: {}",
                resolved_legacy.display()
            ));
        }
    }

    let normalized = normalized_path_string(path);
    if normalized.contains("xiii_bots_full_copy") || normalized.contains("\\opt\\xiii\\") {
        return Err(
            "--state-output must not point inside an old bot directory or VPS copy".to_owned(),
        );
    }

    Ok(())
}

fn legacy_state_paths(config: &SuperbotConfig) -> Vec<&Path> {
    vec![
        config.legacy_paths.ticket_db.resolved.as_path(),
        config.legacy_paths.voice_db.resolved.as_path(),
        config.legacy_paths.recruit_db.resolved.as_path(),
        config.legacy_paths.vacation_db.resolved.as_path(),
        config.legacy_paths.discipline_db.resolved.as_path(),
        config.legacy_paths.temp_voice_db.resolved.as_path(),
        config.legacy_paths.clanlist_data_dir.resolved.as_path(),
    ]
}

fn resolve_existing_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn path_is_equal_or_inside(path: &Path, ancestor: &Path) -> bool {
    paths_equivalent(path, ancestor)
        || normalized_path_string(path).starts_with(&format!(
            "{}{}",
            normalized_path_string(ancestor),
            std::path::MAIN_SEPARATOR
        ))
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    normalized_path_string(left) == normalized_path_string(right)
}

fn normalized_path_string(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        text.to_ascii_lowercase()
    } else {
        text
    }
}

struct DiscordFetchOutcome {
    report: Report,
    roles: Option<Vec<xiii_clanlist::DiscordRoleSnapshotInput>>,
    members: Option<Vec<xiii_clanlist::DiscordMemberSnapshotInput>>,
}

async fn fetch_discord_clanlist_snapshot(
    token: &str,
    guild_id: u64,
    include_members: bool,
) -> DiscordFetchOutcome {
    let client = DiscordHttpClient::new(token.to_owned());
    fetch_discord_clanlist_snapshot_with_client(&client, guild_id, include_members).await
}

async fn fetch_discord_clanlist_snapshot_with_client(
    client: &DiscordHttpClient,
    guild_id: u64,
    include_members: bool,
) -> DiscordFetchOutcome {
    let mut report = Report::new();
    let guild_id = Id::<GuildMarker>::new(guild_id);

    let roles = match fetch_guild_roles_with_retry(client, guild_id, &mut report).await {
        Ok(roles) => {
            let roles = roles
                .into_iter()
                .map(|role| xiii_clanlist::DiscordRoleSnapshotInput {
                    id: role.id.get(),
                    name: role.name,
                })
                .collect::<Vec<_>>();
            report.ok("discord", "connected to Discord read-only");
            report.ok("discord", format!("roles fetched = {}", roles.len()));
            roles
        }
        Err(err) => {
            report.fail("discord", err);
            return DiscordFetchOutcome {
                report,
                roles: None,
                members: None,
            };
        }
    };

    let members = if include_members {
        fetch_all_guild_members(client, guild_id, &mut report).await
    } else {
        report.ok("discord", "member fetch skipped by --roles-only");
        None
    };

    DiscordFetchOutcome {
        report,
        roles: Some(roles),
        members,
    }
}

async fn fetch_guild_roles_with_retry(
    client: &DiscordHttpClient,
    guild_id: Id<GuildMarker>,
    report: &mut Report,
) -> Result<Vec<DiscordRole>, String> {
    for attempt in 1..=DISCORD_HTTP_MAX_ATTEMPTS {
        match client.roles(guild_id).await {
            Ok(response) => {
                return response.model().await.map_err(|err| {
                    format!("failed to decode Discord guild roles response: {err}")
                });
            }
            Err(err) => {
                if let Some(delay) =
                    retry_delay_for_http_error(&err, attempt, report, "guild roles")
                {
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(format!(
                    "failed to fetch guild roles via Discord HTTP after {attempt} attempt(s): {err}"
                ));
            }
        }
    }

    Err(format!(
        "failed to fetch guild roles via Discord HTTP after {DISCORD_HTTP_MAX_ATTEMPTS} attempt(s)"
    ))
}

async fn fetch_all_guild_members(
    client: &DiscordHttpClient,
    guild_id: Id<GuildMarker>,
    report: &mut Report,
) -> Option<Vec<xiii_clanlist::DiscordMemberSnapshotInput>> {
    let mut after: Option<Id<UserMarker>> = None;
    let mut members = Vec::new();

    loop {
        let page = match fetch_guild_members_page_with_retry(client, guild_id, after, report).await
        {
            Ok(page) => page,
            Err(err) => {
                if members.is_empty() {
                    report.fail("discord", err);
                    return None;
                }
                report.warn(
                    "discord",
                    format!(
                        "member fetch stopped after {} members; partial snapshot only: {}",
                        members.len(),
                        err
                    ),
                );
                return Some(members);
            }
        };

        if page.is_empty() {
            break;
        }

        let page_len = page.len();
        after = page.last().map(|member| member.user.id);
        members.extend(page.into_iter().map(|member| {
            let display_name = member
                .nick
                .clone()
                .or(member.user.global_name.clone())
                .unwrap_or_else(|| member.user.name.clone());
            xiii_clanlist::DiscordMemberSnapshotInput {
                user_id: member.user.id.get(),
                display_name,
                role_ids: member
                    .roles
                    .into_iter()
                    .map(|role_id| role_id.get())
                    .collect(),
            }
        }));

        if page_len < 1000 {
            break;
        }
    }

    report.ok("discord", format!("members fetched = {}", members.len()));
    Some(members)
}

async fn fetch_guild_members_page_with_retry(
    client: &DiscordHttpClient,
    guild_id: Id<GuildMarker>,
    after: Option<Id<UserMarker>>,
    report: &mut Report,
) -> Result<Vec<DiscordMember>, String> {
    for attempt in 1..=DISCORD_HTTP_MAX_ATTEMPTS {
        let mut request = client.guild_members(guild_id).limit(1000);
        if let Some(after_id) = after {
            request = request.after(after_id);
        }

        match request.await {
            Ok(response) => {
                return response.model().await.map_err(|err| {
                    format!("failed to decode Discord guild members response: {err}")
                });
            }
            Err(err) => {
                if let Some(delay) =
                    retry_delay_for_http_error(&err, attempt, report, "guild members page")
                {
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(format!(
                    "failed to fetch guild members via Discord HTTP after {attempt} attempt(s): {err}"
                ));
            }
        }
    }

    Err(format!(
        "failed to fetch guild members via Discord HTTP after {DISCORD_HTTP_MAX_ATTEMPTS} attempt(s)"
    ))
}

async fn fetch_current_user_with_retry(
    client: &DiscordHttpClient,
    report: &mut Report,
) -> Result<CurrentUser, String> {
    for attempt in 1..=DISCORD_HTTP_MAX_ATTEMPTS {
        match client.current_user().await {
            Ok(response) => {
                return response.model().await.map_err(|err| {
                    format!("failed to decode Discord current user response: {err}")
                });
            }
            Err(err) => {
                if let Some(delay) =
                    retry_delay_for_http_error(&err, attempt, report, "current bot user")
                {
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(format!(
                    "failed to fetch current bot user via Discord HTTP after {attempt} attempt(s): {err}"
                ));
            }
        }
    }

    Err(format!(
        "failed to fetch current bot user via Discord HTTP after {DISCORD_HTTP_MAX_ATTEMPTS} attempt(s)"
    ))
}

async fn fetch_target_message_with_retry(
    client: &DiscordHttpClient,
    channel_id: u64,
    message_id: u64,
    report: &mut Report,
    resource: &str,
) -> Result<DiscordMessage, String> {
    let channel_id = Id::<ChannelMarker>::new(channel_id);
    let message_id = Id::<MessageMarker>::new(message_id);

    for attempt in 1..=DISCORD_HTTP_MAX_ATTEMPTS {
        match client.message(channel_id, message_id).await {
            Ok(response) => {
                return response
                    .model()
                    .await
                    .map_err(|err| format!("failed to decode Discord message response: {err}"));
            }
            Err(err) => {
                if let Some(delay) = retry_delay_for_http_error(&err, attempt, report, resource) {
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(format!(
                    "failed to fetch {resource} via Discord HTTP after {attempt} attempt(s): {err}"
                ));
            }
        }
    }

    Err(format!(
        "failed to fetch {resource} via Discord HTTP after {DISCORD_HTTP_MAX_ATTEMPTS} attempt(s)"
    ))
}

fn observation_from_discord_message(
    target: &xiii_clanlist::TargetMessageTarget,
    message: DiscordMessage,
) -> xiii_clanlist::TargetMessageObservationInput {
    let first_embed = message.embeds.first();
    xiii_clanlist::TargetMessageObservationInput {
        panel_name: target.panel_name,
        channel_id: target.channel_id,
        message_id: target.message_id,
        exists: true,
        failure_reason: None,
        author_id: Some(message.author.id.get()),
        embed_count: Some(message.embeds.len()),
        first_embed_title: first_embed.and_then(|embed| embed.title.clone()),
        first_embed_footer_text: first_embed
            .and_then(|embed| embed.footer.as_ref().map(|footer| footer.text.clone())),
        first_embed_footer_icon_url: first_embed.and_then(|embed| {
            embed
                .footer
                .as_ref()
                .and_then(|footer| footer.icon_url.clone())
        }),
        first_embed_marker_url: first_embed
            .and_then(|embed| embed.author.as_ref().and_then(|author| author.url.clone())),
    }
}

async fn send_bootstrap_panel_message_with_retry(
    client: &DiscordHttpClient,
    payload: &xiii_clanlist::BootstrapMessagePayload,
    unix_timestamp_seconds: i64,
    report: &mut Report,
) -> Result<u64, String> {
    let channel_id = Id::<ChannelMarker>::new(payload.channel_id);
    let embeds = bootstrap_embeds_to_twilight(payload, unix_timestamp_seconds)?;
    let allowed_mentions = AllowedMentions::default();
    let resource = format!("{} bootstrap create_message", payload.panel_name);

    for attempt in 1..=DISCORD_HTTP_MAX_ATTEMPTS {
        match client
            .create_message(channel_id)
            .allowed_mentions(Some(&allowed_mentions))
            .embeds(&embeds)
            .await
        {
            Ok(response) => {
                return response
                    .model()
                    .await
                    .map(|message| message.id.get())
                    .map_err(|err| {
                        format!("failed to decode Discord create message response: {err}")
                    });
            }
            Err(err) => {
                if let Some(delay) = retry_delay_for_http_error(&err, attempt, report, &resource) {
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(format!(
                    "failed to create {resource} via Discord HTTP after {attempt} attempt(s): {err}"
                ));
            }
        }
    }

    Err(format!(
        "failed to create {resource} via Discord HTTP after {DISCORD_HTTP_MAX_ATTEMPTS} attempt(s)"
    ))
}

async fn update_panel_message_with_retry(
    client: &DiscordHttpClient,
    payload: &xiii_clanlist::UpdateMessagePayload,
    unix_timestamp_seconds: i64,
    report: &mut Report,
) -> Result<u64, String> {
    let channel_id = Id::<ChannelMarker>::new(payload.channel_id);
    let message_id = Id::<MessageMarker>::new(payload.message_id);
    let embeds = update_embeds_to_twilight(payload, unix_timestamp_seconds)?;
    let allowed_mentions = AllowedMentions::default();
    let resource = format!("{} update_message", payload.panel_name);

    for attempt in 1..=DISCORD_HTTP_MAX_ATTEMPTS {
        match client
            .update_message(channel_id, message_id)
            .allowed_mentions(Some(&allowed_mentions))
            .content(None)
            .embeds(Some(&embeds))
            .await
        {
            Ok(response) => {
                return response
                    .model()
                    .await
                    .map(|message| message.id.get())
                    .map_err(|err| {
                        format!("failed to decode Discord update message response: {err}")
                    });
            }
            Err(err) => {
                if let Some(delay) = retry_delay_for_http_error(&err, attempt, report, &resource) {
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(format!(
                    "failed to edit {resource} via Discord HTTP after {attempt} attempt(s): {err}"
                ));
            }
        }
    }

    Err(format!(
        "failed to edit {resource} via Discord HTTP after {DISCORD_HTTP_MAX_ATTEMPTS} attempt(s)"
    ))
}

fn bootstrap_embeds_to_twilight(
    payload: &xiii_clanlist::BootstrapMessagePayload,
    unix_timestamp_seconds: i64,
) -> Result<Vec<Embed>, String> {
    embeds_to_twilight(&payload.embeds, unix_timestamp_seconds)
}

fn update_embeds_to_twilight(
    payload: &xiii_clanlist::UpdateMessagePayload,
    unix_timestamp_seconds: i64,
) -> Result<Vec<Embed>, String> {
    embeds_to_twilight(&payload.embeds, unix_timestamp_seconds)
}

fn embeds_to_twilight(
    embeds: &[xiii_clanlist::BootstrapEmbedPayload],
    unix_timestamp_seconds: i64,
) -> Result<Vec<Embed>, String> {
    let timestamp = Timestamp::from_secs(unix_timestamp_seconds)
        .map_err(|err| format!("failed to build Discord embed timestamp: {err}"))?;
    Ok(embeds
        .iter()
        .map(|embed| Embed {
            author: Some(EmbedAuthor {
                icon_url: None,
                name: "XIII Clanlist".to_owned(),
                proxy_icon_url: None,
                url: Some(embed.marker_url.to_owned()),
            }),
            color: parse_embed_color(embed.color_hex),
            description: Some(embed.description.clone()),
            fields: Vec::new(),
            footer: Some(EmbedFooter {
                icon_url: None,
                proxy_icon_url: None,
                text: embed.footer_text.clone(),
            }),
            image: None,
            kind: "rich".to_owned(),
            provider: None,
            thumbnail: None,
            timestamp: Some(timestamp),
            title: Some(embed.title.clone()),
            url: None,
            video: None,
        })
        .collect())
}

fn parse_embed_color(color_hex: &str) -> Option<u32> {
    color_hex
        .strip_prefix('#')
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
}

fn write_panel_state_json(
    path: &Path,
    state: &xiii_clanlist::ClanlistPanelState,
) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(state)
        .map_err(|err| format!("failed to render Clanlist panel state JSON: {err}"))?;
    fs::write(path, json).map_err(|err| {
        format!(
            "failed to write Clanlist panel state {}: {err}",
            path.display()
        )
    })
}

fn write_panel_state_json_atomic(
    path: &Path,
    state: &xiii_clanlist::ClanlistPanelState,
) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(state)
        .map_err(|err| format!("failed to render Clanlist panel state JSON: {err}"))?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("state path has no parent: {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("state path has no valid file name: {}", path.display()))?;
    let temp_path = parent.join(format!(".{file_name}.tmp"));
    fs::write(&temp_path, json).map_err(|err| {
        format!(
            "failed to write temporary Clanlist panel state {}: {err}",
            temp_path.display()
        )
    })?;
    if path.exists() {
        fs::remove_file(path).map_err(|err| {
            format!(
                "failed to replace existing Clanlist panel state {}: {err}",
                path.display()
            )
        })?;
    }
    fs::rename(&temp_path, path).map_err(|err| {
        format!(
            "failed to rename temporary Clanlist panel state {} to {}: {err}",
            temp_path.display(),
            path.display()
        )
    })
}

fn partial_recovery_path(config: &SuperbotConfig, timestamp_slug: &str) -> PathBuf {
    let default_path = resolve_state_output_path(None, config, true)
        .unwrap_or_else(|_| PathBuf::from("data").join("clanlist_panel_state.json"));
    default_path.with_file_name(format!("clanlist_bootstrap_partial_{timestamp_slug}.json"))
}

fn update_partial_recovery_path(config: &SuperbotConfig, timestamp_slug: &str) -> PathBuf {
    let default_path = resolve_state_output_path(None, config, true)
        .unwrap_or_else(|_| PathBuf::from("data").join("clanlist_panel_state.json"));
    default_path.with_file_name(format!("clanlist_update_partial_{timestamp_slug}.json"))
}

fn write_partial_recovery_json(
    path: &Path,
    result: &xiii_clanlist::ClanlistBootstrapNewPanelsResult,
) -> Result<(), String> {
    let json = xiii_clanlist::render_bootstrap_new_panels_json(result)
        .map_err(|err| format!("failed to render partial recovery JSON: {err}"))?;
    fs::write(path, json.as_bytes()).map_err(|err| {
        format!(
            "failed to write partial bootstrap recovery file {}: {err}",
            path.display()
        )
    })
}

fn write_partial_update_json(
    path: &Path,
    result: &xiii_clanlist::ClanlistUpdatePanelsResult,
) -> Result<(), String> {
    let json = xiii_clanlist::render_update_panels_json(result)
        .map_err(|err| format!("failed to render partial update JSON: {err}"))?;
    fs::write(path, json.as_bytes()).map_err(|err| {
        format!(
            "failed to write partial update recovery file {}: {err}",
            path.display()
        )
    })
}

fn retry_delay_for_http_error(
    err: &TwilightHttpError,
    attempt: usize,
    report: &mut Report,
    resource: &str,
) -> Option<Duration> {
    let rate_limit = discord_rate_limit_from_error(err)?;
    let delay =
        retry_delay_for_attempt(attempt, DISCORD_HTTP_MAX_ATTEMPTS, rate_limit.retry_after)?;
    report.warn(
        "discord",
        format!(
            "rate limited while fetching {resource}; retrying after {:.3}s (attempt {attempt}/{DISCORD_HTTP_MAX_ATTEMPTS})",
            rate_limit.retry_after_seconds
        ),
    );
    Some(delay)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DiscordRateLimit {
    retry_after: Duration,
    retry_after_seconds: f64,
}

fn discord_rate_limit_from_error(err: &TwilightHttpError) -> Option<DiscordRateLimit> {
    match err.kind() {
        TwilightHttpErrorType::Response { body, status, .. } if status.get() == 429 => {
            let (retry_after, retry_after_seconds) =
                parse_retry_after_from_body(body).unwrap_or((Duration::from_secs(1), 1.0));
            Some(DiscordRateLimit {
                retry_after,
                retry_after_seconds,
            })
        }
        _ => None,
    }
}

fn parse_retry_after_from_body(body: &[u8]) -> Option<(Duration, f64)> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let seconds = value
        .get("retry_after")
        .and_then(serde_json::Value::as_f64)
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)?;
    Some((duration_from_seconds(seconds), seconds))
}

fn retry_delay_for_attempt(
    attempt: usize,
    max_attempts: usize,
    retry_after: Duration,
) -> Option<Duration> {
    if attempt >= max_attempts {
        None
    } else if retry_after.is_zero() {
        Some(Duration::from_millis(250))
    } else {
        Some(retry_after)
    }
}

fn duration_from_seconds(seconds: f64) -> Duration {
    Duration::from_millis((seconds * 1000.0).ceil() as u64)
}

fn read_secret_from_env_file(path: &Path, name: &str) -> Result<String, String> {
    let iter = dotenvy::from_path_iter(path)
        .map_err(|_| format!("failed to read env file for {name}; raw line suppressed"))?;
    for item in iter {
        let (key, value) =
            item.map_err(|_| format!("failed to parse env entry for {name}; raw line suppressed"))?;
        if key == name {
            let trimmed = value.trim();
            return if trimmed.is_empty() {
                Err(format!("{name} is <EMPTY>"))
            } else {
                Ok(trimmed.to_owned())
            };
        }
    }
    Err(format!("{name} is <MISSING>"))
}

async fn verify_legacy(env_file: PathBuf) -> ExitCode {
    println!("XIII Superbot Legacy Verification");
    println!("Mode: READ ONLY");
    println!("Discord login: DISABLED");
    println!("DB writes: DISABLED");
    println!("Migrations: DISABLED");
    println!("Env file: {}", env_file.display());
    println!();

    let load = match SuperbotConfig::load_from_env_file(&env_file) {
        Ok(load) => load,
        Err(err) => {
            println!("[FAIL] config {err}");
            return ExitCode::from(2);
        }
    };

    print_report("Config Validation", &load.report);
    println!();

    let report = xiii_db::verify_legacy(&load.config).await;
    print_report("Legacy State Verification", &report.report);
    if load.report.has_failures() || report.has_critical_failures() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_manifest() -> ExitCode {
    let manifests = module_manifests();
    let router = CentralRouter::from_manifests(&manifests);
    let scheduler = SchedulerRegistry::from_manifests(&manifests);
    let discord_plan = DiscordRuntimePlan::scaffold_only();

    println!("XIII Superbot Manifest");
    println!("Discord stack: {}", discord_plan.stack);
    println!("Discord login: DISABLED");
    println!("Gateway connection: DISABLED");
    println!("Registered modules: {}", manifests.len());
    println!("Slash commands: {}", router.slash_commands().len());
    println!("Component routes: {}", router.component_routes().len());
    println!("Scheduler jobs: {}", scheduler.jobs().len());
    println!("Clanlist writes: gated fresh-panel edits and Superbot-owned state/health only");
    println!("Non-Clanlist write_state_allowed=false");
    println!();

    for manifest in manifests {
        println!("Module: {}", manifest.name);
        println!("  id: {}", manifest.id);
        println!("  old_source: {}", manifest.old_source_path);
        println!("  migration_difficulty: {}", manifest.migration_difficulty);
        if manifest.id == ModuleId::Clanlist {
            println!(
                "  write_state_allowed: gated Superbot-owned Clanlist panel state/health only"
            );
        } else if manifest.id == ModuleId::TempVoice {
            println!(
                "  write_state_allowed: gated legacy temp voice DB writes and DB-owned voice channel deletion only"
            );
        } else {
            println!("  write_state_allowed: false");
        }
        if manifest.id == ModuleId::Clanlist {
            println!("  dry_run_capabilities:");
            println!("    - clanlist-preview: reads legacy clanlist JSON/cache only; Discord and Google disabled");
            println!("    - discord-readonly-clanlist-snapshot: requires --allow-discord-read; reads guild roles/members through Discord HTTP only; Discord writes disabled");
            println!("    - clanlist-render-preview: requires --allow-discord-read; renders parity preview from Discord reads and legacy JSON/cache; Discord/Google/legacy writes disabled");
            println!("    - clanlist-write-plan: requires --allow-discord-read and --allow-write-plan; builds allowed=false edit plan only; Discord/Google/legacy writes disabled");
            println!("    - clanlist-target-message-check: requires --allow-discord-read; reads /users/@me and the three exact target messages through Discord HTTP only; Discord writes disabled");
            println!("    - clanlist-bootstrap-new-panels: requires --allow-discord-read, --allow-discord-write, and --confirm-create-new-panels; creates exactly 3 fresh messages only when not --dry-run; old panels and legacy JSON untouched");
            println!("    - clanlist-update-panels: requires --allow-discord-read, --allow-discord-write, and --confirm-update-panels; edits only the 3 fresh messages from data/clanlist_panel_state.json; no creates/deletes/legacy writes");
            println!("    - run-clanlist: requires --allow-discord-read, --allow-discord-write, and --confirm-run-clanlist; production Clanlist-only refresher for the 3 fresh messages; no creates/deletes/old-panel edits");
        }
        println!("  legacy_state_files:");
        for state in &manifest.state_dependencies {
            println!(
                "    - {:?}: {} ({:?})",
                state.kind, state.path, state.access
            );
        }
        println!("  env_dependencies:");
        for env in &manifest.env_dependencies {
            let old = env.old_name.as_deref().unwrap_or("-");
            let secret = if env.secret { "secret" } else { "non-secret" };
            println!(
                "    - {} <- {} required={} {} purpose={}",
                env.new_name, old, env.required, secret, env.purpose
            );
        }
        println!("  slash_commands:");
        for command in &manifest.slash_commands {
            let options = if command.subcommands_or_options.is_empty() {
                "-".to_owned()
            } else {
                command.subcommands_or_options.join(", ")
            };
            println!(
                "    - {} options={} mutates_legacy_behavior={} source={}",
                command.name, options, command.mutates_production, command.legacy_source
            );
        }
        println!("  component_custom_ids:");
        for route in &manifest.component_routes {
            println!(
                "    - {} persistent={} mutates_legacy_behavior={} source={}",
                route.custom_id_pattern,
                route.persistent,
                route.mutates_production,
                route.legacy_source
            );
        }
        println!("  scheduler_jobs:");
        for job in &manifest.scheduler_jobs {
            let timing = job
                .interval_seconds
                .map(|seconds| format!("{seconds}s"))
                .unwrap_or_else(|| "startup".to_owned());
            println!(
                "    - {} timing={} must_not_duplicate={} mutates_legacy_behavior={} source={}",
                job.name, timing, job.must_not_duplicate, job.mutates_production, job.legacy_source
            );
        }
        println!();
    }

    let duplicates = router.duplicate_component_patterns();
    if duplicates.is_empty() {
        println!("[OK] manifest component custom_id patterns are unique");
        ExitCode::SUCCESS
    } else {
        println!(
            "[FAIL] duplicate component custom_id patterns: {}",
            duplicates.join(", ")
        );
        ExitCode::from(2)
    }
}

fn print_report(title: &str, report: &Report) {
    println!("{title}");
    for item in &report.items {
        println!("[{}] {} {}", item.severity, item.scope, item.message);
    }
    let counts = report.counts();
    println!(
        "Summary: OK={} WARN={} FAIL={}",
        counts.ok, counts.warn, counts.fail
    );
}

#[cfg(test)]
mod tests {
    use super::{
        bootstrap_permission_failure, build_module_status_report, clanlist_health_from_outcome,
        clanlist_interval_seconds, collect_json_message_targets, discord_read_permission_failure,
        duration_between_iso_clamped, emit_output, legacy_parity_modules, module_manifests,
        module_render_preview, parse_old_service_status, parse_retry_after_from_body,
        production_path_issue, render_preview_json, resolve_health_output_path,
        resolve_state_file_path, resolve_state_output_path, retry_delay_for_attempt,
        run_clanlist_permission_failure, run_superbot_permission_failure, sync_command_plan,
        update_permission_failure, valid_voice_cutover_state, validate_fresh_state_json,
        validate_output_path, write_clanlist_health, write_plan_permission_failure,
        ClanlistRefreshOutcome, LegacyParityStatus, ModuleReadiness, NonOverlapGuard,
        OldServiceStatus, SuperbotModuleKind, SyncPlanStatus, TempVoiceOccupancy,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use xiii_config::{ConfigPath, SuperbotConfig};
    use xiii_core::Report;
    use xiii_discord::CentralRouter;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn custom_id_manifest_patterns_are_unique() {
        let manifests = module_manifests();
        let router = CentralRouter::from_manifests(&manifests);
        assert!(router.duplicate_component_patterns().is_empty());
    }

    #[test]
    fn discord_read_requires_explicit_allow_flag_before_fetch() {
        let report = discord_read_permission_failure(false).unwrap();

        assert!(report.has_failures());
        assert!(report.items[0]
            .message
            .contains("--allow-discord-read is required"));
        assert!(discord_read_permission_failure(true).is_none());
    }

    #[test]
    fn write_plan_requires_explicit_allow_flag_before_fetch() {
        let report = write_plan_permission_failure(false).unwrap();

        assert!(report.has_failures());
        assert!(report.items[0]
            .message
            .contains("--allow-write-plan is required"));
        assert!(write_plan_permission_failure(true).is_none());
    }

    #[test]
    fn bootstrap_requires_write_and_confirm_flags_before_fetch() {
        let report = bootstrap_permission_failure(true, false, false).unwrap();

        assert!(report.has_failures());
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("--allow-discord-write")));
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("--confirm-create-new-panels")));
        assert!(bootstrap_permission_failure(true, true, true).is_none());
    }

    #[test]
    fn update_requires_write_and_confirm_flags_before_fetch() {
        let report = update_permission_failure(true, false, false).unwrap();

        assert!(report.has_failures());
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("--allow-discord-write")));
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("--confirm-update-panels")));
        assert!(update_permission_failure(true, true, true).is_none());
    }

    #[test]
    fn run_clanlist_requires_write_and_confirm_flags_before_fetch() {
        let report = run_clanlist_permission_failure(true, false, false).unwrap();

        assert!(report.has_failures());
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("--allow-discord-write")));
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("--confirm-run-clanlist")));
        assert!(run_clanlist_permission_failure(true, true, true).is_none());
    }

    #[test]
    fn run_superbot_requires_all_runtime_safety_flags_before_fetch() {
        let report = run_superbot_permission_failure(true, false, false).unwrap();

        assert!(report.has_failures());
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("--allow-discord-write")));
        assert!(report
            .items
            .iter()
            .any(|item| item.message.contains("--confirm-run-superbot")));
        assert!(run_superbot_permission_failure(true, true, true).is_none());
    }

    #[test]
    fn readiness_matrix_marks_runnable_modules_ready_full() {
        assert_eq!(
            SuperbotModuleKind::Clanlist.readiness(),
            ModuleReadiness::ReadyFull
        );
        assert_eq!(
            SuperbotModuleKind::TempVoice.readiness(),
            ModuleReadiness::ReadyFull
        );
        assert_eq!(
            SuperbotModuleKind::Vacation.readiness(),
            ModuleReadiness::ReadyFull
        );
        assert_eq!(
            SuperbotModuleKind::Discipline.readiness(),
            ModuleReadiness::ReadyFull
        );
        assert_eq!(
            SuperbotModuleKind::Recruit.readiness(),
            ModuleReadiness::ReadyFull
        );
        assert_eq!(
            SuperbotModuleKind::VoiceActivity.readiness(),
            ModuleReadiness::ReadyFull
        );
        assert_eq!(
            SuperbotModuleKind::Tickets.readiness(),
            ModuleReadiness::ReadyFull
        );
    }

    #[test]
    fn voice_cutover_state_policy_is_explicit_and_duration_is_clamped() {
        let state = xiii_voice_activity::state::VoiceActivityCutoverState {
            source: "voice-finalize-cutover".to_owned(),
            policy: "closed_active_at_cutover".to_owned(),
            guild_id: 42,
            cutover_at_utc: "2026-05-10T10:00:00Z".to_owned(),
            active_sessions_before: 1,
            closed_sessions: Vec::new(),
            note: "Historical completed voice stats are preserved.".to_owned(),
        };

        assert!(valid_voice_cutover_state(&state, 42));
        assert!(!valid_voice_cutover_state(&state, 43));
        assert_eq!(
            duration_between_iso_clamped("2026-05-10T09:00:00Z", "2026-05-10T10:00:00Z"),
            3600
        );
        assert_eq!(
            duration_between_iso_clamped("2026-05-10T11:00:00Z", "2026-05-10T10:00:00Z"),
            0
        );
    }

    #[test]
    fn production_path_issue_accepts_linux_style_vps_paths() {
        assert_eq!(
            production_path_issue(Path::new("/opt/XIII/xiii-superbot/data")),
            None
        );
        assert_eq!(
            production_path_issue(Path::new("/opt/XIII/xiii-vacation-bot/data/vacations.db")),
            None
        );
    }

    #[test]
    fn production_path_issue_rejects_local_validation_and_windows_paths() {
        assert!(production_path_issue(Path::new(
            "../XIII_BOTS_FULL_COPY/opt/xiii-ticketbot/tickets.db"
        ))
        .unwrap()
        .contains("XIII_BOTS_FULL_COPY"));
        assert!(
            production_path_issue(Path::new("D:/clients/XIII 2/xiii-superbot/data"))
                .unwrap()
                .contains("Windows path")
        );
        assert!(production_path_issue(Path::new("data"))
            .unwrap()
            .contains("relative"));
    }

    #[test]
    fn fresh_state_validation_checks_guild_and_message_targets() {
        let dir = fixture_dir("fresh_state_validation");
        let state_path = dir.join("voice_activity_panel_state.json");
        fs::write(
            &state_path,
            r#"{
              "source":"fresh_bootstrap",
              "guild_id":42,
              "bot_user_id":99,
              "public_stats_panel":{"channel_id":100,"message_id":200},
              "created_at_utc":"2026-05-10T10:00:00Z"
            }"#,
        )
        .unwrap();

        let targets = validate_fresh_state_json(&state_path, 42, 1).unwrap();

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].1, 100);
        assert_eq!(targets[0].2, 200);
        assert!(validate_fresh_state_json(&state_path, 43, 1).is_err());
        cleanup_dir(dir);
    }

    #[test]
    fn json_message_target_collector_finds_panel_message_ids() {
        let value: serde_json::Value = serde_json::json!({
            "channel_id": 10,
            "panel_message_id": 20,
            "nested": {"channel_id": 30, "message_id": 40}
        });
        let mut targets = Vec::new();

        collect_json_message_targets("$", &value, &mut targets);

        assert_eq!(targets.len(), 2);
        assert!(targets
            .iter()
            .any(|(_, channel_id, message_id)| { *channel_id == 10 && *message_id == 20 }));
        assert!(targets
            .iter()
            .any(|(_, channel_id, message_id)| { *channel_id == 30 && *message_id == 40 }));
    }

    #[test]
    fn sync_plan_temp_voice_only_has_setup_command() {
        let plan = sync_command_plan(&[SuperbotModuleKind::TempVoice]);

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].status, SyncPlanStatus::Planned);
        assert_eq!(plan[0].commands, vec!["/setup-voice-hub"]);
    }

    #[test]
    fn sync_plan_vacation_only_has_vacations_command() {
        let plan = sync_command_plan(&[SuperbotModuleKind::Vacation]);

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].status, SyncPlanStatus::Planned);
        assert_eq!(plan[0].commands, vec!["/vacations"]);
    }

    #[test]
    fn sync_plan_recruit_only_has_recruit_commands() {
        let plan = sync_command_plan(&[SuperbotModuleKind::Recruit]);

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].status, SyncPlanStatus::Planned);
        assert_eq!(plan[0].commands, vec!["/recruits", "/recruit-panel"]);
    }

    #[test]
    fn sync_plan_voice_activity_has_stats_commands() {
        let plan = sync_command_plan(&[SuperbotModuleKind::VoiceActivity]);

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].status, SyncPlanStatus::Planned);
        assert_eq!(plan[0].commands, vec!["/voice-top", "/inactive-check"]);
    }

    #[test]
    fn sync_plan_ready_modules_are_all_planned() {
        let plan = sync_command_plan(&[
            SuperbotModuleKind::TempVoice,
            SuperbotModuleKind::Vacation,
            SuperbotModuleKind::Recruit,
            SuperbotModuleKind::VoiceActivity,
        ]);

        assert_eq!(plan.len(), 4);
        assert!(plan.iter().all(|row| row.status == SyncPlanStatus::Planned));
    }

    #[test]
    fn sync_plan_mixed_ready_modules_include_discipline_without_recursion() {
        let plan = sync_command_plan(&[
            SuperbotModuleKind::TempVoice,
            SuperbotModuleKind::Vacation,
            SuperbotModuleKind::Discipline,
            SuperbotModuleKind::Recruit,
        ]);

        let discipline = plan
            .iter()
            .find(|row| row.module == "discipline")
            .expect("discipline row");
        assert_eq!(discipline.status, SyncPlanStatus::Planned);
        assert_eq!(discipline.readiness, "READY_FULL");
        assert_eq!(discipline.commands, vec!["/discipline"]);

        let json = serde_json::to_string(&plan).expect("sync plan JSON");
        assert!(json.contains("/setup-voice-hub"));
        assert!(json.contains("/vacations"));
        assert!(json.contains("/discipline"));
        assert!(json.contains("/recruits"));
        assert!(!json.contains("DISCORD_TOKEN"));
        assert!(!json.contains(".env.local"));
    }

    #[test]
    fn real_sync_plan_includes_tickets_after_readiness_completion() {
        let plan = sync_command_plan(&[
            SuperbotModuleKind::TempVoice,
            SuperbotModuleKind::Discipline,
            SuperbotModuleKind::Tickets,
        ]);

        assert!(plan.iter().all(|row| row.status == SyncPlanStatus::Planned));
        let tickets = plan
            .iter()
            .find(|row| row.module == "tickets")
            .expect("tickets row");
        assert_eq!(tickets.commands, vec!["/add", "/remove", "/custom-ticket"]);
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("DISCORD_TOKEN"));
    }

    #[test]
    fn legacy_parity_audit_has_explicit_status_for_every_module() {
        let rows = legacy_parity_modules();

        assert_eq!(rows.len(), 7);
        assert!(rows
            .iter()
            .any(|row| row.module == "clanlist"
                && row.status == LegacyParityStatus::AcceptedDifference));
        assert!(rows
            .iter()
            .any(|row| row.module == "tickets"
                && row.status == LegacyParityStatus::AcceptedDifference));
        assert!(rows
            .iter()
            .any(|row| row.module == "vacation" && row.status == LegacyParityStatus::Exact));
        let json = serde_json::to_string(&rows).unwrap();
        assert!(!json.contains("DISCORD_TOKEN"));
        assert!(!json.contains("PRIVATE_KEY"));
        for needle in [
            char::from_u32(0x00C3).unwrap(),
            char::from_u32(0x00D0).unwrap(),
            char::from_u32(0x00D1).unwrap(),
            char::from_u32(0xFFFD).unwrap(),
        ] {
            assert!(!json.contains(needle));
        }
    }

    #[test]
    fn render_preview_json_is_valid_and_secret_free() {
        let previews = vec![
            module_render_preview(SuperbotModuleKind::Tickets),
            module_render_preview(SuperbotModuleKind::Recruit),
        ];
        let json = render_preview_json(&previews);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["mode"], "read_only");
        assert!(json.contains("panel_apply"));
        assert!(json.contains("xiii_recruit_accept"));
        assert!(json.contains("ACCEPTED_DIFFERENCE"));
        assert!(!json.contains("DISCORD_TOKEN"));
        assert!(!json.contains(".env.local"));
        for needle in [
            char::from_u32(0x00C3).unwrap(),
            char::from_u32(0x00D0).unwrap(),
            char::from_u32(0x00D1).unwrap(),
            char::from_u32(0xFFFD).unwrap(),
        ] {
            assert!(!json.contains(needle));
        }
    }

    #[test]
    fn discipline_ready_state_has_no_stale_placeholder_blockers() {
        assert_eq!(
            SuperbotModuleKind::Discipline.readiness(),
            ModuleReadiness::ReadyFull
        );
        let blockers = SuperbotModuleKind::Discipline.blockers().join(" ");
        assert!(!blockers.contains("BLOCKER"));
        assert!(!blockers.contains("acknowledge"));
        assert!(!blockers.contains("incomplete"));
        assert!(sync_command_plan(&[SuperbotModuleKind::Discipline])[0]
            .commands
            .contains(&"/discipline"));
    }

    #[test]
    fn strict_status_allows_ready_ticket_module_but_requires_state_when_enabled() {
        let dir = fixture_dir("strict_ready_ticket_enabled");
        let legacy_dir = dir.join("legacy-clanlist");
        fs::create_dir_all(&legacy_dir).unwrap();
        let mut config = config_with_legacy_dir(&dir, &legacy_dir);
        config.modules.tickets = true;

        let report = build_module_status_report(&config, &dir.join("data"), true);

        assert!(report.items.iter().any(|item| item.scope == "tickets"
            && item.message.contains("readiness=READY_FULL")
            && item.severity == xiii_core::Severity::Ok));
        assert!(report.items.iter().any(|item| item.scope == "tickets"
            && item.message.contains("fresh state missing")
            && item.severity == xiii_core::Severity::Fail));
        cleanup_dir(dir);
    }

    #[test]
    fn temp_voice_occupancy_detects_empty_old_channel_only_after_update() {
        let mut occupancy = TempVoiceOccupancy::default();
        occupancy.apply_voice_update(1, 10, Some(100));
        occupancy.apply_voice_update(1, 11, Some(100));

        let transition = occupancy.apply_voice_update(1, 10, Some(200));
        assert_eq!(transition.old_channel_id, Some(100));
        assert_eq!(transition.old_channel_member_count, 1);
        assert_eq!(occupancy.member_count(100), 1);

        let transition = occupancy.apply_voice_update(1, 11, None);
        assert_eq!(transition.old_channel_id, Some(100));
        assert_eq!(transition.old_channel_member_count, 0);
        assert_eq!(occupancy.member_count(100), 0);
    }

    #[test]
    fn clanlist_interval_uses_config_default_and_rejects_zero() {
        let dir = fixture_dir("run_interval");
        let legacy_dir = dir.join("legacy-clanlist");
        fs::create_dir_all(&legacy_dir).unwrap();
        let mut config = config_with_legacy_dir(&dir, &legacy_dir);
        config.clanlist.auto_refresh_seconds = 777;

        assert_eq!(clanlist_interval_seconds(None, &config).unwrap(), 777);
        assert_eq!(clanlist_interval_seconds(Some(60), &config).unwrap(), 60);
        assert!(clanlist_interval_seconds(Some(0), &config).is_err());
        cleanup_dir(dir);
    }

    #[test]
    fn non_overlap_guard_allows_only_one_refresh_at_a_time() {
        let mut guard = NonOverlapGuard::default();

        assert!(guard.try_start());
        assert!(!guard.try_start());
        guard.finish();
        assert!(guard.try_start());
    }

    #[test]
    fn output_path_safety_rejects_legacy_clanlist_data_dir() {
        let dir = fixture_dir("output_rejects_legacy");
        let legacy_dir = dir.join("legacy-clanlist");
        fs::create_dir_all(&legacy_dir).unwrap();
        let config = config_with_legacy_dir(&dir, &legacy_dir);

        let err = validate_output_path(&legacy_dir.join("report.json"), &config).unwrap_err();

        assert!(err.contains("LEGACY_CLANLIST_DATA_DIR"));
        cleanup_dir(dir);
    }

    #[test]
    fn write_plan_output_path_safety_is_inherited() {
        let dir = fixture_dir("write_plan_output_rejects_legacy");
        let legacy_dir = dir.join("legacy-clanlist");
        fs::create_dir_all(&legacy_dir).unwrap();
        let config = config_with_legacy_dir(&dir, &legacy_dir);

        let err = validate_output_path(&legacy_dir.join("write-plan.json"), &config).unwrap_err();

        assert!(err.contains("LEGACY_CLANLIST_DATA_DIR"));
        cleanup_dir(dir);
    }

    #[test]
    fn bootstrap_state_output_path_safety_rejects_legacy_paths() {
        let dir = fixture_dir("bootstrap_state_output_rejects_legacy");
        let legacy_dir = dir.join("legacy-clanlist");
        fs::create_dir_all(&legacy_dir).unwrap();
        let config = config_with_legacy_dir(&dir, &legacy_dir);

        let err = resolve_state_output_path(Some(&legacy_dir.join("state.json")), &config, false)
            .unwrap_err();

        assert!(err.contains("LEGACY_CLANLIST_DATA_DIR") || err.contains("old Clanlist"));
        cleanup_dir(dir);
    }

    #[test]
    fn health_output_path_safety_rejects_legacy_paths() {
        let dir = fixture_dir("health_output_rejects_legacy");
        let legacy_dir = dir.join("legacy-clanlist");
        fs::create_dir_all(&legacy_dir).unwrap();
        let config = config_with_legacy_dir(&dir, &legacy_dir);

        let err = resolve_health_output_path(&legacy_dir.join("health.json"), &config).unwrap_err();

        assert!(err.contains("LEGACY_CLANLIST_DATA_DIR") || err.contains("old Clanlist"));
        cleanup_dir(dir);
    }

    #[test]
    fn update_state_file_path_safety_rejects_legacy_paths() {
        let dir = fixture_dir("update_state_file_rejects_legacy");
        let legacy_dir = dir.join("legacy-clanlist");
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::write(legacy_dir.join("state.json"), "{}").unwrap();
        let config = config_with_legacy_dir(&dir, &legacy_dir);

        let err =
            resolve_state_file_path(Some(&legacy_dir.join("state.json")), &config).unwrap_err();

        assert!(err.contains("LEGACY_CLANLIST_DATA_DIR") || err.contains("old Clanlist"));
        cleanup_dir(dir);
    }

    #[test]
    fn utf8_output_writer_preserves_cyrillic() {
        let dir = fixture_dir("utf8_output");
        let legacy_dir = dir.join("legacy-clanlist");
        fs::create_dir_all(&legacy_dir).unwrap();
        let config = config_with_legacy_dir(&dir, &legacy_dir);
        let output = dir.join("report.txt");
        let text = "Список участников XIII";

        emit_output(text, Some(&output), Some(&config)).unwrap();

        assert_eq!(fs::read_to_string(&output).unwrap(), text);
        cleanup_dir(dir);
    }

    #[test]
    fn json_output_writer_writes_valid_json_only() {
        let dir = fixture_dir("json_output");
        let legacy_dir = dir.join("legacy-clanlist");
        fs::create_dir_all(&legacy_dir).unwrap();
        let config = config_with_legacy_dir(&dir, &legacy_dir);
        let output = dir.join("report.json");

        emit_output("{\"title\":\"Список\"}", Some(&output), Some(&config)).unwrap();

        let text = fs::read_to_string(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["title"], "Список");
        cleanup_dir(dir);
    }

    #[test]
    fn output_writer_does_not_leak_secret_text() {
        let dir = fixture_dir("no_secret_output");
        let legacy_dir = dir.join("legacy-clanlist");
        fs::create_dir_all(&legacy_dir).unwrap();
        let config = config_with_legacy_dir(&dir, &legacy_dir);
        let output = dir.join("report.txt");

        emit_output("DISCORD_TOKEN=<SET>", Some(&output), Some(&config)).unwrap();

        let text = fs::read_to_string(&output).unwrap();
        assert!(!text.contains("super-secret-token"));
        assert!(text.contains("<SET>"));
        cleanup_dir(dir);
    }

    #[test]
    fn health_json_is_valid_and_does_not_leak_secrets() {
        let dir = fixture_dir("health_json");
        let legacy_dir = dir.join("legacy-clanlist").join("data");
        fs::create_dir_all(&legacy_dir).unwrap();
        let config = config_with_legacy_dir(&dir, &legacy_dir);
        let path = resolve_health_output_path(&dir.join("clanlist_health.json"), &config).unwrap();
        let mut report = Report::new();
        report.ok(
            "discord",
            "edited main panel message id = 1502618001881436320",
        );
        let outcome = ClanlistRefreshOutcome::finished(
            xiii_clanlist::ClanlistUpdatePanelsResult {
                report,
                model: None,
                safety: xiii_clanlist::UpdateSafety::new(false),
            },
            None,
            "2026-05-09T10:00:00Z".to_owned(),
        );
        let health = clanlist_health_from_outcome(&outcome, None, None);

        write_clanlist_health(&path, &health).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["module"], "clanlist");
        assert!(!text.contains("super-secret-token"));
        cleanup_dir(dir);
    }

    #[test]
    fn retry_policy_stops_after_bounded_attempts() {
        assert_eq!(
            retry_delay_for_attempt(1, 3, Duration::from_secs(3)),
            Some(Duration::from_secs(3))
        );
        assert_eq!(
            retry_delay_for_attempt(2, 3, Duration::from_secs(3)),
            Some(Duration::from_secs(3))
        );
        assert_eq!(retry_delay_for_attempt(3, 3, Duration::from_secs(3)), None);
    }

    #[test]
    fn retry_policy_respects_retry_after_body_value() {
        let (duration, seconds) =
            parse_retry_after_from_body(br#"{"message":"rate limited","retry_after":3}"#).unwrap();

        assert_eq!(duration, Duration::from_secs(3));
        assert_eq!(seconds, 3.0);
    }

    #[test]
    fn service_status_parser_detects_active_running() {
        let status = parse_old_service_status(
            "Loaded: loaded (/etc/systemd/system/xiii-clanlist.service)\nActive: active (running)",
        );

        assert_eq!(status, OldServiceStatus::Running("Active: active"));
    }

    #[test]
    fn service_status_parser_detects_inactive_or_missing() {
        assert_eq!(
            parse_old_service_status("Active: inactive (dead)"),
            OldServiceStatus::Stopped("Active: inactive")
        );
        assert_eq!(
            parse_old_service_status("Unit xiii-clanlist.service could not be found."),
            OldServiceStatus::Stopped("unit could not be found")
        );
    }

    fn fixture_dir(name: &str) -> PathBuf {
        let suffix = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("xiii-superbot-main-{name}-{suffix}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup_dir(dir: PathBuf) {
        let _ = fs::remove_dir_all(dir);
    }

    fn config_with_legacy_dir(root: &Path, legacy_dir: &Path) -> SuperbotConfig {
        let mut config = SuperbotConfig::skeleton();
        config.legacy_paths.clanlist_data_dir =
            ConfigPath::new(legacy_dir.display().to_string(), root);
        config.legacy_paths.ticket_db =
            ConfigPath::new(root.join("tickets.db").display().to_string(), root);
        config.legacy_paths.voice_db =
            ConfigPath::new(root.join("voice.sqlite3").display().to_string(), root);
        config.legacy_paths.recruit_db =
            ConfigPath::new(root.join("recruits.db").display().to_string(), root);
        config.legacy_paths.vacation_db =
            ConfigPath::new(root.join("vacations.db").display().to_string(), root);
        config.legacy_paths.discipline_db =
            ConfigPath::new(root.join("discipline.sqlite").display().to_string(), root);
        config.legacy_paths.temp_voice_db =
            ConfigPath::new(root.join("temp_voice.sqlite3").display().to_string(), root);
        config
    }
}
