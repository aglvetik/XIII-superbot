use thiserror::Error;
use xiii_core::{ComponentRoute, ModuleManifest, SlashCommandDescriptor};

pub mod embeds;
pub mod gateway;
pub mod http;
pub mod interactions;
pub mod messages;
pub mod permissions;

pub const DISCORD_STACK: &str = "twilight";

#[derive(Debug, Error)]
pub enum DiscordPlanError {
    #[error("Discord login is disabled in scaffold mode")]
    LoginDisabled,
}

#[derive(Debug, Clone)]
pub struct DiscordRuntimePlan {
    pub login_enabled: bool,
    pub gateway_connection_enabled: bool,
    pub stack: &'static str,
}

impl DiscordRuntimePlan {
    pub fn scaffold_only() -> Self {
        Self {
            login_enabled: false,
            gateway_connection_enabled: false,
            stack: DISCORD_STACK,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CentralRouter {
    slash_commands: Vec<SlashCommandDescriptor>,
    component_routes: Vec<ComponentRoute>,
}

impl CentralRouter {
    pub fn from_manifests(manifests: &[ModuleManifest]) -> Self {
        Self {
            slash_commands: manifests
                .iter()
                .flat_map(|manifest| manifest.slash_commands.clone())
                .collect(),
            component_routes: manifests
                .iter()
                .flat_map(|manifest| manifest.component_routes.clone())
                .collect(),
        }
    }

    pub fn slash_commands(&self) -> &[SlashCommandDescriptor] {
        &self.slash_commands
    }

    pub fn component_routes(&self) -> &[ComponentRoute] {
        &self.component_routes
    }

    pub fn duplicate_component_patterns(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        let mut duplicates = std::collections::BTreeSet::new();
        for route in &self.component_routes {
            if !seen.insert(route.custom_id_pattern.clone()) {
                duplicates.insert(route.custom_id_pattern.clone());
            }
        }
        duplicates.into_iter().collect()
    }
}
