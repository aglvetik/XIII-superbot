use xiii_core::{
    ComponentRoute, EnvDependency, ModuleId, ModuleManifest, SchedulerJobDescriptor,
    SlashCommandDescriptor, StateDependency,
};

pub mod commands;
pub mod config;
pub mod discord_io;
pub mod interactions;
pub mod render;
pub mod repository;
pub mod runtime;
pub mod state;

#[cfg(test)]
mod tests;

pub fn manifest() -> ModuleManifest {
    ModuleManifest::new(
        ModuleId::VoiceActivity,
        "XIII Voice Activity Bot",
        "D:\\clients\\XIII 2\\XIII_BOTS_FULL_COPY\\opt\\XIII\\xiii-voice-activity-bot",
        "very high",
    )
    .with_state(StateDependency::sqlite(
        "opt/XIII/xiii-voice-activity-bot/data/voice_activity.sqlite3",
        "voice users, completed sessions, active sessions, panel/report state",
    ))
    .with_env(EnvDependency::new(
        Some("DATABASE_PATH"),
        "LEGACY_VOICE_DB_PATH",
        true,
        false,
        "legacy voice DB path",
    ))
    .with_env(EnvDependency::new(
        Some("VACATION_ROLE_ID"),
        "VOICE_VACATION_MARKER_ROLE_ID",
        true,
        false,
        "inactivity report vacation marker role",
    ))
    .with_env(EnvDependency::new(
        Some("PUBLIC_STATS_CHANNEL_ID"),
        "VOICE_STATS_PANEL_CHANNEL_ID",
        true,
        false,
        "public stats panel channel",
    ))
    .with_command(
        SlashCommandDescriptor::new("/voice-top", "app/cogs/public_stats.py:32")
            .with_options(&["period?"]),
    )
    .with_command(SlashCommandDescriptor::new(
        "/inactive-check",
        "app/cogs/inactivity.py:18",
    ))
    .with_component(ComponentRoute::new(
        "public-stats-panel:period",
        "app/views/public_stats_view.py:35",
    ))
    .with_component(ComponentRoute::new(
        "public-stats-panel:previous",
        "app/views/public_stats_view.py:51",
    ))
    .with_component(ComponentRoute::new(
        "public-stats-panel:next",
        "app/views/public_stats_view.py:61",
    ))
    .with_component(ComponentRoute::new(
        "inactive-check:period",
        "app/views/inactive_view.py:45",
    ))
    .with_component(ComponentRoute::new(
        "inactive-check:previous",
        "app/views/pagination.py:27",
    ))
    .with_component(ComponentRoute::new(
        "inactive-check:next",
        "app/views/pagination.py:36",
    ))
    .with_job(
        SchedulerJobDescriptor::interval("voice_heartbeat", 60, "app/services/voice_tracker.py:47")
            .mutating(),
    )
    .with_job(
        SchedulerJobDescriptor::interval(
            "voice_public_stats_refresh",
            60,
            "app/services/public_stats_panel_service.py:43",
        )
        .mutating(),
    )
    .with_job(
        SchedulerJobDescriptor::interval(
            "voice_auto_inactive_reports",
            600,
            "app/services/auto_report_service.py:64",
        )
        .mutating(),
    )
    .with_note(
        "Migrate last; preserve completed sessions and close active sessions only through explicit voice-finalize-cutover policy.",
    )
}
