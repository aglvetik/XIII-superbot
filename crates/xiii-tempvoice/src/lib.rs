use xiii_core::{
    EnvDependency, ModuleId, ModuleManifest, SchedulerJobDescriptor, SlashCommandDescriptor,
    StateDependency,
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
        ModuleId::TempVoice,
        "Temp Voice Bot",
        "D:\\clients\\XIII 2\\XIII_BOTS_FULL_COPY\\opt\\XIII\\temp-voice-bot",
        "medium/high",
    )
    .with_state(StateDependency::sqlite("opt/XIII/temp-voice-bot/data/bot.sqlite3", "hub settings and tracked temp voice channels"))
    .with_env(EnvDependency::new(Some("DATABASE_PATH"), "LEGACY_TEMP_VOICE_DB_PATH", true, false, "legacy temp voice DB path"))
    .with_env(EnvDependency::new(Some("DELETE_AFTER_SECONDS"), "TEMP_VOICE_DELETE_AFTER_SECONDS", false, false, "empty channel deletion delay"))
    .with_command(SlashCommandDescriptor::new("/setup-voice-hub", "app/cogs/voice_hub.py:45").with_options(&["channel_id"]).mutating())
    .with_job(SchedulerJobDescriptor::startup("temp_voice_startup_cleanup", "app/services/temp_voice_service.py:50").mutating())
    .with_note("Preserve hub channel 1499122210542194899 and reconcile temp channels before deleting anything.")
}
