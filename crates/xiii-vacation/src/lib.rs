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
        ModuleId::Vacation,
        "XIII Vacation Bot",
        "D:\\clients\\XIII 2\\XIII_BOTS_FULL_COPY\\opt\\XIII\\xiii-vacation-bot",
        "high",
    )
    .with_state(StateDependency::sqlite(
        "opt/XIII/xiii-vacation-bot/data/vacations.db",
        "vacation requests, active vacations, panel IDs",
    ))
    .with_env(EnvDependency::new(
        Some("DATABASE_PATH"),
        "LEGACY_VACATION_DB_PATH",
        true,
        false,
        "legacy vacation DB path",
    ))
    .with_env(EnvDependency::new(
        Some("VACATION_ROLE_ID"),
        "VACATION_ROLE_ID",
        true,
        false,
        "actual vacation role added/removed",
    ))
    .with_env(EnvDependency::new(
        Some("PANEL_CHANNEL_ID"),
        "VACATION_PANEL_CHANNEL_ID",
        true,
        false,
        "vacation request panel channel",
    ))
    .with_command(SlashCommandDescriptor::new(
        "/vacations",
        "internal/bot/client.go:93",
    ))
    .with_component(ComponentRoute::new("vacation:apply", "internal/bot/embeds.go:53").mutating())
    .with_component(
        ComponentRoute::new("vacation:modal", "internal/bot/interactions.go:40").mutating(),
    )
    .with_component(
        ComponentRoute::new("vacation:approve:{request_id}", "internal/bot/embeds.go:89")
            .mutating(),
    )
    .with_component(
        ComponentRoute::new("vacation:reject:{request_id}", "internal/bot/embeds.go:95").mutating(),
    )
    .with_component(
        ComponentRoute::new("vacation:end:{vacation_id}", "internal/bot/embeds.go:127").mutating(),
    )
    .with_component(
        ComponentRoute::new(
            "vacation:end_confirm:{vacation_id}",
            "internal/bot/embeds.go:249",
        )
        .mutating(),
    )
    .with_component(ComponentRoute::new(
        "vacation:end_cancel:{vacation_id}",
        "internal/bot/embeds.go:254",
    ))
    .with_job(
        SchedulerJobDescriptor::interval("vacation_expiration_worker", 60, "internal/scheduler")
            .mutating(),
    )
    .with_job(
        SchedulerJobDescriptor::interval(
            "vacation_active_panel_refresh",
            60,
            "internal/bot/active_vacations_panel.go",
        )
        .mutating(),
    )
    .with_note(
        "Do not confuse VACATION_ROLE_ID 1498022112131289214 with VOICE_VACATION_MARKER_ROLE_ID.",
    )
}
