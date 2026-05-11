use xiii_core::{ComponentRoute, SlashCommandDescriptor};

#[derive(Debug, Clone, Default)]
pub struct InteractionRegistry {
    pub slash_commands: Vec<SlashCommandDescriptor>,
    pub component_routes: Vec<ComponentRoute>,
}
