#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayRuntimePlan {
    pub one_gateway_connection: bool,
    pub enabled_modules: Vec<String>,
    pub command_sync_on_startup: bool,
}
