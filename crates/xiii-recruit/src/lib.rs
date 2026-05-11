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
        ModuleId::Recruit,
        "XIII Recruit Bot",
        "D:\\clients\\XIII 2\\XIII_BOTS_FULL_COPY\\opt\\XIII\\xiii-recruit-bot",
        "high",
    )
    .with_state(StateDependency::sqlite(
        "opt/XIII/xiii-recruit-bot/data/recruits.db",
        "active recruit, recruit voice sessions, decisions",
    ))
    .with_env(EnvDependency::new(
        Some("DATABASE_PATH"),
        "LEGACY_RECRUIT_DB_PATH",
        true,
        false,
        "legacy recruit DB path",
    ))
    .with_env(EnvDependency::new(
        Some("DECISION_CHANNEL_ID"),
        "RECRUIT_DECISION_CHANNEL_ID",
        true,
        false,
        "recruit decision channel",
    ))
    .with_env(EnvDependency::new(
        Some("DECISION_PING_ROLE_IDS"),
        "RECRUIT_DECISION_PING_ROLE_IDS",
        false,
        false,
        "automatic decision panel pings",
    ))
    .with_command(SlashCommandDescriptor::new(
        "/recruits",
        "app/commands/recruit_commands.py:25",
    ))
    .with_command(
        SlashCommandDescriptor::new("/recruit-panel", "app/commands/recruit_commands.py:46")
            .with_options(&["user"])
            .mutating(),
    )
    .with_component(
        ComponentRoute::new(
            "xiii_recruit_accept:{recruit_id}",
            "app/discord_ui/views.py:42",
        )
        .mutating(),
    )
    .with_component(
        ComponentRoute::new(
            "xiii_recruit_reject:{recruit_id}",
            "app/discord_ui/views.py:50",
        )
        .mutating(),
    )
    .with_component(
        ComponentRoute::new(
            "xiii_recruit_extend:{recruit_id}",
            "app/discord_ui/views.py:58",
        )
        .mutating(),
    )
    .with_job(
        SchedulerJobDescriptor::interval("recruit_probation_checker", 300, "app/bot.py:140")
            .mutating(),
    )
    .with_note(
        "Preserve active recruit user 973660882242519150 and decision message 1501259037357117641.",
    )
}
