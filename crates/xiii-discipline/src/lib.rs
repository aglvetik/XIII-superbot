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
        ModuleId::Discipline,
        "XIII Discipline Bot",
        "D:\\clients\\XIII 2\\XIII_BOTS_FULL_COPY\\opt\\XIII\\xiii-discipline-bot",
        "very high",
    )
    .with_state(StateDependency::sqlite("opt/XIII/xiii-discipline-bot/data/discipline.sqlite", "settings, punishments, action logs, locks, migration state"))
    .with_env(EnvDependency::new(Some("DATABASE_PATH"), "LEGACY_DISCIPLINE_DB_PATH", true, false, "legacy discipline DB path"))
    .with_env(EnvDependency::new(Some("DISCIPLINE_BOARD_CHANNEL_ID"), "DISCIPLINE_BOARD_CHANNEL_ID", true, false, "discipline board channel"))
    .with_env(EnvDependency::new(Some("ADMIN_LOG_CHANNEL_ID"), "DISCIPLINE_LOG_CHANNEL_ID", true, false, "discipline admin log channel"))
    .with_command(SlashCommandDescriptor::new("/discipline", "src/interactions/router.ts:258").with_options(&["setup", "member user", "health"]).mutating())
    .with_component(ComponentRoute::new("xiii:panel:issue", "src/interactions/panel.ts:4").mutating())
    .with_component(ComponentRoute::new("xiii:panel:remove", "src/interactions/panel.ts:5").mutating())
    .with_component(ComponentRoute::new("xiii:panel:history", "src/interactions/panel.ts:6"))
    .with_component(ComponentRoute::new("xiii:board:page:prev", "src/interactions/panel.ts:7").mutating())
    .with_component(ComponentRoute::new("xiii:board:page:next", "src/interactions/panel.ts:8").mutating())
    .with_component(ComponentRoute::new("xiii:issue:*", "src/interactions/issueFlow.ts").mutating().transient())
    .with_component(ComponentRoute::new("xiii:remove:*", "src/interactions/removeFlow.ts").mutating().transient())
    .with_component(ComponentRoute::new("xiii:history:*", "src/interactions/historyFlow.ts").transient())
    .with_job(SchedulerJobDescriptor::interval("discipline_expiration_worker", 60, "src/index.ts").mutating())
    .with_job(SchedulerJobDescriptor::interval("discipline_board_refresh", 60, "src/index.ts").mutating())
    .with_note("Preserve board message 1501664727963664536 and warning/verbal/strict escalation semantics.")
}
