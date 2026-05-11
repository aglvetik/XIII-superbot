use xiii_core::{
    ComponentRoute, EnvDependency, ModuleId, ModuleManifest, SchedulerJobDescriptor,
    SlashCommandDescriptor, StateDependency,
};

pub mod commands;
pub mod config;
pub mod discord_io;
pub mod google;
pub mod interactions;
pub mod render;
pub mod repository;
pub mod runtime;
pub mod state;

#[cfg(test)]
mod tests;

pub fn manifest() -> ModuleManifest {
    ModuleManifest::new(
        ModuleId::Tickets,
        "XIII Ticket Bot",
        "D:\\clients\\XIII 2\\XIII_BOTS_FULL_COPY\\opt\\xiii-ticketbot",
        "very high",
    )
    .with_state(StateDependency::sqlite(
        "opt/xiii-ticketbot/tickets.db",
        "tickets, counters, Google dedupe, panel state",
    ))
    .with_env(EnvDependency::new(
        Some("GUILD_ID"),
        "XIII_GUILD_ID",
        true,
        false,
        "target guild",
    ))
    .with_env(EnvDependency::new(
        Some("OFFICER_REVIEW_CHANNEL_ID"),
        "TICKET_OFFICER_REVIEW_CHANNEL_ID",
        true,
        false,
        "Google Forms officer review channel",
    ))
    .with_env(EnvDependency::new(
        Some("GOOGLE_CREDENTIALS_PATH"),
        "TICKET_GOOGLE_CREDENTIALS_FILE",
        true,
        true,
        "ticket Google service account file",
    ))
    .with_env(EnvDependency::new(
        Some("GOOGLE_SHEET_ID"),
        "TICKET_GOOGLE_SHEET_ID",
        true,
        true,
        "ticket form Google Sheet ID",
    ))
    .with_command(
        SlashCommandDescriptor::new("/add", "app/discord_app/commands/moderation.py:33")
            .with_options(&["member"])
            .mutating(),
    )
    .with_command(
        SlashCommandDescriptor::new("/remove", "app/discord_app/commands/moderation.py:39")
            .with_options(&["member"])
            .mutating(),
    )
    .with_command(
        SlashCommandDescriptor::new(
            "/custom-ticket",
            "app/discord_app/commands/moderation.py:45",
        )
        .with_options(&["name", "user?", "reason?"])
        .mutating(),
    )
    .with_command(
        SlashCommandDescriptor::new("!panel", "app/discord_app/commands/panel.py:8").mutating(),
    )
    .with_command(
        SlashCommandDescriptor::new(
            "!accept|!принять",
            "app/discord_app/commands/moderation.py:11",
        )
        .mutating(),
    )
    .with_command(
        SlashCommandDescriptor::new(
            "!reject|!отклонить",
            "app/discord_app/commands/moderation.py:21",
        )
        .mutating(),
    )
    .with_component(ComponentRoute::new("panel_apply", "app/config/constants.py:112").mutating())
    .with_component(ComponentRoute::new("panel_question", "app/config/constants.py:114").mutating())
    .with_component(ComponentRoute::new("panel_idea", "app/config/constants.py:116").mutating())
    .with_component(ComponentRoute::new("ticket_close", "app/config/constants.py:119").mutating())
    .with_component(
        ComponentRoute::new("ticket_staff_notes", "app/config/constants.py:121").mutating(),
    )
    .with_component(
        ComponentRoute::new("ticket_close_confirm", "app/config/constants.py:124").mutating(),
    )
    .with_component(ComponentRoute::new(
        "ticket_close_cancel",
        "app/config/constants.py:126",
    ))
    .with_component(ComponentRoute::new("ticket_delete", "app/config/constants.py:129").mutating())
    .with_component(
        ComponentRoute::new("ticket_reopen_mod", "app/config/constants.py:131").mutating(),
    )
    .with_component(
        ComponentRoute::new("dm_reopen_generic", "app/config/constants.py:134").mutating(),
    )
    .with_component(ComponentRoute::new("notes_delete", "app/config/constants.py:137").mutating())
    .with_component(
        ComponentRoute::new("app_decision_accept", "app/config/constants.py:140").mutating(),
    )
    .with_component(
        ComponentRoute::new("app_decision_reject", "app/config/constants.py:142").mutating(),
    )
    .with_job(
        SchedulerJobDescriptor::interval(
            "ticket_google_forms_poll",
            30,
            "app/discord_app/handlers/lifecycle.py:37",
        )
        .mutating(),
    )
    .with_job(
        SchedulerJobDescriptor::interval(
            "ticket_cleanup",
            600,
            "app/discord_app/handlers/lifecycle.py:43",
        )
        .mutating(),
    )
    .with_note("Preserve ticket panel message 1499423034359152710 and all counters/dedupe rows.")
    .with_note(
        "Runtime uses fresh Superbot-owned ticket panel state and safe Rust HTML transcripts.",
    )
}
